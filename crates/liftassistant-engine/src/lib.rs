#![forbid(unsafe_code)]

mod diagnostics;

use std::collections::BTreeMap;

pub use diagnostics::{
    DiagnosticAnomaly, DiagnosticEntry, DiagnosticEvent, DiagnosticSequence, FlightRecorder,
    IncidentSnapshot,
};

use liftassistant_core::{
    CallRegistration, CallRegistry, DispatchCandidate, DispatchSelection, HallCallRejection,
    dispatch, validate_hall_call,
};
use liftassistant_model::{
    BuildingTopology, CallId, CallState, Capabilities, CarId, CarObservationUpdate, CarState,
    HallCall, Intent, ObservationSourceId,
};
use liftassistant_runtime::{
    AssembledCarObservation, CarStateResolution, IntentSink, ObservationAssembler,
    ObservationAssemblyError, RuntimeEvent, resolve_car_observation,
};

const DEFAULT_DIAGNOSTIC_CAPACITY: usize = 128;

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
    IntentRejected {
        id: CallId,
        intent: Intent,
        message: String,
    },
    Runtime(RuntimeEvent),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObservationIngestResult {
    Incomplete(Box<AssembledCarObservation>),
    Resolution(CarStateResolution),
    Rejected(ObservationAssemblyError),
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
    observations: ObservationAssembler,
    states: BTreeMap<(ObservationSourceId, CarId), CarState>,
    capabilities: BTreeMap<CarId, Capabilities>,
    diagnostics: FlightRecorder,
}

impl Engine {
    pub fn new(topology: BuildingTopology) -> Self {
        Self::with_diagnostic_capacity(topology, DEFAULT_DIAGNOSTIC_CAPACITY)
    }

    pub fn with_diagnostic_capacity(topology: BuildingTopology, capacity: usize) -> Self {
        Self {
            topology,
            last_call: None,
            decision: None,
            calls: CallRegistry::new(),
            observations: ObservationAssembler::new(),
            states: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            diagnostics: FlightRecorder::new(capacity),
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

    pub fn diagnostics(&self) -> &FlightRecorder {
        &self.diagnostics
    }

    pub fn set_capabilities(&mut self, car: CarId, capabilities: Capabilities) {
        self.capabilities.insert(car, capabilities);
    }

    pub fn car_states(&self) -> Vec<CarState> {
        self.states.values().cloned().collect()
    }

    pub fn car_state(&self, source: &ObservationSourceId, car: &CarId) -> Option<&CarState> {
        self.states.get(&(source.clone(), car.clone()))
    }

    pub fn evidence(
        &self,
        source: &ObservationSourceId,
        car: &CarId,
    ) -> Option<&AssembledCarObservation> {
        self.observations.current(source, car)
    }

    pub fn ingest_observation_update(
        &mut self,
        update: CarObservationUpdate,
    ) -> ObservationIngestResult {
        let key = (update.provenance.source.clone(), update.car.clone());
        self.diagnostics.record_observation(update.clone());
        let assembled = match self.observations.apply(update) {
            Ok(assembled) => assembled,
            Err(error) => return ObservationIngestResult::Rejected(error),
        };
        let Some(snapshot) = assembled.snapshot() else {
            self.states.remove(&key);
            return ObservationIngestResult::Incomplete(Box::new(assembled));
        };
        let resolution = resolve_car_observation(&snapshot);
        match &resolution {
            CarStateResolution::Resolved(state) => {
                self.states.insert(key, state.clone());
            }
            CarStateResolution::Unresolved { .. } => {
                self.states.remove(&key);
            }
        }
        ObservationIngestResult::Resolution(resolution)
    }

    pub fn submit_call<B>(&mut self, call: HallCall, backend: &mut B) -> Vec<EngineEvent>
    where
        B: IntentSink,
    {
        let mut events = vec![EngineEvent::HallCall(call)];
        self.last_call = Some(call);
        if let Err(reason) = validate_hall_call(&call, &self.topology) {
            self.decision = None;
            events.push(EngineEvent::InvalidCall(reason));
            self.diagnostics.record_engine_events(&events);
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
                    self.diagnostics.record_engine_events(&events);
                    return events;
                }
                self.decision = None;
            }
        }
        events.extend(self.dispatch_pending(backend));
        self.diagnostics.record_engine_events(&events);
        events
    }

    pub fn retry_pending<B>(&mut self, backend: &mut B) -> Vec<EngineEvent>
    where
        B: IntentSink,
    {
        let events = self.dispatch_pending(backend);
        self.diagnostics.record_engine_events(&events);
        events
    }

