use std::{
    env,
    error::Error,
    io,
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

use liftassistant_model::ObservationSessionId;
use liftassistant_rmf::{
    GatewayCommandRequest, GatewayIngestError, PrivateIngress, PrivateIngressConnection,
    RmfAdapterError, RmfCommandTransport, RmfFreshnessPolicy, RmfIntentGateway, RmfLiftCommand,
    RmfLiftRequestPublisher, RmfLivePayload, RmfLiveSubscriber, RmfReceiveControl,
    RmfReceiveOutcome, load_gateway_config,
};
use signal_hook::consts::signal::{SIGINT, SIGTERM};

const OBSERVATION_QUEUE: usize = 128;
const INGRESS_QUEUE: usize = 8;
const LOOP_POLL: Duration = Duration::from_millis(25);

enum ObservationEvent {
    Payload(RmfLivePayload),
    Stale,
    Transport(String),
}

struct IngressEvent {
    uid: u32,
    request: GatewayCommandRequest,
    connection: PrivateIngressConnection,
}

struct ReconnectingRmfTransport {
    domain_id: u16,
    generation: usize,
    publisher: Option<RmfLiftRequestPublisher>,
    next_attempt: Instant,
    backoff: Duration,
}

impl ReconnectingRmfTransport {
    fn new(domain_id: u16) -> Self {
        let mut transport = Self {
            domain_id,
            generation: 0,
            publisher: None,
            next_attempt: Instant::now(),
            backoff: Duration::from_millis(250),
        };
        transport.reconnect_if_due();
        transport
    }

    fn reconnect_if_due(&mut self) {
        if self.publisher.is_some() || Instant::now() < self.next_attempt {
            return;
        }
        let node_name = format!("rmf_intent_gateway_writer_{}", self.generation);
        self.generation = self.generation.saturating_add(1);
        match RmfLiftRequestPublisher::connect(self.domain_id, &node_name) {
            Ok(publisher) => {
                self.publisher = Some(publisher);
                self.backoff = Duration::from_millis(250);
                eprintln!("RMF gateway command transport created");
            }
            Err(error) => {
                eprintln!("RMF gateway command transport unavailable: {error}");
                self.next_attempt = Instant::now() + self.backoff;
                self.backoff = (self.backoff * 2).min(Duration::from_secs(4));
            }
        }
    }
}

impl RmfCommandTransport for ReconnectingRmfTransport {
    fn available(&self) -> bool {
        self.publisher
            .as_ref()
            .is_some_and(RmfLiftRequestPublisher::has_subscriber)
    }

    fn publish(&mut self, command: RmfLiftCommand) -> Result<(), String> {
        let Some(publisher) = &self.publisher else {
            return Err("RMF command transport is unavailable".into());
        };
        if let Err(error) = publisher.publish(command) {
            self.publisher = None;
            self.next_attempt = Instant::now() + self.backoff;
            self.backoff = (self.backoff * 2).min(Duration::from_secs(4));
            return Err(error.to_string());
        }
        Ok(())
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("LiftAssistant RMF gateway failed closed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let config_path = parse_args()?;
    let config = load_gateway_config(&config_path)?;
    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGINT, shutdown.clone())?;
    signal_hook::flag::register(SIGTERM, shutdown.clone())?;
    let session = fresh_source_session()?;
    let transport = ReconnectingRmfTransport::new(config.domain_id);
    let mut gateway = RmfIntentGateway::new(config.clone(), session, transport)?;
    let ingress = PrivateIngress::bind(&config.socket_path)?;

    let (observation_tx, observation_rx) = mpsc::sync_channel(OBSERVATION_QUEUE);
    let observation_overloaded = Arc::new(AtomicBool::new(false));
    let observation_admission_gate = Arc::new(Mutex::new(()));
    let observer = spawn_observer(
        config.domain_id,
        Duration::from_millis(config.max_payload_silence_ms),
        shutdown.clone(),
        observation_tx,
        observation_overloaded.clone(),
        observation_admission_gate.clone(),
    );
    let (ingress_tx, ingress_rx) = mpsc::sync_channel(INGRESS_QUEUE);
    let ingress_shutdown = shutdown.clone();
    let ingress_thread = thread::spawn(move || {
        while !ingress_shutdown.load(Ordering::Relaxed) {
            let mut connection = match ingress.accept() {
                Ok(connection) => connection,
                Err(error) => {
                    eprintln!("private gateway accept failed: {error}");
                    continue;
                }
            };
            if ingress_shutdown.load(Ordering::Relaxed) {
                break;
            }
            let uid = connection.peer_uid();
            match connection.read_request() {
                Ok(request) => {
                    if ingress_tx
                        .send(IngressEvent {
                            uid,
                            request,
                            connection,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) => eprintln!("private gateway request rejected: {error}"),
            }
        }
    });

    eprintln!(
        "LiftAssistant RMF gateway started: socket={} readiness={:?}",
        config.socket_path.display(),
        gateway.readiness()
    );
    let mut reported_unconfigured_lift = false;
    while !shutdown.load(Ordering::Relaxed) {
        {
            let _admission = observation_admission_gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            apply_observation_overload(&observation_overloaded, &observation_rx, &mut gateway);
        }
        match observation_rx.recv_timeout(LOOP_POLL) {
            Ok(ObservationEvent::Payload(payload)) => {
                if let Err(error) = gateway.ingest(payload) {
                    match &error {
                        GatewayIngestError::Adapter(RmfAdapterError::UnconfiguredLift {
                            ..
                        }) if reported_unconfigured_lift => {}
                        GatewayIngestError::Adapter(RmfAdapterError::UnconfiguredLift {
                            ..
                        }) => {
                            reported_unconfigured_lift = true;
                            eprintln!("RMF observation evidence rejected: {error}");
                        }
                        _ => eprintln!("RMF observation evidence rejected: {error}"),
                    }
                }
            }
            Ok(ObservationEvent::Stale) => {
                gateway.invalidate_observations();
                eprintln!("RMF Operational observation became stale; writes are not ready");
            }
            Ok(ObservationEvent::Transport(error)) => {
                eprintln!("RMF observation transport generation ended: {error}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other("RMF observation supervisor stopped").into());
            }
        }
        for _ in 0..INGRESS_QUEUE {
            let Ok(event) = ingress_rx.try_recv() else {
                break;
            };
            let _admission = observation_admission_gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            apply_observation_overload(&observation_overloaded, &observation_rx, &mut gateway);
            let response = gateway.handle_command(event.uid, event.request);
            if let Err(error) = event.connection.respond(&response) {
                eprintln!("private gateway response failed: {error}");
            }
        }
        gateway.transport_mut().reconnect_if_due();
    }

    // Wake the blocking local accept so its owner can drop and remove the socket.
    if let Ok(stream) = UnixStream::connect(&config.socket_path) {
        let _ = stream.shutdown(Shutdown::Both);
    }
    let _ = ingress_thread.join();
    let _ = observer.join();
    eprintln!("LiftAssistant RMF gateway stopped gracefully");
    Ok(())
}

fn parse_args() -> Result<PathBuf, io::Error> {
    let mut args = env::args_os();
    let _program = args.next();
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--config")) {
        return Err(io::Error::other(
            "usage: liftassistant-rmf-gateway --config <private-config.json>",
        ));
    }
    let path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("--config requires a path"))?;
    if args.next().is_some() {
        return Err(io::Error::other("unexpected extra gateway arguments"));
    }
    Ok(path)
}

fn fresh_source_session() -> Result<ObservationSessionId, io::Error> {
    let mut entropy = [0_u8; 16];
    getrandom::fill(&mut entropy)
        .map_err(|error| io::Error::other(format!("source-session entropy failed: {error}")))?;
    Ok(ObservationSessionId::new(format!(
        "{:032x}",
        u128::from_be_bytes(entropy)
    )))
}

fn spawn_observer(
    domain_id: u16,
    freshness: Duration,
    shutdown: Arc<AtomicBool>,
    events: mpsc::SyncSender<ObservationEvent>,
    overloaded: Arc<AtomicBool>,
    admission_gate: Arc<Mutex<()>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let policy = RmfFreshnessPolicy::new(freshness).expect("validated nonzero freshness");
        let mut generation = 0_usize;
        let mut backoff = Duration::from_millis(250);
        let mut received_in_lifetime = false;
        while !shutdown.load(Ordering::Relaxed) {
            let node_name = format!("rmf_intent_gateway_reader_{generation}");
            generation = generation.saturating_add(1);
            let mut received_in_generation = false;
            let result = RmfLiveSubscriber::connect(domain_id, &node_name).and_then(|subscriber| {
                subscriber.receive_supervised(
                    policy,
                    freshness,
                    Duration::from_millis(100),
                    || shutdown.load(Ordering::Relaxed),
                    |payload| {
                        if let Some(payload) = payload {
                            received_in_generation = true;
                            if queue_observation_event(
                                &events,
                                &overloaded,
                                &admission_gate,
                                ObservationEvent::Payload(payload),
                            ) == RmfReceiveControl::Stop
                            {
                                return RmfReceiveControl::Stop;
                            }
                        }
                        RmfReceiveControl::Continue
                    },
                )
            });
            if received_in_generation {
                received_in_lifetime = true;
                backoff = Duration::from_millis(250);
            }
            match result {
                Ok(RmfReceiveOutcome::AwaitingPayloadTimeout { max_wait }) => {
                    if received_in_lifetime {
                        let _ = queue_observation_event(
                            &events,
                            &overloaded,
                            &admission_gate,
                            ObservationEvent::Stale,
                        );
                    }
                    let _ = queue_observation_event(
                        &events,
                        &overloaded,
                        &admission_gate,
                        ObservationEvent::Transport(format!(
                            "subscriber generation received no payload within {max_wait:?}"
                        )),
                    );
                }
                Ok(RmfReceiveOutcome::PayloadStale { .. }) => {
                    let _ = queue_observation_event(
                        &events,
                        &overloaded,
                        &admission_gate,
                        ObservationEvent::Stale,
                    );
                }
                Ok(RmfReceiveOutcome::ShutdownRequested | RmfReceiveOutcome::HandlerStopped) => {
                    break;
                }
                Err(error) => {
                    let _ = queue_observation_event(
                        &events,
                        &overloaded,
                        &admission_gate,
                        ObservationEvent::Transport(error.to_string()),
                    );
                }
            }
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(4));
        }
    })
}

