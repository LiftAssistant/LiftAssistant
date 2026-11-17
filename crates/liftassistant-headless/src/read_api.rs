//! Experimental read-only API projection.
//!
//! These wire DTOs belong to the host/client boundary. They deliberately do not add serde to
//! canonical model types and do not serialize internal HostEvent/EngineEvent values.

use liftassistant_engine::ObservationBindingRole;
use liftassistant_model::{
    CarLocation, Direction, DoorState, MotionState, OperatingMode, ReportedDestination,
};
use serde::Serialize;

use super::{
    HostDiagnosticEntry, HostDiagnosticLayer, HostLifecycle, ObservationAvailability,
    PayloadActivity, ReadOnlyHost, Readiness, SourceSessionActivity, TransportDiscoveryState,
};

pub(super) const EXPERIMENTAL_API_VERSION: &str = "experimental/v0";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct ExperimentalReadModel {
    api_version: String,
    revision: u64,
    lifecycle: WireLifecycle,
    readiness: WireReadiness,
    operational_cars: Vec<WireCar>,
    sources: Vec<WireSource>,
    recent_diagnostics: Vec<WireDiagnostic>,
}

impl ExperimentalReadModel {
    pub(super) fn project(host: &ReadOnlyHost) -> Self {
        let operational_cars = host.engine.car_states().iter().map(WireCar::from).collect();
        let sources = host
            .source_health
            .iter()
            .map(|(source, health)| WireSource {
                source: source.as_str().to_owned(),
                session: WireSourceSession::from(&health.session),
                transport: WireTransportState::from(health.transport_discovery),
                payload: WirePayloadActivity::from(health.payload),
                observations: health
                    .observations
                    .iter()
                    .map(|(car, availability)| WireSourceObservation {
                        car: car.as_str().to_owned(),
                        role: host
                            .engine
                            .observation_binding_role(source, car)
                            .map(WireBindingRole::from)
                            .expect("source health exists only for explicitly bound cars"),
                        availability: WireObservationAvailability::from(*availability),
                    })
                    .collect(),
            })
            .collect();
        let recent_diagnostics = host
            .diagnostics
            .entries()
            .map(WireDiagnostic::from)
            .collect();

        Self {
            api_version: EXPERIMENTAL_API_VERSION.to_owned(),
            revision: host.read_model_revision,
            lifecycle: WireLifecycle::from(&host.lifecycle),
            readiness: WireReadiness::from(host.readiness()),
            operational_cars,
            sources,
            recent_diagnostics,
        }
    }

