#![forbid(unsafe_code)]

use liftassistant_core::{
    CallRegistration, CallRegistry, DispatchCandidate, DispatchSelection, HallCallRejection,
    dispatch, validate_hall_call,
};
use liftassistant_model::{BuildingTopology, CallId, CallState, CarObservation, HallCall, Intent};
use liftassistant_runtime::{
    CarStateResolution, IntentSink, ObservationSource, RuntimeEvent, resolve_car_observation,
};

#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
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
    RuntimeError(String),
    Runtime(RuntimeEvent),
}

/// Deterministic semantic orchestration shared by interactive and headless callers.
///
/// Adapters, time driving, UI state, persistence, and transport remain outside this boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct Engine {
    topology: BuildingTopology,
    last_call: Option<HallCall>,
    decision: Option<Intent>,
    calls: CallRegistry,
}

impl Engine {
    pub fn new(topology: BuildingTopology) -> Self {
        Self {
            topology,
            last_call: None,
            decision: None,
            calls: CallRegistry::new(),
        }
    }

    pub fn topology(&self) -> &BuildingTopology {
        &self.topology
    }

    pub fn last_call(&self) -> Option<HallCall> {
        self.last_call
    }

    pub fn decision(&self) -> Option<&Intent> {
        self.decision.as_ref()
    }

    pub fn calls(&self) -> &CallRegistry {
        &self.calls
    }

    pub fn resolve_observation(&self, observation: &CarObservation) -> CarStateResolution {
        resolve_car_observation(observation)
    }

    pub fn submit_call<B>(&mut self, call: HallCall, backend: &mut B) -> Vec<EngineEvent>
    where
        B: ObservationSource + IntentSink,
    {
        let mut events = vec![EngineEvent::HallCall(call)];
        self.last_call = Some(call);
        if let Err(reason) = validate_hall_call(&call, &self.topology) {
            self.decision = None;
            events.push(EngineEvent::InvalidCall(reason));
            return events;
        }
        match self.calls.register(call) {
            CallRegistration::Created(id) => {
                self.decision = None;
                events.push(EngineEvent::CallRegistered(id));
            }
            CallRegistration::Duplicate(id) => {
                events.push(EngineEvent::CallDeduplicated(id));
                if self
                    .calls
                    .get(id)
                    .is_some_and(|record| record.state != CallState::Pending)
                {
                    return events;
                }
                self.decision = None;
            }
        }
        events.extend(self.dispatch_pending(backend));
        events
    }

    pub fn retry_pending<B>(&mut self, backend: &mut B) -> Vec<EngineEvent>
    where
        B: ObservationSource + IntentSink,
    {
        self.dispatch_pending(backend)
    }

    pub fn consume_runtime_events<B>(
        &mut self,
        runtime_events: Vec<RuntimeEvent>,
        backend: &mut B,
    ) -> Vec<EngineEvent>
    where
        B: ObservationSource + IntentSink,
    {
        let mut events = Vec::new();

        for runtime_event in runtime_events {
            events.push(EngineEvent::Runtime(runtime_event.clone()));
            match &runtime_event {
                RuntimeEvent::IntentAccepted(Intent::ServeFloor { car, floor }) => {
                    if let Some(id) = self.calls.assigned_call(car, *floor) {
                        assert!(self.calls.mark_serving(id, car));
                        events.push(EngineEvent::CallStateChanged {
                            id,
                            state: CallState::Serving(car.clone()),
                        });
                    }
                }
                RuntimeEvent::IntentAccepted(Intent::ParkAt { .. }) => {}
                RuntimeEvent::Arrived { car, floor } => {
                    self.calls.note_arrival(car, *floor);
                }
                RuntimeEvent::DoorStateChanged {
                    car,
                    state: liftassistant_model::DoorState::Open,
                } => {
                    let Some(id) = self.calls.complete_open_call(car) else {
                        continue;
                    };
                    events.push(EngineEvent::CallStateChanged {
                        id,
                        state: CallState::Completed,
                    });
                    events.extend(self.dispatch_pending(backend));
                }
                RuntimeEvent::PositionChanged(_) | RuntimeEvent::DoorStateChanged { .. } => {}
            }
        }
        events
    }

