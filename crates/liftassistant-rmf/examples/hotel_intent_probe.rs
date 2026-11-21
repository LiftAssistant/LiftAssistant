//! One high-level trip through Engine against the isolated Hotel lift plugin.

use std::{
    collections::BTreeMap,
    error::Error,
    fs, io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use liftassistant_engine::{
    Admission, ControllerId, Engine, ObservationBindingRole, PrincipalId, Receipt, Reconciliation,
};
use liftassistant_model::{
    BuildingTopology, CarId, DoorState, ExecutionCapabilities, Floor, Intent, IntentId,
    ObservationSessionId, ObservationSourceId,
};
use liftassistant_rmf::{
    RmfAdapterError, RmfFreshnessPolicy, RmfInteractionModeProjection, RmfLiftBinding,
    RmfLiftState, RmfLivePayload, RmfLiveSubscriber, RmfObservationAdapter, RmfReceiveControl,
};
use liftassistant_runtime::{ControlledIntentSink, IntentReceipt, ReceiptStatus, Submission};
use ros2_client::{
    Context, ContextOptions, MessageTypeName, Name, Node, NodeName, NodeOptions, Publisher,
    builtin_interfaces::Time, ros2,
};
use serde::Serialize;

const DOMAIN: u16 = 47;
const HOLDER_PREFIX: &str = "liftassistant-hotel-engine-probe";

fn start_reader(generation: usize, tx: mpsc::SyncSender<RmfLivePayload>) {
    thread::spawn(move || {
        let result =
            RmfLiveSubscriber::connect(DOMAIN, &format!("hotel_intent_probe_reader_{generation}"))
                .and_then(|reader| {
                    reader.receive_until_stale(
                        RmfFreshnessPolicy::new(Duration::from_secs(4)).expect("nonzero silence"),
                        |sample| {
                            if tx.send(sample).is_ok() {
                                RmfReceiveControl::Continue
                            } else {
                                RmfReceiveControl::Stop
                            }
                        },
                    )
                });
        if let Err(error) = result {
            eprintln!("subscriber generation {generation} ended with {error}");
        }
    });
}

fn observation_adapters(
    source: &ObservationSourceId,
    session: &ObservationSessionId,
    projection: RmfInteractionModeProjection,
    floors: &BTreeMap<String, Floor>,
    a: &CarId,
    b: &CarId,
) -> Result<(RmfObservationAdapter, RmfObservationAdapter), Box<dyn Error>> {
    let mapper = RmfObservationAdapter::new(
        source.clone(),
        session.clone(),
        projection,
        [
            (
                "Lift1".into(),
                RmfLiftBinding::new(a.clone(), floors.clone()),
            ),
            (
                "Lift2".into(),
                RmfLiftBinding::new(b.clone(), floors.clone()),
            ),
        ],
    )?;
    let single_lift = RmfObservationAdapter::new(
        source.clone(),
        session.clone(),
        projection,
        [(
            "Lift1".into(),
            RmfLiftBinding::new(a.clone(), floors.clone()),
        )],
    )?;
    Ok((mapper, single_lift))
}

fn require_isolation() -> io::Result<()> {
    // The private test subnet has no gateway and is never used by the read-only Host.
    let routes = fs::read_to_string("/proc/net/route")?;
    let local_addresses = fs::read_to_string("/proc/net/fib_trie")?;
    let has_default = routes
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some("00000000"));
    let interfaces = fs::read_dir("/sys/class/net")?.collect::<Result<Vec<_>, _>>()?;
    if has_default
        || interfaces.len() != 2
        || !local_addresses
            .lines()
            .any(|line| line.split_whitespace().last() == Some("10.253.71.2"))
        || std::env::var("ROS_DOMAIN_ID").as_deref() != Ok("47")
    {
        return Err(io::Error::other(
            "requires only the dedicated gateway-free Hotel test network",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
struct LiftRequest {
    lift_name: String,
    request_time: Time,
    session_id: String,
    request_type: u8,
    destination_floor: String,
    door_state: u8,
}

struct HotelWriter {
    id: ControllerId,
    node: Node,
    publisher: Publisher<LiftRequest>,
    issued: Option<IntentId>,
    last_source_time: i64,
    native_floor: String,
    transport_writes: usize,
    last_request: Option<LiftRequest>,
    holder: Option<String>,
}

impl HotelWriter {
    fn connect(native_floor: &str, generation: usize) -> Result<Self, Box<dyn Error>> {
        let context = Context::with_options(ContextOptions::new().domain_id(DOMAIN))?;
        let mut node = context.new_node(
            NodeName::new("/", &format!("hotel_intent_probe_writer_{generation}"))?,
            NodeOptions::new(),
        )?;
        let qos = ros2::QosPolicyBuilder::new()
            .history(ros2::policy::History::KeepLast { depth: 1 })
            .durability(ros2::policy::Durability::Volatile)
            .reliability(ros2::policy::Reliability::Reliable {
                max_blocking_time: ros2::Duration::from_millis(100),
            })
            .build();
        let topic = node.create_topic(
            &Name::new("/", "lift_requests")?,
            MessageTypeName::new("rmf_lift_msgs", "LiftRequest"),
            &qos,
        )?;
        let publisher = node.create_publisher(&topic, Some(qos))?;
        let spinner = node.spinner()?;
        smol::spawn(async move {
            let _ = spinner.spin().await;
        })
        .detach();
        Ok(Self {
            id: ControllerId::new("isolated-hotel-probe"),
            node,
            publisher,
            issued: None,
            last_source_time: 0,
            native_floor: native_floor.into(),
            transport_writes: 0,
            last_request: None,
            holder: None,
        })
    }

    fn write(&mut self, request: LiftRequest) -> Submission {
        self.transport_writes += 1;
        self.last_request = Some(request.clone());
        if let Err(error) = self.publisher.publish(request) {
            eprintln!("local publish was uncertain: {error:?}");
            Submission::Indeterminate
        } else {
            Submission::Submitted
        }
    }

    fn retry_transport(&mut self) -> Submission {
        let request = self.last_request.clone().expect("a command was dispatched");
        println!(
            "transport retry for {:?}, write={}",
            self.issued,
            self.transport_writes + 1
        );
        self.write(request)
    }

    fn holder(&self) -> Option<&str> {
        self.holder.as_deref()
    }

    fn end_current_session(&mut self, source_time: i64) -> Submission {
        let request = LiftRequest {
            lift_name: "Lift1".into(),
            request_time: Time::from_nanos(source_time),
            session_id: self
                .holder
                .clone()
                .expect("a session can end only after dispatch"),
            request_type: 0, // REQUEST_END_SESSION
            destination_floor: self.native_floor.clone(),
            door_state: 0, // DOOR_CLOSED
        };
        self.write(request)
    }
}

impl ControlledIntentSink for HotelWriter {
    fn controller_id(&self) -> &ControllerId {
        &self.id
    }

    fn available(&self, car: &CarId) -> bool {
        car.as_str() == "A" && self.publisher.get_subscription_count(&self.node) > 0
    }

    fn submit(&mut self, id: IntentId, intent: &Intent) -> Submission {
        if self.issued == Some(id) {
            return Submission::Submitted;
        }
        if !matches!(intent, Intent::ServeFloor { car, .. } if car.as_str() == "A")
            || !self.available(intent.car())
        {
            return Submission::NotSubmitted;
        }
        self.issued = Some(id);
        let holder = format!("{HOLDER_PREFIX}-{id:?}");
        self.holder = Some(holder.clone());
        let request = LiftRequest {
            lift_name: "Lift1".into(),
            request_time: Time::from_nanos(self.last_source_time),
            session_id: holder.clone(),
            request_type: 1, // REQUEST_AGV_MODE
            destination_floor: self.native_floor.clone(),
            door_state: 2, // DOOR_OPEN
        };
        println!("dispatch {id:?} holder={holder}");
        self.write(request)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    require_isolation()?;
    let case = std::env::var("LIFTASSISTANT_HOTEL_CASE").unwrap_or_else(|_| "happy".into());
    let (target, native_floor) = match case.as_str() {
        "happy" => (Floor::new(2), "L2"),
        "stale-same-floor" => (Floor::new(2), "L2"),
        "wrong-state" => (Floor::new(2), "L3"),
        "missed-completion" => (Floor::new(3), "L3"),
        "native-reject" => (Floor::new(2), "not-a-hotel-floor"),
        other => return Err(io::Error::other(format!("unknown Hotel case {other:?}")).into()),
    };
    let a = CarId::new("A");
    let b = CarId::new("B");
    let floors: BTreeMap<_, _> = [("L1", 1), ("L2", 2), ("L3", 3)]
        .map(|(name, floor)| (name.to_owned(), Floor::new(floor)))
        .into();
    let source = ObservationSourceId::new("official-hotel-probe");
    let session = ObservationSessionId::new("hotel-probe-reader");
    let projection = RmfInteractionModeProjection::conservative().with_agv_as_normal();
    let (mut mapper, mut single_lift) =
        observation_adapters(&source, &session, projection, &floors, &a, &b)?;
    let mut engine = Engine::new(BuildingTopology::new(floors.values().copied())?);
    for car in [&a, &b] {
        engine
            .bind_observation(
                source.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .map_err(|e| io::Error::other(format!("{e:?}")))?;
    }
    engine
        .begin_source_session(source.clone(), session)
        .map_err(|e| io::Error::other(format!("{e:?}")))?;
    let mut writer_generation = 0;
    let mut writer_created_at = Instant::now();
    let mut writer = HotelWriter::connect(native_floor, writer_generation)?;
    engine
        .bind_controlled_car(a.clone(), writer.controller_id().clone())
        .map_err(|e| io::Error::other(format!("{e:?}")))?;
    engine.set_execution_capabilities(
        a.clone(),
        ExecutionCapabilities::new(true, false, floors.values().copied()),
    );
    let grant = engine
        .grant_authority(&a, PrincipalId::new("hotel-probe-operator"))
        .map_err(|e| io::Error::other(format!("{e:?}")))?;

    let (tx, rx) = mpsc::sync_channel(64);
    let mut reader_generation = 0;
    start_reader(reader_generation, tx.clone());
    let deadline = Instant::now()
        + if case == "native-reject" {
            Duration::from_secs(25)
        } else {
            Duration::from_secs(70)
        };
    let mut request = engine.issue_controlled_intent(
        Intent::ServeFloor {
            car: a.clone(),
            floor: target,
        },
        None,
    );
    let mut id = request.id();
    let mut receipt_recorded = false;
    let mut source_times = BTreeMap::new();
    let mut last_a_update = None;
    let mut submitted = false;
    let mut stale_phase = 0_u8;
    let mut first_stale_intent = None;
    let mut pre_dispatch_restart_done = false;
    let mut post_dispatch_restart_done = false;
    let mut blackout_until = None;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            if case == "native-reject" {
                assert_eq!(writer.transport_writes, 1);
                assert_eq!(
                    engine.intent_record(id).unwrap().reconciliation,
                    Reconciliation::Pending
                );
                assert_eq!(engine.intent_record(id).unwrap().receipt, Receipt::Awaiting);
                engine.controlled_transport_lost(writer.controller_id());
                assert_eq!(
                    engine.intent_record(id).unwrap().receipt,
                    Receipt::Indeterminate
                );
                println!(
                    "external request had no acceptance or completion evidence; \
                     bounded adapter deadline left {id:?} indeterminate"
                );
                return Ok(());
            }
            eprintln!(
                "probe deadline: lifts={:?} A={:?} B={:?} writer_available={} submitted={submitted}",
                source_times.keys().collect::<Vec<_>>(),
                engine.operational_car_state(&a),
                engine.operational_car_state(&b),
                writer.available(&a)
            );
            return Err(mpsc::RecvTimeoutError::Timeout.into());
        }
        let sample = match rx.recv_timeout(remaining.min(Duration::from_secs(8))) {
            Ok(sample) => sample,
            Err(mpsc::RecvTimeoutError::Timeout) if reader_generation < 3 => {
                reader_generation += 1;
                start_reader(reader_generation, tx.clone());
                println!(
                    "subscriber generation {reader_generation} created after a payload-free window"
                );
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let stamp = sample.source_time_nanos;
        let state = &sample.state;
        if let Some(until) = blackout_until {
            if Instant::now() < until {
                continue;
            }
            blackout_until = None;
            println!("adapter data plane reconnected after the external trip window");
        }
        if case == "missed-completion"
            && !post_dispatch_restart_done
            && writer.issued.is_some()
            && state.lift_name == "Lift1"
            && writer.holder() == Some(state.session_id.as_str())
            && (state.current_floor != native_floor
                || state.motion_state != RmfLiftState::MOTION_STOPPED)
        {
            let restarted = ObservationSessionId::new("hotel-probe-post-dispatch-restart");
            engine
                .begin_source_session(source.clone(), restarted.clone())
                .map_err(|e| io::Error::other(format!("{e:?}")))?;
            (mapper, single_lift) =
                observation_adapters(&source, &restarted, projection, &floors, &a, &b)?;
            source_times.clear();
            post_dispatch_restart_done = true;
            blackout_until = Some(Instant::now() + Duration::from_secs(20));
            assert!(engine.operational_car_state(&a).is_none());
            println!(
                "adapter source session restarted after dispatch; \
                 observations disconnected while the official simulator continued"
            );
            continue;
        }
        if source_times
            .get(&state.lift_name)
            .is_some_and(|latest| stamp <= *latest)
        {
            continue;
        }
        if source_times
            .insert(state.lift_name.clone(), stamp)
            .is_none()
        {
            println!(
                "first payload in current source session: {} at {}",
                state.lift_name, state.current_floor
            );
        }
        if state.lift_name == "Lift2" {
            assert!(matches!(
                single_lift.map(state),
                Err(RmfAdapterError::UnconfiguredLift { .. })
            ));
        }
        let mapped = mapper.map(state)?;
        let mapped_car = mapped.update.car.clone();
        if mapped.update.car == a {
            last_a_update = Some(mapped.update.clone());
        }
        if submitted && writer.issued.is_some() && mapped.update.car == a {
            // Only the unique holder for the current test command supplies execution evidence.
            if writer.holder() != Some(state.session_id.as_str()) {
                continue;
            }
            if !receipt_recorded
                && state.destination_floor == native_floor
                && (state.current_floor != native_floor || case == "stale-same-floor")
            {
                assert_eq!(
                    engine.intent_record(id).unwrap().reconciliation,
                    Reconciliation::Pending
                );
                engine.receive_intent_receipt(IntentReceipt {
                    controller: writer.controller_id().clone(),
                    intent: id,
                    status: ReceiptStatus::Accepted,
                });
                assert_eq!(
                    engine.intent_record(id).unwrap().reconciliation,
                    Reconciliation::Pending
                );
                receipt_recorded = true;
                println!(
                    "holder confirmed {id:?} source_time={stamp} current={} motion={}",
                    state.current_floor, state.motion_state
                );
                if case == "happy" && stale_phase == 0 {
                    assert_eq!(writer.retry_transport(), Submission::Submitted);
                }
            }
        }
        engine.ingest_observation_update(mapped.update);
        if !submitted
            && !writer.available(&a)
            && writer_generation < 3
            && writer_created_at.elapsed() >= Duration::from_secs(8)
            && engine.operational_car_state(&a).is_some()
            && engine.operational_car_state(&b).is_some()
        {
            writer_generation += 1;
            writer = HotelWriter::connect(native_floor, writer_generation)?;
            writer_created_at = Instant::now();
            println!(
                "writer generation {writer_generation} recreated after a discovery-only window"
            );
        }
        let required_start = if case == "stale-same-floor" && stale_phase == 1 {
            Floor::new(2)
        } else {
            Floor::new(1)
        };
        if !submitted
            && writer.available(&a)
            && engine.operational_car_state(&a).is_some_and(|state| {
                state.location.floor_reference() == required_start
                    && (case != "stale-same-floor"
                        || stale_phase == 0
                        || state.door == DoorState::Closed)
            })
            && engine.operational_car_state(&b).is_some()
        {
            assert_eq!(source_times.len(), 2);
            if !pre_dispatch_restart_done {
                let restarted = ObservationSessionId::new("hotel-probe-pre-dispatch-restart");
                engine
                    .begin_source_session(source.clone(), restarted.clone())
                    .map_err(|e| io::Error::other(format!("{e:?}")))?;
                (mapper, single_lift) =
                    observation_adapters(&source, &restarted, projection, &floors, &a, &b)?;
                source_times.clear();
                pre_dispatch_restart_done = true;
                assert!(engine.operational_car_state(&a).is_none());
                assert!(engine.operational_car_state(&b).is_none());
                println!("pre-dispatch adapter source session restart revoked old observations");
                continue;
            }
            println!("explicit bindings Lift1=A Lift2=B, Lift2 rejected by A-only mapping");
            writer.last_source_time = source_times["Lift1"];
            assert_eq!(
                engine.submit_controlled(request.clone(), &grant, &mut writer),
                Admission::Admitted
            );
            submitted = true;
            assert_eq!(engine.intent_record(id).unwrap().receipt, Receipt::Awaiting);
            for _ in 0..10 {
                assert_eq!(
                    engine.submit_controlled(request.clone(), &grant, &mut writer),
                    Admission::Retry
                );
            }
        }
        if mapped_car != a {
            continue;
        }
        let outcome = engine.intent_record(id).map(|record| record.reconciliation);
        if outcome == Some(Reconciliation::Satisfied) {
            assert!(
                receipt_recorded || case == "missed-completion",
                "no independent pre-arrival evidence of RMF acceptance"
            );
            assert_ne!(
                case, "wrong-state",
                "wrong external floor satisfied the intent"
            );
            assert_eq!(state.current_floor, native_floor);
            assert_eq!(state.motion_state, RmfLiftState::MOTION_STOPPED);
            if case == "missed-completion" {
                assert!(post_dispatch_restart_done);
            }
            if case == "stale-same-floor" && stale_phase == 0 {
                if state.door_state != RmfLiftState::DOOR_OPEN {
                    continue;
                }
                let first = id;
                assert_eq!(writer.end_current_session(stamp), Submission::Submitted);
                let stale = last_a_update
                    .clone()
                    .expect("the first completion supplied an A observation");
                let restarted = ObservationSessionId::new("hotel-probe-before-same-target-intent");
                engine
                    .begin_source_session(source.clone(), restarted.clone())
                    .map_err(|e| io::Error::other(format!("{e:?}")))?;
                engine.ingest_observation_update(stale);
                assert!(engine.operational_car_state(&a).is_none());
                (mapper, single_lift) =
                    observation_adapters(&source, &restarted, projection, &floors, &a, &b)?;
                source_times.clear();
                first_stale_intent = Some(first);
                stale_phase = 1;
                request = engine.issue_controlled_intent(
                    Intent::ServeFloor {
                        car: a.clone(),
                        floor: target,
                    },
                    None,
                );
                id = request.id();
                assert_ne!(id, first);
                receipt_recorded = false;
                submitted = false;
                println!(
                    "first {first:?} reached L2; stale old-session completion was fenced before new {id:?}"
                );
                continue;
            }
            println!(
                "external physical arrival {id:?} source_time={stamp} door={} transport_writes={}",
                state.door_state, writer.transport_writes
            );
            if case == "stale-same-floor" {
                assert_eq!(stale_phase, 1);
                assert!(first_stale_intent.is_some_and(|first| first != id));
            }
            return Ok(());
        }
        if let Some(Reconciliation::Diverged { observed }) = outcome {
            assert_eq!(case, "wrong-state");
            assert_eq!(observed, Floor::new(3));
            println!(
                "external divergence {id:?} desired={target} observed={observed} source_time={stamp}"
            );
            return Ok(());
        }
    }
}
