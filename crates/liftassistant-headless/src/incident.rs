use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use liftassistant_engine::{DiagnosticAnomaly, DiagnosticEvent, ObservationBindingRole};
use liftassistant_model::{
    BuildingTopology, CarId, CarObservationUpdate, Direction, DoorEvidence, DoorState, Floor,
    MotionEvidence, MotionState, ObservationProvenance, ObservationSequence, ObservationSessionId,
    ObservationSourceId, OperatingMode, OperatingModeEvidence, Position, PositionEvidence,
    ReportedDestination,
};
use serde::{Deserialize, Serialize};

use super::{HostDiagnosticId, HostEvent, HostLifecycle, ReadOnlyHost, Readiness};

pub(super) const INCIDENT_CAPSULE_VERSION: &str = "liftassistant.incident-capsule/1";
const MAX_CAPSULE_BYTES: u64 = 1024 * 1024;
const MAX_CAPSULE_ACTIONS: usize = 256;
const MAX_CAPSULE_BINDINGS: usize = 256;
const MAX_CAPSULE_FLOORS: usize = 1024;

#[derive(Clone, Debug)]
pub(super) struct IncidentRecorder {
    capacity: usize,
    bindings: BTreeMap<(String, String), CapsuleBinding>,
    baseline: CapsuleBaseline,
    actions: VecDeque<CapsuleAction>,
}

impl IncidentRecorder {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.clamp(1, MAX_CAPSULE_ACTIONS),
            bindings: BTreeMap::new(),
            baseline: CapsuleBaseline::default(),
            actions: VecDeque::new(),
        }
    }

    pub(super) fn record_binding(
        &mut self,
        source: &ObservationSourceId,
        car: &CarId,
        role: ObservationBindingRole,
    ) {
        let binding = CapsuleBinding {
            source: source.as_str().to_owned(),
            car: car.as_str().to_owned(),
            role: role.into(),
        };
        self.bindings
            .insert((binding.source.clone(), binding.car.clone()), binding);
    }

    pub(super) fn record_begin_session(
        &mut self,
        source: &ObservationSourceId,
        session: &ObservationSessionId,
    ) {
        self.push(CapsuleAction::BeginSourceSession {
            source: source.as_str().to_owned(),
            session: session.as_str().to_owned(),
        });
    }

    pub(super) fn record_observation(&mut self, update: &CarObservationUpdate) {
        self.push(CapsuleAction::Observation {
            update: update.into(),
        });
    }

    pub(super) fn record_invalidation(
        &mut self,
        source: &ObservationSourceId,
        session: &ObservationSessionId,
    ) {
        self.push(CapsuleAction::InvalidateSource {
            source: source.as_str().to_owned(),
            session: session.as_str().to_owned(),
        });
    }

    pub(super) fn capture(
        &self,
        topology: &BuildingTopology,
        lifecycle: &HostLifecycle,
        readiness: Readiness,
        trigger: CapsuleTrigger,
    ) -> IncidentCapsule {
        let mut baseline = self.baseline.clone();
        let mut window = self.actions.clone();
        let mut actions = baseline.preamble_actions(&self.bindings);
        while actions.len() + window.len() > MAX_CAPSULE_ACTIONS && window.len() > 1 {
            if let Some(evicted) = window.pop_front() {
                baseline.apply_evicted(&self.bindings, evicted);
                actions = baseline.preamble_actions(&self.bindings);
            }
        }
        if actions.len() + window.len() > MAX_CAPSULE_ACTIONS {
            baseline.compact_auxiliary(&self.bindings, &window, &trigger);
            actions = baseline.preamble_actions(&self.bindings);
        }
        while actions.len() + window.len() > MAX_CAPSULE_ACTIONS && !window.is_empty() {
            if let Some(evicted) = window.pop_front() {
                baseline.apply_evicted(&self.bindings, evicted);
                baseline.compact_auxiliary(&self.bindings, &window, &trigger);
                actions = baseline.preamble_actions(&self.bindings);
            }
        }
        actions.extend(window);

        IncidentCapsule {
            version: INCIDENT_CAPSULE_VERSION.to_owned(),
            context: CapsuleContext {
                producer: "liftassistant-headless".into(),
                topology: topology
                    .served_floors()
                    .iter()
                    .map(|floor| floor.index())
                    .collect(),
                bindings: self.bindings.values().cloned().collect(),
                sanitization: None,
            },
            actions,
            trigger,
            expected_lifecycle: lifecycle.into(),
            expected_readiness: readiness.into(),
        }
    }

    fn push(&mut self, action: CapsuleAction) {
        if self.actions.len() == self.capacity
            && let Some(evicted) = self.actions.pop_front()
        {
            self.baseline.apply_evicted(&self.bindings, evicted);
        }
        self.actions.push_back(action);
    }
}

#[derive(Clone, Debug, Default)]
struct CapsuleBaseline {
    sources: BTreeMap<String, BaselineSource>,
    saw_operational_activity: bool,
    degrading: Option<BaselineDegradation>,
}

#[derive(Clone, Debug)]
struct BaselineSource {
    session: String,
    prior_session: Option<String>,
    invalidated_at: Option<Option<(u64, String)>>,
    last_sequence: Option<(u64, String)>,
    cars: BTreeMap<String, BaselineCar>,
}

#[derive(Clone, Debug)]
struct BaselineDegradation {
    source: String,
    active_session: String,
    kind: BaselineDegradationKind,
}

#[derive(Clone, Debug)]
enum BaselineDegradationKind {
    NonIncreasingSequence {
        watermark_car: String,
        previous: u64,
        car: String,
        received: u64,
    },
    InactiveSession {
        received_session: String,
        car: String,
        sequence: u64,
    },
}

#[derive(Clone, Debug)]
struct BaselineCar {
    latest_sequence: u64,
    position: Option<CapsulePositionEvidence>,
    motion: Option<CapsuleMotionEvidence>,
    door: Option<CapsuleDoorEvidence>,
    mode: Option<CapsuleOperatingModeEvidence>,
    reported_destination: Option<CapsuleReportedDestination>,
}

impl BaselineCar {
    fn empty(sequence: u64) -> Self {
        Self {
            latest_sequence: sequence,
            position: None,
            motion: None,
            door: None,
            mode: None,
            reported_destination: None,
        }
    }

