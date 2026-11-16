#![forbid(unsafe_code)]

use std::{error::Error, fmt, time::Duration};

use liftassistant_model::{
    Capabilities, CarId, CarObservation, CarState, DoorEvidence, DoorState, Floor, Intent,
    MotionEvidence, MotionState, OperatingModeEvidence, Position, PositionEvidence,
};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnresolvedCarStateReason {
    ExactPositionUnavailable,
    MotionUnknown,
    DoorDirectionUnknown,
    OperatingModeUnknown,
    OperatingModeSourceSpecific,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CarStateResolution {
    Resolved(CarState),
    Unresolved {
        car: CarId,
        reasons: Vec<UnresolvedCarStateReason>,
    },
}

/// Resolves one canonical observation without history, probability, or source-specific inference.
pub fn resolve_car_observation(observation: &CarObservation) -> CarStateResolution {
    let mut reasons = Vec::new();

    let position = match observation.position {
        PositionEvidence::Exact(position) => Some(position),
        PositionEvidence::FloorReference(floor)
            if observation.motion == MotionEvidence::Known(MotionState::Stopped)
                && observation.door == DoorEvidence::Known(DoorState::Open) =>
        {
            Some(Position::at_floor(floor))
        }
        PositionEvidence::FloorReference(_) => {
            reasons.push(UnresolvedCarStateReason::ExactPositionUnavailable);
            None
        }
    };

    let motion = match observation.motion {
        MotionEvidence::Known(motion) => Some(motion),
        MotionEvidence::Unknown => {
            reasons.push(UnresolvedCarStateReason::MotionUnknown);
            None
        }
    };

    let door = match observation.door {
        DoorEvidence::Known(door) => Some(door),
        DoorEvidence::MovingUnknownDirection => {
            reasons.push(UnresolvedCarStateReason::DoorDirectionUnknown);
            None
        }
    };

    let mode = match observation.mode {
        OperatingModeEvidence::Known(mode) => Some(mode),
        OperatingModeEvidence::Unknown => {
            reasons.push(UnresolvedCarStateReason::OperatingModeUnknown);
            None
        }
        OperatingModeEvidence::SourceSpecific => {
            reasons.push(UnresolvedCarStateReason::OperatingModeSourceSpecific);
            None
        }
    };

    match (position, motion, door, mode) {
        (Some(position), Some(motion), Some(door), Some(mode)) => {
            CarStateResolution::Resolved(CarState {
                id: observation.car.clone(),
                position,
                motion,
                door,
                mode,
                target: observation.target,
            })
        }
        _ => CarStateResolution::Unresolved {
            car: observation.car.clone(),
            reasons,
        },
    }
}

/// Read-side access to resolved operational state.
///
/// Canonical partial observations must pass through the resolver before entering this view.
pub trait ObservationSource {
    fn source_name(&self) -> &str;
    fn car_states(&self) -> Vec<CarState>;
    fn capabilities(&self, car: &CarId) -> Option<Capabilities>;
}

/// Submission boundary for LiftAssistant high-level intents.
pub trait IntentSink {
    fn submit_intent(&mut self, intent: Intent) -> Result<(), RuntimeError>;
}

/// Explicit-time driver used only by deterministic simulators and recorded replay.
pub trait SimulationDriver {
    fn advance(&mut self, elapsed: Duration) -> Vec<RuntimeEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{
        Direction, ObservationProvenance, ObservationSequence, ObservationSourceId, OperatingMode,
    };

    fn observation() -> CarObservation {
        CarObservation {
            provenance: ObservationProvenance {
                source: ObservationSourceId::new("fixture/open-rmf"),
                sequence: ObservationSequence::new(1),
            },
            car: CarId::new("lift-A"),
            position: PositionEvidence::FloorReference(Floor::new(3)),
            motion: MotionEvidence::Known(MotionState::Moving(Direction::Up)),
            door: DoorEvidence::MovingUnknownDirection,
            mode: OperatingModeEvidence::Known(OperatingMode::FireService),
            target: Some(Floor::new(4)),
        }
    }

    #[test]
    fn incomplete_evidence_is_not_promoted_to_operational_state() {
        assert_eq!(
            resolve_car_observation(&observation()),
            CarStateResolution::Unresolved {
                car: CarId::new("lift-A"),
                reasons: vec![
                    UnresolvedCarStateReason::ExactPositionUnavailable,
                    UnresolvedCarStateReason::DoorDirectionUnknown,
                ],
            }
        );
    }

    #[test]
    fn stopped_open_floor_reference_resolves_at_floor() {
        let mut observation = observation();
        observation.provenance.sequence = ObservationSequence::new(2);
        observation.position = PositionEvidence::FloorReference(Floor::new(4));
        observation.motion = MotionEvidence::Known(MotionState::Stopped);
        observation.door = DoorEvidence::Known(DoorState::Open);
        observation.target = None;

        assert_eq!(
            resolve_car_observation(&observation),
            CarStateResolution::Resolved(CarState {
                id: CarId::new("lift-A"),
                position: Position::at_floor(Floor::new(4)),
                motion: MotionState::Stopped,
                door: DoorState::Open,
                mode: liftassistant_model::OperatingMode::FireService,
                target: None,
            })
        );
    }

    #[test]
    fn source_specific_mode_stays_unresolved() {
        let mut observation = observation();
        observation.position = PositionEvidence::Exact(Position::at_floor(Floor::new(3)));
        observation.motion = MotionEvidence::Known(MotionState::Stopped);
        observation.door = DoorEvidence::Known(DoorState::Open);
        observation.mode = OperatingModeEvidence::SourceSpecific;

        assert_eq!(
            resolve_car_observation(&observation),
            CarStateResolution::Unresolved {
                car: CarId::new("lift-A"),
                reasons: vec![UnresolvedCarStateReason::OperatingModeSourceSpecific],
            }
        );
    }
}
