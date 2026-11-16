#![forbid(unsafe_code)]

use std::fmt;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CarId(String);

impl CarId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Floor(i16);

impl Floor {
    pub const fn new(index: i16) -> Self {
        Self(index)
    }

    pub const fn index(self) -> i16 {
        self.0
    }
}

impl fmt::Display for Floor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}F", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildingTopology {
    served_floors: Vec<Floor>,
}

impl BuildingTopology {
    pub fn new(floors: impl IntoIterator<Item = Floor>) -> Result<Self, BuildingTopologyError> {
        let mut served_floors: Vec<_> = floors.into_iter().collect();
        served_floors.sort_unstable();
        served_floors.dedup();
        if served_floors.is_empty() {
            return Err(BuildingTopologyError::NoServedFloors);
        }
        Ok(Self { served_floors })
    }

    pub fn reference_eight_floor() -> Self {
        Self::new((1..=8).map(Floor::new)).expect("reference topology has served floors")
    }

    pub fn served_floors(&self) -> &[Floor] {
        &self.served_floors
    }

    pub fn contains(&self, floor: Floor) -> bool {
        self.served_floors.binary_search(&floor).is_ok()
    }

    pub fn lowest_floor(&self) -> Floor {
        self.served_floors[0]
    }

    pub fn highest_floor(&self) -> Floor {
        self.served_floors[self.served_floors.len() - 1]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildingTopologyError {
    NoServedFloors,
}

impl fmt::Display for BuildingTopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoServedFloors => f.write_str("building topology requires a served floor"),
        }
    }
}

impl std::error::Error for BuildingTopologyError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub floor_position: f64,
    pub nearest_floor: Floor,
}

impl Position {
    pub fn at_floor(floor: Floor) -> Self {
        Self {
            floor_position: f64::from(floor.index()),
            nearest_floor: floor,
        }
    }
}

/// Operational car location with explicit precision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CarLocation {
    /// The source associates the car with a floor but supplies no continuous coordinate.
    Floor(Floor),
    /// The source supplies a continuous position in LiftAssistant's floor coordinate space.
    Continuous(Position),
}

impl CarLocation {
    pub const fn floor_reference(self) -> Floor {
        match self {
            Self::Floor(floor) => floor,
            Self::Continuous(position) => position.nearest_floor,
        }
    }

    pub const fn continuous_position(self) -> Option<Position> {
        match self {
            Self::Floor(_) => None,
            Self::Continuous(position) => Some(position),
        }
    }
}

