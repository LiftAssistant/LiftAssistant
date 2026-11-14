#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use liftassistant_model::{
    CallId, CallRecord, CallState, Capabilities, CarId, CarState, Floor, HallCall, Intent,
    OperatingMode,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallRegistration {
    Created(CallId),
    Duplicate(CallId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallRegistry {
    next_id: u64,
    calls: BTreeMap<CallId, CallRecord>,
    arrived: BTreeSet<CallId>,
}

impl CallRegistry {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            calls: BTreeMap::new(),
            arrived: BTreeSet::new(),
        }
    }

    pub fn register(&mut self, call: HallCall) -> CallRegistration {
        if let Some(existing) = self
            .calls
            .values()
            .find(|existing| existing.call == call && existing.state.is_active())
        {
            return CallRegistration::Duplicate(existing.id);
        }

        let id = CallId::new(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("deterministic call ID space exhausted");
        self.calls.insert(
            id,
            CallRecord {
                id,
                call,
                state: CallState::Pending,
            },
        );
        CallRegistration::Created(id)
    }

    pub fn get(&self, id: CallId) -> Option<&CallRecord> {
        self.calls.get(&id)
    }

    pub fn calls(&self) -> impl Iterator<Item = &CallRecord> {
        self.calls.values()
    }

    pub fn pending_calls(&self) -> impl Iterator<Item = (CallId, HallCall)> + '_ {
        self.calls.values().filter_map(|record| {
            (record.state == CallState::Pending).then_some((record.id, record.call))
        })
    }

    pub fn assigned_call(&self, car: &CarId, floor: Floor) -> Option<CallId> {
        self.calls.values().find_map(|record| {
            (record.call.floor == floor && record.state == CallState::Assigned(car.clone()))
                .then_some(record.id)
        })
    }

    pub fn serving_call(&self, car: &CarId, floor: Floor) -> Option<CallId> {
        self.calls.values().find_map(|record| {
            (record.call.floor == floor && record.state == CallState::Serving(car.clone()))
                .then_some(record.id)
        })
    }

    pub fn note_arrival(&mut self, car: &CarId, floor: Floor) -> Option<CallId> {
        let id = self.serving_call(car, floor)?;
        self.arrived.insert(id);
        Some(id)
    }

    pub fn complete_open_call(&mut self, car: &CarId) -> Option<CallId> {
        let id = self.arrived.iter().copied().find(|id| {
            self.calls.get(id).is_some_and(
                |record| matches!(&record.state, CallState::Serving(assigned) if assigned == car),
            )
        })?;
        self.complete(id, car).then_some(id)
    }

    pub fn car_is_occupied(&self, car: &CarId) -> bool {
        self.calls.values().any(|record| {
            matches!(
                &record.state,
                CallState::Assigned(assigned) | CallState::Serving(assigned) if assigned == car
            )
        })
    }

    pub fn assign(&mut self, id: CallId, car: CarId) -> bool {
        let Some(record) = self.calls.get_mut(&id) else {
            return false;
        };
        if record.state != CallState::Pending {
            return false;
        }
        record.state = CallState::Assigned(car);
        true
    }

    pub fn mark_serving(&mut self, id: CallId, car: &CarId) -> bool {
        let Some(record) = self.calls.get_mut(&id) else {
            return false;
        };
        if record.state != CallState::Assigned(car.clone()) {
            return false;
        }
        record.state = CallState::Serving(car.clone());
        true
    }

    pub fn complete(&mut self, id: CallId, car: &CarId) -> bool {
        let Some(record) = self.calls.get_mut(&id) else {
            return false;
        };
        if record.state != CallState::Serving(car.clone()) {
            return false;
        }
        record.state = CallState::Completed;
        self.arrived.remove(&id);
        true
    }
}

