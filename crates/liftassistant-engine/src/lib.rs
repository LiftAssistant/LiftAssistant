#![forbid(unsafe_code)]

mod diagnostics;

use std::collections::{BTreeMap, BTreeSet};

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
    HallCall, Intent, ObservationSessionId, ObservationSourceId,
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
    Rejected(ObservationIngestError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationBindingRole {
    Operational,
    Auxiliary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationBindingError {
    AlreadyBound {
        source: ObservationSourceId,
        car: CarId,
        role: ObservationBindingRole,
    },
    OperationalAuthorityConflict {
        car: CarId,
        existing_source: ObservationSourceId,
        requested_source: ObservationSourceId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationIngestError {
    UnboundObservation {
        source: ObservationSourceId,
        car: CarId,
    },
    NoActiveSourceSession {
        source: ObservationSourceId,
        received: ObservationSessionId,
    },
    InactiveSourceSession {
        source: ObservationSourceId,
        active: ObservationSessionId,
        received: ObservationSessionId,
    },
    Assembly(ObservationAssemblyError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceSessionError {
    UnboundSource {
        source: ObservationSourceId,
    },
    AlreadyActive {
        source: ObservationSourceId,
        session: ObservationSessionId,
    },
    RetiredSession {
        source: ObservationSourceId,
        session: ObservationSessionId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSessionTransition {
    pub source: ObservationSourceId,
    pub previous_session: Option<ObservationSessionId>,
    pub active_session: ObservationSessionId,
    pub invalidated_cars: Vec<CarId>,
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
    observation_bindings: BTreeMap<(ObservationSourceId, CarId), ObservationBindingRole>,
    operational_authorities: BTreeMap<CarId, ObservationSourceId>,
    active_source_sessions: BTreeMap<ObservationSourceId, ObservationSessionId>,
    retired_source_sessions: BTreeSet<(ObservationSourceId, ObservationSessionId)>,
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
            observation_bindings: BTreeMap::new(),
            operational_authorities: BTreeMap::new(),
            active_source_sessions: BTreeMap::new(),
            retired_source_sessions: BTreeSet::new(),
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

    pub fn bind_observation(
        &mut self,
        source: ObservationSourceId,
        car: CarId,
        role: ObservationBindingRole,
    ) -> Result<(), ObservationBindingError> {
        let key = (source.clone(), car.clone());
        if let Some(existing) = self.observation_bindings.get(&key) {
            return Err(ObservationBindingError::AlreadyBound {
                source,
                car,
                role: *existing,
            });
        }
        if role == ObservationBindingRole::Operational
            && let Some(existing_source) = self.operational_authorities.get(&car)
        {
            return Err(ObservationBindingError::OperationalAuthorityConflict {
                car,
                existing_source: existing_source.clone(),
                requested_source: source,
            });
        }

        self.observation_bindings.insert(key, role);
        if role == ObservationBindingRole::Operational {
            self.operational_authorities.insert(car, source);
        }
        Ok(())
    }

    pub fn observation_binding_role(
        &self,
        source: &ObservationSourceId,
        car: &CarId,
    ) -> Option<ObservationBindingRole> {
        self.observation_bindings
            .get(&(source.clone(), car.clone()))
            .copied()
    }

    pub fn operational_observation_source(&self, car: &CarId) -> Option<&ObservationSourceId> {
        self.operational_authorities.get(car)
    }

    pub fn active_source_session(
        &self,
        source: &ObservationSourceId,
    ) -> Option<&ObservationSessionId> {
        self.active_source_sessions.get(source)
    }

    pub fn begin_source_session(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
    ) -> Result<SourceSessionTransition, SourceSessionError> {
        if !self
            .observation_bindings
            .keys()
            .any(|(bound_source, _)| bound_source == &source)
        {
            return Err(SourceSessionError::UnboundSource { source });
        }
        if self.active_source_sessions.get(&source) == Some(&session) {
            return Err(SourceSessionError::AlreadyActive { source, session });
        }
        if self
            .retired_source_sessions
            .contains(&(source.clone(), session.clone()))
        {
            return Err(SourceSessionError::RetiredSession { source, session });
        }

        let previous_session = self
            .active_source_sessions
            .insert(source.clone(), session.clone());
        if let Some(previous) = &previous_session {
            self.retired_source_sessions
                .insert((source.clone(), previous.clone()));
        }
        self.observations.remove_source(&source);
        let invalidated_cars = self
            .states
            .keys()
            .filter_map(|(candidate, car)| (candidate == &source).then_some(car.clone()))
            .collect();
        self.states.retain(|(candidate, _), _| candidate != &source);

        Ok(SourceSessionTransition {
            source,
            previous_session,
            active_session: session,
            invalidated_cars,
        })
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
        let session = self.active_source_sessions.get(source)?;
        self.observations.current(source, session, car)
    }

    pub fn ingest_observation_update(
        &mut self,
        update: CarObservationUpdate,
    ) -> ObservationIngestResult {
        let key = (update.provenance.source.clone(), update.car.clone());
        if !self.observation_bindings.contains_key(&key) {
            return ObservationIngestResult::Rejected(ObservationIngestError::UnboundObservation {
                source: update.provenance.source,
                car: update.car,
            });
        }
        let received_session = update.provenance.session.clone();
        let Some(active_session) = self.active_source_sessions.get(&update.provenance.source)
        else {
            return ObservationIngestResult::Rejected(
                ObservationIngestError::NoActiveSourceSession {
                    source: update.provenance.source,
                    received: received_session,
                },
            );
        };
        if active_session != &received_session {
            return ObservationIngestResult::Rejected(
                ObservationIngestError::InactiveSourceSession {
                    source: update.provenance.source,
                    active: active_session.clone(),
                    received: received_session,
                },
            );
        }
        self.diagnostics.record_observation(update.clone());
        let assembled = match self.observations.apply(update) {
            Ok(assembled) => assembled,
            Err(error) => {
                return ObservationIngestResult::Rejected(ObservationIngestError::Assembly(error));
            }
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
        ObservationProvenance, ObservationSequence, ObservationSessionId, ObservationSourceId,
        PositionEvidence, ReportedDestination,
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
            session: ObservationSessionId::new("engine-test-session-A"),
            sequence: ObservationSequence::new(sequence),
        }
    }

    fn seed_state(engine: &mut Engine, state: &CarState, sequence: u64) {
        let source = ObservationSourceId::new("engine-test");
        if engine
            .observation_binding_role(&source, &state.id)
            .is_none()
        {
            engine
                .bind_observation(
                    source.clone(),
                    state.id.clone(),
                    ObservationBindingRole::Operational,
                )
                .expect("test observation binding is unique");
        }
        if engine.active_source_session(&source).is_none() {
            engine
                .begin_source_session(source, ObservationSessionId::new("engine-test-session-A"))
                .expect("test source session starts once");
        }
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
        engine
            .bind_observation(
                ObservationSourceId::new("engine-test"),
                CarId::new("A"),
                ObservationBindingRole::Auxiliary,
            )
            .expect("diagnostic observation binding is unique");
        engine
            .begin_source_session(
                ObservationSourceId::new("engine-test"),
                ObservationSessionId::new("engine-test-session-A"),
            )
            .expect("diagnostic source session starts once");
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

    #[test]
    fn explicit_bindings_allow_multi_car_sources_and_multi_source_cars() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let controller = ObservationSourceId::new("controller-main");
        let bacnet = ObservationSourceId::new("bacnet-main");

        assert_eq!(
            engine.bind_observation(
                controller.clone(),
                CarId::new("A"),
                ObservationBindingRole::Operational,
            ),
            Ok(())
        );
        assert_eq!(
            engine.bind_observation(
                controller.clone(),
                CarId::new("B"),
                ObservationBindingRole::Operational,
            ),
            Ok(())
        );
        assert_eq!(
            engine.bind_observation(
                bacnet.clone(),
                CarId::new("A"),
                ObservationBindingRole::Auxiliary,
            ),
            Ok(())
        );
        assert_eq!(
            engine.observation_binding_role(&controller, &CarId::new("B")),
            Some(ObservationBindingRole::Operational)
        );
        assert_eq!(
            engine.observation_binding_role(&bacnet, &CarId::new("A")),
            Some(ObservationBindingRole::Auxiliary)
        );
        assert_eq!(
            engine.operational_observation_source(&CarId::new("A")),
            Some(&controller)
        );
    }

    #[test]
    fn duplicate_and_second_operational_bindings_are_rejected_explicitly() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let controller = ObservationSourceId::new("controller-main");
        let rmf = ObservationSourceId::new("open-rmf-main");
        let car = CarId::new("A");
        engine
            .bind_observation(
                controller.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("first operational binding is valid");

        assert_eq!(
            engine.bind_observation(
                controller.clone(),
                car.clone(),
                ObservationBindingRole::Auxiliary,
            ),
            Err(ObservationBindingError::AlreadyBound {
                source: controller.clone(),
                car: car.clone(),
                role: ObservationBindingRole::Operational,
            })
        );
        assert_eq!(
            engine.bind_observation(
                rmf.clone(),
                car.clone(),
                ObservationBindingRole::Operational
            ),
            Err(ObservationBindingError::OperationalAuthorityConflict {
                car,
                existing_source: controller,
                requested_source: rmf,
            })
        );
    }

    #[test]
    fn unbound_observation_is_rejected_without_name_based_inference() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let state = CarState::new("A", Floor::new(3));
        let update = CarObservationUpdate::from_state(
            ObservationProvenance {
                source: ObservationSourceId::new("A"),
                session: ObservationSessionId::new("same-text-session"),
                sequence: ObservationSequence::new(1),
            },
            &state,
        );

        assert_eq!(
            engine.ingest_observation_update(update),
            ObservationIngestResult::Rejected(ObservationIngestError::UnboundObservation {
                source: ObservationSourceId::new("A"),
                car: CarId::new("A"),
            })
        );
        assert!(engine.car_states().is_empty());
        assert!(
            engine
                .evidence(&ObservationSourceId::new("A"), &CarId::new("A"))
                .is_none()
        );
    }

    #[test]
    fn explicit_session_restart_invalidates_old_evidence_and_rejects_delayed_updates() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let source = ObservationSourceId::new("controller-main");
        let car = CarId::new("A");
        let session_a = ObservationSessionId::new("boot-opaque-A");
        let session_b = ObservationSessionId::new("boot-opaque-B");
        engine
            .bind_observation(
                source.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("one operational binding is valid");

        let state_a = CarState::new("A", Floor::new(3));
        let update = |session: ObservationSessionId, sequence, state: &CarState| {
            CarObservationUpdate::from_state(
                ObservationProvenance {
                    source: source.clone(),
                    session,
                    sequence: ObservationSequence::new(sequence),
                },
                state,
            )
        };
        assert_eq!(
            engine.ingest_observation_update(update(session_a.clone(), 100, &state_a)),
            ObservationIngestResult::Rejected(ObservationIngestError::NoActiveSourceSession {
                source: source.clone(),
                received: session_a.clone(),
            })
        );

        assert_eq!(
            engine.begin_source_session(source.clone(), session_a.clone()),
            Ok(SourceSessionTransition {
                source: source.clone(),
                previous_session: None,
                active_session: session_a.clone(),
                invalidated_cars: Vec::new(),
            })
        );
        assert_eq!(
            engine.ingest_observation_update(update(session_a.clone(), 100, &state_a)),
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(state_a.clone()))
        );
        assert!(engine.evidence(&source, &car).is_some());
        assert_eq!(engine.car_state(&source, &car), Some(&state_a));

        assert_eq!(
            engine.begin_source_session(source.clone(), session_b.clone()),
            Ok(SourceSessionTransition {
                source: source.clone(),
                previous_session: Some(session_a.clone()),
                active_session: session_b.clone(),
                invalidated_cars: vec![car.clone()],
            })
        );
        assert!(engine.evidence(&source, &car).is_none());
        assert!(engine.car_state(&source, &car).is_none());

        let state_b = CarState::new("A", Floor::new(4));
        assert_eq!(
            engine.ingest_observation_update(update(session_b.clone(), 1, &state_b)),
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(state_b.clone()))
        );
        assert_eq!(engine.car_state(&source, &car), Some(&state_b));
        assert!(
            !engine
                .diagnostics()
                .entries()
                .any(|entry| matches!(entry.event, DiagnosticEvent::Anomaly(_)))
        );
        assert_eq!(
            engine.ingest_observation_update(update(session_a.clone(), 101, &state_a)),
            ObservationIngestResult::Rejected(ObservationIngestError::InactiveSourceSession {
                source: source.clone(),
                active: session_b.clone(),
                received: session_a.clone(),
            })
        );
        assert_eq!(engine.car_state(&source, &car), Some(&state_b));
        assert_eq!(
            engine.begin_source_session(source.clone(), session_a.clone()),
            Err(SourceSessionError::RetiredSession {
                source: source.clone(),
                session: session_a,
            })
        );
        assert_eq!(
            engine.begin_source_session(source.clone(), session_b.clone()),
            Err(SourceSessionError::AlreadyActive {
                source: source.clone(),
                session: session_b.clone(),
            })
        );
        assert_eq!(
            engine.ingest_observation_update(update(session_b.clone(), 1, &state_b)),
            ObservationIngestResult::Rejected(ObservationIngestError::Assembly(
                ObservationAssemblyError::NonIncreasingSequence {
                    source,
                    session: session_b,
                    previous: ObservationSequence::new(1),
                    received: ObservationSequence::new(1),
                }
            ))
        );
    }

    #[test]
    fn observation_sequence_is_source_global_and_session_local_across_cars() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let source = ObservationSourceId::new("group-controller");
        let session = ObservationSessionId::new("connection-token-A");
        for car in [CarId::new("A"), CarId::new("B")] {
            engine
                .bind_observation(source.clone(), car, ObservationBindingRole::Operational)
                .expect("one source may bind multiple cars");
        }
        engine
            .begin_source_session(source.clone(), session.clone())
            .expect("configured group source starts explicitly");

        for (car, sequence, floor) in [("A", 1, 2), ("B", 2, 5), ("A", 3, 3)] {
            let state = CarState::new(car, Floor::new(floor));
            assert_eq!(
                engine.ingest_observation_update(CarObservationUpdate::from_state(
                    ObservationProvenance {
                        source: source.clone(),
                        session: session.clone(),
                        sequence: ObservationSequence::new(sequence),
                    },
                    &state,
                )),
                ObservationIngestResult::Resolution(CarStateResolution::Resolved(state))
            );
        }

        assert_eq!(engine.car_states().len(), 2);
        assert!(
            !engine
                .diagnostics()
                .entries()
                .any(|entry| matches!(entry.event, DiagnosticEvent::Anomaly(_)))
        );
    }
}
