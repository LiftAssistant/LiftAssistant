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
    actions: VecDeque<CapsuleAction>,
}

impl IncidentRecorder {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.clamp(1, MAX_CAPSULE_ACTIONS),
            bindings: BTreeMap::new(),
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
            },
            actions: self.actions.iter().cloned().collect(),
            trigger,
            expected_lifecycle: lifecycle.into(),
            expected_readiness: readiness.into(),
        }
    }

    fn push(&mut self, action: CapsuleAction) {
        if self.actions.len() == self.capacity {
            self.actions.pop_front();
        }
        self.actions.push_back(action);
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
        for binding in &self.context.bindings {
            if !bindings.insert((binding.source.as_str(), binding.car.as_str())) {
                return Err("incident capsule contains a duplicate observation binding".into());
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct CapsuleContext {
    producer: String,
    topology: Vec<i16>,
    bindings: Vec<CapsuleBinding>,
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
                host.engine.diagnostics().entries().any(|entry| {
                    matches!(&entry.event, DiagnosticEvent::Anomaly(observed) if observed == &anomaly)
                }) && host.diagnostics().entries().any(|entry| {
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