impl Default for CallRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn dispatch(
    call: &HallCall,
    cars: &[CarState],
    capabilities: &BTreeMap<CarId, Capabilities>,
) -> Option<Intent> {
    let car = cars
        .iter()
        .filter(|car| car.mode == OperatingMode::Normal)
        .filter(|car| car.target.is_none())
        .filter(|car| {
            capabilities
                .get(&car.id)
                .is_some_and(|caps| caps.serve_floor)
        })
        .min_by(|left, right| {
            let left_distance =
                (left.position.floor_position - f64::from(call.floor.index())).abs();
            let right_distance =
                (right.position.floor_position - f64::from(call.floor.index())).abs();

            left_distance
                .total_cmp(&right_distance)
                .then_with(|| left.id.cmp(&right.id))
        })?;

    Some(Intent::ServeFloor {
        car: car.id.clone(),
        floor: call.floor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{Direction, Floor};

    fn capabilities(cars: &[CarState]) -> BTreeMap<CarId, Capabilities> {
        cars.iter()
            .map(|car| (car.id.clone(), Capabilities::reference_defaults()))
            .collect()
    }

    #[test]
    fn call_ids_are_deterministic_and_only_active_duplicates_are_reused() {
        let call = HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        };
        let mut registry = CallRegistry::new();

        assert_eq!(
            registry.register(call),
            CallRegistration::Created(CallId::new(1))
        );
        assert_eq!(
            registry.register(call),
            CallRegistration::Duplicate(CallId::new(1))
        );
        assert!(registry.assign(CallId::new(1), CarId::new("A")));
        assert!(registry.mark_serving(CallId::new(1), &CarId::new("A")));
        assert!(registry.complete(CallId::new(1), &CarId::new("A")));
        assert_eq!(
            registry.get(CallId::new(1)).map(|record| &record.state),
            Some(&CallState::Completed)
        );
        assert_eq!(
            registry.register(call),
            CallRegistration::Created(CallId::new(2))
        );
    }

    #[test]
    fn lifecycle_transitions_require_the_current_assignment() {
        let mut registry = CallRegistry::new();
        let CallRegistration::Created(id) = registry.register(HallCall {
            floor: Floor::new(5),
            direction: Direction::Down,
        }) else {
            panic!("first registration must create a call");
        };

        assert!(!registry.mark_serving(id, &CarId::new("A")));
        assert!(registry.assign(id, CarId::new("A")));
        assert!(!registry.mark_serving(id, &CarId::new("B")));
        assert!(registry.mark_serving(id, &CarId::new("A")));
        assert!(!registry.complete(id, &CarId::new("B")));
        assert!(registry.complete(id, &CarId::new("A")));
        assert!(!registry.assign(id, CarId::new("B")));
    }

    #[test]
    fn arrival_evidence_can_precede_the_door_open_feedback() {
        let mut registry = CallRegistry::new();
        let CallRegistration::Created(id) = registry.register(HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        }) else {
            panic!("first registration must create a call");
        };
        let car = CarId::new("A");
        assert!(registry.assign(id, car.clone()));
        assert!(registry.mark_serving(id, &car));

        assert_eq!(registry.complete_open_call(&car), None);
        assert_eq!(registry.note_arrival(&car, Floor::new(7)), Some(id));
        assert_eq!(registry.complete_open_call(&car), Some(id));
        assert_eq!(
            registry.get(id).map(|record| &record.state),
            Some(&CallState::Completed)
        );
    }

    #[test]
    fn nearest_eligible_car_is_selected() {
        let cars = vec![
            CarState::new("A", Floor::new(3)),
            CarState::new("B", Floor::new(1)),
        ];
        let call = HallCall {
            floor: Floor::new(8),
            direction: Direction::Down,
        };

        let decision = dispatch(&call, &cars, &capabilities(&cars));

        assert_eq!(
            decision,
            Some(Intent::ServeFloor {
                car: CarId::new("A"),
                floor: Floor::new(8),
            })
        );
    }

    #[test]
    fn out_of_service_car_is_not_selected() {
        let mut a = CarState::new("A", Floor::new(7));
        a.mode = OperatingMode::OutOfService;
        let cars = vec![a, CarState::new("B", Floor::new(1))];
        let call = HallCall {
            floor: Floor::new(8),
            direction: Direction::Down,
        };

        let decision = dispatch(&call, &cars, &capabilities(&cars));

        assert_eq!(
            decision,
            Some(Intent::ServeFloor {
                car: CarId::new("B"),
                floor: Floor::new(8),
            })
        );
    }

    #[test]
    fn busy_car_is_not_selected_even_when_it_is_nearest() {
        let mut a = CarState::new("A", Floor::new(7));
        a.target = Some(Floor::new(8));
        let cars = vec![a, CarState::new("B", Floor::new(1))];
        let call = HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        };

        let decision = dispatch(&call, &cars, &capabilities(&cars));

        assert_eq!(
            decision,
            Some(Intent::ServeFloor {
                car: CarId::new("B"),
                floor: Floor::new(7),
            })
        );
    }
}