    pub fn consume_runtime_events<B>(
        &mut self,
        runtime_events: Vec<RuntimeEvent>,
        backend: &mut B,
    ) -> Vec<EngineEvent>
    where
        B: IntentSink,
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
                RuntimeEvent::Observation(update) => {
                    self.ingest_observation_update(update.clone());
                }
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
                RuntimeEvent::DoorStateChanged { .. } => {}
            }
        }
        self.diagnostics.record_engine_events(&events);
        events
    }

    fn dispatch_pending<B>(&mut self, backend: &mut B) -> Vec<EngineEvent>
    where
        B: IntentSink,
    {
        let pending: Vec<_> = self.calls.pending_calls().collect();
        let mut events = Vec::new();

        for (id, call) in pending {
            let states = self.car_states();
            let occupied_cars = self.calls.occupied_cars();
            let decision = dispatch(&call, &states, &self.capabilities, &occupied_cars);
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
            match backend.submit_intent(intent.clone()) {
                Ok(()) => {
                    let assigned = CallState::Assigned(car.clone());
                    assert!(self.calls.assign(id, car));
                    events.push(EngineEvent::CallStateChanged {
                        id,
                        state: assigned,
                    });
                }
                Err(error) => events.push(EngineEvent::IntentRejected {
                    id,
                    intent,
                    message: error.to_string(),
                }),
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{
        Capabilities, CarId, CarObservationUpdate, CarState, Direction, DoorState, Floor,
        ObservationProvenance, ObservationSequence, ObservationSourceId, PositionEvidence,
        ReportedDestination,
    };
    use liftassistant_runtime::RuntimeError;

    struct TestBackend {
        intents: Vec<Intent>,
    }

    impl IntentSink for TestBackend {
        fn submit_intent(&mut self, intent: Intent) -> Result<(), RuntimeError> {
            self.intents.push(intent);
            Ok(())
        }
    }

    fn provenance(sequence: u64) -> ObservationProvenance {
        ObservationProvenance {
            source: ObservationSourceId::new("engine-test"),
            sequence: ObservationSequence::new(sequence),
        }
    }

    fn seed_state(engine: &mut Engine, state: &CarState, sequence: u64) {
        engine.set_capabilities(state.id.clone(), Capabilities::reference_defaults());
        assert_eq!(
            engine.ingest_observation_update(CarObservationUpdate::from_state(
                provenance(sequence),
                state,
            )),
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(state.clone()))
        );
    }

    #[test]
    fn engine_owns_call_lifecycle_and_runtime_feedback_orchestration() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let state = CarState::new("A", Floor::new(3));
        seed_state(&mut engine, &state, 1);
        let mut backend = TestBackend {
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
        assert!(engine.diagnostics().entries().any(|entry| matches!(
            entry.event,
            DiagnosticEvent::Engine(EngineEvent::IntentGenerated { .. })
        )));
        assert!(
            !engine
                .diagnostics()
                .entries()
                .any(|entry| matches!(entry.event, DiagnosticEvent::Anomaly(_)))
        );
    }

    #[test]
    fn unsolicited_intent_acceptance_creates_bounded_replayable_incident() {
        let mut engine =
            Engine::with_diagnostic_capacity(BuildingTopology::reference_eight_floor(), 3);
        let mut backend = TestBackend {
            intents: Vec::new(),
        };
        let intent = Intent::ServeFloor {
            car: CarId::new("A"),
            floor: Floor::new(7),
        };

        engine.consume_runtime_events(
            vec![RuntimeEvent::IntentAccepted(intent.clone())],
            &mut backend,
        );
        for sequence in 1..=2 {
            engine.ingest_observation_update(CarObservationUpdate::from_state(
                provenance(sequence),
                &CarState::new("A", Floor::new(3)),
            ));
        }

        assert_eq!(engine.diagnostics().entries().count(), 3);
        let incident = engine
            .diagnostics()
            .incidents()
            .next()
            .expect("anomaly creates an incident snapshot");
        let anomaly = DiagnosticAnomaly::IntentAcceptedWithoutGeneration { intent };
        assert_eq!(incident.trigger, anomaly);
        assert!(incident.after_window_complete);
        assert!(incident.replay_anomalies().contains(&anomaly));
    }

    #[test]
    fn desired_destination_survives_unknown_reported_destination() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let state = CarState::new("A", Floor::new(3));
        seed_state(&mut engine, &state, 1);
        let mut backend = TestBackend {
            intents: Vec::new(),
        };
        let desired = Intent::ServeFloor {
            car: CarId::new("A"),
            floor: Floor::new(7),
        };

        engine.submit_call(
            HallCall {
                floor: Floor::new(7),
                direction: Direction::Up,
            },
            &mut backend,
        );
        let mut reported_unknown = state;
        reported_unknown.reported_destination = ReportedDestination::Unknown;
        engine.ingest_observation_update(CarObservationUpdate::from_state(
            provenance(2),
            &reported_unknown,
        ));

        assert_eq!(engine.decision(), Some(&desired));
        assert_eq!(backend.intents, vec![desired]);
        assert_eq!(
            engine.calls().get(CallId::new(1)).map(|call| &call.state),
            Some(&CallState::Assigned(CarId::new("A")))
        );

        let second = engine.submit_call(
            HallCall {
                floor: Floor::new(5),
                direction: Direction::Up,
            },
            &mut backend,
        );
        assert!(second.contains(&EngineEvent::NoEligibleCar(CallId::new(2))));
        assert_eq!(backend.intents.len(), 1);
    }

    #[test]
    fn explicit_unknown_invalidates_engine_owned_operational_state() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let state = CarState::new("A", Floor::new(3));
        seed_state(&mut engine, &state, 1);

        let mut update = CarObservationUpdate::from_state(provenance(2), &state);
        update.position = Some(PositionEvidence::Unknown);
        assert_eq!(
            engine.ingest_observation_update(update),
            ObservationIngestResult::Resolution(CarStateResolution::Unresolved {
                car: CarId::new("A"),
                reasons: vec![liftassistant_runtime::UnresolvedCarStateReason::PositionUnknown],
            })
        );
        assert!(
            engine
                .car_state(&ObservationSourceId::new("engine-test"), &CarId::new("A"))
                .is_none()
        );
    }
}
