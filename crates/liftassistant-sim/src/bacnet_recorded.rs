use std::{collections::BTreeMap, error::Error, fmt};

use liftassistant_model::{
    BuildingTopology, CarId, CarObservation, DoorEvidence, DoorState, Floor, MotionEvidence,
    MotionState, ObservationProvenance, OperatingMode, OperatingModeEvidence, PositionEvidence,
    ReportedDestination,
};

/// Decoded BACnet Lift object properties used by the recorded mapping experiment.
///
/// This is an internal projection of selected properties, not a BACnet packet, object
/// implementation, transport, or public schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedBacnetLiftProperties {
    pub object_name: String,
    pub car_position: u8,
    pub car_moving_direction: BacnetLiftCarDirection,
    pub car_door_status: Vec<BacnetDoorStatus>,
    pub car_mode: Option<BacnetLiftCarMode>,
    pub next_stopping_floor: Option<u8>,
    /// BACnet object decoupling status, not the lift car's operating mode.
    pub out_of_service: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BacnetLiftCarDirection {
    Unknown,
    Stopped,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BacnetDoorStatus {
    Unknown,
    Closing,
    Closed,
    Opening,
    Opened,
    SafetyLocked,
    LimitedOpened,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BacnetLiftCarMode {
    Unknown,
    Normal,
    Vip,
    Homing,
    Parking,
    AttendantControl,
    FirefighterControl,
    EmergencyPower,
    Inspection,
    CabinetRecall,
    EarthquakeOperation,
    FireOperation,
    OutOfService,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BacnetLiftSourceContext {
    pub object_name: String,
    pub universal_car_position: u8,
    pub moving_direction: BacnetLiftCarDirection,
    pub door_statuses: Vec<BacnetDoorStatus>,
    pub car_mode: Option<BacnetLiftCarMode>,
    pub universal_next_stopping_floor: Option<u8>,
    pub object_out_of_service: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappedBacnetObservation {
    pub observation: CarObservation,
    pub topology: BuildingTopology,
    pub source_context: BacnetLiftSourceContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BacnetMappingError {
    EmptyObjectName,
    NoConfiguredFloors,
    UnknownUniversalFloor(u8),
    NoCarDoors,
}

impl fmt::Display for BacnetMappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyObjectName => f.write_str("BACnet Lift Object_Name is empty"),
            Self::NoConfiguredFloors => {
                f.write_str("BACnet Lift mapping requires a configured universal floor map")
            }
            Self::UnknownUniversalFloor(floor) => write!(
                f,
                "BACnet universal floor number {floor} has no configured mapping"
            ),
            Self::NoCarDoors => f.write_str("BACnet Lift Car_Door_Status contains no car doors"),
        }
    }
}

impl Error for BacnetMappingError {}

pub fn map_bacnet_lift_properties(
    source: &RecordedBacnetLiftProperties,
    car: CarId,
    universal_floors: &BTreeMap<u8, Floor>,
    provenance: ObservationProvenance,
) -> Result<MappedBacnetObservation, BacnetMappingError> {
    if source.object_name.is_empty() {
        return Err(BacnetMappingError::EmptyObjectName);
    }
    if universal_floors.is_empty() {
        return Err(BacnetMappingError::NoConfiguredFloors);
    }
    if source.car_door_status.is_empty() {
        return Err(BacnetMappingError::NoCarDoors);
    }

    let topology = BuildingTopology::new(universal_floors.values().copied())
        .map_err(|_| BacnetMappingError::NoConfiguredFloors)?;
    let source_context = BacnetLiftSourceContext {
        object_name: source.object_name.clone(),
        universal_car_position: source.car_position,
        moving_direction: source.car_moving_direction,
        door_statuses: source.car_door_status.clone(),
        car_mode: source.car_mode,
        universal_next_stopping_floor: source.next_stopping_floor,
        object_out_of_service: source.out_of_service,
    };

    let (position, motion, door, mode, reported_destination) = if source.out_of_service {
        // BACnet defines this as the object being decoupled from the represented lift.
        // Its property values therefore cannot establish current operational state.
        (
            PositionEvidence::Unknown,
            MotionEvidence::Unknown,
            DoorEvidence::Unknown,
            OperatingModeEvidence::Unknown,
            ReportedDestination::Unknown,
        )
    } else {
        (
            PositionEvidence::FloorReference(mapped_floor(source.car_position, universal_floors)?),
            map_motion(source.car_moving_direction),
            aggregate_doors(&source.car_door_status),
            map_mode(source.car_mode),
            match source.next_stopping_floor {
                Some(floor) => ReportedDestination::Known(mapped_floor(floor, universal_floors)?),
                None => ReportedDestination::Unknown,
            },
        )
    };

    Ok(MappedBacnetObservation {
        observation: CarObservation {
            provenance,
            car,
            position,
            motion,
            door,
            mode,
            reported_destination,
        },
        topology,
        source_context,
    })
}

fn mapped_floor(
    universal_floor: u8,
    universal_floors: &BTreeMap<u8, Floor>,
) -> Result<Floor, BacnetMappingError> {
    universal_floors
        .get(&universal_floor)
        .copied()
        .ok_or(BacnetMappingError::UnknownUniversalFloor(universal_floor))
}

fn map_motion(direction: BacnetLiftCarDirection) -> MotionEvidence {
    match direction {
        BacnetLiftCarDirection::Unknown => MotionEvidence::Unknown,
        BacnetLiftCarDirection::Stopped => MotionEvidence::Known(MotionState::Stopped),
        BacnetLiftCarDirection::Up => {
            MotionEvidence::Known(MotionState::Moving(liftassistant_model::Direction::Up))
        }
        BacnetLiftCarDirection::Down => {
            MotionEvidence::Known(MotionState::Moving(liftassistant_model::Direction::Down))
        }
    }
}

fn aggregate_doors(statuses: &[BacnetDoorStatus]) -> DoorEvidence {
    let first = map_door(statuses[0]);
    if statuses
        .iter()
        .copied()
        .skip(1)
        .all(|status| map_door(status) == first)
    {
        first
    } else {
        DoorEvidence::Unknown
    }
}

fn map_door(status: BacnetDoorStatus) -> DoorEvidence {
    match status {
        BacnetDoorStatus::Unknown | BacnetDoorStatus::LimitedOpened => DoorEvidence::Unknown,
        BacnetDoorStatus::Closing => DoorEvidence::Known(DoorState::Closing),
        BacnetDoorStatus::Closed | BacnetDoorStatus::SafetyLocked => {
            DoorEvidence::Known(DoorState::Closed)
        }
        BacnetDoorStatus::Opening => DoorEvidence::Known(DoorState::Opening),
        BacnetDoorStatus::Opened => DoorEvidence::Known(DoorState::Open),
    }
}

fn map_mode(mode: Option<BacnetLiftCarMode>) -> OperatingModeEvidence {
    match mode {
        None | Some(BacnetLiftCarMode::Unknown) => OperatingModeEvidence::Unknown,
        Some(BacnetLiftCarMode::Normal) => OperatingModeEvidence::Known(OperatingMode::Normal),
        Some(BacnetLiftCarMode::FirefighterControl) => {
            OperatingModeEvidence::Known(OperatingMode::FireService)
        }
        Some(BacnetLiftCarMode::Inspection) => {
            OperatingModeEvidence::Known(OperatingMode::Inspection)
        }
        Some(BacnetLiftCarMode::OutOfService) => {
            OperatingModeEvidence::Known(OperatingMode::OutOfService)
        }
        Some(
            BacnetLiftCarMode::Vip
            | BacnetLiftCarMode::Homing
            | BacnetLiftCarMode::Parking
            | BacnetLiftCarMode::AttendantControl
            | BacnetLiftCarMode::EmergencyPower
            | BacnetLiftCarMode::CabinetRecall
            | BacnetLiftCarMode::EarthquakeOperation
            | BacnetLiftCarMode::FireOperation,
        ) => OperatingModeEvidence::SourceSpecific,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{
        CarLocation, ObservationSequence, ObservationSessionId, ObservationSourceId,
    };
    use liftassistant_runtime::{
        CarStateResolution, UnresolvedCarStateReason, resolve_car_observation,
    };

    fn floors() -> BTreeMap<u8, Floor> {
        [(1, 1), (2, 2), (3, 3), (4, 4)]
            .into_iter()
            .map(|(universal, floor)| (universal, Floor::new(floor)))
            .collect()
    }

    fn provenance(sequence: u64) -> ObservationProvenance {
        ObservationProvenance {
            source: ObservationSourceId::new("fixture/bacnet-lift"),
            session: ObservationSessionId::new("bacnet-fixture-session-A"),
            sequence: ObservationSequence::new(sequence),
        }
    }

    fn properties() -> RecordedBacnetLiftProperties {
        RecordedBacnetLiftProperties {
            object_name: "Lift 1".into(),
            car_position: 3,
            car_moving_direction: BacnetLiftCarDirection::Up,
            car_door_status: vec![BacnetDoorStatus::Closing],
            car_mode: Some(BacnetLiftCarMode::FirefighterControl),
            next_stopping_floor: Some(4),
            out_of_service: false,
        }
    }

    #[test]
    fn floor_level_properties_resolve_without_continuous_precision() {
        let mapped = map_bacnet_lift_properties(
            &properties(),
            CarId::new("bacnet-lift-1"),
            &floors(),
            provenance(1),
        )
        .expect("configured standard properties map");

        assert_eq!(
            mapped.observation.position,
            PositionEvidence::FloorReference(Floor::new(3))
        );
        assert_eq!(
            mapped.observation.motion,
            MotionEvidence::Known(MotionState::Moving(liftassistant_model::Direction::Up))
        );
        assert_eq!(
            mapped.observation.door,
            DoorEvidence::Known(DoorState::Closing)
        );
        assert_eq!(
            mapped.observation.mode,
            OperatingModeEvidence::Known(OperatingMode::FireService)
        );
        assert_eq!(
            mapped.observation.reported_destination,
            ReportedDestination::Known(Floor::new(4))
        );
        let CarStateResolution::Resolved(state) = resolve_car_observation(&mapped.observation)
        else {
            panic!("floor-level BACnet evidence should resolve without a continuous coordinate")
        };
        assert_eq!(state.location, CarLocation::Floor(Floor::new(3)));
        assert!(state.location.continuous_position().is_none());
    }

    #[test]
    fn stopped_open_floor_reference_resolves_without_continuous_position() {
        let mut source = properties();
        source.car_position = 4;
        source.car_moving_direction = BacnetLiftCarDirection::Stopped;
        source.car_door_status = vec![BacnetDoorStatus::Opened];
        source.car_mode = Some(BacnetLiftCarMode::Normal);
        source.next_stopping_floor = None;
        let mapped = map_bacnet_lift_properties(
            &source,
            CarId::new("bacnet-lift-1"),
            &floors(),
            provenance(2),
        )
        .expect("configured standard properties map");

        let CarStateResolution::Resolved(state) = resolve_car_observation(&mapped.observation)
        else {
            panic!("stopped/open BACnet floor reference should resolve")
        };
        assert_eq!(state.location, CarLocation::Floor(Floor::new(4)));
        assert_eq!(state.mode, OperatingMode::Normal);
        assert_eq!(state.reported_destination, ReportedDestination::Unknown);
        assert_eq!(mapped.source_context.universal_car_position, 4);
    }

    #[test]
    fn decoupled_object_values_cannot_establish_operational_state() {
        let mut source = properties();
        source.out_of_service = true;
        source.car_moving_direction = BacnetLiftCarDirection::Stopped;
        source.car_door_status = vec![BacnetDoorStatus::Opened];
        source.car_mode = Some(BacnetLiftCarMode::OutOfService);
        let mapped = map_bacnet_lift_properties(
            &source,
            CarId::new("bacnet-lift-1"),
            &floors(),
            provenance(3),
        )
        .expect("decoupled object remains valid source context");

        assert_eq!(mapped.observation.position, PositionEvidence::Unknown);
        assert_eq!(mapped.observation.door, DoorEvidence::Unknown);
        assert_eq!(
            mapped.observation.reported_destination,
            ReportedDestination::Unknown
        );
        assert!(mapped.source_context.object_out_of_service);
        assert_eq!(
            resolve_car_observation(&mapped.observation),
            CarStateResolution::Unresolved {
                car: CarId::new("bacnet-lift-1"),
                reasons: vec![
                    UnresolvedCarStateReason::PositionUnknown,
                    UnresolvedCarStateReason::MotionUnknown,
                    UnresolvedCarStateReason::DoorUnknown,
                    UnresolvedCarStateReason::OperatingModeUnknown,
                ],
            }
        );
    }

    #[test]
    fn multiple_door_statuses_are_not_collapsed_into_a_false_car_state() {
        let mut source = properties();
        source.car_moving_direction = BacnetLiftCarDirection::Stopped;
        source.car_door_status = vec![BacnetDoorStatus::Opened, BacnetDoorStatus::Closed];
        source.car_mode = Some(BacnetLiftCarMode::Normal);
        let mapped = map_bacnet_lift_properties(
            &source,
            CarId::new("bacnet-lift-1"),
            &floors(),
            provenance(4),
        )
        .expect("multi-door status remains valid source context");

        assert_eq!(mapped.observation.door, DoorEvidence::Unknown);
        assert_eq!(
            mapped.source_context.door_statuses,
            [BacnetDoorStatus::Opened, BacnetDoorStatus::Closed]
        );
        assert_eq!(
            resolve_car_observation(&mapped.observation),
            CarStateResolution::Unresolved {
                car: CarId::new("bacnet-lift-1"),
                reasons: vec![UnresolvedCarStateReason::DoorUnknown],
            }
        );

        source.car_door_status.clear();
        assert_eq!(
            map_bacnet_lift_properties(
                &source,
                CarId::new("bacnet-lift-1"),
                &floors(),
                provenance(5),
            ),
            Err(BacnetMappingError::NoCarDoors)
        );
    }

    #[test]
    fn source_specific_car_modes_are_not_guessed_as_operational_modes() {
        let mut source = properties();
        source.car_position = 4;
        source.car_moving_direction = BacnetLiftCarDirection::Stopped;
        source.car_door_status = vec![BacnetDoorStatus::Opened];
        source.car_mode = Some(BacnetLiftCarMode::FireOperation);
        source.next_stopping_floor = None;
        let mapped = map_bacnet_lift_properties(
            &source,
            CarId::new("bacnet-lift-1"),
            &floors(),
            provenance(5),
        )
        .expect("standard but non-equivalent mode remains source context");

        assert_eq!(
            mapped.observation.mode,
            OperatingModeEvidence::SourceSpecific
        );
        assert_eq!(
            resolve_car_observation(&mapped.observation),
            CarStateResolution::Unresolved {
                car: CarId::new("bacnet-lift-1"),
                reasons: vec![UnresolvedCarStateReason::OperatingModeSourceSpecific],
            }
        );
    }

    #[test]
    fn unconfigured_universal_floor_is_rejected() {
        let mut source = properties();
        source.car_position = 99;

        assert_eq!(
            map_bacnet_lift_properties(
                &source,
                CarId::new("bacnet-lift-1"),
                &floors(),
                provenance(1),
            ),
            Err(BacnetMappingError::UnknownUniversalFloor(99))
        );
    }
}
