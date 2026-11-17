use std::{collections::BTreeMap, collections::VecDeque, time::Duration};

use liftassistant_model::{
    Capabilities, CarId, CarObservationUpdate, CarState, Intent, ObservationProvenance,
    ObservationSequence, ObservationSessionId, ObservationSourceId,
};
use liftassistant_runtime::{
    CarStateResolution, ObservationAssembler, RuntimeError, RuntimeEvent, SimulationDriver,
    resolve_car_observation,
};

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
    assembler: ObservationAssembler,
    source_session: ObservationSessionId,
    initial_observations: Vec<CarObservationUpdate>,
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

        let mut assembler = ObservationAssembler::new();
        let source = ObservationSourceId::new("recorded-read-only");
        let source_session = ObservationSessionId::new("recorded-session-1");
        let mut next_sequence = 1_u64;
        let initial_observations = states
            .values()
            .map(|state| {
                let update = CarObservationUpdate::from_state(
                    ObservationProvenance {
                        source: source.clone(),
                        session: source_session.clone(),
                        sequence: ObservationSequence::new(next_sequence),
                    },
                    state,
                );
                next_sequence = next_sequence
                    .checked_add(1)
                    .expect("recorded initial observation sequence exhausted");
                assembler
                    .apply(update.clone())
                    .expect("generated initial observations are ordered");
                update
            })
            .collect();
        let frames: VecDeque<_> = frames.into_iter().collect();
        let mut validation_assembler = assembler.clone();
        for frame in &frames {
            for event in &frame.events {
                let car = event_car(event);
                if !states.contains_key(car) {
                    return Err(RuntimeError::new(format!(
                        "recorded event references unknown car {car}"
                    )));
                }
                if let RuntimeEvent::Observation(update) = event {
                    if update.provenance.source != source {
                        return Err(RuntimeError::new(format!(
                            "recorded observation source {} does not match feed source {source}",
                            update.provenance.source
                        )));
                    }
                    if update.provenance.session != source_session {
                        return Err(RuntimeError::new(format!(
                            "recorded observation session {} does not match feed session {source_session}",
                            update.provenance.session
                        )));
                    }
                    validation_assembler
                        .apply(update.clone())
                        .map_err(|error| {
                            RuntimeError::new(format!(
                                "recorded observation sequence is invalid: {error:?}"
                            ))
                        })?;
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
            assembler,
            source_session,
            initial_observations,
            frames,
            until_next_frame,
        })
    }

    pub fn remaining_frames(&self) -> usize {
        self.frames.len()
    }

    pub fn source_name(&self) -> &str {
        "recorded-read-only"
    }

    pub fn source_session(&self) -> &ObservationSessionId {
        &self.source_session
    }

    pub fn car_states(&self) -> Vec<CarState> {
        self.states.values().cloned().collect()
    }

    pub fn capabilities(&self, car: &CarId) -> Option<Capabilities> {
        self.capabilities.get(car).cloned()
    }

    pub fn take_observation_updates(&mut self) -> Vec<CarObservationUpdate> {
        std::mem::take(&mut self.initial_observations)
    }

    fn apply(&mut self, event: &RuntimeEvent) {
        match event {
            RuntimeEvent::IntentAccepted(_) => {}
            RuntimeEvent::Observation(update) => {
                let car = update.car.clone();
                let assembled = self
                    .assembler
                    .apply(update.clone())
                    .expect("recorded observation sequences are validated while applying");
                match assembled
                    .snapshot()
                    .map(|snapshot| resolve_car_observation(&snapshot))
                {
                    Some(CarStateResolution::Resolved(state)) => {
                        self.states.insert(car, state);
                    }
                    Some(CarStateResolution::Unresolved { .. }) | None => {
                        self.states.remove(&car);
                    }
                }
            }
            RuntimeEvent::Arrived { .. } | RuntimeEvent::DoorStateChanged { .. } => {}
        }
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
        RuntimeEvent::Observation(update) => &update.car,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{
        CarLocation, Direction, DoorState, Floor, MotionState, OperatingMode, Position,
        PositionEvidence, ReportedDestination,
    };

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

    fn observation(sequence: u64, state: &CarState) -> RuntimeEvent {
        RuntimeEvent::Observation(CarObservationUpdate::from_state(
            ObservationProvenance {
                source: ObservationSourceId::new("recorded-read-only"),
                session: ObservationSessionId::new("recorded-session-1"),
                sequence: ObservationSequence::new(sequence),
            },
            state,
        ))
    }

    fn recording() -> RecordedRuntime {
        RecordedRuntime::new(
            [CarState::new("A", Floor::new(3))],
            [
                RecordedFrame::after(
                    Duration::from_millis(500),
                    vec![observation(2, &moving_state(3.5))],
                ),
                RecordedFrame::after(
                    Duration::from_millis(500),
                    vec![
                        observation(3, &{
                            let mut state = CarState::new("A", Floor::new(4));
                            state.door = DoorState::Open;
                            state
                        }),
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
        assert_eq!(runtime.advance(Duration::from_millis(1)).len(), 3);

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

        assert!(matches!(events[0], RuntimeEvent::Observation(_)));
        assert!(matches!(events[1], RuntimeEvent::Observation(_)));
        assert!(matches!(events[2], RuntimeEvent::Arrived { .. }));
        assert!(matches!(events[3], RuntimeEvent::DoorStateChanged { .. }));
        assert_eq!(runtime.remaining_frames(), 0);
        assert_eq!(
            runtime.car_states()[0].location,
            CarLocation::Floor(Floor::new(4))
        );
    }

    #[test]
    fn lifecycle_events_do_not_bypass_canonical_observation_state() {
        let mut runtime = RecordedRuntime::new(
            [CarState::new("A", Floor::new(3))],
            [RecordedFrame::after(
                Duration::ZERO,
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
            )],
        )
        .expect("recorded car is known");

        runtime.advance(Duration::ZERO);

        let state = &runtime.car_states()[0];
        assert_eq!(state.location, CarLocation::Floor(Floor::new(3)));
        assert_eq!(state.door, DoorState::Closed);
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

    #[test]
    fn recording_rejects_non_increasing_observation_sequence_before_playback() {
        let state = CarState::new("A", Floor::new(3));
        let result = RecordedRuntime::new(
            [state.clone()],
            [RecordedFrame::after(
                Duration::ZERO,
                vec![observation(1, &state)],
            )],
        );

        assert!(
            matches!(result, Err(error) if error.to_string().contains("NonIncreasingSequence"))
        );
    }

    #[test]
    fn recording_rejects_cross_source_observation_mixing() {
        let state = CarState::new("A", Floor::new(3));
        let mut update = match observation(2, &state) {
            RuntimeEvent::Observation(update) => update,
            _ => unreachable!("observation helper always returns an observation"),
        };
        update.provenance.source = ObservationSourceId::new("fixture/other");

        let result = RecordedRuntime::new(
            [state],
            [RecordedFrame::after(
                Duration::ZERO,
                vec![RuntimeEvent::Observation(update)],
            )],
        );

        assert!(
            matches!(result, Err(error) if error.to_string().contains("does not match feed source"))
        );
    }

    #[test]
    fn explicit_unknown_removes_stale_resolved_runtime_view() {
        let state = CarState::new("A", Floor::new(3));
        let mut update = match observation(2, &state) {
            RuntimeEvent::Observation(update) => update,
            _ => unreachable!("observation helper always returns an observation"),
        };
        update.position = Some(PositionEvidence::Unknown);
        let mut runtime = RecordedRuntime::new(
            [state],
            [RecordedFrame::after(
                Duration::ZERO,
                vec![RuntimeEvent::Observation(update)],
            )],
        )
        .expect("recorded car is known");

        runtime.advance(Duration::ZERO);

        assert!(runtime.car_states().is_empty());
    }
}
