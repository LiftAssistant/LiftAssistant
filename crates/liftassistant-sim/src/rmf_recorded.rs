use std::{collections::BTreeMap, error::Error, fmt};

use liftassistant_model::{
    BuildingTopology, CarId, CarObservation, Direction, DoorEvidence, DoorState, Floor,
    MotionEvidence, MotionState, ObservationProvenance, OperatingMode, OperatingModeEvidence,
    PositionEvidence, ReportedDestination,
};

/// Recorded fields from Open-RMF `rmf_lift_msgs/LiftState` used by this experiment.
///
/// This is an internal mapping input, not a ROS message implementation or public schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RmfLiftState {
    pub lift_name: String,
    pub available_floors: Vec<String>,
    pub current_floor: String,
    pub destination_floor: String,
    pub door_state: u8,
    pub motion_state: u8,
    pub available_modes: Vec<u8>,
    pub current_mode: u8,
    pub session_id: String,
}

impl RmfLiftState {
    pub const DOOR_CLOSED: u8 = 0;
    pub const DOOR_MOVING: u8 = 1;
    pub const DOOR_OPEN: u8 = 2;

    pub const MOTION_STOPPED: u8 = 0;
    pub const MOTION_UP: u8 = 1;
    pub const MOTION_DOWN: u8 = 2;
    pub const MOTION_UNKNOWN: u8 = 3;

    pub const MODE_UNKNOWN: u8 = 0;
    pub const MODE_HUMAN: u8 = 1;
    pub const MODE_AGV: u8 = 2;
    pub const MODE_FIRE: u8 = 3;
    pub const MODE_OFFLINE: u8 = 4;
    pub const MODE_EMERGENCY: u8 = 5;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RmfSourceContext {
    pub current_floor_label: String,
    pub destination_floor_label: String,
    pub source_mode: u8,
    pub available_source_modes: Vec<u8>,
    pub source_session_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappedRmfObservation {
    pub observation: CarObservation,
    pub topology: BuildingTopology,
    pub source_context: RmfSourceContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RmfMappingError {
    EmptyLiftName,
    NoAvailableFloors,
    UnknownFloorLabel(String),
    FloorNotAdvertised(String),
    UnknownDoorState(u8),
    UnknownMotionState(u8),
    UnknownOperatingMode(u8),
}

impl fmt::Display for RmfMappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLiftName => f.write_str("Open-RMF lift_name is empty"),
            Self::NoAvailableFloors => f.write_str("Open-RMF available_floors is empty"),
            Self::UnknownFloorLabel(label) => {
                write!(
                    f,
                    "Open-RMF floor label {label:?} has no configured mapping"
                )
            }
            Self::FloorNotAdvertised(label) => write!(
                f,
                "Open-RMF floor label {label:?} is not present in available_floors"
            ),
            Self::UnknownDoorState(value) => {
                write!(f, "unknown Open-RMF door_state {value}")
            }
            Self::UnknownMotionState(value) => {
                write!(f, "unknown Open-RMF motion_state {value}")
            }
            Self::UnknownOperatingMode(value) => {
                write!(f, "unknown Open-RMF current_mode {value}")
            }
        }
    }
}

impl Error for RmfMappingError {}

