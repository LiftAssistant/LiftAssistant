use std::{collections::BTreeMap, error::Error, fmt};

use liftassistant_model::{
    BuildingTopology, CarId, CarState, DoorState, Floor, MotionState, OperatingMode, Position,
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

#[derive(Clone, Debug, PartialEq)]
pub struct MappedRmfLiftState {
    pub car_state: CarState,
    pub topology: BuildingTopology,
    pub source_mode: u8,
    pub available_source_modes: Vec<u8>,
    pub source_session_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RmfMappingError {
    EmptyLiftName,
    NoAvailableFloors,
    UnknownFloorLabel(String),
    FloorNotAdvertised(String),
    AmbiguousDoorMoving,
    UnknownDoorState(u8),
    MotionLacksContinuousPosition(u8),
    UnknownMotionState(u8),
    PositionNotConfirmedAtFloor,
    ControlModeIsNotOperatingMode(u8),
    UnsupportedOperatingMode(u8),
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
            Self::AmbiguousDoorMoving => {
                f.write_str("Open-RMF DOOR_MOVING does not distinguish opening from closing")
            }
            Self::UnknownDoorState(value) => {
                write!(f, "unknown Open-RMF door_state {value}")
            }
            Self::MotionLacksContinuousPosition(value) => write!(
                f,
                "Open-RMF motion_state {value} has no continuous position to map"
            ),
            Self::UnknownMotionState(value) => {
                write!(f, "unsupported Open-RMF motion_state {value}")
            }
            Self::PositionNotConfirmedAtFloor => {
                f.write_str("Open-RMF state does not confirm an exact at-floor position")
            }
            Self::ControlModeIsNotOperatingMode(value) => write!(
                f,
                "Open-RMF control mode {value} is not a LiftAssistant operating mode"
            ),
            Self::UnsupportedOperatingMode(value) => {
                write!(f, "unsupported Open-RMF operating mode {value}")
            }
        }
    }
}

impl Error for RmfMappingError {}