    fn merge(&mut self, update: &CapsuleObservationUpdate) {
        self.latest_sequence = update.sequence;
        if update.position.is_some() {
            self.position = update.position;
        }
        if update.motion.is_some() {
            self.motion = update.motion;
        }
        if update.door.is_some() {
            self.door = update.door;
        }
        if update.mode.is_some() {
            self.mode = update.mode;
        }
        if update.reported_destination.is_some() {
            self.reported_destination = update.reported_destination;
        }
    }

    fn to_observation_update(
        &self,
        source: &str,
        session: &str,
        car: &str,
    ) -> CapsuleObservationUpdate {
        CapsuleObservationUpdate {
            source: source.to_owned(),
            session: session.to_owned(),
            sequence: self.latest_sequence,
            car: car.to_owned(),
            position: self.position,
            motion: self.motion,
            door: self.door,
            mode: self.mode,
            reported_destination: self.reported_destination,
        }
    }

    fn is_resolved(&self, source: &str, session: &str, car: &str) -> bool {
        let update: CarObservationUpdate =
            (&self.to_observation_update(source, session, car)).into();
        let Some(observation) = (liftassistant_runtime::AssembledCarObservation {
            latest_provenance: update.provenance.clone(),
            car: update.car,
            position: update
                .position
                .map(|value| liftassistant_runtime::EvidenceSample {
                    value,
                    provenance: update.provenance.clone(),
                }),
            motion: update
                .motion
                .map(|value| liftassistant_runtime::EvidenceSample {
                    value,
                    provenance: update.provenance.clone(),
                }),
            door: update
                .door
                .map(|value| liftassistant_runtime::EvidenceSample {
                    value,
                    provenance: update.provenance.clone(),
                }),
            mode: update
                .mode
                .map(|value| liftassistant_runtime::EvidenceSample {
                    value,
                    provenance: update.provenance.clone(),
                }),
            reported_destination: update.reported_destination.map(|value| {
                liftassistant_runtime::EvidenceSample {
                    value,
                    provenance: update.provenance.clone(),
                }
            }),
        })
        .snapshot() else {
            return false;
        };
        matches!(
            liftassistant_runtime::resolve_car_observation(&observation),
            liftassistant_runtime::CarStateResolution::Resolved(_)
        )
    }
}

impl CapsuleBaseline {
    fn apply_evicted(
        &mut self,
        bindings: &BTreeMap<(String, String), CapsuleBinding>,
        action: CapsuleAction,
    ) {
        match action {
            CapsuleAction::BeginSourceSession { source, session } => {
                if !bindings
                    .keys()
                    .any(|(bound_source, _)| bound_source == &source)
                {
                    return;
                }
                let mut prior_session = None;
                if let Some(previous) = self.sources.get(&source) {
                    prior_session = Some(previous.session.clone());
                    let invalidated_operational = previous.cars.iter().any(|(car, state)| {
                        is_operational_binding(bindings, &source, car)
                            && state.is_resolved(&source, &previous.session, car)
                    });
                    if invalidated_operational {
                        self.saw_operational_activity = true;
                        self.degrading = None;
                    }
                }
                self.sources.insert(
                    source,
                    BaselineSource {
                        session,
                        prior_session,
                        invalidated_at: None,
                        last_sequence: None,
                        cars: BTreeMap::new(),
                    },
                );
            }
            CapsuleAction::InvalidateSource { source, session } => {
                let has_operational = source_has_operational_binding(bindings, &source);
                let Some(source_state) = self.sources.get_mut(&source) else {
                    return;
                };
                if source_state.session != session {
                    return;
                }
                source_state.invalidated_at = Some(source_state.last_sequence.clone());
                source_state.cars.clear();
                if has_operational {
                    self.saw_operational_activity = true;
                    self.degrading = None;
                }
            }
            CapsuleAction::Observation { update } => {
                let Some(binding) = bindings.get(&(update.source.clone(), update.car.clone()))
                else {
                    return;
                };
                let is_operational = binding.role == CapsuleBindingRole::Operational;
                let Some(source_state) = self.sources.get_mut(&update.source) else {
                    return;
                };
                if is_operational {
                    self.saw_operational_activity = true;
                    self.degrading = None;
                }
                if source_state.session != update.session {
                    if is_operational {
                        self.degrading = Some(BaselineDegradation {
                            source: update.source,
                            active_session: source_state.session.clone(),
                            kind: BaselineDegradationKind::InactiveSession {
                                received_session: update.session,
                                car: update.car,
                                sequence: update.sequence,
                            },
                        });
                    }
                    return;
                }
                if let Some((previous_seq, watermark_car)) = &source_state.last_sequence
                    && update.sequence <= *previous_seq
                {
                    if is_operational {
                        self.degrading = Some(BaselineDegradation {
                            source: update.source,
                            active_session: source_state.session.clone(),
                            kind: BaselineDegradationKind::NonIncreasingSequence {
                                watermark_car: watermark_car.clone(),
                                previous: *previous_seq,
                                car: update.car,
                                received: update.sequence,
                            },
                        });
                    }
                    return;
                }
                source_state.last_sequence = Some((update.sequence, update.car.clone()));
                source_state
                    .cars
                    .entry(update.car.clone())
                    .or_insert_with(|| BaselineCar::empty(update.sequence))
                    .merge(&update);
            }
        }
    }

    fn compact_auxiliary(
        &mut self,
        bindings: &BTreeMap<(String, String), CapsuleBinding>,
        window: &VecDeque<CapsuleAction>,
        trigger: &CapsuleTrigger,
    ) {
        let mut referenced_sources = BTreeSet::<&str>::new();
        let mut trigger_car = None::<(&str, &str)>;
        match trigger {
            CapsuleTrigger::ObservationSequenceNotIncreasing { source, car, .. }
            | CapsuleTrigger::ObservationUnresolved { source, car, .. } => {
                referenced_sources.insert(source.as_str());
                trigger_car = Some((source.as_str(), car.as_str()));
            }
            CapsuleTrigger::SourcePayloadStale { source, .. } => {
                referenced_sources.insert(source.as_str());
            }
            CapsuleTrigger::ReadinessChanged { .. } => {}
        }
        for action in window {
            match action {
                CapsuleAction::BeginSourceSession { source, .. }
                | CapsuleAction::InvalidateSource { source, .. } => {
                    referenced_sources.insert(source.as_str());
                }
                CapsuleAction::Observation { update } => {
                    referenced_sources.insert(update.source.as_str());
                }
            }
        }
        if let Some(deg) = &self.degrading {
            referenced_sources.insert(deg.source.as_str());
        }

        self.sources.retain(|source, _| {
            source_has_operational_binding(bindings, source)
                || referenced_sources.contains(source.as_str())
        });
        for (source, state) in &mut self.sources {
            state.cars.retain(|car, _| {
                is_operational_binding(bindings, source, car)
                    || trigger_car == Some((source.as_str(), car.as_str()))
            });
            if referenced_sources.contains(source.as_str())
                && let Some((max_seq, max_car)) = &state.last_sequence
            {
                let invalidated_watermark = state
                    .invalidated_at
                    .as_ref()
                    .and_then(|watermark| watermark.as_ref())
                    .map(|(seq, _)| *seq);
                let cars_max = state.cars.values().map(|car| car.latest_sequence).max();
                if cars_max != Some(*max_seq) && invalidated_watermark != Some(*max_seq) {
                    state
                        .cars
                        .entry(max_car.clone())
                        .or_insert_with(|| BaselineCar::empty(*max_seq));
                }
            }
        }
    }