pub fn map_rmf_lift_state(
    source: &RmfLiftState,
    floor_labels: &BTreeMap<String, Floor>,
    provenance: ObservationProvenance,
) -> Result<MappedRmfObservation, RmfMappingError> {
    if source.lift_name.is_empty() {
        return Err(RmfMappingError::EmptyLiftName);
    }
    if source.available_floors.is_empty() {
        return Err(RmfMappingError::NoAvailableFloors);
    }

    let served_floors = source
        .available_floors
        .iter()
        .map(|label| mapped_floor(label, floor_labels))
        .collect::<Result<Vec<_>, _>>()?;
    let topology =
        BuildingTopology::new(served_floors).map_err(|_| RmfMappingError::NoAvailableFloors)?;
    if !source
        .available_floors
        .iter()
        .any(|label| label == &source.current_floor)
    {
        return Err(RmfMappingError::FloorNotAdvertised(
            source.current_floor.clone(),
        ));
    }
    let current_floor = mapped_floor(&source.current_floor, floor_labels)?;
    let reported_destination = if source.destination_floor.is_empty() {
        // The official message defines a string field but assigns no semantics to an empty
        // value. Preserve that absence as unknown instead of claiming explicit no destination.
        ReportedDestination::Unknown
    } else {
        if !source
            .available_floors
            .iter()
            .any(|label| label == &source.destination_floor)
        {
            return Err(RmfMappingError::FloorNotAdvertised(
                source.destination_floor.clone(),
            ));
        }
        ReportedDestination::Known(mapped_floor(&source.destination_floor, floor_labels)?)
    };
    let door = match source.door_state {
        RmfLiftState::DOOR_CLOSED => DoorEvidence::Known(DoorState::Closed),
        RmfLiftState::DOOR_OPEN => DoorEvidence::Known(DoorState::Open),
        RmfLiftState::DOOR_MOVING => DoorEvidence::MovingUnknownDirection,
        other => return Err(RmfMappingError::UnknownDoorState(other)),
    };
    let motion = match source.motion_state {
        RmfLiftState::MOTION_STOPPED => MotionEvidence::Known(MotionState::Stopped),
        RmfLiftState::MOTION_UP => MotionEvidence::Known(MotionState::Moving(Direction::Up)),
        RmfLiftState::MOTION_DOWN => MotionEvidence::Known(MotionState::Moving(Direction::Down)),
        RmfLiftState::MOTION_UNKNOWN => MotionEvidence::Unknown,
        other => return Err(RmfMappingError::UnknownMotionState(other)),
    };
    let mode = match source.current_mode {
        RmfLiftState::MODE_FIRE => OperatingModeEvidence::Known(OperatingMode::FireService),
        RmfLiftState::MODE_OFFLINE => OperatingModeEvidence::Known(OperatingMode::OutOfService),
        RmfLiftState::MODE_UNKNOWN => OperatingModeEvidence::Unknown,
        RmfLiftState::MODE_HUMAN | RmfLiftState::MODE_AGV | RmfLiftState::MODE_EMERGENCY => {
            OperatingModeEvidence::SourceSpecific
        }
        other => return Err(RmfMappingError::UnknownOperatingMode(other)),
    };

    Ok(MappedRmfObservation {
        observation: CarObservation {
            provenance,
            car: CarId::new(&source.lift_name),
            position: PositionEvidence::FloorReference(current_floor),
            motion,
            door,
            mode,
            reported_destination,
        },
        topology,
        source_context: RmfSourceContext {
            current_floor_label: source.current_floor.clone(),
            destination_floor_label: source.destination_floor.clone(),
            source_mode: source.current_mode,
            available_source_modes: source.available_modes.clone(),
            source_session_id: source.session_id.clone(),
        },
    })
}

