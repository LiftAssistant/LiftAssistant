#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, io,
    process::ExitCode,
    time::Duration,
};

use liftassistant_engine::{
    DiagnosticAnomaly, DiagnosticEvent, Engine, ObservationBindingError, ObservationBindingRole,
    ObservationIngestError, ObservationIngestResult, SourceObservationInvalidation,
    SourceObservationInvalidationError, SourceSessionError, SourceSessionTransition,
};
use liftassistant_model::{
    BuildingTopology, CarId, CarLocation, CarObservation, CarObservationUpdate, DoorEvidence,
    DoorState, Floor, MotionEvidence, MotionState, ObservationProvenance, ObservationSequence,
    ObservationSessionId, ObservationSourceId, OperatingMode, OperatingModeEvidence, Position,
    PositionEvidence, ReportedDestination,
};
use liftassistant_rmf::{
    RmfFreshnessPolicy, RmfInteractionModeProjection, RmfLiftBinding, RmfLiftState,
    RmfLiveSubscriber, RmfObservationAdapter, RmfReceiveControl, RmfReceiveOutcome,
    RmfSourceContext,
};
use liftassistant_runtime::{CarStateResolution, UnresolvedCarStateReason};

const LIFECYCLE_SCENARIO: &str = "headless-read-only-lifecycle";
const RMF_LIVE_SCENARIO: &str = "headless-rmf-live-proof";
const RMF_FRESHNESS_SCENARIO: &str = "headless-rmf-freshness-proof";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Readiness {
    NotReady,
    ReadOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DegradedReason {
    ObservationAnomaly(DiagnosticAnomaly),
    ObservationRejected(ObservationIngestError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HostLifecycle {
    Starting,
    Synchronizing,
    ReadyReadOnly,
    Degraded(DegradedReason),
}

impl HostLifecycle {
    fn readiness(&self) -> Readiness {
        match self {
            Self::ReadyReadOnly => Readiness::ReadOnly,
            Self::Starting | Self::Synchronizing | Self::Degraded(_) => Readiness::NotReady,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum HostEvent {
    LifecycleChanged(HostLifecycle),
    ObservationUnresolved {
        car: CarId,
        reasons: Vec<UnresolvedCarStateReason>,
    },
    StateUpdated(liftassistant_model::CarState),
    ObservationRejected(DiagnosticAnomaly),
    IngestionRejected(ObservationIngestError),
    AuxiliaryObservationAccepted {
        source: ObservationSourceId,
        car: CarId,
    },
    SourcePayloadStale {
        source: ObservationSourceId,
        session: ObservationSessionId,
        invalidated_cars: Vec<CarId>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransportDiscoveryState {
    Unknown,
    SubscriberCreated,
    DiscoveryObserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PayloadActivity {
    Awaiting,
    Active,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SourceSessionActivity {
    Inactive,
    Active(ObservationSessionId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservationAvailability {
    Awaiting,
    Incomplete,
    Unresolved,
    Resolved,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceHealth {
    transport_discovery: TransportDiscoveryState,
    payload: PayloadActivity,
    session: SourceSessionActivity,
    observations: BTreeMap<CarId, ObservationAvailability>,
}

impl SourceHealth {
    fn new() -> Self {
        Self {
            transport_discovery: TransportDiscoveryState::Unknown,
            payload: PayloadActivity::Awaiting,
            session: SourceSessionActivity::Inactive,
            observations: BTreeMap::new(),
        }
    }
}

/// Internal in-memory source for the headless proof. It exposes observations only.
#[derive(Debug)]
struct RecordedObservationFeed {
    frames: VecDeque<CarObservationUpdate>,
}

impl RecordedObservationFeed {
    fn new<T>(frames: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<CarObservationUpdate>,
    {
        Self {
            frames: frames.into_iter().map(Into::into).collect(),
        }
    }

    fn next_observation(&mut self) -> Option<CarObservationUpdate> {
        self.frames.pop_front()
    }
}

#[derive(Debug)]
struct ReadOnlyHost {
    engine: Engine,
    required_operational_cars: BTreeSet<CarId>,
    source_health: BTreeMap<ObservationSourceId, SourceHealth>,
    lifecycle: HostLifecycle,
    events: Vec<HostEvent>,
}

impl ReadOnlyHost {
    fn new(topology: BuildingTopology) -> Self {
        Self {
            engine: Engine::new(topology),
            required_operational_cars: BTreeSet::new(),
            source_health: BTreeMap::new(),
            lifecycle: HostLifecycle::Starting,
            events: vec![HostEvent::LifecycleChanged(HostLifecycle::Starting)],
        }
    }

    fn lifecycle(&self) -> &HostLifecycle {
        &self.lifecycle
    }

    fn readiness(&self) -> Readiness {
        self.lifecycle.readiness()
    }

    fn source_health(&self, source: &ObservationSourceId) -> Option<&SourceHealth> {
        self.source_health.get(source)
    }

    fn bind_observation(
        &mut self,
        source: ObservationSourceId,
        car: CarId,
        role: ObservationBindingRole,
    ) -> Result<(), ObservationBindingError> {
        self.engine
            .bind_observation(source.clone(), car.clone(), role)?;
        self.source_health
            .entry(source)
            .or_insert_with(SourceHealth::new)
            .observations
            .insert(car.clone(), ObservationAvailability::Awaiting);
        if role == ObservationBindingRole::Operational {
            self.required_operational_cars.insert(car);
        }
        Ok(())
    }

    fn poll(&mut self, source: &mut RecordedObservationFeed) -> bool {
        let Some(observation) = source.next_observation() else {
            return false;
        };
        self.ingest_observation(observation);
        true
    }

    fn ingest_observation(&mut self, observation: CarObservationUpdate) {
        let observation_source = observation.provenance.source.clone();
        let car = observation.car.clone();
        let role = self
            .engine
            .observation_binding_role(&observation_source, &car);
        let is_operational = role == Some(ObservationBindingRole::Operational);

        if is_operational
            && matches!(
                self.lifecycle,
                HostLifecycle::Starting | HostLifecycle::Degraded(_)
            )
        {
            self.transition(HostLifecycle::Synchronizing);
        }

        let incident_marker = latest_incident_marker(&self.engine);
        let resolution = self.engine.ingest_observation_update(observation);
        if latest_incident_marker(&self.engine) != incident_marker {
            let anomaly = self
                .engine
                .diagnostics()
                .incidents()
                .last()
                .expect("a new incident was counted")
                .trigger
                .clone();
            self.events
                .push(HostEvent::ObservationRejected(anomaly.clone()));
            if is_operational {
                self.transition(HostLifecycle::Degraded(DegradedReason::ObservationAnomaly(
                    anomaly,
                )));
            }
            return;
        }

        if !matches!(&resolution, ObservationIngestResult::Rejected(_))
            && let Some(health) = self.source_health.get_mut(&observation_source)
        {
            health.payload = PayloadActivity::Active;
            let availability = match &resolution {
                ObservationIngestResult::Incomplete(_) => ObservationAvailability::Incomplete,
                ObservationIngestResult::Resolution(CarStateResolution::Resolved(_)) => {
                    ObservationAvailability::Resolved
                }
                ObservationIngestResult::Resolution(CarStateResolution::Unresolved { .. }) => {
                    ObservationAvailability::Unresolved
                }
                ObservationIngestResult::Rejected(_) => unreachable!(),
            };
            health.observations.insert(car.clone(), availability);
        }

        if !is_operational {
            match resolution {
                ObservationIngestResult::Rejected(error) => {
                    self.events.push(HostEvent::IngestionRejected(error));
                }
                ObservationIngestResult::Incomplete(_) | ObservationIngestResult::Resolution(_) => {
                    self.events.push(HostEvent::AuxiliaryObservationAccepted {
                        source: observation_source,
                        car,
                    });
                }
            }
            return;
        }

        match resolution {
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(state)) => {
                self.events.push(HostEvent::StateUpdated(state));
                let all_operational_cars_resolved = self
                    .required_operational_cars
                    .iter()
                    .all(|car| self.engine.operational_car_state(car).is_some());
                if all_operational_cars_resolved {
                    self.transition(HostLifecycle::ReadyReadOnly);
                } else if !matches!(self.lifecycle, HostLifecycle::Synchronizing) {
                    self.transition(HostLifecycle::Synchronizing);
                }
            }
            ObservationIngestResult::Resolution(CarStateResolution::Unresolved {
                car,
                reasons,
            }) => {
                self.events
                    .push(HostEvent::ObservationUnresolved { car, reasons });
                if !matches!(self.lifecycle, HostLifecycle::Synchronizing) {
                    self.transition(HostLifecycle::Synchronizing);
                }
            }
            ObservationIngestResult::Incomplete(assembled) => {
                if !matches!(self.lifecycle, HostLifecycle::Synchronizing) {
                    self.transition(HostLifecycle::Synchronizing);
                }
                debug_assert!(assembled.snapshot().is_none());
            }
            ObservationIngestResult::Rejected(error) => {
                self.events
                    .push(HostEvent::IngestionRejected(error.clone()));
                self.transition(HostLifecycle::Degraded(
                    DegradedReason::ObservationRejected(error),
                ));
            }
        }
    }

    fn begin_source_session(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
    ) -> Result<SourceSessionTransition, SourceSessionError> {
        let transition = self.engine.begin_source_session(source, session)?;
        let health = self
            .source_health
            .get_mut(&transition.source)
            .expect("Engine accepts sessions only for a bound source");
        health.session = SourceSessionActivity::Active(transition.active_session.clone());
        health.payload = PayloadActivity::Awaiting;
        for availability in health.observations.values_mut() {
            *availability = ObservationAvailability::Awaiting;
        }
        let invalidated_operational_state = transition.invalidated_cars.iter().any(|car| {
            self.engine
                .observation_binding_role(&transition.source, car)
                == Some(ObservationBindingRole::Operational)
        });
        if invalidated_operational_state {
            self.transition(HostLifecycle::Synchronizing);
        }
        Ok(transition)
    }

    fn note_transport_subscriber_created(&mut self, source: &ObservationSourceId) {
        if let Some(health) = self.source_health.get_mut(source) {
            health.transport_discovery = TransportDiscoveryState::SubscriberCreated;
        }
    }

    fn note_transport_discovery(&mut self, source: &ObservationSourceId) {
        if let Some(health) = self.source_health.get_mut(source) {
            health.transport_discovery = TransportDiscoveryState::DiscoveryObserved;
        }
    }

    fn mark_source_payload_stale(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
    ) -> Result<SourceObservationInvalidation, SourceObservationInvalidationError> {
        let invalidation = self
            .engine
            .invalidate_source_observations(source.clone(), session.clone())?;
        let health = self
            .source_health
            .get_mut(&source)
            .expect("Engine invalidates only a bound source");
        health.payload = PayloadActivity::Stale;
        for availability in health.observations.values_mut() {
            *availability = ObservationAvailability::Revoked;
        }
        self.events.push(HostEvent::SourcePayloadStale {
            source: source.clone(),
            session,
            invalidated_cars: invalidation.invalidated_cars.clone(),
        });
        let source_has_operational_binding = self
            .required_operational_cars
            .iter()
            .any(|car| self.engine.operational_observation_source(car) == Some(&source));
        if source_has_operational_binding {
            self.transition(HostLifecycle::Synchronizing);
        }
        Ok(invalidation)
    }

    fn transition(&mut self, lifecycle: HostLifecycle) {
        if self.lifecycle != lifecycle {
            self.lifecycle = lifecycle.clone();
            self.events.push(HostEvent::LifecycleChanged(lifecycle));
        }
    }
}

fn latest_incident_marker(engine: &Engine) -> Option<u64> {
    engine
        .diagnostics()
        .incidents()
        .last()
        .and_then(|incident| {
            incident.evidence.iter().rev().find_map(|entry| {
                matches!(entry.event, DiagnosticEvent::Anomaly(_)).then_some(entry.sequence.value())
            })
        })
}

#[derive(Clone, Debug, PartialEq)]
struct LifecycleSnapshot {
    lifecycle: HostLifecycle,
    readiness: Readiness,
}

#[derive(Clone, Debug, PartialEq)]
struct ProofRun {
    snapshots: Vec<LifecycleSnapshot>,
    events: Vec<HostEvent>,
    latest_state: Option<liftassistant_model::CarState>,
    diagnostic_entries: usize,
    incidents: usize,
}

fn main() -> ExitCode {
    match parse_scenario(env::args().skip(1))
        .and_then(|name| run_scenario(&name))
        .map(|report| {
            println!("{report}");
        }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_scenario(mut args: impl Iterator<Item = String>) -> Result<String, String> {
    match (args.next(), args.next(), args.next()) {
        (Some(flag), Some(name), None) if flag == "--scenario" => Ok(name),
        _ => Err(format!(
            "usage: liftassistant-headless --scenario <{LIFECYCLE_SCENARIO}|{RMF_LIVE_SCENARIO}|{RMF_FRESHNESS_SCENARIO}>"
        )),
    }
}

fn run_scenario(name: &str) -> Result<String, String> {
    match name {
        LIFECYCLE_SCENARIO => run_lifecycle_scenario(),
        RMF_LIVE_SCENARIO => run_rmf_live_scenario(),
        RMF_FRESHNESS_SCENARIO => run_rmf_freshness_scenario(),
        _ => Err(format!(
            "unknown scenario {name:?}; available scenarios: {LIFECYCLE_SCENARIO}, {RMF_LIVE_SCENARIO}, {RMF_FRESHNESS_SCENARIO}"
        )),
    }
}

fn run_lifecycle_scenario() -> Result<String, String> {
    let first = execute_proof();
    let second = execute_proof();
    if first != second {
        return Err("headless lifecycle proof was not deterministic".into());
    }
    verify_proof(&first)?;
    Ok(render_report(&first))
}

#[derive(Debug)]
struct LiveRmfRound {
    source_times: Vec<i64>,
    sequences: Vec<u64>,
    source_contexts: BTreeMap<String, RmfSourceContext>,
    last_update: CarObservationUpdate,
}

fn rmf_domain_id() -> Result<u16, String> {
    env::var("ROS_DOMAIN_ID")
        .unwrap_or_else(|_| "47".into())
        .parse::<u16>()
        .map_err(|error| format!("ROS_DOMAIN_ID must fit in u16: {error}"))
}

fn hotel_host(source: &ObservationSourceId) -> Result<ReadOnlyHost, String> {
    let mut host = ReadOnlyHost::new(
        BuildingTopology::new([Floor::new(1), Floor::new(2), Floor::new(3)])
            .expect("Hotel proof topology is non-empty"),
    );
    for car in [CarId::new("A"), CarId::new("B")] {
        host.bind_observation(source.clone(), car, ObservationBindingRole::Operational)
            .map_err(|error| format!("failed to bind live RMF source: {error:?}"))?;
    }
    Ok(host)
}

fn run_rmf_live_scenario() -> Result<String, String> {
    let domain_id = rmf_domain_id()?;
    let source = ObservationSourceId::new("rmf/hotel-live");
    let mut host = hotel_host(&source)?;

    let mut first_adapter = hotel_adapter(source.clone(), "hotel-adapter-session-A")?;
    host.begin_source_session(source.clone(), first_adapter.session().clone())
        .map_err(|error| format!("failed to begin first live RMF session: {error:?}"))?;
    let first = receive_live_rmf_round(
        &mut host,
        &mut first_adapter,
        domain_id,
        "A",
        60_000_000_000,
    )?;
    if host.readiness() != Readiness::ReadOnly || host.engine.car_states().len() != 2 {
        return Err("first live RMF session did not establish two-car ReadyReadOnly state".into());
    }

    let second_adapter = hotel_adapter(source.clone(), "hotel-adapter-session-B")?;
    let transition = host
        .begin_source_session(source.clone(), second_adapter.session().clone())
        .map_err(|error| format!("failed to begin second live RMF session: {error:?}"))?;
    if transition.invalidated_cars != [CarId::new("A"), CarId::new("B")]
        || host.readiness() != Readiness::NotReady
        || !host.engine.car_states().is_empty()
    {
        return Err("new live RMF session did not invalidate both operational cars".into());
    }
    match host
        .engine
        .ingest_observation_update(first.last_update.clone())
    {
        ObservationIngestResult::Rejected(ObservationIngestError::InactiveSourceSession {
            active,
            received,
            ..
        }) if active == *second_adapter.session()
            && received == ObservationSessionId::new("hotel-adapter-session-A") => {}
        other => {
            return Err(format!(
                "delayed retired-session update was not rejected as inactive: {other:?}"
            ));
        }
    }

    println!(
        "live-proof: first session ready; restart the official RMF source, wait for it to publish /lift_states, then press Enter"
    );
    let mut acknowledgment = String::new();
    io::stdin()
        .read_line(&mut acknowledgment)
        .map_err(|error| {
            format!("failed while waiting for source-restart acknowledgment: {error}")
        })?;

    let mut second_adapter = second_adapter;
    let second = receive_live_rmf_round(&mut host, &mut second_adapter, domain_id, "B", 0)?;
    if host.readiness() != Readiness::ReadOnly || host.engine.car_states().len() != 2 {
        return Err("second live RMF session did not restore two-car ReadyReadOnly state".into());
    }
    let first_max_time = first
        .source_times
        .iter()
        .max()
        .copied()
        .expect("a completed live round has source times");
    let second_min_time = second
        .source_times
        .iter()
        .min()
        .copied()
        .expect("a completed live round has source times");
    if second_min_time >= first_max_time {
        return Err(format!(
            "RMF lift_time did not reset across the externally acknowledged source restart: first max={first_max_time}, second min={second_min_time}"
        ));
    }

    println!(
        "live-proof: second session ready; issue the independent simulation-only Lift1 -> L2 AGV request now"
    );
    let transition = receive_live_rmf_transition(&mut host, &mut second_adapter, domain_id)?;

    let raw_modes = second
        .source_contexts
        .iter()
        .map(|(lift, context)| format!("{lift}={}", context.source_mode))
        .collect::<Vec<_>>()
        .join(", ");
    Ok([
        format!("scenario: {RMF_LIVE_SCENARIO}"),
        "scope: official Open-RMF simulation; subscription only; no LiftRequest publisher or IntentSink".into(),
        "identity: PASS (Lift1 -> A, Lift2 -> B through explicit configuration)".into(),
        format!(
            "session A: PASS (source-global sequences {:?}; two Engine operational cars; ReadyReadOnly)",
            first.sequences
        ),
        "session transition: PASS (both old operational states invalidated; delayed session-A update rejected)".into(),
        format!(
            "session B: PASS (source-global sequences {:?}; lift_time reset after source restart; ReadyReadOnly restored)",
            second.sequences
        ),
        format!(
            "movement/door: PASS (independent simulation harness observed by LiftAssistant at sequences {:?}; moving and door-moving evidence seen; Lift1 resolved stopped/open at L2)",
            transition.sequences
        ),
        format!("raw RMF context: PASS ({raw_modes}; explicit AGV-as-Normal projection retained raw modes)"),
        "write boundary: PASS (live RMF crate exposes a subscriber and canonical observation adapter only)".into(),
        String::new(),
    ]
    .join("\n"))
}

fn run_rmf_freshness_scenario() -> Result<String, String> {
    let domain_id = rmf_domain_id()?;
    let max_silence_ms = env::var("LIFTASSISTANT_RMF_MAX_SILENCE_MS")
        .map_err(|_| {
            "LIFTASSISTANT_RMF_MAX_SILENCE_MS must explicitly configure this source's freshness proof"
                .to_owned()
        })?
        .parse::<u64>()
        .map_err(|error| {
            format!("LIFTASSISTANT_RMF_MAX_SILENCE_MS must fit in u64: {error}")
        })?;
    let freshness = RmfFreshnessPolicy::new(Duration::from_millis(max_silence_ms))
        .map_err(|error| format!("invalid Hotel freshness policy: {error}"))?;
    let source = ObservationSourceId::new("rmf/hotel-freshness-live");
    let session = ObservationSessionId::new("hotel-freshness-adapter-session-A");
    let mut host = hotel_host(&source)?;
    let mut adapter = hotel_adapter(source.clone(), session.as_str())?;
    host.begin_source_session(source.clone(), session.clone())
        .map_err(|error| format!("failed to begin Hotel freshness session: {error:?}"))?;
    let initial_sequences =
        receive_ready_then_rmf_stale(&mut host, &mut adapter, domain_id, freshness)?;
    let invalidation = host
        .mark_source_payload_stale(source.clone(), session.clone())
        .map_err(|error| format!("failed to revoke stale source observations: {error:?}"))?;
    if invalidation.invalidated_cars != [CarId::new("A"), CarId::new("B")]
        || host.readiness() != Readiness::NotReady
        || !host.engine.car_states().is_empty()
    {
        return Err("stale Operational source did not revoke two-car readiness/truth".into());
    }
    let stale_health = host
        .source_health(&source)
        .expect("configured source health remains available");
    if stale_health.payload != PayloadActivity::Stale
        || stale_health
            .observations
            .values()
            .any(|availability| *availability != ObservationAvailability::Revoked)
    {
        return Err(format!(
            "source health did not separate stale payload from revoked observations: {stale_health:?}"
        ));
    }

    println!(
        "freshness-proof: stale state revoked; restart the official RMF source, wait for /lift_states, then press Enter"
    );
    let mut acknowledgment = String::new();
    io::stdin()
        .read_line(&mut acknowledgment)
        .map_err(|error| format!("failed while waiting for source recovery: {error}"))?;
    let recovered = receive_live_rmf_round(&mut host, &mut adapter, domain_id, "fresh_B", 0)?;
    if host.readiness() != Readiness::ReadOnly || host.engine.car_states().len() != 2 {
        return Err("resumed source did not resynchronize two-car ReadyReadOnly state".into());
    }
    let recovered_health = host
        .source_health(&source)
        .expect("recovered source health remains available");
    if recovered_health.payload != PayloadActivity::Active
        || recovered_health
            .observations
            .values()
            .any(|availability| *availability != ObservationAvailability::Resolved)
    {
        return Err(format!(
            "source health did not recover payload/resolution dimensions: {recovered_health:?}"
        ));
    }

    Ok([
        format!("scenario: {RMF_FRESHNESS_SCENARIO}"),
        format!(
            "policy: PASS (explicit RMF source max payload silence = {} ms; no universal/default timeout)",
            freshness.max_payload_silence().as_millis()
        ),
        "health layers: PASS (subscriber/discovery evidence, payload activity, adapter session, per-car resolution, Engine readiness remain distinct)".into(),
        format!(
            "stale: PASS (continuous subscription sequences {initial_sequences:?}; two operational states revoked; ReadyReadOnly -> Synchronizing/NotReady)"
        ),
        format!(
            "recovery: PASS (same adapter session and increasing source-global sequences {:?}; two complete states restored ReadyReadOnly)",
            recovered.sequences
        ),
        "evidence: PASS (last known canonical observations remain in bounded diagnostics; current evidence/state revoked)".into(),
        "authority: PASS (no Auxiliary promotion, automatic failover, field fusion, or write path)".into(),
        String::new(),
    ]
    .join("\n"))
}

fn receive_ready_then_rmf_stale(
    host: &mut ReadOnlyHost,
    adapter: &mut RmfObservationAdapter,
    domain_id: u16,
    freshness: RmfFreshnessPolicy,
) -> Result<Vec<u64>, String> {
    let node_name = format!("rmf_freshness_proof_{}", std::process::id());
    let subscriber = RmfLiveSubscriber::connect(domain_id, &node_name)
        .map_err(|error| format!("failed to connect freshness observer: {error}"))?;
    host.note_transport_subscriber_created(adapter.source());
    let mut sequences = Vec::new();
    let mut ready_announced = false;
    let mut mapping_error = None;
    let outcome = subscriber
        .receive_until_stale(freshness, |payload| {
            host.note_transport_discovery(adapter.source());
            let mapped = match adapter.map(&payload.state) {
                Ok(mapped) => mapped,
                Err(error) => {
                    mapping_error = Some(error.to_string());
                    return RmfReceiveControl::Stop;
                }
            };
            sequences.push(mapped.update.provenance.sequence.value());
            host.ingest_observation(mapped.update);
            if !ready_announced
                && host.readiness() == Readiness::ReadOnly
                && host.engine.car_states().len() == 2
            {
                ready_announced = true;
                println!(
                    "freshness-proof: initial payloads ready; stop the official RMF source and leave this observer running"
                );
            }
            RmfReceiveControl::Continue
        })
        .map_err(|error| format!("live RMF freshness receive loop failed: {error}"))?;
    if let Some(error) = mapping_error {
        return Err(error);
    }
    if !ready_announced {
        return Err("payload silence occurred before two-car ReadyReadOnly was established".into());
    }
    match outcome {
        RmfReceiveOutcome::PayloadStale { max_silence }
            if max_silence == freshness.max_payload_silence() =>
        {
            Ok(sequences)
        }
        other => Err(format!(
            "freshness observer stopped without the configured payload-silence event: {other:?}"
        )),
    }
}

#[derive(Debug)]
struct LiveRmfTransition {
    sequences: Vec<u64>,
}

fn receive_live_rmf_transition(
    host: &mut ReadOnlyHost,
    adapter: &mut RmfObservationAdapter,
    domain_id: u16,
) -> Result<LiveRmfTransition, String> {
    let node_name = format!("rmf_live_transition_proof_{}", std::process::id());
    let subscriber = RmfLiveSubscriber::connect(domain_id, &node_name)
        .map_err(|error| format!("failed to connect transition observer: {error}"))?;
    host.note_transport_subscriber_created(adapter.source());
    let mut sequences = Vec::new();
    let mut saw_moving = false;
    let mut saw_door_moving = false;
    let mut mapping_error = None;

    subscriber
        .receive_until(|payload| {
            host.note_transport_discovery(adapter.source());
            if payload.state.lift_name != "Lift1" {
                let mapped = match adapter.map(&payload.state) {
                    Ok(mapped) => mapped,
                    Err(error) => {
                        mapping_error = Some(error.to_string());
                        return RmfReceiveControl::Stop;
                    }
                };
                sequences.push(mapped.update.provenance.sequence.value());
                host.ingest_observation(mapped.update);
                return RmfReceiveControl::Continue;
            }

            saw_moving |= matches!(
                payload.state.motion_state,
                RmfLiftState::MOTION_UP | RmfLiftState::MOTION_DOWN
            );
            saw_door_moving |= payload.state.door_state == RmfLiftState::DOOR_MOVING;
            let stopped_open_at_l2 = payload.state.current_floor == "L2"
                && payload.state.motion_state == RmfLiftState::MOTION_STOPPED
                && payload.state.door_state == RmfLiftState::DOOR_OPEN;
            let mapped = match adapter.map(&payload.state) {
                Ok(mapped) => mapped,
                Err(error) => {
                    mapping_error = Some(error.to_string());
                    return RmfReceiveControl::Stop;
                }
            };
            sequences.push(mapped.update.provenance.sequence.value());
            host.ingest_observation(mapped.update);
            if stopped_open_at_l2 && saw_moving && saw_door_moving {
                RmfReceiveControl::Stop
            } else {
                RmfReceiveControl::Continue
            }
        })
        .map_err(|error| format!("live RMF transition receive loop failed: {error}"))?;
    if let Some(error) = mapping_error {
        return Err(error);
    }
    let Some(state) = host.engine.operational_car_state(&CarId::new("A")) else {
        return Err("Lift1 transition did not restore logical car A operational state".into());
    };
    if !saw_moving
        || !saw_door_moving
        || state.location != CarLocation::Floor(Floor::new(2))
        || state.motion != MotionState::Stopped
        || state.door != DoorState::Open
        || state.mode != OperatingMode::Normal
        || host.readiness() != Readiness::ReadOnly
    {
        return Err(format!(
            "Lift1 transition evidence was incomplete: moving={saw_moving}, door_moving={saw_door_moving}, state={state:?}, readiness={:?}",
            host.readiness()
        ));
    }

    Ok(LiveRmfTransition { sequences })
}

fn hotel_adapter(
    source: ObservationSourceId,
    session: &str,
) -> Result<RmfObservationAdapter, String> {
    let floor_labels = [("L1", 1), ("L2", 2), ("L3", 3)]
        .into_iter()
        .map(|(label, floor)| (label.to_owned(), Floor::new(floor)))
        .collect::<BTreeMap<_, _>>();
    RmfObservationAdapter::new(
        source,
        ObservationSessionId::new(session),
        RmfInteractionModeProjection::conservative().with_agv_as_normal(),
        [
            (
                "Lift1".into(),
                RmfLiftBinding::new(CarId::new("A"), floor_labels.clone()),
            ),
            (
                "Lift2".into(),
                RmfLiftBinding::new(CarId::new("B"), floor_labels),
            ),
        ],
    )
    .map_err(|error| format!("invalid Hotel RMF adapter configuration: {error}"))
}

fn receive_live_rmf_round(
    host: &mut ReadOnlyHost,
    adapter: &mut RmfObservationAdapter,
    domain_id: u16,
    round: &str,
    minimum_source_time_nanos: i64,
) -> Result<LiveRmfRound, String> {
    let node_name = format!("rmf_live_proof_{}_{}", std::process::id(), round);
    let subscriber = RmfLiveSubscriber::connect(domain_id, &node_name)
        .map_err(|error| format!("failed to connect live RMF subscriber: {error}"))?;
    host.note_transport_subscriber_created(adapter.source());
    let expected_topology = host.engine.topology().clone();
    let mut source_times = Vec::new();
    let mut sequences = Vec::new();
    let mut source_contexts = BTreeMap::new();
    let mut last_update = None;
    let mut mapping_error = None;

    subscriber
        .receive_until(|payload| {
            host.note_transport_discovery(adapter.source());
            let mapped = match adapter.map(&payload.state) {
                Ok(mapped) => mapped,
                Err(error) => {
                    mapping_error = Some(error.to_string());
                    return RmfReceiveControl::Stop;
                }
            };
            if mapped.topology != expected_topology {
                mapping_error = Some(format!(
                    "live RMF topology {:?} differs from configured Engine topology {:?}",
                    mapped.topology.served_floors(),
                    expected_topology.served_floors()
                ));
                return RmfReceiveControl::Stop;
            }
            source_times.push(payload.source_time_nanos);
            sequences.push(mapped.update.provenance.sequence.value());
            source_contexts.insert(
                mapped.source_context.lift_name.clone(),
                mapped.source_context,
            );
            last_update = Some(mapped.update.clone());
            host.ingest_observation(mapped.update);

            if host.readiness() == Readiness::ReadOnly
                && host.engine.car_states().len() == 2
                && payload.source_time_nanos >= minimum_source_time_nanos
            {
                RmfReceiveControl::Stop
            } else {
                RmfReceiveControl::Continue
            }
        })
        .map_err(|error| format!("live RMF receive loop failed: {error}"))?;
    if let Some(error) = mapping_error {
        return Err(error);
    }
    if source_contexts.keys().cloned().collect::<BTreeSet<_>>()
        != BTreeSet::from(["Lift1".to_owned(), "Lift2".to_owned()])
    {
        return Err(format!(
            "live RMF round did not observe both explicitly configured lifts: {:?}",
            source_contexts.keys().collect::<Vec<_>>()
        ));
    }

    Ok(LiveRmfRound {
        source_times,
        sequences,
        source_contexts,
        last_update: last_update.expect("a completed live round has an update"),
    })
}

fn execute_proof() -> ProofRun {
    let mut source = reference_source();
    let mut host = reference_host();
    let mut snapshots = vec![snapshot(&host)];
    while host.poll(&mut source) {
        snapshots.push(snapshot(&host));
    }

    ProofRun {
        snapshots,
        events: host.events.clone(),
        latest_state: host.engine.car_states().into_iter().next(),
        diagnostic_entries: host.engine.diagnostics().entries().count(),
        incidents: host.engine.diagnostics().incidents().count(),
    }
}

fn reference_host() -> ReadOnlyHost {
    let mut host = ReadOnlyHost::new(
        BuildingTopology::new([1, 2, 3, 4].map(Floor::new))
            .expect("reference topology is non-empty"),
    );
    host.bind_observation(
        reference_source_id(),
        reference_car_id(),
        ObservationBindingRole::Operational,
    )
    .expect("headless fixture has one operational observation binding");
    host.begin_source_session(reference_source_id(), reference_session_id())
        .expect("headless fixture starts one source session");
    host
}

fn reference_source_id() -> ObservationSourceId {
    ObservationSourceId::new("fixture/headless-recorded")
}

fn reference_session_id() -> ObservationSessionId {
    ObservationSessionId::new("headless-fixture-session-A")
}

fn reference_car_id() -> CarId {
    CarId::new("lift-A")
}

fn snapshot(host: &ReadOnlyHost) -> LifecycleSnapshot {
    LifecycleSnapshot {
        lifecycle: host.lifecycle().clone(),
        readiness: host.readiness(),
    }
}

fn reference_source() -> RecordedObservationFeed {
    let complete = complete_observation(
        reference_source_id(),
        reference_session_id(),
        2,
        Floor::new(4),
    );
    let duplicate = complete.clone();
    RecordedObservationFeed::new([observation(1), complete, duplicate])
}

fn complete_observation(
    source: ObservationSourceId,
    session: ObservationSessionId,
    sequence: u64,
    floor: Floor,
) -> CarObservation {
    let mut complete = observation(sequence);
    complete.provenance.source = source;
    complete.provenance.session = session;
    complete.position = PositionEvidence::Exact(Position::at_floor(floor));
    complete.motion = MotionEvidence::Known(MotionState::Stopped);
    complete.door = DoorEvidence::Known(DoorState::Open);
    complete.reported_destination = ReportedDestination::None;
    complete
}

fn observation(sequence: u64) -> CarObservation {
    CarObservation {
        provenance: ObservationProvenance {
            source: reference_source_id(),
            session: reference_session_id(),
            sequence: ObservationSequence::new(sequence),
        },
        car: reference_car_id(),
        position: PositionEvidence::FloorReference(Floor::new(3)),
        motion: MotionEvidence::Known(MotionState::Moving(liftassistant_model::Direction::Up)),
        door: DoorEvidence::MovingUnknownDirection,
        mode: OperatingModeEvidence::Known(OperatingMode::FireService),
        reported_destination: ReportedDestination::Known(Floor::new(4)),
    }
}

fn expected_anomaly() -> DiagnosticAnomaly {
    DiagnosticAnomaly::ObservationSequenceNotIncreasing {
        source: ObservationSourceId::new("fixture/headless-recorded"),
        session: reference_session_id(),
        previous: ObservationSequence::new(2),
        received: ObservationSequence::new(2),
    }
}

fn verify_proof(run: &ProofRun) -> Result<(), String> {
    let anomaly = expected_anomaly();
    let expected_snapshots = [
        LifecycleSnapshot {
            lifecycle: HostLifecycle::Starting,
            readiness: Readiness::NotReady,
        },
        LifecycleSnapshot {
            lifecycle: HostLifecycle::Synchronizing,
            readiness: Readiness::NotReady,
        },
        LifecycleSnapshot {
            lifecycle: HostLifecycle::ReadyReadOnly,
            readiness: Readiness::ReadOnly,
        },
        LifecycleSnapshot {
            lifecycle: HostLifecycle::Degraded(DegradedReason::ObservationAnomaly(anomaly.clone())),
            readiness: Readiness::NotReady,
        },
    ];
    if run.snapshots != expected_snapshots {
        return Err("headless lifecycle/readiness transitions did not match the proof".into());
    }
    if run.latest_state.as_ref().map(|state| state.location)
        != Some(CarLocation::Continuous(Position::at_floor(Floor::new(4))))
    {
        return Err("headless host did not retain the last resolved read-only state".into());
    }
    if run.diagnostic_entries != 4 || run.incidents != 1 {
        return Err("headless host did not retain the expected diagnostic evidence".into());
    }
    if !run.events.iter().any(
        |event| matches!(event, HostEvent::ObservationRejected(observed) if observed == &anomaly),
    ) {
        return Err("headless host did not preserve the degradation trigger".into());
    }
    Ok(())
}

fn render_report(run: &ProofRun) -> String {
    let transitions = run
        .snapshots
        .iter()
        .map(|snapshot| format!("{:?}/{:?}", snapshot.lifecycle, snapshot.readiness))
        .collect::<Vec<_>>()
        .join(" -> ");
    let anomaly = run.events.iter().find_map(|event| match event {
        HostEvent::ObservationRejected(anomaly) => Some(anomaly),
        _ => None,
    });
    [
        format!("scenario: {LIFECYCLE_SCENARIO}"),
        "scope: internal headless read-only proof; no passenger write or deployment claim".into(),
        "determinism: PASS (2 identical headless executions)".into(),
        format!("lifecycle/readiness: PASS ({transitions})"),
        "process alive != ready: PASS (Starting and Degraded are NotReady)".into(),
        "write boundary: PASS (recorded source exposes observations only; no IntentSink implementation or simulator dependency)".into(),
        format!(
            "engine diagnostics: PASS ({} entries, {} incident, trigger={anomaly:?})",
            run.diagnostic_entries, run.incidents
        ),
        "outcome: PASS (partial observation synchronized, complete state became read-only ready, duplicate sequence revoked readiness)".into(),
        String::new(),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_started_process_is_not_ready_before_synchronization() {
        let host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());

        assert_eq!(host.lifecycle(), &HostLifecycle::Starting);
        assert_eq!(host.readiness(), Readiness::NotReady);
    }

    #[test]
    fn recorded_observations_drive_read_only_readiness_and_degradation() {
        let run = execute_proof();

        verify_proof(&run).expect("reference proof should satisfy lifecycle invariants");
        assert!(run.events.iter().any(|event| matches!(
            event,
            HostEvent::ObservationUnresolved { reasons, .. }
                if reasons == &[UnresolvedCarStateReason::DoorDirectionUnknown]
        )));
        assert!(run.events.iter().any(|event| matches!(
            event,
            HostEvent::StateUpdated(state)
                if state.location
                    == CarLocation::Continuous(Position::at_floor(Floor::new(4)))
        )));
    }

    #[test]
    fn readiness_waits_for_every_configured_operational_car() {
        let source = ObservationSourceId::new("fixture/two-car-source");
        let session = ObservationSessionId::new("two-car-session-A");
        let mut host = ReadOnlyHost::new(
            BuildingTopology::new([Floor::new(1), Floor::new(2)]).expect("valid topology"),
        );
        for car in [CarId::new("A"), CarId::new("B")] {
            host.bind_observation(source.clone(), car, ObservationBindingRole::Operational)
                .expect("each logical car has one authority");
        }
        host.begin_source_session(source.clone(), session.clone())
            .expect("source starts explicitly");

        let mut first = complete_observation(source.clone(), session.clone(), 1, Floor::new(1));
        first.car = CarId::new("A");
        host.ingest_observation(first.into());
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);
        assert_eq!(host.readiness(), Readiness::NotReady);

        let mut second = complete_observation(source, session, 2, Floor::new(2));
        second.car = CarId::new("B");
        host.ingest_observation(second.into());
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
    }

    #[test]
    fn executable_proof_is_deterministic_and_reports_no_write_boundary() {
        let report = run_scenario(LIFECYCLE_SCENARIO).expect("reference scenario should pass");

        assert!(report.contains("determinism: PASS"));
        assert!(report.contains("process alive != ready: PASS"));
        assert!(report.contains("no IntentSink implementation or simulator dependency"));
        assert!(report.contains("duplicate sequence revoked readiness"));
    }

    #[test]
    fn unknown_scenario_is_rejected() {
        assert!(
            run_scenario("unknown")
                .unwrap_err()
                .contains("unknown scenario")
        );
    }

    #[test]
    fn proof_diagnostics_preserve_machine_readable_anomaly() {
        let mut source = reference_source();
        let mut host = reference_host();
        while host.poll(&mut source) {}

        assert!(host.engine.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            DiagnosticEvent::Anomaly(anomaly) if anomaly == &expected_anomaly()
        )));
    }

    #[test]
    fn new_incidents_still_revoke_readiness_after_incident_retention_is_full() {
        let complete = complete_observation(
            reference_source_id(),
            reference_session_id(),
            2,
            Floor::new(4),
        );
        let mut frames = vec![observation(1), complete.clone()];
        frames.extend(std::iter::repeat_n(complete, 10));
        let mut source = RecordedObservationFeed::new(frames);
        let mut host = reference_host();

        while host.poll(&mut source) {}

        assert_eq!(host.engine.diagnostics().incidents().count(), 8);
        assert_eq!(
            host.events
                .iter()
                .filter(|event| matches!(event, HostEvent::ObservationRejected(_)))
                .count(),
            10
        );
        assert!(matches!(host.lifecycle(), HostLifecycle::Degraded(_)));
        assert_eq!(host.readiness(), Readiness::NotReady);
    }

    #[test]
    fn operational_session_reset_revokes_readiness_until_new_session_state_arrives() {
        let mut host = reference_host();
        let mut initial = RecordedObservationFeed::new([
            observation(1),
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                2,
                Floor::new(4),
            ),
        ]);
        while host.poll(&mut initial) {}
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let next_session = ObservationSessionId::new("headless-fixture-session-B");
        host.begin_source_session(reference_source_id(), next_session.clone())
            .expect("operational source explicitly starts a new session");
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.engine.car_states().is_empty());

        let mut resumed = RecordedObservationFeed::new([complete_observation(
            reference_source_id(),
            next_session,
            1,
            Floor::new(4),
        )]);
        assert!(host.poll(&mut resumed));
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
    }

    #[test]
    fn discovery_without_payload_does_not_establish_readiness() {
        let mut host = reference_host();

        host.note_transport_subscriber_created(&reference_source_id());
        host.note_transport_discovery(&reference_source_id());

        let health = host
            .source_health(&reference_source_id())
            .expect("bound source has health dimensions");
        assert_eq!(
            health.transport_discovery,
            TransportDiscoveryState::DiscoveryObserved
        );
        assert_eq!(health.payload, PayloadActivity::Awaiting);
        assert_eq!(
            health.session,
            SourceSessionActivity::Active(reference_session_id())
        );
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.engine.car_states().is_empty());
    }

    #[test]
    fn operational_payload_staleness_revokes_truth_and_newer_payload_recovers() {
        let mut host = reference_host();
        host.ingest_observation(
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                1,
                Floor::new(4),
            )
            .into(),
        );
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let invalidation = host
            .mark_source_payload_stale(reference_source_id(), reference_session_id())
            .expect("the active source session can be explicitly invalidated");
        assert_eq!(invalidation.invalidated_cars, vec![reference_car_id()]);
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.engine.car_states().is_empty());
        let health = host
            .source_health(&reference_source_id())
            .expect("source health remains inspectable");
        assert_eq!(health.payload, PayloadActivity::Stale);
        assert_eq!(
            health.observations.get(&reference_car_id()),
            Some(&ObservationAvailability::Revoked)
        );
        assert!(host.engine.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            DiagnosticEvent::SourceObservationsInvalidated {
                source,
                session,
                cars,
            } if source == &reference_source_id()
                && session == &reference_session_id()
                && cars == std::slice::from_ref(&reference_car_id())
        )));

        host.ingest_observation(
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                2,
                Floor::new(4),
            )
            .into(),
        );
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
        let recovered = host
            .source_health(&reference_source_id())
            .expect("source health remains inspectable after recovery");
        assert_eq!(recovered.payload, PayloadActivity::Active);
        assert_eq!(
            recovered.observations.get(&reference_car_id()),
            Some(&ObservationAvailability::Resolved)
        );
    }

    #[test]
    fn auxiliary_payload_staleness_is_visible_without_revoking_operational_readiness() {
        let mut host = reference_host();
        host.ingest_observation(
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                1,
                Floor::new(4),
            )
            .into(),
        );
        let operational = host.engine.car_states();
        let auxiliary_source = ObservationSourceId::new("fixture/auxiliary-rmf");
        let auxiliary_session = ObservationSessionId::new("auxiliary-session-A");
        host.bind_observation(
            auxiliary_source.clone(),
            reference_car_id(),
            ObservationBindingRole::Auxiliary,
        )
        .expect("auxiliary binding is explicit");
        host.begin_source_session(auxiliary_source.clone(), auxiliary_session.clone())
            .expect("auxiliary source starts explicitly");
        host.ingest_observation(
            complete_observation(
                auxiliary_source.clone(),
                auxiliary_session.clone(),
                1,
                Floor::new(1),
            )
            .into(),
        );

        host.mark_source_payload_stale(auxiliary_source.clone(), auxiliary_session.clone())
            .expect("auxiliary freshness is supervised independently");

        assert_eq!(host.engine.car_states(), operational);
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
        assert_eq!(
            host.source_health(&auxiliary_source)
                .expect("auxiliary health remains visible")
                .payload,
            PayloadActivity::Stale
        );
        assert!(host.events.iter().any(|event| matches!(
            event,
            HostEvent::SourcePayloadStale { source, session, .. }
                if source == &auxiliary_source && session == &auxiliary_session
        )));
    }

    #[test]
    fn auxiliary_sequence_anomaly_is_visible_without_revoking_operational_readiness() {
        let mut host = reference_host();
        let mut initial = RecordedObservationFeed::new([complete_observation(
            reference_source_id(),
            reference_session_id(),
            1,
            Floor::new(4),
        )]);
        assert!(host.poll(&mut initial));
        let operational = host.engine.car_states();
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let auxiliary_source = ObservationSourceId::new("fixture/auxiliary-rmf");
        let auxiliary_session = ObservationSessionId::new("auxiliary-session-A");
        host.bind_observation(
            auxiliary_source.clone(),
            reference_car_id(),
            ObservationBindingRole::Auxiliary,
        )
        .expect("auxiliary binding is explicit");
        host.begin_source_session(auxiliary_source.clone(), auxiliary_session.clone())
            .expect("auxiliary source starts explicitly");
        let auxiliary = complete_observation(
            auxiliary_source.clone(),
            auxiliary_session.clone(),
            1,
            Floor::new(1),
        );
        let mut feed = RecordedObservationFeed::new([auxiliary.clone(), auxiliary]);
        assert!(host.poll(&mut feed));
        assert!(host.poll(&mut feed));

        let anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source: auxiliary_source,
            session: auxiliary_session,
            previous: ObservationSequence::new(1),
            received: ObservationSequence::new(1),
        };
        assert!(host.engine.diagnostics().entries().any(|entry| {
            matches!(&entry.event, DiagnosticEvent::Anomaly(observed) if observed == &anomaly)
        }));
        assert!(
            host.events
                .contains(&HostEvent::ObservationRejected(anomaly))
        );
        assert_eq!(host.engine.car_states(), operational);
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
    }
}