    fn preamble_actions(
        &self,
        bindings: &BTreeMap<(String, String), CapsuleBinding>,
    ) -> Vec<CapsuleAction> {
        let mut preamble = Vec::new();
        if let Some(deg) = &self.degrading {
            for (source, state) in &self.sources {
                if source != &deg.source {
                    emit_active_source_actions(&mut preamble, source, state);
                }
            }
            if let Some(state) = self.sources.get(&deg.source) {
                emit_degraded_source_actions(&mut preamble, &deg.source, state, deg);
            }
            return preamble;
        }

        let active_emits_operational = self.sources.iter().any(|(source, state)| {
            (state.invalidated_at.is_some() && source_has_operational_binding(bindings, source))
                || state
                    .cars
                    .keys()
                    .any(|car| is_operational_binding(bindings, source, car))
        });
        let mut need_prior_synchronizing =
            self.saw_operational_activity && !active_emits_operational;

        for (source, state) in &self.sources {
            if need_prior_synchronizing
                && let Some(prior_session) = &state.prior_session
                && let Some(op_car) = first_operational_car(bindings, source)
            {
                preamble.push(CapsuleAction::BeginSourceSession {
                    source: source.clone(),
                    session: prior_session.clone(),
                });
                preamble.push(CapsuleAction::Observation {
                    update: BaselineCar::empty(1).to_observation_update(
                        source,
                        prior_session,
                        op_car,
                    ),
                });
                need_prior_synchronizing = false;
            }
            emit_active_source_actions(&mut preamble, source, state);
        }
        preamble
    }
}

fn is_operational_binding(
    bindings: &BTreeMap<(String, String), CapsuleBinding>,
    source: &str,
    car: &str,
) -> bool {
    bindings
        .get(&(source.to_owned(), car.to_owned()))
        .is_some_and(|binding| binding.role == CapsuleBindingRole::Operational)
}

fn source_has_operational_binding(
    bindings: &BTreeMap<(String, String), CapsuleBinding>,
    source: &str,
) -> bool {
    first_operational_car(bindings, source).is_some()
}

fn first_operational_car<'a>(
    bindings: &'a BTreeMap<(String, String), CapsuleBinding>,
    source: &str,
) -> Option<&'a str> {
    bindings.values().find_map(|binding| {
        (binding.source == source && binding.role == CapsuleBindingRole::Operational)
            .then_some(binding.car.as_str())
    })
}

fn emit_active_source_actions(
    preamble: &mut Vec<CapsuleAction>,
    source: &str,
    state: &BaselineSource,
) {
    preamble.push(CapsuleAction::BeginSourceSession {
        source: source.to_owned(),
        session: state.session.clone(),
    });
    if let Some(watermark) = &state.invalidated_at {
        if let Some((sequence, car)) = watermark {
            preamble.push(CapsuleAction::Observation {
                update: BaselineCar::empty(*sequence).to_observation_update(
                    source,
                    &state.session,
                    car,
                ),
            });
        }
        preamble.push(CapsuleAction::InvalidateSource {
            source: source.to_owned(),
            session: state.session.clone(),
        });
    }
    let mut cars: Vec<_> = state.cars.iter().collect();
    cars.sort_by_key(|(_, car_state)| car_state.latest_sequence);
    for (car, car_state) in cars {
        preamble.push(CapsuleAction::Observation {
            update: car_state.to_observation_update(source, &state.session, car),
        });
    }
}

