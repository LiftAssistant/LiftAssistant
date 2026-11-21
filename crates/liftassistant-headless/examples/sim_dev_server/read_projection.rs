//! Read-only `experimental/v0` snapshot projection for the isolated Simulator dev server.
//!
//! Mirrors the exact wire shape of `liftassistant-headless`'s read-only API so existing
//! read-only clients (CLI and Flutter) consume live Simulator observations without any
//! schema mutation or write semantics on `/api/experimental/v0/*`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use liftassistant_engine::{
    Engine, ObservationBindingError, ObservationBindingRole, ObservationIngestResult,
    SourceObservationInvalidationError, SourceSessionError,
};
use liftassistant_model::{
    BuildingTopology, CarId, CarLocation, CarObservationUpdate, Direction, DoorState, MotionState,
    ObservationSessionId, ObservationSourceId, OperatingMode, ReportedDestination,
};
use liftassistant_runtime::CarStateResolution;
use serde::Serialize;

pub(crate) const EXPERIMENTAL_API_VERSION: &str = "experimental/v0";
const DIAGNOSTIC_CAPACITY: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SimHostLifecycle {
    Starting,
    Synchronizing,
    ReadyReadOnly,
    Degraded,
}

impl SimHostLifecycle {
    fn readiness(self) -> SimReadiness {
        match self {
            Self::ReadyReadOnly => SimReadiness::ReadOnly,
            Self::Starting | Self::Synchronizing | Self::Degraded => SimReadiness::NotReady,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SimReadiness {
    NotReady,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransportState {
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
pub(crate) enum ObservationAvailability {
    Awaiting,
    Incomplete,
    Unresolved,
    Resolved,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceHealth {
    transport: TransportState,
    payload: PayloadActivity,
    session: SourceSessionActivity,
    observations: BTreeMap<CarId, ObservationAvailability>,
}

impl SourceHealth {
    fn new() -> Self {
        Self {
            transport: TransportState::DiscoveryObserved,
            payload: PayloadActivity::Awaiting,
            session: SourceSessionActivity::Inactive,
            observations: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireDiagnosticLayer {
    Source,
    Observation,
    Readiness,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct WireDiagnostic {
    sequence: u64,
    layer: WireDiagnosticLayer,
    id: String,
    source: Option<String>,
    session: Option<String>,
    car: Option<String>,
    role: Option<WireBindingRole>,
    native_entity: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SimReadModel {
    api_version: String,
    revision: u64,
    lifecycle: WireLifecycle,
    readiness: WireReadiness,
    operational_cars: Vec<WireCar>,
    sources: Vec<WireSource>,
    recent_diagnostics: Vec<WireDiagnostic>,
}

impl SimReadModel {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }
}

#[derive(Debug)]
pub(crate) struct SimReadHost {
    engine: Engine,
    required_operational_cars: BTreeSet<CarId>,
    source_health: BTreeMap<ObservationSourceId, SourceHealth>,
    lifecycle: SimHostLifecycle,
    next_diag_sequence: u64,
    diagnostics: VecDeque<WireDiagnostic>,
    revision: u64,
}

impl SimReadHost {
    pub(crate) fn new(topology: BuildingTopology) -> Self {
        let mut host = Self {
            engine: Engine::new(topology),
            required_operational_cars: BTreeSet::new(),
            source_health: BTreeMap::new(),
            lifecycle: SimHostLifecycle::Starting,
            next_diag_sequence: 1,
            diagnostics: VecDeque::new(),
            revision: 1,
        };
        host.push_diag(
            WireDiagnosticLayer::Readiness,
            "readiness.changed",
            None,
            None,
            None,
            None,
        );
        host
    }

    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }

    pub(crate) fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub(crate) fn configured_cars(&self) -> BTreeSet<CarId> {
        let mut cars = self.required_operational_cars.clone();
        for health in self.source_health.values() {
            for car in health.observations.keys() {
                if self.engine.controlled_controller(car).is_some() {
                    cars.insert(car.clone());
                }
            }
        }
        cars
    }

    pub(crate) fn bind_observation(
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
        self.bump_revision();
        Ok(())
    }

    pub(crate) fn begin_source_session(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
    ) -> Result<(), SourceSessionError> {
        let transition = self.engine.begin_source_session(source, session)?;
        let health = self
            .source_health
            .get_mut(&transition.source)
            .expect("bound source has health entry");
        health.session = SourceSessionActivity::Active(transition.active_session.clone());
        health.payload = PayloadActivity::Awaiting;
        for availability in health.observations.values_mut() {
            *availability = ObservationAvailability::Awaiting;
        }
        self.push_diag(
            WireDiagnosticLayer::Source,
            "source.session-activated",
            Some(&transition.source),
            Some(&transition.active_session),
            None,
            None,
        );
        let invalidated_operational = transition.invalidated_cars.iter().any(|car| {
            self.engine
                .observation_binding_role(&transition.source, car)
                == Some(ObservationBindingRole::Operational)
        });
        if invalidated_operational {
            self.transition(SimHostLifecycle::Synchronizing);
        }
        self.bump_revision();
        Ok(())
    }

    pub(crate) fn ingest_observation(&mut self, observation: CarObservationUpdate) {
        let source = observation.provenance.source.clone();
        let session = observation.provenance.session.clone();
        let car = observation.car.clone();
        let role = self.engine.observation_binding_role(&source, &car);
        let is_operational = role == Some(ObservationBindingRole::Operational);

        if is_operational
            && matches!(
                self.lifecycle,
                SimHostLifecycle::Starting | SimHostLifecycle::Degraded
            )
        {
            self.transition(SimHostLifecycle::Synchronizing);
        }

        let result = self.engine.ingest_observation_update(observation);
        let availability = match &result {
            ObservationIngestResult::Incomplete(_) => Some(ObservationAvailability::Incomplete),
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(_)) => {
                Some(ObservationAvailability::Resolved)
            }
            ObservationIngestResult::Resolution(CarStateResolution::Unresolved { .. }) => {
                Some(ObservationAvailability::Unresolved)
            }
            ObservationIngestResult::Rejected(_) => None,
        };
        if let Some((availability, health)) = availability.zip(self.source_health.get_mut(&source))
        {
            health.payload = PayloadActivity::Active;
            health.observations.insert(car.clone(), availability);
        }

        if is_operational {
            match result {
                ObservationIngestResult::Resolution(CarStateResolution::Resolved(_)) => {
                    self.push_diag(
                        WireDiagnosticLayer::Observation,
                        "observation.resolved",
                        Some(&source),
                        Some(&session),
                        Some(&car),
                        role,
                    );
                    let all_resolved = self
                        .required_operational_cars
                        .iter()
                        .all(|candidate| self.engine.operational_car_state(candidate).is_some());
                    if all_resolved {
                        self.transition(SimHostLifecycle::ReadyReadOnly);
                    } else {
                        self.transition(SimHostLifecycle::Synchronizing);
                    }
                }
                ObservationIngestResult::Resolution(CarStateResolution::Unresolved { .. }) => {
                    self.push_diag(
                        WireDiagnosticLayer::Observation,
                        "observation.unresolved",
                        Some(&source),
                        Some(&session),
                        Some(&car),
                        role,
                    );
                    self.transition(SimHostLifecycle::Synchronizing);
                }
                ObservationIngestResult::Incomplete(_) => {
                    self.push_diag(
                        WireDiagnosticLayer::Observation,
                        "observation.incomplete",
                        Some(&source),
                        Some(&session),
                        Some(&car),
                        role,
                    );
                    self.transition(SimHostLifecycle::Synchronizing);
                }
                ObservationIngestResult::Rejected(_) => {
                    self.push_diag(
                        WireDiagnosticLayer::Observation,
                        "observation.rejected",
                        Some(&source),
                        Some(&session),
                        Some(&car),
                        role,
                    );
                    self.transition(SimHostLifecycle::Degraded);
                }
            }
        } else {
            let diag_id = match result {
                ObservationIngestResult::Rejected(_) => "observation.rejected",
                _ => "observation.auxiliary-accepted",
            };
            self.push_diag(
                WireDiagnosticLayer::Observation,
                diag_id,
                Some(&source),
                Some(&session),
                Some(&car),
                role,
            );
        }

        self.bump_revision();
    }

    pub(crate) fn revoke_car_observations(
        &mut self,
        source: &ObservationSourceId,
        session: &ObservationSessionId,
        car: &CarId,
    ) -> Result<(), SourceObservationInvalidationError> {
        let role = self
            .engine
            .invalidate_car_observations(source, session, car)?;
        if let Some(health) = self.source_health.get_mut(source) {
            health
                .observations
                .insert(car.clone(), ObservationAvailability::Revoked);
            if health
                .observations
                .values()
                .all(|state| *state == ObservationAvailability::Revoked)
            {
                health.payload = PayloadActivity::Stale;
            }
        }
        self.push_diag(
            WireDiagnosticLayer::Observation,
            "observation.revoked",
            Some(source),
            Some(session),
            Some(car),
            Some(role),
        );
        if role == ObservationBindingRole::Operational {
            self.transition(SimHostLifecycle::Synchronizing);
        }
        self.bump_revision();
        Ok(())
    }

    pub(crate) fn read_model(&self) -> SimReadModel {
        let operational_cars = self.engine.car_states().iter().map(WireCar::from).collect();
        let sources = self
            .source_health
            .iter()
            .map(|(source, health)| WireSource {
                source: source.as_str().to_owned(),
                session: match &health.session {
                    SourceSessionActivity::Inactive => WireSourceSession::Inactive,
                    SourceSessionActivity::Active(session) => WireSourceSession::Active {
                        session: session.as_str().to_owned(),
                    },
                },
                transport: match health.transport {
                    TransportState::DiscoveryObserved => WireTransportState::DiscoveryObserved,
                },
                payload: match health.payload {
                    PayloadActivity::Awaiting => WirePayloadActivity::Awaiting,
                    PayloadActivity::Active => WirePayloadActivity::Active,
                    PayloadActivity::Stale => WirePayloadActivity::Stale,
                },
                observations: health
                    .observations
                    .iter()
                    .map(|(car, availability)| WireSourceObservation {
                        car: car.as_str().to_owned(),
                        role: self
                            .engine
                            .observation_binding_role(source, car)
                            .map(WireBindingRole::from)
                            .expect("health entry exists only for bound cars"),
                        availability: WireObservationAvailability::from(*availability),
                    })
                    .collect(),
            })
            .collect();

        SimReadModel {
            api_version: EXPERIMENTAL_API_VERSION.to_owned(),
            revision: self.revision,
            lifecycle: WireLifecycle::from(self.lifecycle),
            readiness: WireReadiness::from(self.lifecycle.readiness()),
            operational_cars,
            sources,
            recent_diagnostics: self.diagnostics.iter().cloned().collect(),
        }
    }

    fn transition(&mut self, next: SimHostLifecycle) {
        if self.lifecycle != next {
            self.lifecycle = next;
            self.push_diag(
                WireDiagnosticLayer::Readiness,
                "readiness.changed",
                None,
                None,
                None,
                None,
            );
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("read-model revision exhausted");
    }

    fn push_diag(
        &mut self,
        layer: WireDiagnosticLayer,
        id: &str,
        source: Option<&ObservationSourceId>,
        session: Option<&ObservationSessionId>,
        car: Option<&CarId>,
        role: Option<ObservationBindingRole>,
    ) {
        let entry = WireDiagnostic {
            sequence: self.next_diag_sequence,
            layer,
            id: id.to_owned(),
            source: source.map(|value| value.as_str().to_owned()),
            session: session.map(|value| value.as_str().to_owned()),
            car: car.map(|value| value.as_str().to_owned()),
            role: role.map(WireBindingRole::from),
            native_entity: None,
        };
        self.next_diag_sequence = self
            .next_diag_sequence
            .checked_add(1)
            .expect("diagnostic sequence exhausted");
        if self.diagnostics.len() == DIAGNOSTIC_CAPACITY {
            self.diagnostics.pop_front();
        }
        self.diagnostics.push_back(entry);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireLifecycle {
    Starting,
    Synchronizing,
    ReadyReadOnly,
    Degraded,
}

impl From<SimHostLifecycle> for WireLifecycle {
    fn from(lifecycle: SimHostLifecycle) -> Self {
        match lifecycle {
            SimHostLifecycle::Starting => Self::Starting,
            SimHostLifecycle::Synchronizing => Self::Synchronizing,
            SimHostLifecycle::ReadyReadOnly => Self::ReadyReadOnly,
            SimHostLifecycle::Degraded => Self::Degraded,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireReadiness {
    NotReady,
    ReadOnly,
}

impl From<SimReadiness> for WireReadiness {
    fn from(readiness: SimReadiness) -> Self {
        match readiness {
            SimReadiness::NotReady => Self::NotReady,
            SimReadiness::ReadOnly => Self::ReadOnly,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct WireCar {
    id: String,
    location: WireLocation,
    motion: WireMotion,
    door: WireDoorState,
    operating_mode: WireOperatingMode,
    reported_destination: WireReportedDestination,
}

impl From<&liftassistant_model::CarState> for WireCar {
    fn from(state: &liftassistant_model::CarState) -> Self {
        Self {
            id: state.id.as_str().to_owned(),
            location: match state.location {
                CarLocation::Floor(floor) => WireLocation::Floor {
                    floor: floor.index(),
                },
                CarLocation::Continuous(position) => WireLocation::Continuous {
                    floor_position: position.floor_position,
                    nearest_floor: position.nearest_floor.index(),
                },
            },
            motion: match state.motion {
                MotionState::Stopped => WireMotion::Stopped,
                MotionState::Moving(direction) => WireMotion::Moving {
                    direction: match direction {
                        Direction::Up => WireDirection::Up,
                        Direction::Down => WireDirection::Down,
                    },
                },
            },
            door: match state.door {
                DoorState::Closed => WireDoorState::Closed,
                DoorState::Opening => WireDoorState::Opening,
                DoorState::Open => WireDoorState::Open,
                DoorState::Closing => WireDoorState::Closing,
                DoorState::Obstructed => WireDoorState::Obstructed,
                DoorState::Fault => WireDoorState::Fault,
            },
            operating_mode: WireOperatingMode::from(state.mode),
            reported_destination: match state.reported_destination {
                ReportedDestination::Known(floor) => WireReportedDestination::Known {
                    floor: floor.index(),
                },
                ReportedDestination::None => WireReportedDestination::None,
                ReportedDestination::Unknown => WireReportedDestination::Unknown,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireLocation {
    Floor {
        floor: i16,
    },
    Continuous {
        floor_position: f64,
        nearest_floor: i16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum WireMotion {
    Stopped,
    Moving { direction: WireDirection },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireDoorState {
    Closed,
    Opening,
    Open,
    Closing,
    Obstructed,
    Fault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireOperatingMode {
    Normal,
    Independent,
    Inspection,
    FireService,
    OutOfService,
}

impl From<OperatingMode> for WireOperatingMode {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireReportedDestination {
    Known { floor: i16 },
    None,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct WireSource {
    source: String,
    session: WireSourceSession,
    transport: WireTransportState,
    payload: WirePayloadActivity,
    observations: Vec<WireSourceObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum WireSourceSession {
    Inactive,
    Active { session: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireTransportState {
    DiscoveryObserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WirePayloadActivity {
    Awaiting,
    Active,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct WireSourceObservation {
    car: String,
    role: WireBindingRole,
    availability: WireObservationAvailability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireBindingRole {
    Operational,
    Auxiliary,
}

impl From<ObservationBindingRole> for WireBindingRole {
    fn from(role: ObservationBindingRole) -> Self {
        match role {
            ObservationBindingRole::Operational => Self::Operational,
            ObservationBindingRole::Auxiliary => Self::Auxiliary,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireObservationAvailability {
    Awaiting,
    Incomplete,
    Unresolved,
    Resolved,
    Revoked,
}

impl From<ObservationAvailability> for WireObservationAvailability {
    fn from(value: ObservationAvailability) -> Self {
        match value {
            ObservationAvailability::Awaiting => Self::Awaiting,
            ObservationAvailability::Incomplete => Self::Incomplete,
            ObservationAvailability::Unresolved => Self::Unresolved,
            ObservationAvailability::Resolved => Self::Resolved,
            ObservationAvailability::Revoked => Self::Revoked,
        }
    }
}
