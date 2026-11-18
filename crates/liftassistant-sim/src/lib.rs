#![forbid(unsafe_code)]

mod bacnet_recorded;
mod recorded;

use std::{collections::BTreeMap, time::Duration};

use liftassistant_model::{
    CarId, CarLocation, CarObservationUpdate, CarState, Direction, DoorState,
    ExecutionCapabilities, Floor, Intent, MotionState, ObservationProvenance, ObservationSequence,
    ObservationSessionId, ObservationSourceId, OperatingMode, Position, ReportedDestination,
};
use liftassistant_runtime::{IntentSink, RuntimeError, RuntimeEvent, SimulationDriver};

pub use bacnet_recorded::{
    BacnetDoorStatus, BacnetLiftCarDirection, BacnetLiftCarMode, BacnetLiftSourceContext,
    BacnetMappingError, MappedBacnetObservation, RecordedBacnetLiftProperties,
    map_bacnet_lift_properties,
};
pub use liftassistant_rmf::{
    MappedRmfObservation, RmfInteractionModeProjection, RmfLiftState, RmfMappingError,
    RmfSourceContext, map_rmf_lift_state, map_rmf_lift_state_with_projection,
};
pub use recorded::{ObservationCoverage, RecordedFrame, RecordedRuntime};

#[derive(Debug)]
struct SimCar {
    state: CarState,
    execution_capabilities: ExecutionCapabilities,
    desired_target: Option<Floor>,
}

#[derive(Debug)]
pub struct SimRuntime {
    cars: BTreeMap<CarId, SimCar>,
    speed_floors_per_second: f64,
    source: ObservationSourceId,
    source_session: ObservationSessionId,
    next_sequence: u64,
    pending_observations: Vec<CarObservationUpdate>,
    pending_events: Vec<RuntimeEvent>,
    reject_next_intent: bool,
}

impl SimRuntime {
    pub fn new(
        cars: impl IntoIterator<Item = (CarState, ExecutionCapabilities)>,
        speed_floors_per_second: f64,
    ) -> Self {
        let cars = cars
            .into_iter()
            .map(|(state, execution_capabilities)| {
                (
                    state.id.clone(),
                    SimCar {
                        state,
                        execution_capabilities,
                        desired_target: None,
                    },
                )
            })
            .collect();

        let mut runtime = Self {
            cars,
            speed_floors_per_second,
            source: ObservationSourceId::new("simulator"),
            source_session: ObservationSessionId::new("simulator-session-1"),
            next_sequence: 1,
            pending_observations: Vec::new(),
            pending_events: Vec::new(),
            reject_next_intent: false,
        };
        let initial_states = runtime.car_states();
        for state in initial_states {
            runtime.queue_observation(state);
        }
        runtime
    }

    pub fn reference_two_car() -> Self {
        Self::new(
            [
                (
                    CarState::with_continuous_position("A", Position::at_floor(Floor::new(3))),
                    ExecutionCapabilities::reference_defaults(),
                ),
                (
                    CarState::with_continuous_position("B", Position::at_floor(Floor::new(1))),
                    ExecutionCapabilities::reference_defaults(),
                ),
            ],
            1.5,
        )
    }

    pub fn set_mode(&mut self, car: &CarId, mode: OperatingMode) -> Result<(), RuntimeError> {
        let state = {
            let sim_car = self
                .cars
                .get_mut(car)
                .ok_or_else(|| RuntimeError::new(format!("unknown car {car}")))?;
            sim_car.state.mode = mode;
            sim_car.state.clone()
        };
        self.queue_observation(state);
        Ok(())
    }

    pub fn source_name(&self) -> &str {
        self.source.as_str()
    }

    pub fn source_session(&self) -> &ObservationSessionId {
        &self.source_session
    }

    pub fn car_states(&self) -> Vec<CarState> {
        self.cars.values().map(|car| car.state.clone()).collect()
    }

    pub fn execution_capabilities(&self, car: &CarId) -> Option<ExecutionCapabilities> {
        self.cars
            .get(car)
            .map(|sim_car| sim_car.execution_capabilities.clone())
    }

    pub fn all_execution_capabilities(&self) -> Vec<(CarId, ExecutionCapabilities)> {
        self.cars
            .iter()
            .map(|(car, sim_car)| (car.clone(), sim_car.execution_capabilities.clone()))
            .collect()
    }