fn emit_degraded_source_actions(
    preamble: &mut Vec<CapsuleAction>,
    source: &str,
    state: &BaselineSource,
    deg: &BaselineDegradation,
) {
    if state.session != deg.active_session {
        preamble.push(CapsuleAction::BeginSourceSession {
            source: source.to_owned(),
            session: deg.active_session.clone(),
        });
        match &deg.kind {
            BaselineDegradationKind::NonIncreasingSequence {
                watermark_car,
                previous,
                car,
                received,
            } => {
                preamble.push(CapsuleAction::Observation {
                    update: BaselineCar::empty(*previous).to_observation_update(
                        source,
                        &deg.active_session,
                        watermark_car,
                    ),
                });
                preamble.push(CapsuleAction::Observation {
                    update: BaselineCar::empty(*received).to_observation_update(
                        source,
                        &deg.active_session,
                        car,
                    ),
                });
            }
            BaselineDegradationKind::InactiveSession {
                received_session,
                car,
                sequence,
            } => {
                preamble.push(CapsuleAction::Observation {
                    update: BaselineCar::empty(*sequence).to_observation_update(
                        source,
                        received_session,
                        car,
                    ),
                });
            }
        }
        emit_active_source_actions(preamble, source, state);
        return;
    }

    preamble.push(CapsuleAction::BeginSourceSession {
        source: source.to_owned(),
        session: state.session.clone(),
    });
    let mut emitted_watermark = None;
    if let Some(watermark) = &state.invalidated_at {
        if let Some((sequence, car)) = watermark {
            preamble.push(CapsuleAction::Observation {
                update: BaselineCar::empty(*sequence).to_observation_update(
                    source,
                    &state.session,
                    car,
                ),
            });
            emitted_watermark = Some(*sequence);
        }
        preamble.push(CapsuleAction::InvalidateSource {
            source: source.to_owned(),
            session: state.session.clone(),
        });
    }
    let mut cars: Vec<_> = state.cars.iter().collect();
    cars.sort_by_key(|(_, car_state)| car_state.latest_sequence);

    match &deg.kind {
        BaselineDegradationKind::NonIncreasingSequence {
            watermark_car,
            previous,
            car,
            received,
        } => {
            for (bound_car, car_state) in cars
                .iter()
                .copied()
                .filter(|(_, car_state)| car_state.latest_sequence <= *previous)
            {
                preamble.push(CapsuleAction::Observation {
                    update: car_state.to_observation_update(source, &state.session, bound_car),
                });
                emitted_watermark = Some(car_state.latest_sequence);
            }
            if emitted_watermark != Some(*previous) {
                preamble.push(CapsuleAction::Observation {
                    update: BaselineCar::empty(*previous).to_observation_update(
                        source,
                        &state.session,
                        watermark_car,
                    ),
                });
            }
            preamble.push(CapsuleAction::Observation {
                update: BaselineCar::empty(*received).to_observation_update(
                    source,
                    &state.session,
                    car,
                ),
            });
            for (bound_car, car_state) in cars
                .into_iter()
                .filter(|(_, car_state)| car_state.latest_sequence > *previous)
            {
                preamble.push(CapsuleAction::Observation {
                    update: car_state.to_observation_update(source, &state.session, bound_car),
                });
            }
        }
        BaselineDegradationKind::InactiveSession {
            received_session,
            car,
            sequence,
        } => {
            for (bound_car, car_state) in cars {
                preamble.push(CapsuleAction::Observation {
                    update: car_state.to_observation_update(source, &state.session, bound_car),
                });
            }
            preamble.push(CapsuleAction::Observation {
                update: BaselineCar::empty(*sequence).to_observation_update(
                    source,
                    received_session,
                    car,
                ),
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct IncidentCapsule {
    version: String,
    context: CapsuleContext,
    actions: Vec<CapsuleAction>,
    trigger: CapsuleTrigger,
    expected_lifecycle: CapsuleLifecycle,
    expected_readiness: CapsuleReadiness,
}

impl IncidentCapsule {
    pub(super) fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to encode incident capsule: {error}"))
    }

    pub(super) fn from_json(json: &str) -> Result<Self, String> {
        let capsule: Self = serde_json::from_str(json)
            .map_err(|error| format!("failed to decode incident capsule: {error}"))?;
        capsule.validate()?;
        Ok(capsule)
    }

    pub(super) fn write_new(&self, path: &Path) -> Result<(), String> {
        let json = self.to_json()?;
        if json.len() as u64 > MAX_CAPSULE_BYTES {
            return Err(format!(
                "incident capsule exceeds the internal {MAX_CAPSULE_BYTES}-byte limit"
            ));
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("failed to create incident capsule {path:?}: {error}"))?;
        file.write_all(json.as_bytes())
            .map_err(|error| format!("failed to write incident capsule {path:?}: {error}"))
    }

    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path)
            .map_err(|error| format!("failed to inspect incident capsule {path:?}: {error}"))?;
        if metadata.len() > MAX_CAPSULE_BYTES {
            return Err(format!(
                "incident capsule {path:?} exceeds the internal {MAX_CAPSULE_BYTES}-byte limit"
            ));
        }
        let json = fs::read_to_string(path)
            .map_err(|error| format!("failed to read incident capsule {path:?}: {error}"))?;
        Self::from_json(&json)
    }

    pub(super) fn replay(&self) -> Result<IncidentReplayReport, String> {
        self.validate()?;
        let topology = BuildingTopology::new(self.context.topology.iter().copied().map(Floor::new))
            .map_err(|error| format!("invalid capsule topology: {error}"))?;
        let mut host = ReadOnlyHost::new(topology);
        for binding in &self.context.bindings {
            host.bind_observation(
                ObservationSourceId::new(&binding.source),
                CarId::new(&binding.car),
                binding.role.into(),
            )
            .map_err(|error| format!("capsule binding replay failed: {error:?}"))?;
        }

        let mut active_sessions = BTreeMap::<String, String>::new();
        for action in &self.actions {
            match action {
                CapsuleAction::BeginSourceSession { source, session } => {
                    host.begin_source_session(
                        ObservationSourceId::new(source),
                        ObservationSessionId::new(session),
                    )
                    .map_err(|error| format!("capsule session replay failed: {error:?}"))?;
                    active_sessions.insert(source.clone(), session.clone());
                }
                CapsuleAction::Observation { update } => {
                    if !active_sessions.contains_key(&update.source) {
                        return Err(format!(
                            "capsule observation for source {:?} appears before an explicit session action",
                            update.source
                        ));
                    }
                    host.ingest_observation(update.into());
                }
                CapsuleAction::InvalidateSource { source, session } => {
                    if !active_sessions.contains_key(source) {
                        return Err(format!(
                            "capsule invalidation for source {source:?} appears before an explicit session action"
                        ));
                    }
                    host.mark_source_payload_stale(
                        ObservationSourceId::new(source),
                        ObservationSessionId::new(session),
                    )
                    .map_err(|error| format!("capsule invalidation replay failed: {error:?}"))?;
                }
            }
        }

        let trigger_reproduced = self.trigger.is_reproduced(&host);
        let lifecycle = CapsuleLifecycle::from(host.lifecycle());
        let readiness = CapsuleReadiness::from(host.readiness());
        if !trigger_reproduced {
            return Err(format!(
                "incident capsule did not reproduce trigger {}",
                self.trigger.id()
            ));
        }
        if lifecycle != self.expected_lifecycle || readiness != self.expected_readiness {
            return Err(format!(
                "incident capsule outcome diverged: expected {:?}/{:?}, observed {lifecycle:?}/{readiness:?}",
                self.expected_lifecycle, self.expected_readiness
            ));
        }

        Ok(IncidentReplayReport {
            version: self.version.clone(),
            trigger_id: self.trigger.id(),
            action_count: self.actions.len(),
            lifecycle,
            readiness,
        })
    }

    pub(super) fn sanitize_candidate(
        &self,
    ) -> Result<(IncidentCapsule, SanitizationReport), String> {
        self.validate()?;
        if self.context.sanitization.is_some() {
            return Err("incident capsule is already a sanitized candidate".into());
        }

        let source_map = self
            .context
            .bindings
            .iter()
            .map(|binding| binding.source.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .enumerate()
            .map(|(index, source)| (source, format!("source-{:03}", index + 1)))
            .collect::<BTreeMap<_, _>>();
        let car_map = self
            .context
            .bindings
            .iter()
            .map(|binding| binding.car.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .enumerate()
            .map(|(index, car)| (car, format!("car-{:03}", index + 1)))
            .collect::<BTreeMap<_, _>>();
        let mut sessions_by_source = BTreeMap::<String, BTreeSet<String>>::new();
        for action in &self.actions {
            match action {
                CapsuleAction::BeginSourceSession { source, session }
                | CapsuleAction::InvalidateSource { source, session } => {
                    sessions_by_source
                        .entry(source.clone())
                        .or_default()
                        .insert(session.clone());
                }
                CapsuleAction::Observation { update } => {
                    sessions_by_source
                        .entry(update.source.clone())
                        .or_default()
                        .insert(update.session.clone());
                }
            }
        }
        let session_map = sessions_by_source
            .into_iter()
            .flat_map(|(source, sessions)| {
                sessions
                    .into_iter()
                    .enumerate()
                    .map(move |(index, session)| {
                        (
                            (source.clone(), session),
                            format!("session-{:03}", index + 1),
                        )
                    })
            })
            .collect::<BTreeMap<_, _>>();
        let floor_map = self
            .context
            .topology
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .enumerate()
            .map(|(index, floor)| (floor, (index + 1) as i16))
            .collect::<BTreeMap<_, _>>();

        let mut candidate = self.clone();
        candidate.context.topology = candidate
            .context
            .topology
            .iter()
            .map(|floor| mapped_floor(&floor_map, *floor))
            .collect::<Result<Vec<_>, _>>()?;
        candidate.context.topology.sort_unstable();
        for binding in &mut candidate.context.bindings {
            binding.source = mapped_identity(&source_map, &binding.source, "source")?;
            binding.car = mapped_identity(&car_map, &binding.car, "car")?;
        }
        candidate
            .context
            .bindings
            .sort_by(|left, right| (&left.source, &left.car).cmp(&(&right.source, &right.car)));
        for action in &mut candidate.actions {
            sanitize_action(action, &source_map, &session_map, &car_map, &floor_map)?;
        }
        sanitize_trigger(&mut candidate.trigger, &source_map, &session_map, &car_map)?;
        candidate.context.sanitization = Some(CapsuleSanitization {
            profile: "deterministic-pseudonymization/1".into(),
            manual_review_required: true,
        });
        candidate.validate()?;

        Ok((
            candidate,
            SanitizationReport {
                source_count: source_map.len(),
                session_count: session_map.len(),
                car_count: car_map.len(),
                floor_count: floor_map.len(),
                manual_review_required: true,
            },
        ))
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != INCIDENT_CAPSULE_VERSION {
            return Err(format!(
                "unsupported incident capsule version {:?}; expected {INCIDENT_CAPSULE_VERSION:?}",
                self.version
            ));
        }
        if self.context.producer != "liftassistant-headless" {
            return Err(format!(
                "unsupported incident capsule producer {:?}",
                self.context.producer
            ));
        }
        if let Some(sanitization) = &self.context.sanitization
            && (sanitization.profile != "deterministic-pseudonymization/1"
                || !sanitization.manual_review_required)
        {
            return Err("incident capsule has unsupported sanitization metadata".into());
        }
        if self.context.topology.is_empty() || self.context.topology.len() > MAX_CAPSULE_FLOORS {
            return Err("incident capsule topology size is outside internal bounds".into());
        }
        if self
            .context
            .topology
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != self.context.topology.len()
        {
            return Err("incident capsule topology contains duplicate floors".into());
        }
        if self.context.bindings.is_empty() || self.context.bindings.len() > MAX_CAPSULE_BINDINGS {
            return Err("incident capsule binding count is outside internal bounds".into());
        }
        if self.actions.is_empty() || self.actions.len() > MAX_CAPSULE_ACTIONS {
            return Err("incident capsule action count is outside internal bounds".into());
        }
        let mut bindings = BTreeSet::new();
        let mut bound_sources = BTreeSet::new();
        let mut operational_cars = BTreeSet::new();
        for binding in &self.context.bindings {
            if !bindings.insert((binding.source.as_str(), binding.car.as_str())) {
                return Err("incident capsule contains a duplicate observation binding".into());
            }
            if binding.role == CapsuleBindingRole::Operational
                && !operational_cars.insert(binding.car.as_str())
            {
                return Err(format!(
                    "incident capsule contains conflicting operational bindings for car {:?}",
                    binding.car
                ));
            }
            bound_sources.insert(binding.source.as_str());
        }
        let mut active_sessions = BTreeMap::<&str, &str>::new();
        for action in &self.actions {
            match action {
                CapsuleAction::BeginSourceSession { source, session } => {
                    if !bound_sources.contains(source.as_str()) {
                        return Err(format!("incident capsule starts unbound source {source:?}"));
                    }
                    active_sessions.insert(source, session);
                }
                CapsuleAction::Observation { update } => {
                    if !active_sessions.contains_key(update.source.as_str()) {
                        return Err(format!(
                            "incident capsule observation for source {:?} lacks an explicit session action",
                            update.source
                        ));
                    }
                    if !bindings.contains(&(update.source.as_str(), update.car.as_str())) {
                        return Err(format!(
                            "incident capsule observation for source {:?} car {:?} lacks a binding",
                            update.source, update.car
                        ));
                    }
                }
                CapsuleAction::InvalidateSource { source, session } => {
                    if active_sessions.get(source.as_str()).copied() != Some(session.as_str()) {
                        return Err(format!(
                            "incident capsule invalidation for source {source:?} does not target its explicitly active session"
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

fn mapped_identity(
    mapping: &BTreeMap<String, String>,
    value: &str,
    kind: &str,
) -> Result<String, String> {
    mapping
        .get(value)
        .cloned()
        .ok_or_else(|| format!("incident capsule {kind} {value:?} has no sanitization mapping"))
}

fn mapped_session(
    mapping: &BTreeMap<(String, String), String>,
    source: &str,
    session: &str,
) -> Result<String, String> {
    mapping
        .get(&(source.to_owned(), session.to_owned()))
        .cloned()
        .ok_or_else(|| {
            format!(
                "incident capsule source/session ({source:?}, {session:?}) has no sanitization mapping"
            )
        })
}

fn mapped_floor(mapping: &BTreeMap<i16, i16>, floor: i16) -> Result<i16, String> {
    mapping
        .get(&floor)
        .copied()
        .ok_or_else(|| format!("incident capsule floor {floor} is absent from its topology"))
}

fn sanitize_action(
    action: &mut CapsuleAction,
    sources: &BTreeMap<String, String>,
    sessions: &BTreeMap<(String, String), String>,
    cars: &BTreeMap<String, String>,
    floors: &BTreeMap<i16, i16>,
) -> Result<(), String> {
    match action {
        CapsuleAction::BeginSourceSession { source, session }
        | CapsuleAction::InvalidateSource { source, session } => {
            let sanitized_source = mapped_identity(sources, source, "source")?;
            let sanitized_session = mapped_session(sessions, source, session)?;
            *source = sanitized_source;
            *session = sanitized_session;
        }
        CapsuleAction::Observation { update } => {
            let sanitized_source = mapped_identity(sources, &update.source, "source")?;
            let sanitized_session = mapped_session(sessions, &update.source, &update.session)?;
            update.source = sanitized_source;
            update.session = sanitized_session;
            update.car = mapped_identity(cars, &update.car, "car")?;
            if let Some(position) = update.position.as_mut() {
                match position {
                    CapsulePositionEvidence::Exact {
                        floor_position,
                        nearest_floor,
                    } => {
                        if !floor_position.is_finite() {
                            return Err(
                                "incident capsule exact position is not finite and cannot be sanitized"
                                    .into(),
                            );
                        }
                        let old_nearest = *nearest_floor;
                        let new_nearest = mapped_floor(floors, old_nearest)?;
                        *floor_position =
                            f64::from(new_nearest) + (*floor_position - f64::from(old_nearest));
                        *nearest_floor = new_nearest;
                    }
                    CapsulePositionEvidence::FloorReference { floor } => {
                        *floor = mapped_floor(floors, *floor)?;
                    }
                    CapsulePositionEvidence::Unknown => {}
                }
            }
            if let Some(CapsuleReportedDestination::Known { floor }) =
                update.reported_destination.as_mut()
            {
                *floor = mapped_floor(floors, *floor)?;
            }
        }
    }
    Ok(())
}

fn sanitize_trigger(
    trigger: &mut CapsuleTrigger,
    sources: &BTreeMap<String, String>,
    sessions: &BTreeMap<(String, String), String>,
    cars: &BTreeMap<String, String>,
) -> Result<(), String> {
    match trigger {
        CapsuleTrigger::ObservationSequenceNotIncreasing {
            source,
            session,
            car,
            ..
        }
        | CapsuleTrigger::ObservationUnresolved {
            source,
            session,
            car,
        } => {
            let sanitized_source = mapped_identity(sources, source, "source")?;
            let sanitized_session = mapped_session(sessions, source, session)?;
            *source = sanitized_source;
            *session = sanitized_session;
            *car = mapped_identity(cars, car, "car")?;
        }
        CapsuleTrigger::SourcePayloadStale { source, session } => {
            let sanitized_source = mapped_identity(sources, source, "source")?;
            let sanitized_session = mapped_session(sessions, source, session)?;
            *source = sanitized_source;
            *session = sanitized_session;
        }
        CapsuleTrigger::ReadinessChanged { .. } => {}
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct CapsuleContext {
    producer: String,
    topology: Vec<i16>,
    bindings: Vec<CapsuleBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sanitization: Option<CapsuleSanitization>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct CapsuleSanitization {
    profile: String,
    manual_review_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct CapsuleBinding {
    source: String,
    car: String,
    role: CapsuleBindingRole,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CapsuleBindingRole {
    Operational,
    Auxiliary,
}

impl From<ObservationBindingRole> for CapsuleBindingRole {
    fn from(role: ObservationBindingRole) -> Self {
        match role {
            ObservationBindingRole::Operational => Self::Operational,
            ObservationBindingRole::Auxiliary => Self::Auxiliary,
        }
    }
}

impl From<CapsuleBindingRole> for ObservationBindingRole {
    fn from(role: CapsuleBindingRole) -> Self {
        match role {
            CapsuleBindingRole::Operational => Self::Operational,
            CapsuleBindingRole::Auxiliary => Self::Auxiliary,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum CapsuleAction {
    BeginSourceSession { source: String, session: String },
    Observation { update: CapsuleObservationUpdate },
    InvalidateSource { source: String, session: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CapsuleObservationUpdate {
    source: String,
    session: String,
    sequence: u64,
    car: String,
    position: Option<CapsulePositionEvidence>,
    motion: Option<CapsuleMotionEvidence>,
    door: Option<CapsuleDoorEvidence>,
    mode: Option<CapsuleOperatingModeEvidence>,
    reported_destination: Option<CapsuleReportedDestination>,
}

impl From<&CarObservationUpdate> for CapsuleObservationUpdate {
    fn from(update: &CarObservationUpdate) -> Self {
        Self {
            source: update.provenance.source.as_str().to_owned(),
            session: update.provenance.session.as_str().to_owned(),
            sequence: update.provenance.sequence.value(),
            car: update.car.as_str().to_owned(),
            position: update.position.map(Into::into),
            motion: update.motion.map(Into::into),
            door: update.door.map(Into::into),
            mode: update.mode.map(Into::into),
            reported_destination: update.reported_destination.map(Into::into),
        }
    }
}

impl From<&CapsuleObservationUpdate> for CarObservationUpdate {
    fn from(update: &CapsuleObservationUpdate) -> Self {
        Self {
            provenance: ObservationProvenance {
                source: ObservationSourceId::new(&update.source),
                session: ObservationSessionId::new(&update.session),
                sequence: ObservationSequence::new(update.sequence),
            },
            car: CarId::new(&update.car),
            position: update.position.map(Into::into),
            motion: update.motion.map(Into::into),
            door: update.door.map(Into::into),
            mode: update.mode.map(Into::into),
            reported_destination: update.reported_destination.map(Into::into),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "evidence", rename_all = "snake_case")]
enum CapsulePositionEvidence {
    Exact {
        floor_position: f64,
        nearest_floor: i16,
    },
    FloorReference {
        floor: i16,
    },
    Unknown,
}

impl From<PositionEvidence> for CapsulePositionEvidence {
    fn from(evidence: PositionEvidence) -> Self {
        match evidence {
            PositionEvidence::Exact(position) => Self::Exact {
                floor_position: position.floor_position,
                nearest_floor: position.nearest_floor.index(),
            },
            PositionEvidence::FloorReference(floor) => Self::FloorReference {
                floor: floor.index(),
            },
            PositionEvidence::Unknown => Self::Unknown,
        }
    }
}

impl From<CapsulePositionEvidence> for PositionEvidence {
    fn from(evidence: CapsulePositionEvidence) -> Self {
        match evidence {
            CapsulePositionEvidence::Exact {
                floor_position,
                nearest_floor,
            } => Self::Exact(Position {
                floor_position,
                nearest_floor: Floor::new(nearest_floor),
            }),
            CapsulePositionEvidence::FloorReference { floor } => {
                Self::FloorReference(Floor::new(floor))
            }
            CapsulePositionEvidence::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "evidence", rename_all = "snake_case")]
enum CapsuleMotionEvidence {
    Stopped,
    Moving { direction: CapsuleDirection },
    Unknown,
}

impl From<MotionEvidence> for CapsuleMotionEvidence {
    fn from(evidence: MotionEvidence) -> Self {
        match evidence {
            MotionEvidence::Known(MotionState::Stopped) => Self::Stopped,
            MotionEvidence::Known(MotionState::Moving(direction)) => Self::Moving {
                direction: direction.into(),
            },
            MotionEvidence::Unknown => Self::Unknown,
        }
    }
}

impl From<CapsuleMotionEvidence> for MotionEvidence {
    fn from(evidence: CapsuleMotionEvidence) -> Self {
        match evidence {
            CapsuleMotionEvidence::Stopped => Self::Known(MotionState::Stopped),
            CapsuleMotionEvidence::Moving { direction } => {
                Self::Known(MotionState::Moving(direction.into()))
            }
            CapsuleMotionEvidence::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CapsuleDirection {
    Up,
    Down,
}

impl From<Direction> for CapsuleDirection {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::Up => Self::Up,
            Direction::Down => Self::Down,
        }
    }
}

impl From<CapsuleDirection> for Direction {
    fn from(direction: CapsuleDirection) -> Self {
        match direction {
            CapsuleDirection::Up => Self::Up,
            CapsuleDirection::Down => Self::Down,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "evidence", content = "state", rename_all = "snake_case")]
enum CapsuleDoorEvidence {
    Known(CapsuleDoorState),
    MovingUnknownDirection,
    Unknown,
}

impl From<DoorEvidence> for CapsuleDoorEvidence {
    fn from(evidence: DoorEvidence) -> Self {
        match evidence {
            DoorEvidence::Known(state) => Self::Known(state.into()),
            DoorEvidence::MovingUnknownDirection => Self::MovingUnknownDirection,
            DoorEvidence::Unknown => Self::Unknown,
        }
    }
}

impl From<CapsuleDoorEvidence> for DoorEvidence {
    fn from(evidence: CapsuleDoorEvidence) -> Self {
        match evidence {
            CapsuleDoorEvidence::Known(state) => Self::Known(state.into()),
            CapsuleDoorEvidence::MovingUnknownDirection => Self::MovingUnknownDirection,
            CapsuleDoorEvidence::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CapsuleDoorState {
    Closed,
    Opening,
    Open,
    Closing,
    Obstructed,
    Fault,
}

impl From<DoorState> for CapsuleDoorState {
    fn from(state: DoorState) -> Self {
        match state {
            DoorState::Closed => Self::Closed,
            DoorState::Opening => Self::Opening,
            DoorState::Open => Self::Open,
            DoorState::Closing => Self::Closing,
            DoorState::Obstructed => Self::Obstructed,
            DoorState::Fault => Self::Fault,
        }
    }
}

impl From<CapsuleDoorState> for DoorState {
    fn from(state: CapsuleDoorState) -> Self {
        match state {
            CapsuleDoorState::Closed => Self::Closed,
            CapsuleDoorState::Opening => Self::Opening,
            CapsuleDoorState::Open => Self::Open,
            CapsuleDoorState::Closing => Self::Closing,
            CapsuleDoorState::Obstructed => Self::Obstructed,
            CapsuleDoorState::Fault => Self::Fault,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "evidence", content = "mode", rename_all = "snake_case")]
enum CapsuleOperatingModeEvidence {
    Known(CapsuleOperatingMode),
    Unknown,
    SourceSpecific,
}

impl From<OperatingModeEvidence> for CapsuleOperatingModeEvidence {
    fn from(evidence: OperatingModeEvidence) -> Self {
        match evidence {
            OperatingModeEvidence::Known(mode) => Self::Known(mode.into()),
            OperatingModeEvidence::Unknown => Self::Unknown,
            OperatingModeEvidence::SourceSpecific => Self::SourceSpecific,
        }
    }
}

impl From<CapsuleOperatingModeEvidence> for OperatingModeEvidence {
    fn from(evidence: CapsuleOperatingModeEvidence) -> Self {
        match evidence {
            CapsuleOperatingModeEvidence::Known(mode) => Self::Known(mode.into()),
            CapsuleOperatingModeEvidence::Unknown => Self::Unknown,
            CapsuleOperatingModeEvidence::SourceSpecific => Self::SourceSpecific,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CapsuleOperatingMode {
    Normal,
    Independent,
    Inspection,
    FireService,
    OutOfService,
}

impl From<OperatingMode> for CapsuleOperatingMode {
    fn from(mode: OperatingMode) -> Self {
        match mode {
            OperatingMode::Normal => Self::Normal,
            OperatingMode::Independent => Self::Independent,
            OperatingMode::Inspection => Self::Inspection,
            OperatingMode::FireService => Self::FireService,
            OperatingMode::OutOfService => Self::OutOfService,
        }
    }
}

impl From<CapsuleOperatingMode> for OperatingMode {
    fn from(mode: CapsuleOperatingMode) -> Self {
        match mode {
            CapsuleOperatingMode::Normal => Self::Normal,
            CapsuleOperatingMode::Independent => Self::Independent,
            CapsuleOperatingMode::Inspection => Self::Inspection,
            CapsuleOperatingMode::FireService => Self::FireService,
            CapsuleOperatingMode::OutOfService => Self::OutOfService,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "evidence", rename_all = "snake_case")]
enum CapsuleReportedDestination {
    Known { floor: i16 },
    None,
    Unknown,
}

impl From<ReportedDestination> for CapsuleReportedDestination {
    fn from(destination: ReportedDestination) -> Self {
        match destination {
            ReportedDestination::Known(floor) => Self::Known {
                floor: floor.index(),
            },
            ReportedDestination::None => Self::None,
            ReportedDestination::Unknown => Self::Unknown,
        }
    }
}

impl From<CapsuleReportedDestination> for ReportedDestination {
    fn from(destination: CapsuleReportedDestination) -> Self {
        match destination {
            CapsuleReportedDestination::Known { floor } => Self::Known(Floor::new(floor)),
            CapsuleReportedDestination::None => Self::None,
            CapsuleReportedDestination::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "trigger", rename_all = "snake_case")]
pub(super) enum CapsuleTrigger {
    ObservationSequenceNotIncreasing {
        source: String,
        session: String,
        car: String,
        role: CapsuleBindingRole,
        previous: u64,
        received: u64,
    },
    SourcePayloadStale {
        source: String,
        session: String,
    },
    ObservationUnresolved {
        source: String,
        session: String,
        car: String,
    },
    ReadinessChanged {
        lifecycle: CapsuleLifecycle,
    },
}

impl CapsuleTrigger {
    pub(super) fn source_payload_stale(
        source: &ObservationSourceId,
        session: &ObservationSessionId,
    ) -> Self {
        Self::SourcePayloadStale {
            source: source.as_str().to_owned(),
            session: session.as_str().to_owned(),
        }
    }

    pub(super) fn observation_sequence_not_increasing(
        car: &CarId,
        role: ObservationBindingRole,
        anomaly: &DiagnosticAnomaly,
    ) -> Result<Self, String> {
        let DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source,
            session,
            previous,
            received,
        } = anomaly
        else {
            return Err("incident trigger is not an observation sequence anomaly".into());
        };
        Ok(Self::ObservationSequenceNotIncreasing {
            source: source.as_str().to_owned(),
            session: session.as_str().to_owned(),
            car: car.as_str().to_owned(),
            role: role.into(),
            previous: previous.value(),
            received: received.value(),
        })
    }

    pub(super) const fn id(&self) -> &'static str {
        match self {
            Self::ObservationSequenceNotIncreasing { .. } => "observation.sequence-not-increasing",
            Self::SourcePayloadStale { .. } => "source.payload-stale",
            Self::ObservationUnresolved { .. } => "observation.unresolved",
            Self::ReadinessChanged { .. } => "readiness.changed",
        }
    }

    fn is_reproduced(&self, host: &ReadOnlyHost) -> bool {
        match self {
            Self::ObservationSequenceNotIncreasing {
                source,
                session,
                car,
                role,
                previous,
                received,
            } => {
                let anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
                    source: ObservationSourceId::new(source),
                    session: ObservationSessionId::new(session),
                    previous: ObservationSequence::new(*previous),
                    received: ObservationSequence::new(*received),
                };
                let engine_has_anomaly = host.engine.diagnostics().entries().any(|entry| {
                    matches!(&entry.event, DiagnosticEvent::Anomaly(observed) if observed == &anomaly)
                }) || host
                    .engine
                    .diagnostics()
                    .incidents()
                    .any(|incident| incident.trigger == anomaly);
                engine_has_anomaly
                    && host.diagnostics().entries().any(|entry| {
                        entry.id == HostDiagnosticId::ObservationRejected
                            && entry.context.source.as_ref()
                                == Some(&ObservationSourceId::new(source))
                            && entry.context.session.as_ref()
                                == Some(&ObservationSessionId::new(session))
                            && entry.context.car.as_ref() == Some(&CarId::new(car))
                            && entry.context.role == Some((*role).into())
                    })
            }
            Self::SourcePayloadStale { source, session } => {
                host.diagnostics().entries().any(|entry| {
                    entry.id == HostDiagnosticId::SourcePayloadStale
                        && entry.context.source.as_ref() == Some(&ObservationSourceId::new(source))
                        && entry.context.session.as_ref()
                            == Some(&ObservationSessionId::new(session))
                })
            }
            Self::ObservationUnresolved {
                source,
                session,
                car,
            } => host.diagnostics().entries().any(|entry| {
                entry.id == HostDiagnosticId::ObservationUnresolved
                    && entry.context.source.as_ref() == Some(&ObservationSourceId::new(source))
                    && entry.context.session.as_ref() == Some(&ObservationSessionId::new(session))
                    && entry.context.car.as_ref() == Some(&CarId::new(car))
            }),
            Self::ReadinessChanged { lifecycle } => host.diagnostics().entries().any(|entry| {
                matches!(
                    &entry.event,
                    HostEvent::LifecycleChanged(observed)
                        if CapsuleLifecycle::from(observed) == *lifecycle
                )
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CapsuleLifecycle {
    Starting,
    Synchronizing,
    ReadyReadOnly,
    Degraded,
}

impl From<&HostLifecycle> for CapsuleLifecycle {
    fn from(lifecycle: &HostLifecycle) -> Self {
        match lifecycle {
            HostLifecycle::Starting => Self::Starting,
            HostLifecycle::Synchronizing => Self::Synchronizing,
            HostLifecycle::ReadyReadOnly => Self::ReadyReadOnly,
            HostLifecycle::Degraded(_) => Self::Degraded,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CapsuleReadiness {
    NotReady,
    ReadOnly,
}

impl From<Readiness> for CapsuleReadiness {
    fn from(readiness: Readiness) -> Self {
        match readiness {
            Readiness::NotReady => Self::NotReady,
            Readiness::ReadOnly => Self::ReadOnly,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct IncidentReplayReport {
    pub(super) version: String,
    pub(super) trigger_id: &'static str,
    pub(super) action_count: usize,
    lifecycle: CapsuleLifecycle,
    readiness: CapsuleReadiness,
}

impl IncidentReplayReport {
    pub(super) fn render(&self) -> String {
        format!(
            "incident replay: PASS version={} trigger={} actions={} lifecycle={:?} readiness={:?}",
            self.version, self.trigger_id, self.action_count, self.lifecycle, self.readiness
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SanitizationReport {
    source_count: usize,
    session_count: usize,
    car_count: usize,
    floor_count: usize,
    pub(super) manual_review_required: bool,
}

impl SanitizationReport {
    pub(super) fn render(&self) -> String {
        format!(
            "sanitization: PASS profile=deterministic-pseudonymization/1 sources={} sessions={} cars={} floors={} manual_review=REQUIRED",
            self.source_count, self.session_count, self.car_count, self.floor_count
        )
    }
}
