#![forbid(unsafe_code)]

mod controlled;
mod diagnostics;

use std::collections::{BTreeMap, BTreeSet};

pub use controlled::{
    Admission, AuthorityError, AuthorityGrant, AuthorityGrantId, ControlledEvent, IntentRecord,
    IntentRequest, PrincipalId, Receipt, ReceiptResult, Reconciliation, WriteBlocker,
};
pub use diagnostics::{
    ControlledIssue, DiagnosticAnomaly, DiagnosticEntry, DiagnosticEvent, DiagnosticSequence,
    FlightRecorder, IncidentSnapshot,
};
pub use liftassistant_runtime::ControllerId;

use liftassistant_core::{
    CallRegistration, CallRegistry, DispatchCandidate, DispatchSelection, HallCallRejection,
    dispatch, validate_hall_call,
};
use liftassistant_model::{
    BuildingTopology, CallId, CallState, CarId, CarObservationUpdate, CarState,
    ExecutionCapabilities, HallCall, Intent, ObservationSessionId, ObservationSourceId,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSessionTransition {
    pub source: ObservationSourceId,
    pub previous_session: Option<ObservationSessionId>,
    pub active_session: ObservationSessionId,
    pub invalidated_cars: Vec<CarId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceObservationInvalidationError {
    UnboundCar {
        source: ObservationSourceId,
        car: CarId,
    },
    NoActiveSourceSession {
        source: ObservationSourceId,
        requested: ObservationSessionId,
    },
    InactiveSourceSession {
        source: ObservationSourceId,
        active: ObservationSessionId,
        requested: ObservationSessionId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceObservationInvalidation {
    pub source: ObservationSourceId,
    pub session: ObservationSessionId,
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
    observations: ObservationAssembler,
    source_states: BTreeMap<(ObservationSourceId, CarId), CarState>,
    operational_states: BTreeMap<CarId, CarState>,
    execution_capabilities: BTreeMap<CarId, ExecutionCapabilities>,
    controlled: controlled::ControlledIntents,
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
            observations: ObservationAssembler::new(),
            source_states: BTreeMap::new(),
            operational_states: BTreeMap::new(),
            execution_capabilities: BTreeMap::new(),
            controlled: controlled::ControlledIntents::default(),
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

    pub fn set_execution_capabilities(&mut self, car: CarId, capabilities: ExecutionCapabilities) {
        self.execution_capabilities.insert(car, capabilities);
    }

    pub fn execution_capabilities(&self, car: &CarId) -> Option<&ExecutionCapabilities> {
        self.execution_capabilities.get(car)
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
        if let Some(existing_source) = self
            .operational_authorities
            .get(&car)
            .filter(|_| role == ObservationBindingRole::Operational)
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

    /// Activates a fresh incarnation supplied by the ingestion-lifecycle owner.
    /// The caller must never assign a historical token to a new lifetime.
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
        let previous_session = self
            .active_source_sessions
            .insert(source.clone(), session.clone());
        if let Some(previous) = &previous_session {
            self.diagnostics
                .forget_observation_stream(&source, previous);
        }
        self.observations.remove_source(&source);
        let mut invalidated_cars: BTreeSet<_> = self
            .source_states
            .keys()
            .filter_map(|(candidate, car)| (candidate == &source).then_some(car.clone()))
            .collect();
        self.source_states
            .retain(|(candidate, _), _| candidate != &source);
        for ((bound_source, car), role) in &self.observation_bindings {
            if bound_source == &source
                && *role == ObservationBindingRole::Operational
                && self.operational_states.remove(car).is_some()
            {
                invalidated_cars.insert(car.clone());
            }
        }

        for car in &invalidated_cars {
            if self.operational_authorities.get(car) == Some(&source) {
                self.note_controlled_readiness_loss(car, WriteBlocker::ObservationUnavailable);
            }
        }
        Ok(SourceSessionTransition {
            source,
            previous_session,
            active_session: session,
            invalidated_cars: invalidated_cars.into_iter().collect(),
        })
    }

    /// Explicitly revokes current observations from one active source session.
    ///
    /// The caller owns the lifecycle/freshness policy and any wall clock. Engine only applies
    /// the deterministic invalidation action. The session remains active and its ordering
    /// watermark is preserved, so a delayed or duplicate sample cannot restore stale evidence.
    pub fn invalidate_source_observations(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
    ) -> Result<SourceObservationInvalidation, SourceObservationInvalidationError> {
        self.check_invalidation_session(&source, &session)?;

        self.observations.invalidate_source_evidence(&source);
        let mut invalidated_cars: BTreeSet<_> = self
            .source_states
            .keys()
            .filter_map(|(candidate, car)| (candidate == &source).then_some(car.clone()))
            .collect();
        self.source_states
            .retain(|(candidate, _), _| candidate != &source);
        for ((bound_source, car), role) in &self.observation_bindings {
            if bound_source == &source
                && *role == ObservationBindingRole::Operational
                && self.operational_states.remove(car).is_some()
            {
                invalidated_cars.insert(car.clone());
            }
        }
        let invalidated_cars = invalidated_cars.into_iter().collect::<Vec<_>>();
        for car in &invalidated_cars {
            if self.operational_authorities.get(car) == Some(&source) {
                self.note_controlled_readiness_loss(car, WriteBlocker::ObservationUnavailable);
            }
        }
        self.diagnostics.record_source_invalidation(
            source.clone(),
            session.clone(),
            invalidated_cars.clone(),
        );

        Ok(SourceObservationInvalidation {
            source,
            session,
            invalidated_cars,
        })
    }

    /// Revokes one bound car's evidence and resolved state in the current incarnation.
    /// Other bindings and the source-global sequence watermark remain intact.
    pub fn invalidate_car_observations(
        &mut self,
        source: &ObservationSourceId,
        session: &ObservationSessionId,
        car: &CarId,
    ) -> Result<ObservationBindingRole, SourceObservationInvalidationError> {
        self.check_invalidation_session(source, session)?;
        let role = self.observation_binding_role(source, car).ok_or_else(|| {
            SourceObservationInvalidationError::UnboundCar {
                source: source.clone(),
                car: car.clone(),
            }
        })?;
        self.observations
            .invalidate_car_evidence(source, session, car);
        self.source_states.remove(&(source.clone(), car.clone()));
        if role == ObservationBindingRole::Operational
            && self.operational_states.remove(car).is_some()
        {
            self.note_controlled_readiness_loss(car, WriteBlocker::ObservationUnavailable);
        }
        self.diagnostics.record_source_invalidation(
            source.clone(),
            session.clone(),
            vec![car.clone()],
        );
        Ok(role)
    }

    fn check_invalidation_session(
        &self,
        source: &ObservationSourceId,
        session: &ObservationSessionId,
    ) -> Result<(), SourceObservationInvalidationError> {
        let Some(active) = self.active_source_sessions.get(source) else {
            return Err(SourceObservationInvalidationError::NoActiveSourceSession {
                source: source.clone(),
                requested: session.clone(),
            });
        };
        if active != session {
            return Err(SourceObservationInvalidationError::InactiveSourceSession {
                source: source.clone(),
                active: active.clone(),
                requested: session.clone(),
            });
        }
        Ok(())
    }

    pub fn car_states(&self) -> Vec<CarState> {
        self.operational_states.values().cloned().collect()
    }

    pub fn operational_car_state(&self, car: &CarId) -> Option<&CarState> {
        self.operational_states.get(car)
    }

    pub fn source_state(&self, source: &ObservationSourceId, car: &CarId) -> Option<&CarState> {
        self.source_states.get(&(source.clone(), car.clone()))
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
        let Some(role) = self.observation_bindings.get(&key).copied() else {
            return ObservationIngestResult::Rejected(ObservationIngestError::UnboundObservation {
                source: update.provenance.source,
                car: update.car,
            });
        };
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
        self.diagnostics.record_observation(update.clone(), role);
        let assembled = match self.observations.apply(update) {
            Ok(assembled) => assembled,
            Err(error) => {
                return ObservationIngestResult::Rejected(ObservationIngestError::Assembly(error));
            }
        };
        let Some(snapshot) = assembled.snapshot() else {
            self.source_states.remove(&key);
            if role == ObservationBindingRole::Operational
                && self.operational_states.remove(&assembled.car).is_some()
            {
                self.note_controlled_readiness_loss(
                    &assembled.car,
                    WriteBlocker::ObservationUnavailable,
                );
            }
            return ObservationIngestResult::Incomplete(Box::new(assembled));
        };
        let resolution = resolve_car_observation(&snapshot);
        match &resolution {
            CarStateResolution::Resolved(state) => {
                self.source_states.insert(key, state.clone());
                if role == ObservationBindingRole::Operational {
                    let previous = self
                        .operational_states
                        .insert(state.id.clone(), state.clone());
                    if state.mode != liftassistant_model::OperatingMode::Normal
                        && previous.is_some_and(|previous| {
                            previous.mode == liftassistant_model::OperatingMode::Normal
                        })
                    {
                        self.note_controlled_readiness_loss(
                            &state.id,
                            WriteBlocker::Mode(state.mode),
                        );
                    }
                    self.reconcile_controlled(state, &assembled);
                }
            }
            CarStateResolution::Unresolved { car, .. } => {
                self.source_states.remove(&key);
                if role == ObservationBindingRole::Operational
                    && self.operational_states.remove(car).is_some()
                {
                    self.note_controlled_readiness_loss(car, WriteBlocker::ObservationUnavailable);
                }
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
        if self.is_controlled() {
            return Vec::new();
        }
        let pending: Vec<_> = self.calls.pending_calls().collect();
        let mut events = Vec::new();

        for (id, call) in pending {
            let states = self.car_states();
            let occupied_cars = self.calls.occupied_cars();
            let decision = dispatch(&call, &states, &self.execution_capabilities, &occupied_cars);
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
        CarId, CarObservationUpdate, CarState, Direction, DoorEvidence, DoorState,
        ExecutionCapabilities, Floor, ObservationProvenance, ObservationSequence,
        ObservationSessionId, ObservationSourceId, PositionEvidence, ReportedDestination,
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
        engine.set_execution_capabilities(
            state.id.clone(),
            ExecutionCapabilities::reference_defaults(),
        );
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
                .source_state(&ObservationSourceId::new("engine-test"), &CarId::new("A"))
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
        assert_eq!(engine.source_state(&source, &car), Some(&state_a));

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
        assert!(engine.source_state(&source, &car).is_none());

        let state_b = CarState::new("A", Floor::new(4));
        assert_eq!(
            engine.ingest_observation_update(update(session_b.clone(), 1, &state_b)),
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(state_b.clone()))
        );
        assert_eq!(engine.source_state(&source, &car), Some(&state_b));
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
        assert_eq!(engine.source_state(&source, &car), Some(&state_b));
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
    fn explicit_source_invalidation_revokes_truth_without_resetting_session_order() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let source = ObservationSourceId::new("controller-main");
        let session = ObservationSessionId::new("connection-A");
        let car = CarId::new("A");
        engine
            .bind_observation(
                source.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("one operational binding is valid");
        engine
            .begin_source_session(source.clone(), session.clone())
            .expect("source session starts explicitly");
        let state = CarState::new("A", Floor::new(3));
        let update = |sequence| {
            CarObservationUpdate::from_state(
                ObservationProvenance {
                    source: source.clone(),
                    session: session.clone(),
                    sequence: ObservationSequence::new(sequence),
                },
                &state,
            )
        };
        assert!(matches!(
            engine.ingest_observation_update(update(100)),
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(_))
        ));

        assert_eq!(
            engine.invalidate_source_observations(source.clone(), session.clone()),
            Ok(SourceObservationInvalidation {
                source: source.clone(),
                session: session.clone(),
                invalidated_cars: vec![car.clone()],
            })
        );
        assert!(engine.operational_car_state(&car).is_none());
        assert!(engine.source_state(&source, &car).is_none());
        assert!(engine.evidence(&source, &car).is_none());
        assert_eq!(engine.active_source_session(&source), Some(&session));
        assert!(engine.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            DiagnosticEvent::SourceObservationsInvalidated {
                source: observed_source,
                session: observed_session,
                cars,
            } if observed_source == &source
                && observed_session == &session
                && cars == std::slice::from_ref(&car)
        )));

        assert!(matches!(
            engine.ingest_observation_update(update(100)),
            ObservationIngestResult::Rejected(ObservationIngestError::Assembly(
                ObservationAssemblyError::NonIncreasingSequence { .. }
            ))
        ));
        assert!(engine.operational_car_state(&car).is_none());
        assert_eq!(
            engine.ingest_observation_update(update(101)),
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(state.clone()))
        );
        assert_eq!(engine.operational_car_state(&car), Some(&state));
    }

    #[test]
    fn stale_invalidation_from_an_old_session_cannot_revoke_the_active_session() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let source = ObservationSourceId::new("controller-main");
        let old_session = ObservationSessionId::new("connection-A");
        let active_session = ObservationSessionId::new("connection-B");
        let car = CarId::new("A");
        engine
            .bind_observation(
                source.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("one operational binding is valid");
        engine
            .begin_source_session(source.clone(), old_session.clone())
            .expect("first session starts");
        engine
            .begin_source_session(source.clone(), active_session.clone())
            .expect("second session starts");
        let state = CarState::new("A", Floor::new(4));
        engine.ingest_observation_update(CarObservationUpdate::from_state(
            ObservationProvenance {
                source: source.clone(),
                session: active_session.clone(),
                sequence: ObservationSequence::new(1),
            },
            &state,
        ));

        assert_eq!(
            engine.invalidate_source_observations(source.clone(), old_session.clone()),
            Err(SourceObservationInvalidationError::InactiveSourceSession {
                source: source.clone(),
                active: active_session,
                requested: old_session,
            })
        );
        assert_eq!(engine.operational_car_state(&car), Some(&state));
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

    #[test]
    fn operational_view_is_unique_and_auxiliary_state_cannot_change_dispatch() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let controller = ObservationSourceId::new("controller-main");
        let controller_session = ObservationSessionId::new("controller-session-A");
        let auxiliary = ObservationSourceId::new("bacnet-read-only");
        let auxiliary_session = ObservationSessionId::new("bacnet-session-A");
        for car in [CarId::new("A"), CarId::new("B")] {
            engine
                .bind_observation(
                    controller.clone(),
                    car.clone(),
                    ObservationBindingRole::Operational,
                )
                .expect("controller has one operational binding per car");
            engine.set_execution_capabilities(car, ExecutionCapabilities::reference_defaults());
        }
        engine
            .bind_observation(
                auxiliary.clone(),
                CarId::new("A"),
                ObservationBindingRole::Auxiliary,
            )
            .expect("read-only source is explicitly auxiliary");
        engine
            .begin_source_session(controller.clone(), controller_session.clone())
            .expect("controller session starts explicitly");
        engine
            .begin_source_session(auxiliary.clone(), auxiliary_session.clone())
            .expect("auxiliary session starts explicitly");

        for (car, sequence, floor) in [("A", 1, 1), ("B", 2, 5)] {
            let state = CarState::new(car, Floor::new(floor));
            engine.ingest_observation_update(CarObservationUpdate::from_state(
                ObservationProvenance {
                    source: controller.clone(),
                    session: controller_session.clone(),
                    sequence: ObservationSequence::new(sequence),
                },
                &state,
            ));
        }
        let contradictory_auxiliary = CarState::new("A", Floor::new(8));
        engine.ingest_observation_update(CarObservationUpdate::from_state(
            ObservationProvenance {
                source: auxiliary.clone(),
                session: auxiliary_session,
                sequence: ObservationSequence::new(1),
            },
            &contradictory_auxiliary,
        ));

        assert_eq!(
            engine.car_states(),
            vec![
                CarState::new("A", Floor::new(1)),
                CarState::new("B", Floor::new(5)),
            ]
        );
        assert_eq!(
            engine.source_state(&auxiliary, &CarId::new("A")),
            Some(&contradictory_auxiliary)
        );
        assert_eq!(
            engine.execution_capabilities(&CarId::new("A")).cloned(),
            Some(ExecutionCapabilities::reference_defaults())
        );
        assert!(engine.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            DiagnosticEvent::Observation {
                update,
                role: ObservationBindingRole::Auxiliary,
            } if update.provenance.source == auxiliary && update.car == CarId::new("A")
        )));
        let mut backend = TestBackend {
            intents: Vec::new(),
        };
        engine.submit_call(
            HallCall {
                floor: Floor::new(8),
                direction: Direction::Down,
            },
            &mut backend,
        );
        assert_eq!(
            backend.intents,
            vec![Intent::ServeFloor {
                car: CarId::new("B"),
                floor: Floor::new(8),
            }]
        );
    }

    #[test]
    fn incomplete_operational_evidence_is_not_fused_with_complete_auxiliary_state() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let controller = ObservationSourceId::new("controller-main");
        let controller_session = ObservationSessionId::new("controller-session-A");
        let auxiliary = ObservationSourceId::new("open-rmf-read-only");
        let auxiliary_session_a = ObservationSessionId::new("rmf-session-A");
        let car = CarId::new("A");
        engine
            .bind_observation(
                controller.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("controller is the observation authority");
        engine
            .bind_observation(
                auxiliary.clone(),
                car.clone(),
                ObservationBindingRole::Auxiliary,
            )
            .expect("Open-RMF is inspection-only");
        engine
            .begin_source_session(controller.clone(), controller_session.clone())
            .expect("controller session starts explicitly");
        engine
            .begin_source_session(auxiliary.clone(), auxiliary_session_a.clone())
            .expect("auxiliary session starts explicitly");

        let operational = CarState::new("A", Floor::new(3));
        engine.ingest_observation_update(CarObservationUpdate::from_state(
            ObservationProvenance {
                source: controller.clone(),
                session: controller_session.clone(),
                sequence: ObservationSequence::new(1),
            },
            &operational,
        ));
        let mut auxiliary_complete = CarState::new("A", Floor::new(7));
        auxiliary_complete.door = DoorState::Open;
        engine.ingest_observation_update(CarObservationUpdate::from_state(
            ObservationProvenance {
                source: auxiliary.clone(),
                session: auxiliary_session_a,
                sequence: ObservationSequence::new(1),
            },
            &auxiliary_complete,
        ));
        assert_eq!(engine.operational_car_state(&car), Some(&operational));

        let mut operational_incomplete = CarObservationUpdate::from_state(
            ObservationProvenance {
                source: controller.clone(),
                session: controller_session.clone(),
                sequence: ObservationSequence::new(2),
            },
            &operational,
        );
        operational_incomplete.door = Some(DoorEvidence::Unknown);
        assert!(matches!(
            engine.ingest_observation_update(operational_incomplete),
            ObservationIngestResult::Resolution(CarStateResolution::Unresolved { .. })
        ));
        assert!(engine.operational_car_state(&car).is_none());
        assert!(engine.car_states().is_empty());
        assert_eq!(
            engine.source_state(&auxiliary, &car),
            Some(&auxiliary_complete)
        );

        engine.ingest_observation_update(CarObservationUpdate::from_state(
            ObservationProvenance {
                source: controller,
                session: controller_session,
                sequence: ObservationSequence::new(3),
            },
            &operational,
        ));
        let auxiliary_transition = engine
            .begin_source_session(
                auxiliary.clone(),
                ObservationSessionId::new("rmf-session-B"),
            )
            .expect("auxiliary source may explicitly restart");
        assert_eq!(auxiliary_transition.invalidated_cars, vec![car.clone()]);
        assert_eq!(engine.operational_car_state(&car), Some(&operational));
        assert!(engine.source_state(&auxiliary, &car).is_none());
    }
}
