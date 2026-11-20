use std::{
    env,
    error::Error,
    io,
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use liftassistant_model::ObservationSessionId;
use liftassistant_rmf::{
    GatewayCommandRequest, PrivateIngress, PrivateIngressConnection, RmfCommandTransport,
    RmfFreshnessPolicy, RmfIntentGateway, RmfLiftCommand, RmfLiftRequestPublisher, RmfLivePayload,
    RmfLiveSubscriber, RmfReceiveControl, RmfReceiveOutcome, load_gateway_config,
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
    let observer = spawn_observer(
        config.domain_id,
        Duration::from_millis(config.max_payload_silence_ms),
        shutdown.clone(),
        observation_tx,
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
    while !shutdown.load(Ordering::Relaxed) {
        match observation_rx.recv_timeout(LOOP_POLL) {
            Ok(ObservationEvent::Payload(payload)) => {
                if let Err(error) = gateway.ingest(payload) {
                    eprintln!("RMF observation evidence rejected: {error}");
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
        while let Ok(event) = ingress_rx.try_recv() {
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
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let policy = RmfFreshnessPolicy::new(freshness).expect("validated nonzero freshness");
        let mut generation = 0_usize;
        let mut backoff = Duration::from_millis(250);
        while !shutdown.load(Ordering::Relaxed) {
            let node_name = format!("rmf_intent_gateway_reader_{generation}");
            generation = generation.saturating_add(1);
            let result = RmfLiveSubscriber::connect(domain_id, &node_name).and_then(|subscriber| {
                subscriber.receive_until_stale_or_shutdown(
                    policy,
                    Duration::from_millis(100),
                    || shutdown.load(Ordering::Relaxed),
                    |payload| {
                        if let Some(payload) = payload {
                            if events.send(ObservationEvent::Payload(payload)).is_err() {
                                return RmfReceiveControl::Stop;
                            }
                        }
                        RmfReceiveControl::Continue
                    },
                )
            });
            match result {
                Ok(RmfReceiveOutcome::PayloadStale { .. }) => {
                    let _ = events.send(ObservationEvent::Stale);
                }
                Ok(RmfReceiveOutcome::ShutdownRequested | RmfReceiveOutcome::HandlerStopped) => {
                    break;
                }
                Err(error) => {
                    let _ = events.send(ObservationEvent::Transport(error.to_string()));
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