    pub fn take_observation_updates(&mut self) -> Vec<CarObservationUpdate> {
        std::mem::take(&mut self.pending_observations)
    }

    /// Arms a one-shot developer control at the simulator boundary.
    ///
    /// The next submitted intent is rejected without changing car state. This models an
    /// integration failure for deterministic scenarios; it is not an equipment fault signal.
    pub fn reject_next_intent(&mut self) {
        self.reject_next_intent = true;
    }

    fn set_target(&mut self, intent: Intent) -> Result<(), RuntimeError> {
        let (car_id, floor) = match &intent {
            Intent::ServeFloor { car, floor } | Intent::ParkAt { car, floor } => (car, *floor),
        };

        let state = {
            let sim_car = self
                .cars
                .get_mut(car_id)
                .ok_or_else(|| RuntimeError::new(format!("unknown car {car_id}")))?;

            if sim_car.state.mode != OperatingMode::Normal {
                return Err(RuntimeError::new(format!(
                    "car {car_id} is not available in normal mode"
                )));
            }

            let (supported, capability) = match intent {
                Intent::ServeFloor { .. } => {
                    (sim_car.execution_capabilities.serve_floor, "serve_floor")
                }
                Intent::ParkAt { .. } => (sim_car.execution_capabilities.park, "park"),
            };
            if !supported {
                return Err(RuntimeError::new(format!(
                    "car {car_id} does not expose {capability}"
                )));
            }
            if !sim_car.execution_capabilities.supports_floor(floor) {
                return Err(RuntimeError::new(format!(
                    "car {car_id} cannot execute at floor {floor}"
                )));
            }

            let current = match sim_car.state.location {
                CarLocation::Floor(floor) => f64::from(floor.index()),
                CarLocation::Continuous(position) => position.floor_position,
            };
            let delta = f64::from(floor.index()) - current;
            sim_car.state.location = CarLocation::Continuous(Position {
                floor_position: current,
                nearest_floor: sim_car.state.location.floor_reference(),
            });
            sim_car.desired_target = Some(floor);
            sim_car.state.reported_destination = ReportedDestination::Known(floor);
            sim_car.state.door = DoorState::Closed;
            sim_car.state.motion = if delta > 0.0 {
                MotionState::Moving(Direction::Up)
            } else if delta < 0.0 {
                MotionState::Moving(Direction::Down)
            } else {
                MotionState::Stopped
            };
            sim_car.state.clone()
        };

        self.queue_observation(state);
        self.pending_events
            .push(RuntimeEvent::IntentAccepted(intent));
        Ok(())
    }

    fn queue_observation(&mut self, state: CarState) {
        let provenance = ObservationProvenance {
            source: self.source.clone(),
            session: self.source_session.clone(),
            sequence: ObservationSequence::new(self.next_sequence),
        };
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("simulator observation sequence exhausted");
        self.pending_observations
            .push(CarObservationUpdate::from_state(provenance, &state));
    }
}

impl IntentSink for SimRuntime {
    fn submit_intent(&mut self, intent: Intent) -> Result<(), RuntimeError> {
        if std::mem::take(&mut self.reject_next_intent) {
            return Err(RuntimeError::new(
                "simulator injected one-shot intent rejection",
            ));
        }
        self.set_target(intent)
    }
}

