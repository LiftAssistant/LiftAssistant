use std::collections::{BTreeMap, VecDeque};

use liftassistant_model::{
    CallId, CallState, CarId, CarLocation, CarState, DoorState, Floor, Intent, IntentId,
    MotionState, ObservationProvenance, OperatingMode, ReportedDestination,
};
use liftassistant_runtime::{
    AssembledCarObservation, ControlledIntentSink, ControllerId, IntentReceipt, ReceiptStatus,
    Submission,
};

use crate::Engine;

const RECENT_INTENTS: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalId(String);

impl PrincipalId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorityGrantId(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityGrant {
    pub id: AuthorityGrantId,
    pub principal: PrincipalId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityError {
    ControllerUnbound,
    ControllerAlreadyBound,
    ScopeOccupied,
    AlreadyGranted,
    Fenced,
    IdentityExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentRequest {
    pub id: IntentId,
    pub intent: Intent,
    pub call: Option<CallId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntentRecord {
    pub request: IntentRequest,
    pub controller: ControllerId,
    pub authority: AuthorityGrant,
    pub receipt: Receipt,
    pub reconciliation: Reconciliation,
    baseline: ObservationProvenance,
    started_at: CarLocation,
    saw_progress: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Receipt {
    NotSubmitted,
    Awaiting,
    Accepted,
    Rejected,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reconciliation {
    Pending,
    Satisfied,
    Diverged { observed: Floor },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptResult {
    Recorded,
    Duplicate,
    Unexpected,
    Inconsistent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlledEvent {
    Admission {
        request: IntentRequest,
        grant: AuthorityGrantId,
        controller: ControllerId,
        result: Admission,
    },
    Submission(IntentId, Submission),
    Receipt {
        receipt: IntentReceipt,
        result: ReceiptResult,
    },
    Reconciliation {
        intent: IntentId,
        desired: Floor,
        outcome: Reconciliation,
    },
    ReadinessLost {
        intent: IntentId,
        reason: WriteBlocker,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteBlocker {
    AuthorityFenced,
    ControllerUnbound,
    WrongController,
    ControllerUnavailable,
    ObservationUnavailable,
    Mode(OperatingMode),
    Busy,
    CapabilitiesUnavailable,
    OperationUnsupported,
    UnsupportedFloor,
    InvalidCall,
    ActiveIntent(IntentId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Admission {
    Admitted,
    Retry,
    IdentityConflict,
    RetiredIdentity,
    Blocked(WriteBlocker),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ControlledIntents {
    controllers: BTreeMap<CarId, ControllerId>,
    grants: BTreeMap<CarId, AuthorityGrant>,
    last_grant: u64,
    active: BTreeMap<CarId, IntentRecord>,
    recent: VecDeque<IntentRecord>,
    last_admitted: Option<IntentId>,
}

impl ControlledIntents {
    fn record(&self, id: IntentId) -> Option<&IntentRecord> {
        self.active
            .values()
            .chain(self.recent.iter())
            .find(|record| record.request.id == id)
    }

    fn record_mut(&mut self, id: IntentId) -> Option<&mut IntentRecord> {
        self.active
            .values_mut()
            .chain(self.recent.iter_mut())
            .find(|record| record.request.id == id)
    }

    fn finish(&mut self, car: &CarId) {
        let record = self.active.remove(car).expect("finishing an active intent");
        if self.recent.len() == RECENT_INTENTS {
            self.recent.pop_front();
        }
        self.recent.push_back(record);
    }

    pub(crate) fn has_controller(&self, car: &CarId) -> bool {
        self.controllers.contains_key(car)
    }

    pub(crate) fn is_configured(&self) -> bool {
        !self.controllers.is_empty()
    }

    fn check_grant(&self, car: &CarId, grant: &AuthorityGrant) -> Result<(), WriteBlocker> {
        if self.grants.get(car) != Some(grant) {
            return Err(WriteBlocker::AuthorityFenced);
        }
        Ok(())
    }
}

impl Engine {
    pub fn bind_controlled_car(
        &mut self,
        car: CarId,
        controller: ControllerId,
    ) -> Result<(), AuthorityError> {
        if !self.is_controlled() && !self.calls.occupied_cars().is_empty() {
            return Err(AuthorityError::ScopeOccupied);
        }
        if self.controlled.has_controller(&car) {
            return Err(AuthorityError::ControllerAlreadyBound);
        }
        self.controlled.controllers.insert(car, controller);
        Ok(())
    }

    pub(crate) fn is_controlled(&self) -> bool {
        self.controlled.is_configured()
    }

    pub fn grant_authority(
        &mut self,
        car: &CarId,
        principal: PrincipalId,
    ) -> Result<AuthorityGrant, AuthorityError> {
        if !self.controlled.has_controller(car) {
            return Err(AuthorityError::ControllerUnbound);
        }
        if self.controlled.grants.contains_key(car) {
            return Err(AuthorityError::AlreadyGranted);
        }
        let id = self
            .controlled
            .last_grant
            .checked_add(1)
            .ok_or(AuthorityError::IdentityExhausted)?;
        self.controlled.last_grant = id;
        let grant = AuthorityGrant {
            id: AuthorityGrantId(id),
            principal,
        };
        self.controlled.grants.insert(car.clone(), grant.clone());
        Ok(grant)
    }

    pub fn revoke_authority(
        &mut self,
        car: &CarId,
        grant: &AuthorityGrant,
    ) -> Result<(), AuthorityError> {
        self.controlled
            .check_grant(car, grant)
            .map_err(|_| AuthorityError::Fenced)?;
        self.controlled.grants.remove(car);
        Ok(())
    }

    pub fn intent_record(&self, id: IntentId) -> Option<&IntentRecord> {
        self.controlled.record(id)
    }

    /// Per-request write readiness. The adapter supplies its current availability.
    pub fn write_readiness(
        &self,
        request: &IntentRequest,
        grant: &AuthorityGrant,
        controller: &ControllerId,
        controller_available: bool,
    ) -> Result<(), WriteBlocker> {
        let car = request.intent.car();
        let expected = self
            .controlled
            .controllers
            .get(car)
            .ok_or(WriteBlocker::ControllerUnbound)?;
        self.controlled.check_grant(car, grant)?;
        if expected != controller {
            return Err(WriteBlocker::WrongController);
        }
        if !controller_available {
            return Err(WriteBlocker::ControllerUnavailable);
        }
        let state = self
            .operational_car_state(car)
            .ok_or(WriteBlocker::ObservationUnavailable)?;
        if state.mode != OperatingMode::Normal {
            return Err(WriteBlocker::Mode(state.mode));
        }
        let caps = self
            .execution_capabilities(car)
            .ok_or(WriteBlocker::CapabilitiesUnavailable)?;
        let supported = match &request.intent {
            Intent::ServeFloor { .. } => caps.serve_floor,
            Intent::ParkAt { .. } => caps.park,
        };
        if !supported {
            return Err(WriteBlocker::OperationUnsupported);
        }
        let floor = request.intent.floor();
        if !self.topology.contains(floor) || !caps.supports_floor(floor) {
            return Err(WriteBlocker::UnsupportedFloor);
        }
        if let Some(active) = self.controlled.active.get(car) {
            return Err(WriteBlocker::ActiveIntent(active.request.id));
        }
        if let Some(id) = request.call {
            let valid = matches!(&request.intent, Intent::ServeFloor { .. })
                && self.calls.get(id).is_some_and(|call| {
                    call.state == CallState::Pending && call.call.floor == floor
                })
                && !self
                    .controlled
                    .active
                    .values()
                    .any(|record| record.request.call == Some(id));
            if !valid {
                return Err(WriteBlocker::InvalidCall);
            }
        }
        if self.calls.occupied_cars().contains(car)
            || state.motion != MotionState::Stopped
            || match state.reported_destination {
                ReportedDestination::None => false,
                ReportedDestination::Unknown => true,
                ReportedDestination::Known(target) => target != state.location.floor_reference(),
            }
        {
            return Err(WriteBlocker::Busy);
        }
        Ok(())
    }

    /// Register a hall call for the controlled composition. Dispatch is driven by an
    /// explicitly authorized request, with the CallId carried separately.
    pub fn register_controlled_call(
        &mut self,
        call: liftassistant_model::HallCall,
    ) -> Result<liftassistant_core::CallRegistration, liftassistant_core::HallCallRejection> {
        liftassistant_core::validate_hall_call(&call, &self.topology)?;
        Ok(self.calls.register(call))
    }

    /// Virtual intent admission reserves a car before lower-controller submission.
    fn admit_virtual_intent(
        &mut self,
        request: IntentRequest,
        grant: &AuthorityGrant,
        controller: &ControllerId,
        controller_available: bool,
    ) -> Admission {
        if let Err(reason) = self.controlled.check_grant(request.intent.car(), grant) {
            return Admission::Blocked(reason);
        }
        if let Some(record) = self.controlled.record(request.id) {
            return if record.request == request {
                Admission::Retry
            } else {
                Admission::IdentityConflict
            };
        }
        // M6A uses one deterministically allocated, monotonically issued ID domain.
        // After bounded outcome retention, the watermark still prevents re-execution.
        if self
            .controlled
            .last_admitted
            .is_some_and(|last| request.id <= last)
        {
            return Admission::RetiredIdentity;
        }
        if let Err(reason) = self.write_readiness(&request, grant, controller, controller_available)
        {
            return Admission::Blocked(reason);
        }
        let car = request.intent.car();
        let state = self.operational_car_state(car).expect("ready car");
        let source = self
            .operational_authorities
            .get(car)
            .expect("ready authority");
        let baseline = self
            .evidence(source, car)
            .expect("ready evidence")
            .latest_provenance
            .clone();
        let started_at = state.location;
        self.controlled.last_admitted = Some(request.id);
        self.controlled.active.insert(
            car.clone(),
            IntentRecord {
                request,
                controller: controller.clone(),
                authority: grant.clone(),
                receipt: Receipt::NotSubmitted,
                reconciliation: Reconciliation::Pending,
                baseline,
                started_at,
                saw_progress: false,
            },
        );
        Admission::Admitted
    }

    /// Reserve and submit once. Local delivery, receipt, and physical reconciliation
    /// advance independently.
    pub fn submit_controlled(
        &mut self,
        request: IntentRequest,
        grant: &AuthorityGrant,
        backend: &mut impl ControlledIntentSink,
    ) -> Admission {
        let car = request.intent.car();
        let controller = backend.controller_id().clone();
        let available = backend.available(car);
        let admission = self.admit_virtual_intent(request.clone(), grant, &controller, available);
        self.diagnostics
            .record_controlled(ControlledEvent::Admission {
                request: request.clone(),
                grant: grant.id,
                controller,
                result: admission,
            });
        if admission == Admission::Admitted {
            let submission = backend.submit(request.id, &request.intent);
            self.diagnostics
                .record_controlled(ControlledEvent::Submission(request.id, submission));
            self.controlled
                .active
                .get_mut(car)
                .expect("admitted record")
                .receipt = match submission {
                Submission::NotSubmitted => Receipt::NotSubmitted,
                Submission::Submitted => Receipt::Awaiting,
                Submission::Indeterminate => Receipt::Indeterminate,
            };
            if submission == Submission::NotSubmitted {
                self.controlled.finish(car);
            }
        }
        admission
    }

    pub fn receive_intent_receipt(&mut self, receipt: IntentReceipt) -> ReceiptResult {
        let result = self.apply_intent_receipt(&receipt);
        self.diagnostics
            .record_controlled(ControlledEvent::Receipt { receipt, result });
        result
    }

    fn apply_intent_receipt(&mut self, receipt: &IntentReceipt) -> ReceiptResult {
        let Some(record) = self.controlled.record_mut(receipt.intent) else {
            return ReceiptResult::Unexpected;
        };
        if record.controller != receipt.controller {
            return ReceiptResult::Unexpected;
        }
        let next = match receipt.status {
            ReceiptStatus::Accepted => Receipt::Accepted,
            ReceiptStatus::Rejected => Receipt::Rejected,
            ReceiptStatus::Indeterminate => Receipt::Indeterminate,
        };
        if record.receipt == next {
            return ReceiptResult::Duplicate;
        }
        match record.receipt {
            Receipt::NotSubmitted | Receipt::Accepted | Receipt::Rejected => {
                return ReceiptResult::Inconsistent;
            }
            Receipt::Awaiting | Receipt::Indeterminate => {}
        }
        record.receipt = next;
        if next == Receipt::Rejected {
            let car = record.request.intent.car().clone();
            if record.reconciliation != Reconciliation::Pending || record.saw_progress {
                return ReceiptResult::Inconsistent;
            }
            if self
                .controlled
                .active
                .get(&car)
                .is_some_and(|active| active.request.id == receipt.intent)
            {
                self.controlled.finish(&car);
            }
        }
        ReceiptResult::Recorded
    }

    pub(crate) fn note_controlled_readiness_loss(&mut self, car: &CarId, reason: WriteBlocker) {
        if let Some(record) = self.controlled.active.get(car) {
            self.diagnostics
                .record_controlled(ControlledEvent::ReadinessLost {
                    intent: record.request.id,
                    reason,
                });
        }
    }

    pub(crate) fn reconcile_controlled(
        &mut self,
        state: &CarState,
        evidence: &AssembledCarObservation,
    ) {
        let Some(record) = self.controlled.active.get_mut(&state.id) else {
            return;
        };
        let newer = |provenance: &ObservationProvenance| {
            provenance.source == record.baseline.source
                && (provenance.session != record.baseline.session
                    || provenance.sequence > record.baseline.sequence)
        };
        if !evidence
            .position
            .as_ref()
            .is_some_and(|sample| newer(&sample.provenance))
            || !evidence
                .motion
                .as_ref()
                .is_some_and(|sample| newer(&sample.provenance))
        {
            return;
        }
        let position = |location| match location {
            CarLocation::Floor(floor) => f64::from(floor.index()),
            CarLocation::Continuous(position) => position.floor_position,
        };
        let floor = state.location.floor_reference();
        let at_floor = position(state.location) == f64::from(floor.index());
        let requested = record.request.intent.floor();
        let stopped = state.motion == MotionState::Stopped;
        if record.reconciliation == Reconciliation::Pending {
            record.saw_progress |=
                !stopped || position(state.location) != position(record.started_at);
            if stopped && at_floor && floor == requested {
                record.reconciliation = Reconciliation::Satisfied;
            } else if stopped
                && at_floor
                && record.saw_progress
                && evidence
                    .reported_destination
                    .as_ref()
                    .is_some_and(|sample| newer(&sample.provenance))
                && (state.reported_destination == ReportedDestination::None
                    || state.reported_destination == ReportedDestination::Known(floor))
            {
                record.reconciliation = Reconciliation::Diverged { observed: floor };
            }
            if record.reconciliation != Reconciliation::Pending {
                self.diagnostics
                    .record_controlled(ControlledEvent::Reconciliation {
                        intent: record.request.id,
                        desired: requested,
                        outcome: record.reconciliation,
                    });
                if record.receipt == Receipt::Rejected {
                    self.diagnostics
                        .record_controlled(ControlledEvent::Receipt {
                            receipt: IntentReceipt {
                                controller: record.controller.clone(),
                                intent: record.request.id,
                                status: ReceiptStatus::Rejected,
                            },
                            result: ReceiptResult::Inconsistent,
                        });
                }
            }
        }
        match record.reconciliation {
            Reconciliation::Pending => return,
            Reconciliation::Satisfied => {
                if let Some(call) = record.request.call {
                    if !stopped
                        || !at_floor
                        || floor != requested
                        || state.door != DoorState::Open
                        || !evidence
                            .door
                            .as_ref()
                            .is_some_and(|sample| newer(&sample.provenance))
                    {
                        return;
                    }
                    assert!(self.calls.assign(call, state.id.clone()));
                    assert!(self.calls.mark_serving(call, &state.id));
                    assert!(self.calls.complete(call, &state.id));
                }
            }
            Reconciliation::Diverged { .. } => {}
        }
        self.controlled.finish(&state.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{
        BuildingTopology, CarObservationUpdate, CarState, ExecutionCapabilities, Floor,
        ObservationSequence, ObservationSessionId, ObservationSourceId,
    };

    fn observe(engine: &mut Engine, state: &CarState, sequence: u64) {
        engine.ingest_observation_update(CarObservationUpdate::from_state(
            ObservationProvenance {
                source: ObservationSourceId::new("physical"),
                session: ObservationSessionId::new("test-incarnation"),
                sequence: ObservationSequence::new(sequence),
            },
            state,
        ));
    }

    struct TestController(ControllerId);

    impl ControlledIntentSink for TestController {
        fn controller_id(&self) -> &ControllerId {
            &self.0
        }

        fn available(&self, _car: &CarId) -> bool {
            true
        }

        fn submit(&mut self, _id: IntentId, _intent: &Intent) -> Submission {
            Submission::Submitted
        }
    }

    #[test]
    fn accepted_receipt_does_not_complete_intent_or_hall_call() {
        use liftassistant_core::CallRegistration;
        use liftassistant_model::{Direction, HallCall};

        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let car = CarId::new("A");
        let mut controller = TestController(ControllerId::new("virtual"));
        engine
            .bind_controlled_car(car.clone(), controller.0.clone())
            .unwrap();
        let grant = engine
            .grant_authority(&car, PrincipalId::new("test"))
            .unwrap();
        let source = ObservationSourceId::new("physical");
        engine
            .bind_observation(
                source.clone(),
                car.clone(),
                crate::ObservationBindingRole::Operational,
            )
            .unwrap();
        engine
            .begin_source_session(source, ObservationSessionId::new("test-incarnation"))
            .unwrap();
        engine.set_execution_capabilities(car.clone(), ExecutionCapabilities::reference_defaults());
        let mut state = CarState::new("A", Floor::new(1));
        observe(&mut engine, &state, 1);
        let floor = Floor::new(5);
        let CallRegistration::Created(call) = engine
            .register_controlled_call(HallCall {
                floor,
                direction: Direction::Up,
            })
            .unwrap()
        else {
            panic!("new call");
        };
        let id = IntentId::new(1);
        let request = IntentRequest {
            id,
            call: Some(call),
            intent: Intent::ServeFloor { car, floor },
        };
        assert_eq!(
            engine.submit_controlled(request, &grant, &mut controller),
            Admission::Admitted
        );
        let receipt = IntentReceipt {
            controller: controller.0,
            intent: id,
            status: ReceiptStatus::Accepted,
        };
        assert_eq!(
            engine.receive_intent_receipt(receipt),
            ReceiptResult::Recorded
        );
        assert_eq!(
            engine.intent_record(id).unwrap().reconciliation,
            Reconciliation::Pending
        );
        state.location = CarLocation::Floor(floor);
        observe(&mut engine, &state, 1);
        assert_eq!(
            engine.intent_record(id).unwrap().reconciliation,
            Reconciliation::Pending
        );
        observe(&mut engine, &state, 2);
        assert_eq!(
            engine.intent_record(id).unwrap().reconciliation,
            Reconciliation::Satisfied
        );
        assert!(engine.calls().get(call).unwrap().state.is_active());
        state.door = DoorState::Open;
        observe(&mut engine, &state, 3);
        assert_eq!(
            engine.calls().get(call).unwrap().state,
            CallState::Completed
        );
    }

    #[test]
    fn admission_deduplicates_before_reservation_and_fences_revoked_grants() {
        let topology = BuildingTopology::new((1..=8).map(Floor::new)).unwrap();
        let mut engine = Engine::new(topology);
        let car = CarId::new("A");
        let controller = ControllerId::new("virtual");
        let principal = PrincipalId::new("writer");
        engine
            .bind_controlled_car(car.clone(), controller.clone())
            .unwrap();
        let grant = engine.grant_authority(&car, principal.clone()).unwrap();
        assert_eq!(
            engine.grant_authority(&car, PrincipalId::new("other")),
            Err(AuthorityError::AlreadyGranted)
        );
        let mut request = IntentRequest {
            id: IntentId::new(1),
            intent: Intent::ServeFloor {
                car: car.clone(),
                floor: Floor::new(5),
            },
            call: None,
        };
        assert_eq!(
            engine.admit_virtual_intent(request.clone(), &grant, &controller, true),
            Admission::Blocked(WriteBlocker::ObservationUnavailable)
        );
        let source = ObservationSourceId::new("physical");
        engine
            .bind_observation(
                source.clone(),
                car.clone(),
                crate::ObservationBindingRole::Operational,
            )
            .unwrap();
        engine
            .begin_source_session(source, ObservationSessionId::new("test-incarnation"))
            .unwrap();
        observe(&mut engine, &CarState::new("A", Floor::new(1)), 1);
        engine.set_execution_capabilities(car.clone(), ExecutionCapabilities::reference_defaults());
        assert_eq!(
            engine.admit_virtual_intent(request.clone(), &grant, &controller, true),
            Admission::Admitted
        );
        for _ in 0..10 {
            assert_eq!(
                engine.admit_virtual_intent(request.clone(), &grant, &controller, true),
                Admission::Retry
            );
        }
        let before = engine.intent_record(request.id).cloned();
        request.intent = Intent::ParkAt {
            car: car.clone(),
            floor: Floor::new(2),
        };
        assert_eq!(
            engine.admit_virtual_intent(request.clone(), &grant, &controller, true),
            Admission::IdentityConflict
        );
        assert_eq!(engine.intent_record(request.id), before.as_ref());
        engine.revoke_authority(&car, &grant).unwrap();
        assert_ne!(
            engine.grant_authority(&car, principal).unwrap().id,
            grant.id
        );
        assert_eq!(
            engine.admit_virtual_intent(request, &grant, &controller, true),
            Admission::Blocked(WriteBlocker::AuthorityFenced)
        );
    }
}