impl fmt::Display for CarLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Floor(floor) => write!(f, "floor {floor}"),
            Self::Continuous(position) => write!(
                f,
                "{:.2}F (near {})",
                position.floor_position, position.nearest_floor
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionState {
    Stopped,
    Moving(Direction),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoorState {
    Closed,
    Opening,
    Open,
    Closing,
    Obstructed,
    Fault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatingMode {
    Normal,
    Independent,
    Inspection,
    FireService,
    OutOfService,
}

/// Destination or next-stop state reported by an external controller.
///
/// This is observation-side state. LiftAssistant's desired destination remains in its
/// call/intent lifecycle and must not be inferred from this value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportedDestination {
    Known(Floor),
    None,
    Unknown,
}

impl fmt::Display for ReportedDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(floor) => floor.fmt(f),
            Self::None => f.write_str("none"),
            Self::Unknown => f.write_str("unknown"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Capabilities {
    pub observe_position: bool,
    pub observe_door: bool,
    pub serve_floor: bool,
    pub park: bool,
}

impl Capabilities {
    pub const fn reference_defaults() -> Self {
        Self {
            observe_position: true,
            observe_door: true,
            serve_floor: true,
            park: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CarState {
    pub id: CarId,
    pub location: CarLocation,
    pub motion: MotionState,
    pub door: DoorState,
    pub mode: OperatingMode,
    pub reported_destination: ReportedDestination,
}

impl CarState {
    pub fn new(id: impl Into<String>, floor: Floor) -> Self {
        Self {
            id: CarId::new(id),
            location: CarLocation::Floor(floor),
            motion: MotionState::Stopped,
            door: DoorState::Closed,
            mode: OperatingMode::Normal,
            reported_destination: ReportedDestination::None,
        }
    }

    pub fn with_continuous_position(id: impl Into<String>, position: Position) -> Self {
        Self {
            id: CarId::new(id),
            location: CarLocation::Continuous(position),
            motion: MotionState::Stopped,
            door: DoorState::Closed,
            mode: OperatingMode::Normal,
            reported_destination: ReportedDestination::None,
        }
    }
}

/// Identifies the configured source that produced an observation.
///
/// This remains an internal provenance value, not a public adapter identifier schema.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObservationSourceId(String);

impl ObservationSourceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ObservationSourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObservationSequence(u64);

impl ObservationSequence {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationProvenance {
    pub source: ObservationSourceId,
    pub sequence: ObservationSequence,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PositionEvidence {
    Exact(Position),
    /// The source named a floor but did not establish an exact at-floor position.
    FloorReference(Floor),
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionEvidence {
    Known(MotionState),
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoorEvidence {
    Known(DoorState),
    /// The source reports movement without saying whether the door opens or closes.
    MovingUnknownDirection,
    /// The source did not establish one operational door state for the car.
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatingModeEvidence {
    Known(OperatingMode),
    Unknown,
    /// The source mode is real context but is not a LiftAssistant operating mode.
    SourceSpecific,
}

/// Vendor-neutral evidence that may be insufficient to construct an operational [`CarState`].
///
/// Source-specific context is deliberately retained by the adapter beside this value instead of
/// being promoted into canonical domain semantics.
#[derive(Clone, Debug, PartialEq)]
pub struct CarObservation {
    pub provenance: ObservationProvenance,
    pub car: CarId,
    pub position: PositionEvidence,
    pub motion: MotionEvidence,
    pub door: DoorEvidence,
    pub mode: OperatingModeEvidence,
    pub reported_destination: ReportedDestination,
}

/// Incremental canonical observation update.
///
/// An outer [`Option::None`] means that this update omitted the field and must not change
/// previously assembled evidence. An evidence enum's `Unknown` variant is an explicit source
/// statement and replaces the previous value.
#[derive(Clone, Debug, PartialEq)]
pub struct CarObservationUpdate {
    pub provenance: ObservationProvenance,
    pub car: CarId,
    pub position: Option<PositionEvidence>,
    pub motion: Option<MotionEvidence>,
    pub door: Option<DoorEvidence>,
    pub mode: Option<OperatingModeEvidence>,
    pub reported_destination: Option<ReportedDestination>,
}

impl From<CarObservation> for CarObservationUpdate {
    fn from(observation: CarObservation) -> Self {
        Self {
            provenance: observation.provenance,
            car: observation.car,
            position: Some(observation.position),
            motion: Some(observation.motion),
            door: Some(observation.door),
            mode: Some(observation.mode),
            reported_destination: Some(observation.reported_destination),
        }
    }
}

impl From<&CarObservation> for CarObservationUpdate {
    fn from(observation: &CarObservation) -> Self {
        observation.clone().into()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HallCall {
    pub floor: Floor,
    pub direction: Direction,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallId(u64);

impl CallId {
    pub const fn new(sequence: u64) -> Self {
        Self(sequence)
    }

    pub const fn sequence(self) -> u64 {
        self.0
    }
}

impl fmt::Display for CallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallState {
    Pending,
    Assigned(CarId),
    Serving(CarId),
    Completed,
}

impl CallState {
    pub const fn is_active(&self) -> bool {
        !matches!(self, Self::Completed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallRecord {
    pub id: CallId,
    pub call: HallCall,
    pub state: CallState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Intent {
    ServeFloor { car: CarId, floor: Floor },
    ParkAt { car: CarId, floor: Floor },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_topology_canonicalizes_served_floors() {
        let topology =
            BuildingTopology::new([Floor::new(5), Floor::new(1), Floor::new(3), Floor::new(3)])
                .expect("nonempty topology");

        assert_eq!(
            topology.served_floors(),
            &[Floor::new(1), Floor::new(3), Floor::new(5)]
        );
        assert_eq!(topology.lowest_floor(), Floor::new(1));
        assert_eq!(topology.highest_floor(), Floor::new(5));
        assert!(topology.contains(Floor::new(3)));
        assert!(!topology.contains(Floor::new(2)));
    }

    #[test]
    fn building_topology_rejects_an_empty_floor_set() {
        assert_eq!(
            BuildingTopology::new([]),
            Err(BuildingTopologyError::NoServedFloors)
        );
    }
}