    pub(super) const fn revision(&self) -> u64 {
        self.revision
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

impl From<&HostLifecycle> for WireLifecycle {
    fn from(lifecycle: &HostLifecycle) -> Self {
        match lifecycle {
            HostLifecycle::Starting => Self::Starting,
            HostLifecycle::Synchronizing => Self::Synchronizing,
            HostLifecycle::ReadyReadOnly => Self::ReadyReadOnly,
            HostLifecycle::Degraded(_) => Self::Degraded,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireReadiness {
    NotReady,
    ReadOnly,
}

impl From<Readiness> for WireReadiness {
    fn from(readiness: Readiness) -> Self {
        match readiness {
            Readiness::NotReady => Self::NotReady,
            Readiness::ReadOnly => Self::ReadOnly,
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
            location: WireLocation::from(state.location),
            motion: WireMotion::from(state.motion),
            door: WireDoorState::from(state.door),
            operating_mode: WireOperatingMode::from(state.mode),
            reported_destination: WireReportedDestination::from(state.reported_destination),
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

impl From<CarLocation> for WireLocation {
    fn from(location: CarLocation) -> Self {
        match location {
            CarLocation::Floor(floor) => Self::Floor {
                floor: floor.index(),
            },
            CarLocation::Continuous(position) => Self::Continuous {
                floor_position: position.floor_position,
                nearest_floor: position.nearest_floor.index(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum WireMotion {
    Stopped,
    Moving { direction: WireDirection },
}

impl From<MotionState> for WireMotion {
    fn from(motion: MotionState) -> Self {
        match motion {
            MotionState::Stopped => Self::Stopped,
            MotionState::Moving(direction) => Self::Moving {
                direction: WireDirection::from(direction),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireDirection {
    Up,
    Down,
}

impl From<Direction> for WireDirection {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::Up => Self::Up,
            Direction::Down => Self::Down,
        }
    }
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

impl From<DoorState> for WireDoorState {
    fn from(door: DoorState) -> Self {
        match door {
            DoorState::Closed => Self::Closed,
            DoorState::Opening => Self::Opening,
            DoorState::Open => Self::Open,
            DoorState::Closing => Self::Closing,
            DoorState::Obstructed => Self::Obstructed,
            DoorState::Fault => Self::Fault,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireOperatingMode {
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

impl From<ReportedDestination> for WireReportedDestination {
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

impl From<&SourceSessionActivity> for WireSourceSession {
    fn from(session: &SourceSessionActivity) -> Self {
        match session {
            SourceSessionActivity::Inactive => Self::Inactive,
            SourceSessionActivity::Active(session) => Self::Active {
                session: session.as_str().to_owned(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireTransportState {
    Unknown,
    SubscriberCreated,
    DiscoveryObserved,
}

impl From<TransportDiscoveryState> for WireTransportState {
    fn from(transport: TransportDiscoveryState) -> Self {
        match transport {
            TransportDiscoveryState::Unknown => Self::Unknown,
            TransportDiscoveryState::SubscriberCreated => Self::SubscriberCreated,
            TransportDiscoveryState::DiscoveryObserved => Self::DiscoveryObserved,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WirePayloadActivity {
    Awaiting,
    Active,
    Stale,
}

impl From<PayloadActivity> for WirePayloadActivity {
    fn from(payload: PayloadActivity) -> Self {
        match payload {
            PayloadActivity::Awaiting => Self::Awaiting,
            PayloadActivity::Active => Self::Active,
            PayloadActivity::Stale => Self::Stale,
        }
    }
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
    fn from(availability: ObservationAvailability) -> Self {
        match availability {
            ObservationAvailability::Awaiting => Self::Awaiting,
            ObservationAvailability::Incomplete => Self::Incomplete,
            ObservationAvailability::Unresolved => Self::Unresolved,
            ObservationAvailability::Resolved => Self::Resolved,
            ObservationAvailability::Revoked => Self::Revoked,
        }
    }
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

impl From<&HostDiagnosticEntry> for WireDiagnostic {
    fn from(entry: &HostDiagnosticEntry) -> Self {
        Self {
            sequence: entry.sequence,
            layer: WireDiagnosticLayer::from(entry.layer),
            id: entry.id.as_str().to_owned(),
            source: entry
                .context
                .source
                .as_ref()
                .map(|source| source.as_str().to_owned()),
            session: entry
                .context
                .session
                .as_ref()
                .map(|session| session.as_str().to_owned()),
            car: entry
                .context
                .car
                .as_ref()
                .map(|car| car.as_str().to_owned()),
            role: entry.context.role.map(WireBindingRole::from),
            native_entity: entry.context.native_entity.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum WireDiagnosticLayer {
    Transport,
    Source,
    Mapping,
    Observation,
    Readiness,
}

impl From<HostDiagnosticLayer> for WireDiagnosticLayer {
    fn from(layer: HostDiagnosticLayer) -> Self {
        match layer {
            HostDiagnosticLayer::Transport => Self::Transport,
            HostDiagnosticLayer::Source => Self::Source,
            HostDiagnosticLayer::Mapping => Self::Mapping,
            HostDiagnosticLayer::Observation => Self::Observation,
            HostDiagnosticLayer::Readiness => Self::Readiness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_engine::ObservationBindingRole;
    use liftassistant_model::{
        BuildingTopology, CarId, CarState, DoorState, Floor, MotionState, ObservationProvenance,
        ObservationSequence, ObservationSessionId, ObservationSourceId, OperatingMode, Position,
        ReportedDestination,
    };
    use serde_json::{Value, json};

    fn provenance(sequence: u64) -> ObservationProvenance {
        ObservationProvenance {
            source: ObservationSourceId::new("fixture/read-api"),
            session: ObservationSessionId::new("read-api-session-A"),
            sequence: ObservationSequence::new(sequence),
        }
    }

    #[test]
    fn projection_preserves_location_destination_and_state_distinctions() {
        let source = ObservationSourceId::new("fixture/read-api");
        let session = ObservationSessionId::new("read-api-session-A");
        let mut host = ReadOnlyHost::new(
            BuildingTopology::new([Floor::new(1), Floor::new(2), Floor::new(3), Floor::new(4)])
                .expect("valid topology"),
        );
        for car in ["floor-car", "continuous-car", "unknown-destination-car"] {
            host.bind_observation(
                source.clone(),
                CarId::new(car),
                ObservationBindingRole::Operational,
            )
            .expect("explicit binding is valid");
        }
        host.begin_source_session(source.clone(), session)
            .expect("source session starts explicitly");

        let mut floor = CarState::new("floor-car", Floor::new(3));
        floor.reported_destination = ReportedDestination::Known(Floor::new(4));
        host.ingest_observation(liftassistant_model::CarObservationUpdate::from_state(
            provenance(1),
            &floor,
        ));

        let mut continuous = CarState::with_continuous_position(
            "continuous-car",
            Position {
                floor_position: 2.25,
                nearest_floor: Floor::new(2),
            },
        );
        continuous.motion = MotionState::Moving(Direction::Up);
        continuous.door = DoorState::Opening;
        continuous.mode = OperatingMode::Inspection;
        continuous.reported_destination = ReportedDestination::None;
        host.ingest_observation(liftassistant_model::CarObservationUpdate::from_state(
            provenance(2),
            &continuous,
        ));

        let mut unknown = CarState::new("unknown-destination-car", Floor::new(1));
        unknown.door = DoorState::Fault;
        unknown.mode = OperatingMode::OutOfService;
        unknown.reported_destination = ReportedDestination::Unknown;
        host.ingest_observation(liftassistant_model::CarObservationUpdate::from_state(
            provenance(3),
            &unknown,
        ));

        let value = serde_json::to_value(host.read_model()).expect("read model serializes");
        assert_eq!(value["api_version"], EXPERIMENTAL_API_VERSION);
        assert_eq!(value["lifecycle"], "ready_read_only");
        assert_eq!(value["readiness"], "read_only");
        assert_eq!(
            value["operational_cars"][0]["location"],
            json!({"kind": "continuous", "floor_position": 2.25, "nearest_floor": 2})
        );
        assert_eq!(
            value["operational_cars"][0]["reported_destination"],
            json!({"kind": "none"})
        );
        assert_eq!(
            value["operational_cars"][1]["location"],
            json!({"kind": "floor", "floor": 3})
        );
        assert_eq!(
            value["operational_cars"][1]["reported_destination"],
            json!({"kind": "known", "floor": 4})
        );
        assert_eq!(
            value["operational_cars"][2]["reported_destination"],
            json!({"kind": "unknown"})
        );
        assert_eq!(value["sources"][0]["session"]["state"], "active");
        assert_eq!(
            value["sources"][0]["session"]["session"],
            "read-api-session-A"
        );
        assert_eq!(
            value["sources"][0]["observations"][0]["role"],
            "operational"
        );
        assert!(
            value["recent_diagnostics"]
                .as_array()
                .is_some_and(|entries| !entries.is_empty())
        );
    }

    #[test]
    fn revision_is_host_local_and_advances_with_projected_changes() {
        let source = ObservationSourceId::new("fixture/revision");
        let session = ObservationSessionId::new("revision-session-A");
        let mut host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());
        let initial = host.read_model().revision();

        host.bind_observation(
            source.clone(),
            CarId::new("A"),
            ObservationBindingRole::Operational,
        )
        .expect("binding succeeds");
        let after_binding = host.read_model().revision();
        host.begin_source_session(source.clone(), session.clone())
            .expect("session starts");
        let after_session = host.read_model().revision();
        host.ingest_observation(liftassistant_model::CarObservationUpdate::from_state(
            ObservationProvenance {
                source,
                session,
                sequence: ObservationSequence::new(99),
            },
            &CarState::new("A", Floor::new(3)),
        ));
        let after_observation = host.read_model().revision();

        assert_eq!(initial, 1);
        assert!(initial < after_binding);
        assert!(after_binding < after_session);
        assert!(after_session < after_observation);
        assert_ne!(after_observation, 99);
        assert_eq!(
            ReadOnlyHost::new(BuildingTopology::reference_eight_floor())
                .read_model()
                .revision(),
            1
        );
    }

    #[test]
    fn diagnostics_are_projected_without_serializing_internal_events() {
        let mut host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());
        let source = ObservationSourceId::new("fixture/diagnostic");
        let session = ObservationSessionId::new("diagnostic-session-A");
        host.bind_observation(
            source.clone(),
            CarId::new("A"),
            ObservationBindingRole::Auxiliary,
        )
        .expect("binding succeeds");
        host.begin_source_session(source.clone(), session.clone())
            .expect("session starts");
        host.record_mapping_rejection(
            source,
            session,
            "native-lift".into(),
            "source-specific mapping evidence".into(),
        );

        let value = serde_json::to_value(host.read_model()).expect("read model serializes");
        let diagnostics = value["recent_diagnostics"]
            .as_array()
            .expect("diagnostics are an array");
        let mapping = diagnostics
            .iter()
            .find(|entry| entry["id"] == "mapping.rejected")
            .expect("mapping diagnostic is projected");
        assert_eq!(mapping["layer"], "mapping");
        assert_eq!(mapping["native_entity"], "native-lift");
        assert_eq!(mapping["role"], Value::Null);
        assert!(mapping.get("event").is_none());
        assert!(mapping.get("error").is_none());
        assert!(mapping.get("message").is_none());
    }
}