fn mapped_floor(
    label: &str,
    floor_labels: &BTreeMap<String, Floor>,
) -> Result<Floor, RmfMappingError> {
    floor_labels
        .get(label)
        .copied()
        .ok_or_else(|| RmfMappingError::UnknownFloorLabel(label.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{CarLocation, ObservationSequence, ObservationSourceId};
    use liftassistant_runtime::{
        CarStateResolution, UnresolvedCarStateReason, resolve_car_observation,
    };

    fn floor_labels() -> BTreeMap<String, Floor> {
        [("L1", 1), ("L2", 2), ("L3", 3), ("L4", 4)]
            .into_iter()
            .map(|(label, floor)| (label.to_owned(), Floor::new(floor)))
            .collect()
    }

    fn provenance(sequence: u64) -> ObservationProvenance {
        ObservationProvenance {
            source: ObservationSourceId::new("fixture/open-rmf"),
            sequence: ObservationSequence::new(sequence),
        }
    }

    fn fire_service_state() -> RmfLiftState {
        RmfLiftState {
            lift_name: "lift-A".into(),
            available_floors: vec!["L1".into(), "L2".into(), "L3".into(), "L4".into()],
            current_floor: "L3".into(),
            destination_floor: "L4".into(),
            door_state: RmfLiftState::DOOR_OPEN,
            motion_state: RmfLiftState::MOTION_STOPPED,
            available_modes: vec![RmfLiftState::MODE_HUMAN, RmfLiftState::MODE_FIRE],
            current_mode: RmfLiftState::MODE_FIRE,
            session_id: "recorded-session".into(),
        }
    }

    #[test]
    fn incomplete_fields_are_valid_evidence_without_fabricated_precision() {
        let mut source = fire_service_state();
        source.door_state = RmfLiftState::DOOR_MOVING;
        source.motion_state = RmfLiftState::MOTION_UP;

        let mapped = map_rmf_lift_state(&source, &floor_labels(), provenance(1))
            .expect("known partial evidence is a valid observation");

        assert_eq!(
            mapped.observation.position,
            PositionEvidence::FloorReference(Floor::new(3))
        );
        assert_eq!(
            mapped.observation.motion,
            MotionEvidence::Known(MotionState::Moving(Direction::Up))
        );
        assert_eq!(
            mapped.observation.door,
            DoorEvidence::MovingUnknownDirection
        );
        assert_eq!(
            resolve_car_observation(&mapped.observation),
            CarStateResolution::Unresolved {
                car: CarId::new("lift-A"),
                reasons: vec![UnresolvedCarStateReason::DoorDirectionUnknown],
            }
        );
    }

    #[test]
    fn stopped_open_floor_reference_resolves_and_context_is_preserved() {
        let mut source = fire_service_state();
        source.current_floor = "L4".into();
        source.destination_floor.clear();

        let mapped = map_rmf_lift_state(&source, &floor_labels(), provenance(2))
            .expect("known source fields map to canonical evidence");
        let CarStateResolution::Resolved(state) = resolve_car_observation(&mapped.observation)
        else {
            panic!("stopped/open floor reference should resolve");
        };

        assert_eq!(state.id, CarId::new("lift-A"));
        assert_eq!(state.location, CarLocation::Floor(Floor::new(4)));
        assert_eq!(state.motion, MotionState::Stopped);
        assert_eq!(state.door, DoorState::Open);
        assert_eq!(state.mode, OperatingMode::FireService);
        assert_eq!(state.reported_destination, ReportedDestination::Unknown);
        assert_eq!(mapped.observation.provenance, provenance(2));
        assert_eq!(mapped.source_context.current_floor_label, "L4");
        assert_eq!(mapped.source_context.destination_floor_label, "");
        assert_eq!(mapped.source_context.source_mode, RmfLiftState::MODE_FIRE);
        assert_eq!(mapped.source_context.source_session_id, "recorded-session");
        assert_eq!(
            mapped.topology.served_floors(),
            &[Floor::new(1), Floor::new(2), Floor::new(3), Floor::new(4)]
        );
    }

    #[test]
    fn source_control_modes_remain_context_instead_of_operating_modes() {
        let mut source = fire_service_state();
        source.current_mode = RmfLiftState::MODE_AGV;

        let mapped = map_rmf_lift_state(&source, &floor_labels(), provenance(1))
            .expect("source-specific mode remains valid observation context");

        assert_eq!(
            mapped.observation.mode,
            OperatingModeEvidence::SourceSpecific
        );
        assert_eq!(mapped.source_context.source_mode, RmfLiftState::MODE_AGV);
        assert_eq!(
            resolve_car_observation(&mapped.observation),
            CarStateResolution::Unresolved {
                car: CarId::new("lift-A"),
                reasons: vec![UnresolvedCarStateReason::OperatingModeSourceSpecific],
            }
        );
    }

    #[test]
    fn source_unknowns_are_evidence_but_unrecognized_values_are_errors() {
        let mut source = fire_service_state();
        source.motion_state = RmfLiftState::MOTION_UNKNOWN;
        source.current_mode = RmfLiftState::MODE_UNKNOWN;
        let mapped = map_rmf_lift_state(&source, &floor_labels(), provenance(1))
            .expect("declared unknown values remain valid evidence");
        assert_eq!(mapped.observation.motion, MotionEvidence::Unknown);
        assert_eq!(mapped.observation.mode, OperatingModeEvidence::Unknown);

        source.motion_state = 99;
        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels(), provenance(1)),
            Err(RmfMappingError::UnknownMotionState(99))
        );
        source.motion_state = RmfLiftState::MOTION_STOPPED;
        source.current_mode = 99;
        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels(), provenance(1)),
            Err(RmfMappingError::UnknownOperatingMode(99))
        );
    }

    #[test]
    fn every_external_floor_label_requires_explicit_configuration() {
        let mut source = fire_service_state();
        source.available_floors.push("ROOF".into());
        source.destination_floor = "ROOF".into();

        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels(), provenance(1)),
            Err(RmfMappingError::UnknownFloorLabel("ROOF".into()))
        );

        let mut source = fire_service_state();
        source.destination_floor = "ROOF".into();
        let mut labels = floor_labels();
        labels.insert("ROOF".into(), Floor::new(8));
        assert_eq!(
            map_rmf_lift_state(&source, &labels, provenance(1)),
            Err(RmfMappingError::FloorNotAdvertised("ROOF".into()))
        );
    }
}
