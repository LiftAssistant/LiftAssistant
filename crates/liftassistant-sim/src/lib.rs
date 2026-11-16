#![forbid(unsafe_code)]

mod bacnet_recorded;
mod recorded;
mod rmf_recorded;

use std::{collections::BTreeMap, time::Duration};

use liftassistant_model::{
    Capabilities, CarId, CarLocation, CarState, Direction, DoorState, Floor, Intent, MotionState,
    OperatingMode, Position, ReportedDestination,
};
use liftassistant_runtime::{
    IntentSink, ObservationSource, RuntimeError, RuntimeEvent, SimulationDriver,
};

pub use bacnet_recorded::{
    BacnetDoorStatus, BacnetLiftCarDirection, BacnetLiftCarMode, BacnetLiftSourceContext,
    BacnetMappingError, MappedBacnetObservation, RecordedBacnetLiftProperties,
    map_bacnet_lift_properties,
};
pub use recorded::{RecordedFrame, RecordedRuntime};
pub use rmf_recorded::{
    MappedRmfObservation, RmfLiftState, RmfMappingError, RmfSourceContext, map_rmf_lift_state,
};

#[derive(Debug)]
struct SimCar {
    state: CarState,
    capabilities: Capabilities,
    desired_target: Option<Floor>,
}

#[derive(Debug)]
pub struct SimRuntime {
    cars: BTreeMap<CarId, SimCar>,
    speed_floors_per_second: f64,
    pending_events: Vec<RuntimeEvent>,
    reject_next_intent: bool,
}

impl SimRuntime {
    pub fn new(
        cars: impl IntoIterator<Item = (CarState, Capabilities)>,
        speed_floors_per_second: f64,
    ) -> Self {
        let cars = cars
            .into_iter()
            .map(|(state, capabilities)| {
                (
                    state.id.clone(),
                    SimCar {
                        state,
                        capabilities,
                        desired_target: None,
                    },
                )
            })
            .collect();

        Self {
            cars,
            speed_floors_per_second,
            pending_events: Vec::new(),
            reject_next_intent: false,
        }
    }

    pub fn reference_two_car() -> Self {
        Self::new(
            [
                (
                    CarState::with_continuous_position("A", Position::at_floor(Floor::new(3))),
                    Capabilities::reference_defaults(),
                ),
                (
                    CarState::with_continuous_position("B", Position::at_floor(Floor::new(1))),
                    Capabilities::reference_defaults(),
                ),
            ],
            1.5,
        )
    }

    pub fn set_mode(&mut self, car: &CarId, mode: OperatingMode) -> Result<(), RuntimeError> {
        let sim_car = self
            .cars
            .get_mut(car)
            .ok_or_else(|| RuntimeError::new(format!("unknown car {car}")))?;
        sim_car.state.mode = mode;
        Ok(())
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

        let sim_car = self
            .cars
            .get_mut(car_id)
            .ok_or_else(|| RuntimeError::new(format!("unknown car {car_id}")))?;

        if sim_car.state.mode != OperatingMode::Normal {
            return Err(RuntimeError::new(format!(
                "car {car_id} is not available in normal mode"
            )));
        }

        if !sim_car.capabilities.serve_floor {
            return Err(RuntimeError::new(format!(
                "car {car_id} does not expose serve_floor"
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

        self.pending_events
            .push(RuntimeEvent::IntentAccepted(intent));
        Ok(())
    }
}

impl ObservationSource for SimRuntime {
    fn source_name(&self) -> &str {
        "simulator"
    }

    fn car_states(&self) -> Vec<CarState> {
        self.cars.values().map(|car| car.state.clone()).collect()
    }

    fn capabilities(&self, car: &CarId) -> Option<Capabilities> {
        self.cars
            .get(car)
            .map(|sim_car| sim_car.capabilities.clone())
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
        let step = self.speed_floors_per_second * elapsed.as_secs_f64();

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

                events.push(RuntimeEvent::PositionChanged(sim_car.state.clone()));
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

            events.push(RuntimeEvent::PositionChanged(sim_car.state.clone()));
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_floor_reaches_target_deterministically() {
        let mut runtime = SimRuntime::reference_two_car();
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
}