fn queue_observation_event(
    events: &mpsc::SyncSender<ObservationEvent>,
    overloaded: &AtomicBool,
    admission_gate: &Mutex<()>,
    event: ObservationEvent,
) -> RmfReceiveControl {
    let _admission = admission_gate
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if overloaded.load(Ordering::Acquire) {
        return RmfReceiveControl::Continue;
    }
    match events.try_send(event) {
        Ok(()) => RmfReceiveControl::Continue,
        Err(TrySendError::Full(_)) => {
            overloaded.store(true, Ordering::Release);
            RmfReceiveControl::Continue
        }
        Err(TrySendError::Disconnected(_)) => RmfReceiveControl::Stop,
    }
}

fn apply_observation_overload<T: RmfCommandTransport>(
    overloaded: &AtomicBool,
    events: &mpsc::Receiver<ObservationEvent>,
    gateway: &mut RmfIntentGateway<T>,
) {
    if overloaded.swap(false, Ordering::AcqRel) {
        while events.try_recv().is_ok() {}
        gateway.mark_observation_overloaded();
        eprintln!(
            "RMF observation queue overloaded; Operational evidence was revoked and writes are not ready"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_rmf::RmfLiftState;

    fn payload(sequence: i64) -> RmfLivePayload {
        RmfLivePayload {
            source_time_nanos: sequence,
            state: RmfLiftState {
                lift_name: "Lift1".into(),
                available_floors: vec!["L1".into()],
                current_floor: "L1".into(),
                destination_floor: "L1".into(),
                door_state: RmfLiftState::DOOR_CLOSED,
                motion_state: RmfLiftState::MOTION_STOPPED,
                available_modes: vec![RmfLiftState::MODE_AGV],
                current_mode: RmfLiftState::MODE_AGV,
                session_id: String::new(),
            },
        }
    }

    #[test]
    fn bounded_observation_queue_signals_overload_and_drops_until_supervisor_acknowledges() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let overloaded = AtomicBool::new(false);
        let admission_gate = Mutex::new(());
        assert_eq!(
            queue_observation_event(
                &sender,
                &overloaded,
                &admission_gate,
                ObservationEvent::Payload(payload(1))
            ),
            RmfReceiveControl::Continue
        );
        assert_eq!(
            queue_observation_event(
                &sender,
                &overloaded,
                &admission_gate,
                ObservationEvent::Payload(payload(2))
            ),
            RmfReceiveControl::Continue
        );
        assert!(overloaded.load(Ordering::Acquire));
        let _ = queue_observation_event(
            &sender,
            &overloaded,
            &admission_gate,
            ObservationEvent::Payload(payload(3)),
        );
        let ObservationEvent::Payload(retained) = receiver.recv().unwrap() else {
            panic!("first payload remains queued until the supervisor revokes evidence");
        };
        assert_eq!(retained.source_time_nanos, 1);
        assert!(receiver.try_recv().is_err());

        overloaded.store(false, Ordering::Release);
        let _ = queue_observation_event(
            &sender,
            &overloaded,
            &admission_gate,
            ObservationEvent::Payload(payload(4)),
        );
        let ObservationEvent::Payload(recovered) = receiver.recv().unwrap() else {
            panic!("fresh payload is accepted after overload acknowledgement");
        };
        assert_eq!(recovered.source_time_nanos, 4);
    }
}
