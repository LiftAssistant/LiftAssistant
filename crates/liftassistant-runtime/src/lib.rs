#![forbid(unsafe_code)]

use std::{error::Error, fmt, time::Duration};

use liftassistant_model::{Capabilities, CarId, CarState, DoorState, Floor, Intent};

#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeEvent {
    IntentAccepted(Intent),
    PositionChanged(CarState),
    Arrived { car: CarId, floor: Floor },
    DoorStateChanged { car: CarId, state: DoorState },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeError {
    message: String,
}

impl RuntimeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for RuntimeError {}

pub trait ElevatorRuntime {
    fn runtime_name(&self) -> &str;
    fn car_states(&self) -> Vec<CarState>;
    fn capabilities(&self, car: &CarId) -> Option<Capabilities>;
    fn submit_intent(&mut self, intent: Intent) -> Result<(), RuntimeError>;
    fn tick(&mut self, elapsed: Duration) -> Vec<RuntimeEvent>;
}