pub fn map_rmf_lift_state(
    source: &RmfLiftState,
    floor_labels: &BTreeMap<String, Floor>,
) -> Result<MappedRmfLiftState, RmfMappingError> {
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
    let target = if source.destination_floor.is_empty() {
        None
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
        Some(mapped_floor(&source.destination_floor, floor_labels)?)
    };
    let door = match source.door_state {
        RmfLiftState::DOOR_CLOSED => DoorState::Closed,
        RmfLiftState::DOOR_OPEN => DoorState::Open,
        RmfLiftState::DOOR_MOVING => return Err(RmfMappingError::AmbiguousDoorMoving),
        other => return Err(RmfMappingError::UnknownDoorState(other)),
    };
    let motion = match source.motion_state {
        RmfLiftState::MOTION_STOPPED => MotionState::Stopped,
        RmfLiftState::MOTION_UP | RmfLiftState::MOTION_DOWN => {
            return Err(RmfMappingError::MotionLacksContinuousPosition(
                source.motion_state,
            ));
        }
        other => return Err(RmfMappingError::UnknownMotionState(other)),
    };
    if door != DoorState::Open {
        return Err(RmfMappingError::PositionNotConfirmedAtFloor);
    }
    let mode = match source.current_mode {
        RmfLiftState::MODE_FIRE => OperatingMode::FireService,
        RmfLiftState::MODE_OFFLINE => OperatingMode::OutOfService,
        RmfLiftState::MODE_HUMAN | RmfLiftState::MODE_AGV => {
            return Err(RmfMappingError::ControlModeIsNotOperatingMode(
                source.current_mode,
            ));
        }
        other => return Err(RmfMappingError::UnsupportedOperatingMode(other)),
    };

    Ok(MappedRmfLiftState {
        car_state: CarState {
            id: CarId::new(&source.lift_name),
            position: Position::at_floor(current_floor),
            motion,
            door,
            mode,
            target,
        },
        topology,
        source_mode: source.current_mode,
        available_source_modes: source.available_modes.clone(),
        source_session_id: source.session_id.clone(),
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

    fn floor_labels() -> BTreeMap<String, Floor> {
        [("L1", 1), ("L2", 2), ("L3", 3), ("L4", 4)]
            .into_iter()
            .map(|(label, floor)| (label.to_owned(), Floor::new(floor)))
            .collect()
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
    fn unambiguous_recorded_fields_map_and_source_context_is_preserved() {
        let mapped = map_rmf_lift_state(&fire_service_state(), &floor_labels())
            .expect("fire/offline modes and stable door/motion map directly");

        assert_eq!(mapped.car_state.id, CarId::new("lift-A"));
        assert_eq!(mapped.car_state.position, Position::at_floor(Floor::new(3)));
        assert_eq!(mapped.car_state.motion, MotionState::Stopped);
        assert_eq!(mapped.car_state.door, DoorState::Open);
        assert_eq!(mapped.car_state.mode, OperatingMode::FireService);
        assert_eq!(mapped.car_state.target, Some(Floor::new(4)));
        assert_eq!(
            mapped.topology.served_floors(),
            &[Floor::new(1), Floor::new(2), Floor::new(3), Floor::new(4)]
        );
        assert_eq!(mapped.source_mode, RmfLiftState::MODE_FIRE);
        assert_eq!(mapped.source_session_id, "recorded-session");

        let mut offline = fire_service_state();
        offline.current_mode = RmfLiftState::MODE_OFFLINE;
        assert_eq!(
            map_rmf_lift_state(&offline, &floor_labels())
                .expect("offline maps to out of service")
                .car_state
                .mode,
            OperatingMode::OutOfService
        );
    }

    #[test]
    fn ambiguous_external_fields_are_not_guessed() {
        let mut source = fire_service_state();
        source.door_state = RmfLiftState::DOOR_MOVING;
        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels()),
            Err(RmfMappingError::AmbiguousDoorMoving)
        );

        source.door_state = RmfLiftState::DOOR_OPEN;
        source.current_mode = RmfLiftState::MODE_AGV;
        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels()),
            Err(RmfMappingError::ControlModeIsNotOperatingMode(
                RmfLiftState::MODE_AGV
            ))
        );

        source.current_mode = RmfLiftState::MODE_FIRE;
        source.motion_state = RmfLiftState::MOTION_UP;
        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels()),
            Err(RmfMappingError::MotionLacksContinuousPosition(
                RmfLiftState::MOTION_UP
            ))
        );

        source.motion_state = RmfLiftState::MOTION_STOPPED;
        source.door_state = RmfLiftState::DOOR_CLOSED;
        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels()),
            Err(RmfMappingError::PositionNotConfirmedAtFloor)
        );
    }

    #[test]
    fn unknown_and_unsupported_values_remain_explicit_errors() {
        let mut source = fire_service_state();
        source.motion_state = RmfLiftState::MOTION_UNKNOWN;
        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels()),
            Err(RmfMappingError::UnknownMotionState(
                RmfLiftState::MOTION_UNKNOWN
            ))
        );

        source.motion_state = RmfLiftState::MOTION_STOPPED;
        source.current_mode = RmfLiftState::MODE_EMERGENCY;
        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels()),
            Err(RmfMappingError::UnsupportedOperatingMode(
                RmfLiftState::MODE_EMERGENCY
            ))
        );
    }

    #[test]
    fn every_external_floor_label_requires_explicit_configuration() {
        let mut source = fire_service_state();
        source.available_floors.push("ROOF".into());
        source.destination_floor = "ROOF".into();

        assert_eq!(
            map_rmf_lift_state(&source, &floor_labels()),
            Err(RmfMappingError::UnknownFloorLabel("ROOF".into()))
        );

        let mut source = fire_service_state();
        source.destination_floor = "ROOF".into();
        let mut labels = floor_labels();
        labels.insert("ROOF".into(), Floor::new(8));
        assert_eq!(
            map_rmf_lift_state(&source, &labels),
            Err(RmfMappingError::FloorNotAdvertised("ROOF".into()))
        );
    }
}
