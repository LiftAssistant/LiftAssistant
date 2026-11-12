//! Developer-only, in-memory simulator inputs; not a public elevator schema.
use liftassistant_model::{CarId, HallCall, Intent};
use liftassistant_runtime::RuntimeEvent;

use crate::app::App;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    HallCall(HallCall),
    /// Exactly one application FIXED_STEP, with no wall-clock dependency.
    Step,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticEvent {
    HallCall(HallCall),
    InvalidCall,
    NoEligibleCar,
    DispatchSelected(CarId),
    IntentGenerated(Intent),
    RuntimeError(String),
    Runtime(RuntimeEvent),
}

pub struct Replay {
    pub app: App,
    pub events: Vec<SemanticEvent>,
}

pub fn run(commands: &[Command]) -> Replay {
    let mut app = App::new();
    let mut events = Vec::new();
    for command in commands {
        events.extend(app.execute(command.clone()));
    }
    Replay { app, events }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{Direction, DoorState, Floor};
    use liftassistant_runtime::ElevatorRuntime;

    #[test]
    fn identical_inputs_replay_the_complete_semantic_flow() {
        let mut commands = vec![Command::HallCall(HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        })];
        commands.extend(vec![Command::Step; 6]);
        let first = run(&commands);
        let second = run(&commands);
        assert_eq!(first.events, second.events);
        assert_eq!(
            first.app.runtime.car_states(),
            second.app.runtime.car_states()
        );
        assert!(
            first
                .events
                .contains(&SemanticEvent::DispatchSelected(CarId::new("A")))
        );
        assert!(
            first
                .events
                .contains(&SemanticEvent::Runtime(RuntimeEvent::Arrived {
                    car: CarId::new("A"),
                    floor: Floor::new(7),
                }))
        );
        assert!(
            first
                .events
                .contains(&SemanticEvent::Runtime(RuntimeEvent::DoorStateChanged {
                    car: CarId::new("A"),
                    state: DoorState::Open,
                }))
        );
        // One fewer explicit step has not arrived: equality isn't just two empty traces.
        let shorter = run(&commands[..commands.len() - 1]);
        assert_ne!(first.events, shorter.events);
        assert_ne!(
            first.app.runtime.car_states(),
            shorter.app.runtime.car_states()
        );
    }

    #[test]
    fn replay_preserves_recording_and_leaves_the_live_session_paused() {
        let mut app = App::new();
        app.call(Direction::Up);
        app.step();
        app.step();
        let commands = app.recording.clone();
        let states = app.runtime.car_states();
        app.replay();
        assert!(app.paused);
        assert_eq!(app.recording, commands);
        assert_eq!(app.runtime.car_states(), states);
        assert!(app.trace.back().unwrap().contains("REPLAY verified"));
        app.step();
        assert_eq!(app.steps, 3);
        app.reset();
        assert!(app.recording.is_empty());
    }
}
