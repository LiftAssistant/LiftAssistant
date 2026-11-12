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
    pub position: Position,
    pub motion: MotionState,
    pub door: DoorState,
    pub mode: OperatingMode,
    pub target: Option<Floor>,
}

impl CarState {
    pub fn new(id: impl Into<String>, floor: Floor) -> Self {
        Self {
            id: CarId::new(id),
            position: Position::at_floor(floor),
            motion: MotionState::Stopped,
            door: DoorState::Closed,
            mode: OperatingMode::Normal,
            target: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HallCall {
    pub floor: Floor,
    pub direction: Direction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Intent {
    ServeFloor { car: CarId, floor: Floor },
    ParkAt { car: CarId, floor: Floor },
}
