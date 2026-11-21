#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use liftassistant_model::{
    BuildingTopology, CallId, CallRecord, CallState, CarId, CarLocation, CarState, Direction,
    ExecutionCapabilities, Floor, HallCall, Intent, MotionState, OperatingMode,
    ReportedDestination,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HallCallRejection {
    FloorNotServed(Floor),
    DirectionUnavailableAtTerminal { floor: Floor, direction: Direction },
}

pub fn validate_hall_call(
    call: &HallCall,
    topology: &BuildingTopology,
) -> Result<(), HallCallRejection> {
    if !topology.contains(call.floor) {
        return Err(HallCallRejection::FloorNotServed(call.floor));
    }
    if (call.floor == topology.lowest_floor() && call.direction == Direction::Down)
        || (call.floor == topology.highest_floor() && call.direction == Direction::Up)
    {
        return Err(HallCallRejection::DirectionUnavailableAtTerminal {
            floor: call.floor,
            direction: call.direction,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallRegistration {
    Created(CallId),
    Duplicate(CallId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallRegistry {
    next_id: u64,
    calls: BTreeMap<CallId, CallRecord>,
    completed: VecDeque<CallRecord>,
    arrived: BTreeSet<CallId>,
}

const COMPLETED_CALL_CAPACITY: usize = 128;

impl CallRegistry {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            calls: BTreeMap::new(),
            completed: VecDeque::new(),
            arrived: BTreeSet::new(),
        }
    }

    pub fn register(&mut self, call: HallCall) -> CallRegistration {
        if let Some(existing) = self.calls.values().find(|existing| existing.call == call) {
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
        self.calls
            .get(&id)
            .or_else(|| self.completed.iter().find(|record| record.id == id))
    }

    pub fn calls(&self) -> impl Iterator<Item = &CallRecord> {
        let mut records: Vec<_> = self.calls.values().chain(self.completed.iter()).collect();
        records.sort_unstable_by_key(|record| record.id);
        records.into_iter()
    }

    pub fn active_calls(&self) -> impl Iterator<Item = &CallRecord> {
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
        self.note_call_arrival(id, car, floor).then_some(id)
    }

    pub fn note_call_arrival(&mut self, id: CallId, car: &CarId, floor: Floor) -> bool {
        if !self.calls.get(&id).is_some_and(|record| {
            record.call.floor == floor && record.state == CallState::Serving(car.clone())
        }) {
            return false;
        }
        self.arrived.insert(id);
        true
    }

    pub fn complete_open_call(&mut self, car: &CarId) -> Option<CallId> {
        let id = self.arrived.iter().copied().find(|id| {
            self.calls.get(id).is_some_and(
                |record| matches!(&record.state, CallState::Serving(assigned) if assigned == car),
            )
        })?;
        self.complete_arrived_call(id, car).then_some(id)
    }

    pub fn complete_arrived_call(&mut self, id: CallId, car: &CarId) -> bool {
        self.arrived.contains(&id) && self.complete(id, car)
    }

    pub fn occupied_cars(&self) -> BTreeSet<CarId> {
        self.calls
            .values()
            .filter_map(|record| match &record.state {
                CallState::Assigned(car) | CallState::Serving(car) => Some(car.clone()),
                CallState::Pending | CallState::Completed => None,
            })
            .collect()
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

    /// Remove work whose execution was prevented before service began.
    pub fn remove_unstarted(&mut self, id: CallId) -> bool {
        if !self.calls.get(&id).is_some_and(|record| {
            matches!(record.state, CallState::Pending | CallState::Assigned(_))
        }) {
            return false;
        }
        self.calls.remove(&id);
        true
    }

    pub fn complete(&mut self, id: CallId, car: &CarId) -> bool {
        if !self.calls.get(&id).is_some_and(
            |record| matches!(&record.state, CallState::Serving(assigned) if assigned == car),
        ) {
            return false;
        }
        let mut record = self.calls.remove(&id).expect("validated serving call");
        record.state = CallState::Completed;
        self.arrived.remove(&id);
        if self.completed.len() == COMPLETED_CALL_CAPACITY {
            self.completed.pop_front();
        }
        self.completed.push_back(record);
        true
    }
}

impl Default for CallRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchRejection {
    Mode(OperatingMode),
    Busy,
    CapabilitiesUnavailable,
    ServeFloorUnsupported,
    UnsupportedFloor(Floor),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispatchCandidate {
    pub car: CarId,
    pub mode: OperatingMode,
    pub busy: bool,
    pub serve_floor: Option<bool>,
    pub distance_floors: f64,
    pub rejections: Vec<DispatchRejection>,
}

impl DispatchCandidate {
    pub fn is_eligible(&self) -> bool {
        self.rejections.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchSelectionReason {
    OnlyEligible,
    Nearest,
    CarIdTieBreak,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispatchSelection {
    pub car: CarId,
    pub distance_floors: f64,
    pub reason: DispatchSelectionReason,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispatchDecision {
    pub candidates: Vec<DispatchCandidate>,
    pub selection: Option<DispatchSelection>,
    pub intent: Option<Intent>,
}

pub fn dispatch(
    call: &HallCall,
    cars: &[CarState],
    capabilities: &BTreeMap<CarId, ExecutionCapabilities>,
    occupied_cars: &BTreeSet<CarId>,
) -> DispatchDecision {
    let mut candidates: Vec<_> = cars
        .iter()
        .map(|car| {
            let mut rejections = Vec::new();
            if car.mode != OperatingMode::Normal {
                rejections.push(DispatchRejection::Mode(car.mode));
            }
            // A stopped car may keep reporting its current floor as its destination.
            // Unknown destination evidence stays conservative, while Engine-owned call
            // assignments independently preserve desired/control occupancy.
            let reported_busy = match car.reported_destination {
                ReportedDestination::Known(destination) => {
                    destination != car.location.floor_reference()
                }
                ReportedDestination::None => false,
                ReportedDestination::Unknown => true,
            };
            let busy = car.motion != MotionState::Stopped
                || reported_busy
                || occupied_cars.contains(&car.id);
            if busy {
                rejections.push(DispatchRejection::Busy);
            }
            let execution = capabilities.get(&car.id);
            match execution {
                None => rejections.push(DispatchRejection::CapabilitiesUnavailable),
                Some(caps) if !caps.serve_floor => {
                    rejections.push(DispatchRejection::ServeFloorUnsupported)
                }
                Some(caps) if !caps.supports_floor(call.floor) => {
                    rejections.push(DispatchRejection::UnsupportedFloor(call.floor))
                }
                Some(_) => {}
            }
            let location_floor_position = match car.location {
                CarLocation::Floor(floor) => f64::from(floor.index()),
                CarLocation::Continuous(position) => position.floor_position,
            };
            DispatchCandidate {
                car: car.id.clone(),
                mode: car.mode,
                busy,
                serve_floor: execution.map(|caps| caps.serve_floor),
                distance_floors: (location_floor_position - f64::from(call.floor.index())).abs(),
                rejections,
            }
        })
        .collect();
    candidates.sort_by(|left, right| left.car.cmp(&right.car));

    let eligible: Vec<_> = candidates
        .iter()
        .filter(|candidate| candidate.is_eligible())
        .collect();
    let selected = eligible.iter().copied().min_by(|left, right| {
        left.distance_floors
            .total_cmp(&right.distance_floors)
            .then_with(|| left.car.cmp(&right.car))
    });
    let selection = selected.map(|selected| {
        let reason = if eligible.len() == 1 {
            DispatchSelectionReason::OnlyEligible
        } else if eligible
            .iter()
            .filter(|candidate| {
                candidate
                    .distance_floors
                    .total_cmp(&selected.distance_floors)
                    .is_eq()
            })
            .count()
            > 1
        {
            DispatchSelectionReason::CarIdTieBreak
        } else {
            DispatchSelectionReason::Nearest
        };
        DispatchSelection {
            car: selected.car.clone(),
            distance_floors: selected.distance_floors,
            reason,
        }
    });
    let intent = selection.as_ref().map(|selection| Intent::ServeFloor {
        car: selection.car.clone(),
        floor: call.floor,
    });

    DispatchDecision {
        candidates,
        selection,
        intent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{Direction, Floor, Position};

    #[test]
    fn completed_calls_leave_active_state_and_recent_history_is_bounded() {
        let mut registry = CallRegistry::new();
        let call = HallCall {
            floor: Floor::new(5),
            direction: Direction::Up,
        };
        let car = CarId::new("A");
        for number in 1..=1_024 {
            let id = CallId::new(number);
            assert_eq!(registry.register(call), CallRegistration::Created(id));
            assert_eq!(registry.register(call), CallRegistration::Duplicate(id));
            assert!(registry.assign(id, car.clone()));
            assert!(registry.mark_serving(id, &car));
            assert_eq!(registry.note_arrival(&car, call.floor), Some(id));
            assert_eq!(registry.complete_open_call(&car), Some(id));
            assert_eq!(registry.active_calls().count(), 0);
            assert!(registry.occupied_cars().is_empty());
        }
        assert_eq!(registry.calls().count(), COMPLETED_CALL_CAPACITY);
        assert!(registry.get(CallId::new(1)).is_none());
        assert_eq!(
            registry.get(CallId::new(1_024)).map(|call| &call.state),
            Some(&CallState::Completed)
        );
    }

    #[test]
    fn nearer_car_excluded_when_target_is_outside_its_execution_floors() {
        let a = CarState::new("A", Floor::new(1));
        let b = CarState::new("B", Floor::new(8));
        let capabilities = BTreeMap::from([
            (
                a.id.clone(),
                ExecutionCapabilities::new(true, true, (1..=10).map(Floor::new)),
            ),
            (b.id.clone(), ExecutionCapabilities::reference_defaults()),
        ]);
        let call = HallCall {
            floor: Floor::new(9),
            direction: Direction::Down,
        };
        let decision = dispatch(&call, &[a, b], &capabilities, &BTreeSet::new());
        assert_eq!(decision.selection.unwrap().car, CarId::new("A"));
        assert_eq!(
            decision.candidates[1].rejections,
            [DispatchRejection::UnsupportedFloor(call.floor)]
        );
    }
    fn capabilities(cars: &[CarState]) -> BTreeMap<CarId, ExecutionCapabilities> {
        cars.iter()
            .map(|car| (car.id.clone(), ExecutionCapabilities::reference_defaults()))
            .collect()
    }

    #[test]
    fn hall_call_validation_uses_served_floors_and_topology_terminals() {
        let topology = BuildingTopology::new([Floor::new(1), Floor::new(3), Floor::new(5)])
            .expect("nonempty topology");

        assert_eq!(
            validate_hall_call(
                &HallCall {
                    floor: Floor::new(2),
                    direction: Direction::Up,
                },
                &topology,
            ),
            Err(HallCallRejection::FloorNotServed(Floor::new(2)))
        );
        assert_eq!(
            validate_hall_call(
                &HallCall {
                    floor: Floor::new(1),
                    direction: Direction::Down,
                },
                &topology,
            ),
            Err(HallCallRejection::DirectionUnavailableAtTerminal {
                floor: Floor::new(1),
                direction: Direction::Down,
            })
        );
        assert!(
            validate_hall_call(
                &HallCall {
                    floor: Floor::new(3),
                    direction: Direction::Down,
                },
                &topology,
            )
            .is_ok()
        );
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

        let decision = dispatch(&call, &cars, &capabilities(&cars), &BTreeSet::new());

        assert_eq!(
            decision.intent,
            Some(Intent::ServeFloor {
                car: CarId::new("A"),
                floor: Floor::new(8),
            })
        );
        assert_eq!(
            decision.selection,
            Some(DispatchSelection {
                car: CarId::new("A"),
                distance_floors: 5.0,
                reason: DispatchSelectionReason::Nearest,
            })
        );
        assert!(
            decision
                .candidates
                .iter()
                .all(|candidate| candidate.is_eligible())
        );
    }

    #[test]
    fn floor_level_and_continuous_locations_use_deterministic_floor_distance() {
        let cars = vec![
            CarState::new("A", Floor::new(3)),
            CarState::with_continuous_position(
                "B",
                Position {
                    floor_position: 4.5,
                    nearest_floor: Floor::new(5),
                },
            ),
        ];
        let call = HallCall {
            floor: Floor::new(5),
            direction: Direction::Down,
        };

        let decision = dispatch(&call, &cars, &capabilities(&cars), &BTreeSet::new());

        assert_eq!(
            decision.selection,
            Some(DispatchSelection {
                car: CarId::new("B"),
                distance_floors: 0.5,
                reason: DispatchSelectionReason::Nearest,
            })
        );
        assert_eq!(decision.candidates[0].distance_floors, 2.0);
        assert_eq!(decision.candidates[1].distance_floors, 0.5);
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

        let decision = dispatch(&call, &cars, &capabilities(&cars), &BTreeSet::new());

        assert_eq!(
            decision.intent,
            Some(Intent::ServeFloor {
                car: CarId::new("B"),
                floor: Floor::new(8),
            })
        );
        assert_eq!(
            decision.selection.map(|selection| selection.reason),
            Some(DispatchSelectionReason::OnlyEligible)
        );
        assert_eq!(
            decision.candidates[0].rejections,
            vec![DispatchRejection::Mode(OperatingMode::OutOfService)]
        );
    }

    #[test]
    fn busy_car_is_not_selected_even_when_it_is_nearest() {
        let mut a = CarState::new("A", Floor::new(7));
        a.reported_destination = ReportedDestination::Known(Floor::new(8));
        let cars = vec![a, CarState::new("B", Floor::new(1))];
        let call = HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        };

        let decision = dispatch(&call, &cars, &capabilities(&cars), &BTreeSet::new());

        assert_eq!(
            decision.intent,
            Some(Intent::ServeFloor {
                car: CarId::new("B"),
                floor: Floor::new(7),
            })
        );
        assert_eq!(
            decision.candidates[0].rejections,
            vec![DispatchRejection::Busy]
        );
    }

    #[test]
    fn stopped_car_reporting_its_current_floor_is_not_busy_from_destination_alone() {
        let mut a = CarState::new("A", Floor::new(7));
        a.reported_destination = ReportedDestination::Known(Floor::new(7));
        let cars = vec![a, CarState::new("B", Floor::new(1))];
        let call = HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        };

        let decision = dispatch(&call, &cars, &capabilities(&cars), &BTreeSet::new());

        assert_eq!(
            decision.intent,
            Some(Intent::ServeFloor {
                car: CarId::new("A"),
                floor: Floor::new(7),
            })
        );
        assert!(!decision.candidates[0].busy);
        assert!(decision.candidates[0].rejections.is_empty());
    }

    #[test]
    fn moving_car_is_busy_even_when_reported_destination_is_its_current_floor() {
        let mut a = CarState::new("A", Floor::new(7));
        a.motion = MotionState::Moving(Direction::Up);
        a.reported_destination = ReportedDestination::Known(Floor::new(7));
        let cars = vec![a, CarState::new("B", Floor::new(1))];
        let call = HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        };

        let decision = dispatch(&call, &cars, &capabilities(&cars), &BTreeSet::new());

        assert_eq!(
            decision.intent,
            Some(Intent::ServeFloor {
                car: CarId::new("B"),
                floor: Floor::new(7),
            })
        );
        assert!(decision.candidates[0].busy);
        assert_eq!(
            decision.candidates[0].rejections,
            vec![DispatchRejection::Busy]
        );
    }

    #[test]
    fn engine_call_occupancy_is_busy_even_with_idle_external_evidence() {
        let a = CarState::new("A", Floor::new(7));
        let cars = vec![a, CarState::new("B", Floor::new(1))];
        let call = HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        };
        let occupied = BTreeSet::from([CarId::new("A")]);

        let decision = dispatch(&call, &cars, &capabilities(&cars), &occupied);

        assert_eq!(
            decision.intent,
            Some(Intent::ServeFloor {
                car: CarId::new("B"),
                floor: Floor::new(7),
            })
        );
        assert!(decision.candidates[0].busy);
        assert_eq!(
            decision.candidates[0].rejections,
            vec![DispatchRejection::Busy]
        );
    }

    #[test]
    fn unknown_reported_destination_is_not_idle_evidence() {
        let mut a = CarState::new("A", Floor::new(7));
        a.reported_destination = ReportedDestination::Unknown;
        let cars = vec![a, CarState::new("B", Floor::new(1))];
        let call = HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        };

        let decision = dispatch(&call, &cars, &capabilities(&cars), &BTreeSet::new());

        assert_eq!(
            decision.intent,
            Some(Intent::ServeFloor {
                car: CarId::new("B"),
                floor: Floor::new(7),
            })
        );
        assert_eq!(
            decision.candidates[0].rejections,
            vec![DispatchRejection::Busy]
        );
    }

    #[test]
    fn car_id_breaks_an_equal_distance_tie_and_is_explained() {
        let cars = vec![
            CarState::new("B", Floor::new(5)),
            CarState::new("A", Floor::new(3)),
        ];
        let call = HallCall {
            floor: Floor::new(4),
            direction: Direction::Up,
        };

        let decision = dispatch(&call, &cars, &capabilities(&cars), &BTreeSet::new());

        assert_eq!(
            decision.selection,
            Some(DispatchSelection {
                car: CarId::new("A"),
                distance_floors: 1.0,
                reason: DispatchSelectionReason::CarIdTieBreak,
            })
        );
        assert_eq!(decision.candidates[0].car, CarId::new("A"));
        assert_eq!(decision.candidates[1].car, CarId::new("B"));
    }

    #[test]
    fn missing_and_unsupported_capabilities_explain_no_selection() {
        let cars = vec![
            CarState::new("A", Floor::new(3)),
            CarState::new("B", Floor::new(1)),
        ];
        let mut capabilities = BTreeMap::new();
        let mut unsupported = ExecutionCapabilities::reference_defaults();
        unsupported.serve_floor = false;
        capabilities.insert(CarId::new("B"), unsupported);
        let call = HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        };

        let decision = dispatch(&call, &cars, &capabilities, &BTreeSet::new());

        assert!(decision.intent.is_none());
        assert!(decision.selection.is_none());
        assert_eq!(
            decision.candidates[0].rejections,
            vec![DispatchRejection::CapabilitiesUnavailable]
        );
        assert_eq!(
            decision.candidates[1].rejections,
            vec![DispatchRejection::ServeFloorUnsupported]
        );
    }
}
