#![forbid(unsafe_code)]

use std::{collections::BTreeMap, time::Duration};

use liftassistant_model::{
    Capabilities, CarId, CarState, Direction, DoorState, Floor, Intent, MotionState, OperatingMode,
    Position,
};
use liftassistant_runtime::{ElevatorRuntime, RuntimeError, RuntimeEvent};

#[derive(Debug)]
struct SimCar {
    state: CarState,
    capabilities: Capabilities,
}

#[derive(Debug)]
pub struct SimRuntime {
    cars: BTreeMap<CarId, SimCar>,
    speed_floors_per_second: f64,
    pending_events: Vec<RuntimeEvent>,
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
                    },
                )
            })
            .collect();

        Self {
            cars,
            speed_floors_per_second,
            pending_events: Vec::new(),
        }
    }

    pub fn reference_two_car() -> Self {
        Self::new(
            [
                (
                    CarState::new("A", Floor::new(3)),
                    Capabilities::reference_defaults(),
                ),
                (
                    CarState::new("B", Floor::new(1)),
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

        let delta = f64::from(floor.index()) - sim_car.state.position.floor_position;
        sim_car.state.target = Some(floor);
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

impl ElevatorRuntime for SimRuntime {
    fn runtime_name(&self) -> &str {
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

    fn submit_intent(&mut self, intent: Intent) -> Result<(), RuntimeError> {
        self.set_target(intent)
    }

    fn tick(&mut self, elapsed: Duration) -> Vec<RuntimeEvent> {
        let mut events = std::mem::take(&mut self.pending_events);
        let step = self.speed_floors_per_second * elapsed.as_secs_f64();

        for sim_car in self.cars.values_mut() {
            let Some(target) = sim_car.state.target else {
                continue;
            };

            let target_position = f64::from(target.index());
            let current = sim_car.state.position.floor_position;
            let delta = target_position - current;

            if delta.abs() <= step {
                sim_car.state.position = Position::at_floor(target);
                sim_car.state.motion = MotionState::Stopped;
                sim_car.state.target = None;
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
            sim_car.state.position = Position {
                floor_position: next,
                nearest_floor: Floor::new(next.round() as i16),
            };
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
            let events = runtime.tick(Duration::from_millis(500));
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
}