impl SimulationDriver for SimRuntime {
    fn advance(&mut self, elapsed: Duration) -> Vec<RuntimeEvent> {
        let mut events = std::mem::take(&mut self.pending_events);
        events.extend(
            self.take_observation_updates()
                .into_iter()
                .map(RuntimeEvent::Observation),
        );
        let step = self.speed_floors_per_second * elapsed.as_secs_f64();
        let source = self.source.clone();
        let source_session = self.source_session.clone();
        let mut next_sequence = self.next_sequence;

        for sim_car in self.cars.values_mut() {
            let Some(target) = sim_car.desired_target else {
                continue;
            };

            let target_position = f64::from(target.index());
            let current = sim_car
                .state
                .location
                .continuous_position()
                .expect("simulator intents establish a continuous starting position")
                .floor_position;
            let delta = target_position - current;

            if delta.abs() <= step {
                sim_car.state.location = CarLocation::Continuous(Position::at_floor(target));
                sim_car.state.motion = MotionState::Stopped;
                sim_car.desired_target = None;
                sim_car.state.reported_destination = ReportedDestination::None;
                sim_car.state.door = DoorState::Open;

                events.push(RuntimeEvent::Observation(observation_from_state(
                    &source,
                    &source_session,
                    &mut next_sequence,
                    &sim_car.state,
                )));
                events.push(RuntimeEvent::Arrived {
                    car: sim_car.state.id.clone(),
                    floor: target,
                });
                events.push(RuntimeEvent::DoorStateChanged {
                    car: sim_car.state.id.clone(),
                    state: DoorState::Open,
                });
                continue;
            }

            let direction = if delta > 0.0 {
                Direction::Up
            } else {
                Direction::Down
            };
            let next = current + step.copysign(delta);
            sim_car.state.location = CarLocation::Continuous(Position {
                floor_position: next,
                nearest_floor: Floor::new(next.round() as i16),
            });
            sim_car.state.motion = MotionState::Moving(direction);
            sim_car.state.door = DoorState::Closed;

            events.push(RuntimeEvent::Observation(observation_from_state(
                &source,
                &source_session,
                &mut next_sequence,
                &sim_car.state,
            )));
        }
        self.next_sequence = next_sequence;

        events
    }
}

fn observation_from_state(
    source: &ObservationSourceId,
    session: &ObservationSessionId,
    next_sequence: &mut u64,
    state: &CarState,
) -> CarObservationUpdate {
    let provenance = ObservationProvenance {
        source: source.clone(),
        session: session.clone(),
        sequence: ObservationSequence::new(*next_sequence),
    };
    *next_sequence = next_sequence
        .checked_add(1)
        .expect("simulator observation sequence exhausted");
    CarObservationUpdate::from_state(provenance, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_floor_reaches_target_deterministically() {
        let mut runtime = SimRuntime::reference_two_car();
        runtime.take_observation_updates();
        runtime
            .submit_intent(Intent::ServeFloor {
                car: CarId::new("A"),
                floor: Floor::new(8),
            })
            .expect("intent should be accepted");

        let mut arrived = false;
        for _ in 0..20 {
            let events = runtime.advance(Duration::from_millis(500));
            arrived |= events.iter().any(|event| {
                matches!(
                    event,
                    RuntimeEvent::Arrived { car, floor }
                        if car == &CarId::new("A") && *floor == Floor::new(8)
                )
            });
            if arrived {
                break;
            }
        }

        assert!(arrived);
    }

    #[test]
    fn one_shot_intent_rejection_preserves_state_and_then_recovers() {
        let mut runtime = SimRuntime::reference_two_car();
        runtime.take_observation_updates();
        let initial = runtime.car_states();
        let intent = Intent::ServeFloor {
            car: CarId::new("A"),
            floor: Floor::new(7),
        };

        runtime.reject_next_intent();
        assert_eq!(
            runtime.submit_intent(intent.clone()),
            Err(RuntimeError::new(
                "simulator injected one-shot intent rejection"
            ))
        );
        assert_eq!(runtime.car_states(), initial);
        assert!(runtime.advance(Duration::ZERO).is_empty());

        runtime
            .submit_intent(intent.clone())
            .expect("one-shot rejection should be consumed");
        assert_eq!(
            runtime.car_states()[0].reported_destination,
            ReportedDestination::Known(Floor::new(7))
        );
        assert!(
            runtime
                .advance(Duration::from_millis(500))
                .contains(&RuntimeEvent::IntentAccepted(intent))
        );
    }

    #[test]
    fn each_execution_intent_checks_its_own_capability() {
        let state = CarState::with_continuous_position("A", Position::at_floor(Floor::new(3)));
        let mut runtime = SimRuntime::new(
            [(
                state,
                ExecutionCapabilities::new(true, false, (1..=8).map(Floor::new)),
            )],
            1.5,
        );
        runtime.take_observation_updates();

        let error = runtime
            .submit_intent(Intent::ParkAt {
                car: CarId::new("A"),
                floor: Floor::new(1),
            })
            .expect_err("park must use the park execution capability");
        assert_eq!(error.to_string(), "car A does not expose park");
        runtime
            .submit_intent(Intent::ServeFloor {
                car: CarId::new("A"),
                floor: Floor::new(4),
            })
            .expect("serve-floor remains independently enabled");
    }
}
