use std::{collections::BTreeMap, collections::VecDeque, time::Duration};

use liftassistant_model::{
    Capabilities, CarId, CarLocation, CarState, Intent, MotionState, ReportedDestination,
};
use liftassistant_runtime::{ObservationSource, RuntimeError, RuntimeEvent, SimulationDriver};

/// A timed batch of native LiftAssistant observations.
///
/// This is an internal in-memory research representation, not a recording file schema.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedFrame {
    pub after: Duration,
    pub events: Vec<RuntimeEvent>,
}

impl RecordedFrame {
    pub fn after(after: Duration, events: Vec<RuntimeEvent>) -> Self {
        Self { after, events }
    }
}

/// Deterministic read-only playback of native semantic runtime observations.
#[derive(Debug)]
pub struct RecordedRuntime {
    states: BTreeMap<CarId, CarState>,
    capabilities: BTreeMap<CarId, Capabilities>,
    frames: VecDeque<RecordedFrame>,
    until_next_frame: Option<Duration>,
}

impl RecordedRuntime {
    pub fn new(
        initial_states: impl IntoIterator<Item = CarState>,
        frames: impl IntoIterator<Item = RecordedFrame>,
    ) -> Result<Self, RuntimeError> {
        let mut states = BTreeMap::new();
        for state in initial_states {
            if states.insert(state.id.clone(), state).is_some() {
                return Err(RuntimeError::new(
                    "recorded runtime initial state contains a duplicate car",
                ));
            }
        }

        let frames: VecDeque<_> = frames.into_iter().collect();
        for frame in &frames {
            for event in &frame.events {
                let car = event_car(event);
                if !states.contains_key(car) {
                    return Err(RuntimeError::new(format!(
                        "recorded event references unknown car {car}"
                    )));
                }
            }
        }

        let capabilities = states
            .keys()
            .cloned()
            .map(|car| {
                (
                    car,
                    Capabilities {
                        observe_position: true,
                        observe_door: true,
                        serve_floor: false,
                        park: false,
                    },
                )
            })
            .collect();
        let until_next_frame = frames.front().map(|frame| frame.after);
        Ok(Self {
            states,
            capabilities,
            frames,
            until_next_frame,
        })
    }

    pub fn remaining_frames(&self) -> usize {
        self.frames.len()
    }

    fn apply(&mut self, event: &RuntimeEvent) {
        match event {
            RuntimeEvent::IntentAccepted(_) => {}
            RuntimeEvent::PositionChanged(state) => {
                self.states.insert(state.id.clone(), state.clone());
            }
            RuntimeEvent::Arrived { car, floor } => {
                let state = self
                    .states
                    .get_mut(car)
                    .expect("recorded event cars are validated at construction");
                state.location = CarLocation::Floor(*floor);
                state.motion = MotionState::Stopped;
                state.reported_destination = ReportedDestination::None;
            }
            RuntimeEvent::DoorStateChanged { car, state: door } => {
                self.states
                    .get_mut(car)
                    .expect("recorded event cars are validated at construction")
                    .door = *door;
            }
        }
    }
}

impl ObservationSource for RecordedRuntime {
    fn source_name(&self) -> &str {
        "recorded-read-only"
    }

    fn car_states(&self) -> Vec<CarState> {
        self.states.values().cloned().collect()
    }

    fn capabilities(&self, car: &CarId) -> Option<Capabilities> {
        self.capabilities.get(car).cloned()
    }
}

impl SimulationDriver for RecordedRuntime {
    fn advance(&mut self, mut elapsed: Duration) -> Vec<RuntimeEvent> {
        let mut emitted = Vec::new();
        while let Some(until_next) = self.until_next_frame {
            if elapsed < until_next {
                self.until_next_frame = Some(until_next - elapsed);
                break;
            }

            elapsed -= until_next;
            let frame = self
                .frames
                .pop_front()
                .expect("next-frame duration exists only with a recorded frame");
            for event in &frame.events {
                self.apply(event);
            }
            emitted.extend(frame.events);
            self.until_next_frame = self.frames.front().map(|next| next.after);
        }
        emitted
    }
}

