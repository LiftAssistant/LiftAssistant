//! Developer-only, in-memory simulator inputs; not a public elevator schema.
use liftassistant_core::{DispatchCandidate, DispatchSelection, HallCallRejection};
use liftassistant_model::{
    BuildingTopology, CallId, CallState, CarId, HallCall, Intent, OperatingMode,
};
use liftassistant_runtime::RuntimeEvent;

#[cfg(test)]
use liftassistant_core::{DispatchRejection, DispatchSelectionReason};

use crate::app::App;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    HallCall(HallCall),
    SetMode {
        car: CarId,
        mode: OperatingMode,
    },
    /// Simulator-only, one-shot developer failure control.
    RejectNextIntent,
    /// Exactly one application FIXED_STEP, with no wall-clock dependency.
    Step,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticEvent {
    HallCall(HallCall),
    InvalidCall(HallCallRejection),
    CallRegistered(CallId),
    CallDeduplicated(CallId),
    CallStateChanged {
        id: CallId,
        state: CallState,
    },
    DispatchCandidate {
        id: CallId,
        candidate: DispatchCandidate,
    },
    NoEligibleCar(CallId),
    DispatchSelected {
        id: CallId,
        selection: DispatchSelection,
    },
    IntentGenerated {
        id: CallId,
        intent: Intent,
    },
    ModeChanged {
        car: CarId,
        mode: OperatingMode,
    },
    SimulatorIntentRejectionArmed,
    RuntimeError(String),
    Runtime(RuntimeEvent),
}

pub struct Replay {
    pub app: App,
    pub events: Vec<SemanticEvent>,
}

#[cfg(test)]
pub fn run(commands: &[Command]) -> Replay {
    run_with_topology(commands, BuildingTopology::reference_eight_floor())
}

pub fn run_with_topology(commands: &[Command], topology: BuildingTopology) -> Replay {
    let mut app = App::with_topology(topology);
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
    use liftassistant_runtime::ObservationSource;

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
        assert!(first.events.contains(&SemanticEvent::DispatchSelected {
            id: CallId::new(1),
            selection: DispatchSelection {
                car: CarId::new("A"),
                distance_floors: 4.0,
                reason: DispatchSelectionReason::Nearest,
            },
        }));
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
        assert!(first.events.contains(&SemanticEvent::CallStateChanged {
            id: CallId::new(1),
            state: CallState::Serving(CarId::new("A")),
        }));
        assert!(first.events.contains(&SemanticEvent::CallStateChanged {
            id: CallId::new(1),
            state: CallState::Completed,
        }));
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

    #[test]
    fn replay_and_reset_preserve_the_building_topology() {
        let topology = BuildingTopology::new([Floor::new(1), Floor::new(3), Floor::new(5)])
            .expect("nonempty topology");
        let mut app = App::with_topology(topology.clone());
        app.call(Direction::Up);
        app.step();

        app.replay();

        assert_eq!(app.topology, topology);
        assert!(app.trace.back().unwrap().contains("REPLAY verified"));
        app.reset();
        assert_eq!(app.topology, topology);
        assert_eq!(app.selected_floor, Floor::new(3));
    }

    #[test]
    fn multiple_calls_use_idle_cars_and_keep_excess_work_pending() {
        let commands = vec![
            Command::HallCall(HallCall {
                floor: Floor::new(7),
                direction: Direction::Up,
            }),
            Command::HallCall(HallCall {
                floor: Floor::new(5),
                direction: Direction::Down,
            }),
            Command::HallCall(HallCall {
                floor: Floor::new(2),
                direction: Direction::Up,
            }),
        ];

        let first = run(&commands);
        let second = run(&commands);
        let states = first.app.runtime.car_states();

        assert_eq!(first.events, second.events);
        assert_eq!(states[0].target, Some(Floor::new(7)));
        assert_eq!(states[1].target, Some(Floor::new(5)));
        assert_eq!(
            first
                .app
                .calls
                .get(CallId::new(1))
                .map(|record| &record.state),
            Some(&CallState::Assigned(CarId::new("A")))
        );
        assert_eq!(
            first
                .app
                .calls
                .get(CallId::new(2))
                .map(|record| &record.state),
            Some(&CallState::Assigned(CarId::new("B")))
        );
        assert_eq!(
            first
                .app
                .calls
                .get(CallId::new(3))
                .map(|record| &record.state),
            Some(&CallState::Pending)
        );
        assert!(
            first
                .events
                .contains(&SemanticEvent::NoEligibleCar(CallId::new(3)))
        );
    }

    #[test]
    fn runtime_feedback_completes_calls_and_reassigns_pending_work() {
        let mut app = App::new();
        for command in [
            Command::HallCall(HallCall {
                floor: Floor::new(7),
                direction: Direction::Up,
            }),
            Command::HallCall(HallCall {
                floor: Floor::new(5),
                direction: Direction::Down,
            }),
            Command::HallCall(HallCall {
                floor: Floor::new(2),
                direction: Direction::Up,
            }),
        ] {
            app.execute(command);
        }

        let mut lifecycle_events = Vec::new();
        for _ in 0..6 {
            lifecycle_events.extend(app.execute(Command::Step));
        }

        assert_eq!(
            app.calls.get(CallId::new(1)).map(|record| &record.state),
            Some(&CallState::Completed)
        );
        assert_eq!(
            app.calls.get(CallId::new(2)).map(|record| &record.state),
            Some(&CallState::Completed)
        );
        assert_eq!(
            app.calls.get(CallId::new(3)).map(|record| &record.state),
            Some(&CallState::Assigned(CarId::new("A")))
        );
        assert_eq!(app.runtime.car_states()[0].target, Some(Floor::new(2)));
        assert!(lifecycle_events.contains(&SemanticEvent::CallStateChanged {
            id: CallId::new(1),
            state: CallState::Completed,
        }));
        assert!(lifecycle_events.contains(&SemanticEvent::DispatchSelected {
            id: CallId::new(3),
            selection: DispatchSelection {
                car: CarId::new("A"),
                distance_floors: 5.0,
                reason: DispatchSelectionReason::OnlyEligible,
            },
        }));
        assert!(
            lifecycle_events.contains(&SemanticEvent::DispatchCandidate {
                id: CallId::new(3),
                candidate: DispatchCandidate {
                    car: CarId::new("B"),
                    mode: OperatingMode::Normal,
                    busy: true,
                    serve_floor: Some(true),
                    distance_floors: 3.0,
                    rejections: vec![DispatchRejection::Busy],
                },
            })
        );

        for _ in 0..7 {
            app.execute(Command::Step);
        }
        assert_eq!(
            app.calls.get(CallId::new(3)).map(|record| &record.state),
            Some(&CallState::Completed)
        );

        let first = run(&app.recording);
        let second = run(&app.recording);
        assert_eq!(first.events, second.events);
        assert_eq!(first.app.calls, app.calls);
        assert_eq!(first.app.runtime.car_states(), app.runtime.car_states());
    }
}