    fn dispatch_pending<B>(&mut self, backend: &mut B) -> Vec<EngineEvent>
    where
        B: ObservationSource + IntentSink,
    {
        let pending: Vec<_> = self.calls.pending_calls().collect();
        let mut events = Vec::new();

        for (id, call) in pending {
            let states = backend.car_states();
            let occupied_cars = self.calls.occupied_cars();
            let capabilities = states
                .iter()
                .filter_map(|car| {
                    backend
                        .capabilities(&car.id)
                        .map(|caps| (car.id.clone(), caps))
                })
                .collect();
            let decision = dispatch(&call, &states, &capabilities, &occupied_cars);
            events.extend(
                decision
                    .candidates
                    .iter()
                    .cloned()
                    .map(|candidate| EngineEvent::DispatchCandidate { id, candidate }),
            );
            let Some(selection) = decision.selection else {
                events.push(EngineEvent::NoEligibleCar(id));
                continue;
            };
            let intent = decision
                .intent
                .expect("a dispatch selection must produce an intent");
            let car = selection.car.clone();
            events.push(EngineEvent::DispatchSelected { id, selection });
            events.push(EngineEvent::IntentGenerated {
                id,
                intent: intent.clone(),
            });
            self.decision = Some(intent.clone());
            match backend.submit_intent(intent) {
                Ok(()) => {
                    let assigned = CallState::Assigned(car.clone());
                    assert!(self.calls.assign(id, car));
                    events.push(EngineEvent::CallStateChanged {
                        id,
                        state: assigned,
                    });
                }
                Err(error) => events.push(EngineEvent::RuntimeError(error.to_string())),
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{Capabilities, CarId, CarState, Direction, DoorState, Floor};
    use liftassistant_runtime::RuntimeError;

    struct TestBackend {
        state: CarState,
        intents: Vec<Intent>,
    }

    impl ObservationSource for TestBackend {
        fn source_name(&self) -> &str {
            "engine-test"
        }

        fn car_states(&self) -> Vec<CarState> {
            vec![self.state.clone()]
        }

        fn capabilities(&self, car: &CarId) -> Option<Capabilities> {
            (car == &self.state.id).then_some(Capabilities::reference_defaults())
        }
    }

    impl IntentSink for TestBackend {
        fn submit_intent(&mut self, intent: Intent) -> Result<(), RuntimeError> {
            self.intents.push(intent);
            Ok(())
        }
    }

    #[test]
    fn engine_owns_call_lifecycle_and_runtime_feedback_orchestration() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let mut backend = TestBackend {
            state: CarState::new("A", Floor::new(3)),
            intents: Vec::new(),
        };
        let call = HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        };

        let submitted = engine.submit_call(call, &mut backend);
        let intent = Intent::ServeFloor {
            car: CarId::new("A"),
            floor: Floor::new(7),
        };
        assert_eq!(backend.intents.as_slice(), std::slice::from_ref(&intent));
        assert!(submitted.contains(&EngineEvent::IntentGenerated {
            id: CallId::new(1),
            intent: intent.clone(),
        }));
        assert_eq!(
            engine.calls().get(CallId::new(1)).map(|call| &call.state),
            Some(&CallState::Assigned(CarId::new("A")))
        );

        let feedback = engine.consume_runtime_events(
            vec![
                RuntimeEvent::IntentAccepted(intent),
                RuntimeEvent::Arrived {
                    car: CarId::new("A"),
                    floor: Floor::new(7),
                },
                RuntimeEvent::DoorStateChanged {
                    car: CarId::new("A"),
                    state: DoorState::Open,
                },
            ],
            &mut backend,
        );

        assert!(feedback.contains(&EngineEvent::CallStateChanged {
            id: CallId::new(1),
            state: CallState::Completed,
        }));
        assert_eq!(
            engine.calls().get(CallId::new(1)).map(|call| &call.state),
            Some(&CallState::Completed)
        );
    }
}