fn event_car(event: &RuntimeEvent) -> &CarId {
    match event {
        RuntimeEvent::IntentAccepted(Intent::ServeFloor { car, .. })
        | RuntimeEvent::IntentAccepted(Intent::ParkAt { car, .. })
        | RuntimeEvent::Arrived { car, .. }
        | RuntimeEvent::DoorStateChanged { car, .. } => car,
        RuntimeEvent::PositionChanged(state) => &state.id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{Direction, DoorState, Floor, OperatingMode, Position};

    fn moving_state(position: f64) -> CarState {
        CarState {
            id: CarId::new("A"),
            location: CarLocation::Continuous(Position {
                floor_position: position,
                nearest_floor: Floor::new(position.round() as i16),
            }),
            motion: MotionState::Moving(Direction::Up),
            door: DoorState::Closed,
            mode: OperatingMode::Normal,
            reported_destination: ReportedDestination::Known(Floor::new(4)),
        }
    }

    fn recording() -> RecordedRuntime {
        RecordedRuntime::new(
            [CarState::new("A", Floor::new(3))],
            [
                RecordedFrame::after(
                    Duration::from_millis(500),
                    vec![RuntimeEvent::PositionChanged(moving_state(3.5))],
                ),
                RecordedFrame::after(
                    Duration::from_millis(500),
                    vec![
                        RuntimeEvent::Arrived {
                            car: CarId::new("A"),
                            floor: Floor::new(4),
                        },
                        RuntimeEvent::DoorStateChanged {
                            car: CarId::new("A"),
                            state: DoorState::Open,
                        },
                    ],
                ),
            ],
        )
        .expect("recorded cars are known")
    }

    #[test]
    fn explicit_elapsed_time_releases_frames_and_updates_observed_state() {
        let mut runtime = recording();

        assert!(runtime.advance(Duration::from_millis(250)).is_empty());
        assert_eq!(
            runtime.car_states()[0].location,
            CarLocation::Floor(Floor::new(3))
        );
        assert_eq!(runtime.advance(Duration::from_millis(250)).len(), 1);
        assert_eq!(
            runtime.car_states()[0].location,
            CarLocation::Continuous(Position {
                floor_position: 3.5,
                nearest_floor: Floor::new(4),
            })
        );
        assert_eq!(runtime.advance(Duration::from_millis(499)).len(), 0);
        assert_eq!(runtime.remaining_frames(), 1);
        assert_eq!(runtime.advance(Duration::from_millis(1)).len(), 2);

        let final_state = &runtime.car_states()[0];
        assert_eq!(final_state.location, CarLocation::Floor(Floor::new(4)));
        assert_eq!(final_state.motion, MotionState::Stopped);
        assert_eq!(final_state.door, DoorState::Open);
        assert_eq!(final_state.reported_destination, ReportedDestination::None);
        assert_eq!(runtime.remaining_frames(), 0);
    }

    #[test]
    fn read_only_source_exposes_observation_and_replay_contracts() {
        let mut runtime = recording();
        let initial = runtime.car_states();

        assert_eq!(runtime.source_name(), "recorded-read-only");
        assert_eq!(runtime.car_states(), initial);
        let capabilities = runtime
            .capabilities(&CarId::new("A"))
            .expect("recorded car has observation capabilities");
        assert!(capabilities.observe_position && capabilities.observe_door);
        assert!(!capabilities.serve_floor && !capabilities.park);
        assert_eq!(runtime.advance(Duration::from_millis(500)).len(), 1);
    }

    #[test]
    fn one_large_advance_releases_all_due_frames_in_order() {
        let mut runtime = recording();

        let events = runtime.advance(Duration::from_secs(2));

        assert!(matches!(events[0], RuntimeEvent::PositionChanged(_)));
        assert!(matches!(events[1], RuntimeEvent::Arrived { .. }));
        assert!(matches!(events[2], RuntimeEvent::DoorStateChanged { .. }));
        assert_eq!(runtime.remaining_frames(), 0);
        assert_eq!(
            runtime.car_states()[0].location,
            CarLocation::Floor(Floor::new(4))
        );
    }

    #[test]
    fn recording_rejects_events_for_unknown_cars() {
        let result = RecordedRuntime::new(
            [CarState::new("A", Floor::new(3))],
            [RecordedFrame::after(
                Duration::ZERO,
                vec![RuntimeEvent::Arrived {
                    car: CarId::new("missing"),
                    floor: Floor::new(4),
                }],
            )],
        );

        assert!(matches!(result, Err(error) if error.to_string().contains("unknown car missing")));
    }
}