#[cfg(test)]
mod availability_tests {
    use super::*;
    use liftassistant_model::{Direction, DoorState, Floor, OperatingMode};
    use liftassistant_runtime::ObservationSource;

    #[test]
    fn unavailable_preferred_car_falls_back_end_to_end_and_replays() {
        let mut app = App::new();
        app.paused = true;
        app.toggle_availability("A");
        app.call(Direction::Up);
        let intent = Intent::ServeFloor {
            car: CarId::new("B"),
            floor: Floor::new(7),
        };
        assert_eq!(app.decision, Some(intent.clone()));
        for _ in 0..8 {
            app.step();
        }
        let states = app.runtime.car_states();
        assert_eq!(states[0].mode, OperatingMode::OutOfService);
        assert_eq!(states[0].position.floor_position, 3.0);
        assert_eq!(states[1].position.floor_position, 7.0);
        assert_eq!(states[1].door, DoorState::Open);
        let first = run(&app.recording);
        let second = run(&app.recording);
        assert_eq!(first.events, second.events);
        assert!(first.events.contains(&SemanticEvent::ModeChanged {
            car: CarId::new("A"),
            mode: OperatingMode::OutOfService
        }));
        assert!(first.events.contains(&SemanticEvent::DispatchSelected {
            id: CallId::new(1),
            selection: DispatchSelection {
                car: CarId::new("B"),
                distance_floors: 6.0,
                reason: DispatchSelectionReason::OnlyEligible,
            },
        }));
        assert!(first.events.contains(&SemanticEvent::IntentGenerated {
            id: CallId::new(1),
            intent: intent.clone(),
        }));
        assert!(
            first
                .events
                .contains(&SemanticEvent::Runtime(RuntimeEvent::IntentAccepted(
                    intent
                )))
        );
        assert!(
            first
                .events
                .contains(&SemanticEvent::Runtime(RuntimeEvent::Arrived {
                    car: CarId::new("B"),
                    floor: Floor::new(7)
                }))
        );
        app.replay();
        assert_eq!(app.runtime.car_states(), states);
        assert!(app.trace.back().unwrap().contains("REPLAY verified"));
        app.reset();
        assert!(
            app.runtime
                .car_states()
                .iter()
                .all(|car| car.mode == OperatingMode::Normal && car.target.is_none())
        );
    }

    #[test]
    fn no_eligible_car_submits_nothing_and_restoring_availability_recovers() {
        let mut app = App::new();
        app.toggle_availability("A");
        app.toggle_availability("B");
        let events = app.execute(Command::HallCall(HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        }));
        assert!(events.contains(&SemanticEvent::NoEligibleCar(CallId::new(1))));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, SemanticEvent::IntentGenerated { .. }))
        );
        assert!(app.decision.is_none());
        assert!(
            app.runtime
                .car_states()
                .iter()
                .all(|car| car.target.is_none())
        );
        app.toggle_availability("A");
        app.call(Direction::Up);
        assert_eq!(
            app.decision,
            Some(Intent::ServeFloor {
                car: CarId::new("A"),
                floor: Floor::new(7)
            })
        );
    }

    #[test]
    fn runtime_errors_leave_the_application_usable() {
        let mut app = App::new();
        let events = app.execute(Command::SetMode {
            car: CarId::new("missing"),
            mode: OperatingMode::OutOfService,
        });
        assert!(
            matches!(&events[..], [SemanticEvent::RuntimeError(message)] if message.contains("unknown car"))
        );
        assert!(app.trace.back().unwrap().contains("Runtime error"));
        app.call(Direction::Up);
        assert!(app.decision.is_some());
        app.step();
        assert_eq!(app.runtime.car_states()[0].position.floor_position, 3.75);
    }
}
