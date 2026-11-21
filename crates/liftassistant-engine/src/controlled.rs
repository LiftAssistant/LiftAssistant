use std::collections::{BTreeMap, VecDeque};

use liftassistant_model::{
    CallId, CallState, CarId, CarLocation, CarState, DoorState, Floor, Intent, IntentId,
    MotionState, ObservationProvenance, OperatingMode, ReportedDestination,
};
use liftassistant_runtime::{
    AssembledCarObservation, ControlledIntentSink, ControllerId, IntentReceipt, ReceiptStatus,
    Submission, WithdrawalOutcome,
};

use crate::Engine;

const RECENT_INTENTS: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalId(String);

impl PrincipalId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorityGrantId(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityGrant {
    id: AuthorityGrantId,
    principal: PrincipalId,
}

impl AuthorityGrant {
    pub const fn id(&self) -> AuthorityGrantId {
        self.id
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }
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
    id: IntentId,
    pub intent: Intent,
    pub call: Option<CallId>,
    original: (Intent, Option<CallId>),
}

impl IntentRequest {
    pub const fn id(&self) -> IntentId {
        self.id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntentRecord {
    pub request: IntentRequest,
    pub controller: ControllerId,
    pub authority: AuthorityGrant,
    pub receipt: Receipt,
    pub reconciliation: Reconciliation,
    pub withdrawal: Option<(WithdrawalReason, WithdrawalOutcome)>,
    baseline: ObservationProvenance,
    started_at: CarLocation,
    saw_progress: bool,
    saw_closed_at_target: bool,
}

impl IntentRecord {
    pub const fn started_at(&self) -> CarLocation {
        self.started_at
    }

    pub const fn saw_progress(&self) -> bool {
        self.saw_progress
    }

    pub const fn saw_closed_at_target(&self) -> bool {
        self.saw_closed_at_target
    }
}

/// Private workspace recovery input for one durably unresolved controlled operation.
///
/// This carries no transport/session identity and grants no authority. The host must
/// establish a fresh grant and current Operational evidence before Engine will restore it.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlledIntentRecovery {
    pub id: IntentId,
    pub intent: Intent,
    pub controller: ControllerId,
    pub receipt: Receipt,
    pub started_at: CarLocation,
    pub saw_progress: bool,
    pub saw_closed_at_target: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlledRecoveryError {
    AuthorityFenced,
    ControllerUnbound,
    WrongController,
    ActiveIntent,
    RetiredIdentity,
    ObservationUnavailable,
    CapabilitiesUnavailable,
    OperationUnsupported,
    UnsupportedFloor,
    TerminalReceipt,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WithdrawalReason {
    Cancelled,
    Expired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WithdrawalError {
    Inactive,
    AlreadyWithdrawn,
    WrongController,
    AuthorityFenced,
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
    Withdrawn {
        intent: IntentId,
        reason: WithdrawalReason,
        outcome: WithdrawalOutcome,
    },
}

/// In-memory proof of virtual restart. This has no persistence or interchange format.
pub struct VirtualCheckpoint(Engine);

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
    last_issued: u64,
    last_admitted: BTreeMap<CarId, IntentId>,
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
        self.note_controlled_readiness_loss(car, WriteBlocker::AuthorityFenced);
        Ok(())
    }

    pub fn intent_record(&self, id: IntentId) -> Option<&IntentRecord> {
        self.controlled.record(id)
    }

    pub fn controlled_controller(&self, car: &CarId) -> Option<&ControllerId> {
        self.controlled.controllers.get(car)
    }

    pub fn active_intent(&self, car: &CarId) -> Option<&IntentRecord> {
        self.controlled.active.get(car)
    }

    /// Issue an identity and retain its original meaning in the caller's opaque request.
    /// A retry can therefore be checked even after the outcome leaves recent history.
    pub fn issue_controlled_intent(
        &mut self,
        intent: Intent,
        call: Option<CallId>,
    ) -> IntentRequest {
        self.controlled.last_issued = self
            .controlled
            .last_issued
            .checked_add(1)
            .expect("virtual intent ID space exhausted");
        IntentRequest {
            id: IntentId::new(self.controlled.last_issued),
            original: (intent.clone(), call),
            intent,
            call,
        }
    }

    /// Restore only the bounded operation-identity watermark owned by a durable host.
    /// This creates no intent, grant, reservation, receipt, or observation state.
    pub fn restore_controlled_intent_watermark(&mut self, last: IntentId) {
        assert!(
            self.controlled.active.is_empty() && self.controlled.last_issued == 0,
            "controlled intent watermark must be restored before issuing work"
        );
        self.controlled.last_issued = last.value();
    }

    /// Restore one previously admitted operation without submitting it again.
    ///
    /// The caller owns durable storage. Engine requires a fresh grant and fresh current
    /// Operational evidence, then establishes a new observation baseline so only evidence
    /// arriving after recovery can complete the operation.
    pub fn recover_controlled_intent(
        &mut self,
        recovery: ControlledIntentRecovery,
        grant: &AuthorityGrant,
    ) -> Result<(), ControlledRecoveryError> {
        let car = recovery.intent.car().clone();
        self.controlled
            .check_grant(&car, grant)
            .map_err(|_| ControlledRecoveryError::AuthorityFenced)?;
        let expected = self
            .controlled
            .controllers
            .get(&car)
            .ok_or(ControlledRecoveryError::ControllerUnbound)?;
        if expected != &recovery.controller {
            return Err(ControlledRecoveryError::WrongController);
        }
        if self.controlled.active.contains_key(&car) {
            return Err(ControlledRecoveryError::ActiveIntent);
        }
        if self
            .controlled
            .last_admitted
            .get(&car)
            .is_some_and(|last| recovery.id <= *last)
        {
            return Err(ControlledRecoveryError::RetiredIdentity);
        }
        if !matches!(recovery.receipt, Receipt::Accepted | Receipt::Indeterminate) {
            return Err(ControlledRecoveryError::TerminalReceipt);
        }
        let caps = self
            .execution_capabilities(&car)
            .ok_or(ControlledRecoveryError::CapabilitiesUnavailable)?;
        if !matches!(recovery.intent, Intent::ServeFloor { .. }) || !caps.serve_floor {
            return Err(ControlledRecoveryError::OperationUnsupported);
        }
        if !self.topology.contains(recovery.intent.floor())
            || !caps.supports_floor(recovery.intent.floor())
        {
            return Err(ControlledRecoveryError::UnsupportedFloor);
        }
        self.operational_car_state(&car)
            .ok_or(ControlledRecoveryError::ObservationUnavailable)?;
        let source = self
            .operational_authorities
            .get(&car)
            .ok_or(ControlledRecoveryError::ObservationUnavailable)?;
        let baseline = self
            .evidence(source, &car)
            .ok_or(ControlledRecoveryError::ObservationUnavailable)?
            .latest_provenance
            .clone();
        let request = IntentRequest {
            id: recovery.id,
            intent: recovery.intent.clone(),
            call: None,
            original: (recovery.intent, None),
        };
        self.controlled.last_issued = self.controlled.last_issued.max(recovery.id.value());
        self.controlled
            .last_admitted
            .insert(car.clone(), recovery.id);
        self.controlled.active.insert(
            car,
            IntentRecord {
                request,
                controller: recovery.controller,
                authority: grant.clone(),
                receipt: recovery.receipt,
                reconciliation: Reconciliation::Pending,
                withdrawal: None,
                baseline,
                started_at: recovery.started_at,
                saw_progress: recovery.saw_progress,
                saw_closed_at_target: recovery.saw_closed_at_target,
            },
        );
        Ok(())
    }

    /// Advisory per-car readiness check without issuing an IntentId or mutating Engine state.
    pub fn preview_write_readiness(
        &self,
        intent: &Intent,
        call: Option<CallId>,
        grant: &AuthorityGrant,
        controller: &ControllerId,
        controller_available: bool,
    ) -> Result<(), WriteBlocker> {
        let car = intent.car();
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
        let supported = match intent {
            Intent::ServeFloor { .. } => caps.serve_floor,
            Intent::ParkAt { .. } => caps.park,
        };
        if !supported {
            return Err(WriteBlocker::OperationUnsupported);
        }
        let floor = intent.floor();
        if !self.topology.contains(floor) || !caps.supports_floor(floor) {
            return Err(WriteBlocker::UnsupportedFloor);
        }
        if let Some(active) = self.controlled.active.get(car) {
            return Err(WriteBlocker::ActiveIntent(active.request.id));
        }
        if let Some(id) = call {
            let valid = matches!(intent, Intent::ServeFloor { .. })
                && !self.external_dispatch.owns(id)
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

    /// Per-car readiness for this operation. Admission separately checks request identity.
    pub fn write_readiness(
        &self,
        request: &IntentRequest,
        grant: &AuthorityGrant,
        controller: &ControllerId,
        controller_available: bool,
    ) -> Result<(), WriteBlocker> {
        self.preview_write_readiness(
            &request.intent,
            request.call,
            grant,
            controller,
            controller_available,
        )
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
        if let Err(reason) = self.controlled.check_grant(request.original.0.car(), grant) {
            return Admission::Blocked(reason);
        }
        if request.intent != request.original.0 || request.call != request.original.1 {
            return Admission::IdentityConflict;
        }
        if let Some(record) = self.controlled.record(request.id) {
            return if record.request == request {
                Admission::Retry
            } else {
                Admission::IdentityConflict
            };
        }
        // New executions for each car advance independently. Its watermark still
        // fences old identities after bounded outcome retention.
        let car = request.intent.car();
        if self
            .controlled
            .last_admitted
            .get(car)
            .is_some_and(|last| request.id <= *last)
        {
            return Admission::RetiredIdentity;
        }
        if let Err(reason) = self.write_readiness(&request, grant, controller, controller_available)
        {
            return Admission::Blocked(reason);
        }
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
        self.controlled
            .last_admitted
            .insert(car.clone(), request.id);
        self.controlled.active.insert(
            car.clone(),
            IntentRecord {
                request,
                controller: controller.clone(),
                authority: grant.clone(),
                receipt: Receipt::NotSubmitted,
                reconciliation: Reconciliation::Pending,
                withdrawal: None,
                baseline,
                started_at,
                saw_progress: false,
                saw_closed_at_target: false,
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
            // If the sink unwinds after possible transmission, preserve uncertainty and
            // keep the admitted identity active so a caller that catches the unwind cannot
            // resend it or accept a conflicting replacement.
            self.controlled
                .active
                .get_mut(car)
                .expect("admitted record")
                .receipt = Receipt::Indeterminate;
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

    pub fn cancel_controlled_intent(
        &mut self,
        id: IntentId,
        grant: &AuthorityGrant,
        backend: &mut impl ControlledIntentSink,
    ) -> Result<WithdrawalOutcome, WithdrawalError> {
        let record = self
            .controlled
            .record(id)
            .ok_or(WithdrawalError::Inactive)?;
        self.controlled
            .check_grant(record.request.intent.car(), grant)
            .map_err(|_| WithdrawalError::AuthorityFenced)?;
        self.withdraw_controlled(id, WithdrawalReason::Cancelled, backend)
    }

    /// The host decides when to issue this action and owns the clock.
    pub fn expire_controlled_intent(
        &mut self,
        id: IntentId,
        backend: &mut impl ControlledIntentSink,
    ) -> Result<WithdrawalOutcome, WithdrawalError> {
        self.withdraw_controlled(id, WithdrawalReason::Expired, backend)
    }

    fn withdraw_controlled(
        &mut self,
        id: IntentId,
        reason: WithdrawalReason,
        backend: &mut impl ControlledIntentSink,
    ) -> Result<WithdrawalOutcome, WithdrawalError> {
        let record = self
            .controlled
            .active
            .values_mut()
            .find(|record| record.request.id == id)
            .ok_or(WithdrawalError::Inactive)?;
        if record.withdrawal.is_some() {
            return Err(WithdrawalError::AlreadyWithdrawn);
        }
        if backend.controller_id() != &record.controller {
            return Err(WithdrawalError::WrongController);
        }
        // Requested is the conservative existing state when the sink may unwind after
        // transmitting the withdrawal. Pre-reserving it fences a second command without
        // claiming that execution was prevented.
        record.withdrawal = Some((reason, WithdrawalOutcome::Requested));
        let outcome = backend.withdraw(id);
        record.withdrawal = Some((reason, outcome));
        let car = record.request.intent.car().clone();
        self.diagnostics
            .record_controlled(ControlledEvent::Withdrawn {
                intent: id,
                reason,
                outcome,
            });
        // Cancellation never changes observed motion. Release only on explicit
        // prevention or an already-observed physical outcome.
        if outcome == WithdrawalOutcome::Prevented
            || record.reconciliation != Reconciliation::Pending
        {
            self.controlled.finish(&car);
        }
        Ok(outcome)
    }

    pub fn receive_intent_receipt(&mut self, receipt: IntentReceipt) -> ReceiptResult {
        let result = self.apply_intent_receipt(&receipt);
        self.diagnostics
            .record_controlled(ControlledEvent::Receipt { receipt, result });
        result
    }

    /// Obtain one explicitly correlated receipt after reconnecting. A query never submits.
    pub fn resynchronize_intent(
        &mut self,
        id: IntentId,
        backend: &impl ControlledIntentSink,
    ) -> Option<ReceiptResult> {
        let expected = &self.intent_record(id)?.controller;
        if backend.controller_id() != expected {
            return Some(ReceiptResult::Unexpected);
        }
        let receipt = backend.receipt(id)?;
        if receipt.intent != id {
            self.diagnostics
                .record_controlled(ControlledEvent::Receipt {
                    receipt,
                    result: ReceiptResult::Unexpected,
                });
            return Some(ReceiptResult::Unexpected);
        }
        Some(self.receive_intent_receipt(receipt))
    }

    /// The host reports loss of the command channel independently of observations.
    pub fn controlled_transport_lost(&mut self, controller: &ControllerId) {
        for record in self
            .controlled
            .active
            .values_mut()
            .filter(|r| &r.controller == controller)
        {
            let intent = record.request.id;
            if record.receipt == Receipt::Awaiting {
                record.receipt = Receipt::Indeterminate;
                self.diagnostics
                    .record_controlled(ControlledEvent::Receipt {
                        receipt: IntentReceipt {
                            intent,
                            controller: controller.clone(),
                            status: ReceiptStatus::Indeterminate,
                        },
                        result: ReceiptResult::Recorded,
                    });
            }
            self.diagnostics
                .record_controlled(ControlledEvent::ReadinessLost {
                    intent,
                    reason: WriteBlocker::ControllerUnavailable,
                });
        }
    }

    /// Consume this orchestration lifetime. No delivery or operation is inferred from
    /// taking a checkpoint.
    pub fn into_virtual_checkpoint(self) -> VirtualCheckpoint {
        VirtualCheckpoint(self)
    }

    pub fn restore_virtual(checkpoint: VirtualCheckpoint) -> Self {
        let mut engine = checkpoint.0;
        engine.controlled.grants.clear();
        engine.external_dispatch.fence_grants();
        for record in engine.controlled.active.values_mut() {
            if record.receipt == Receipt::Awaiting {
                record.receipt = Receipt::Indeterminate;
            }
            let car = record.request.intent.car();
            if let Some(source) = engine.operational_authorities.get(car) {
                if let Some(session) = engine.active_source_sessions.get(source) {
                    if let Some(evidence) = engine.observations.current(source, session, car) {
                        record.baseline = evidence.latest_provenance.clone();
                    }
                }
            }
        }
        for (source, session) in engine.active_source_sessions.clone() {
            engine
                .invalidate_source_observations(source, session)
                .expect("current source session");
        }
        engine
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
        if next == Receipt::Indeterminate
            && matches!(record.receipt, Receipt::Accepted | Receipt::Rejected)
        {
            // Transport loss can no longer make an already known receipt uncertain.
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
                let new_door = evidence
                    .door
                    .as_ref()
                    .is_some_and(|sample| newer(&sample.provenance));
                record.saw_closed_at_target |= new_door && state.door == DoorState::Closed;
                if record.saw_progress
                    || (record.saw_closed_at_target && new_door && state.door == DoorState::Open)
                {
                    record.reconciliation = Reconciliation::Satisfied;
                }
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
                if let Some(call) = record.request.call.filter(|_| record.withdrawal.is_none()) {
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

    struct InterruptingController {
        id: ControllerId,
        submissions: usize,
        withdrawals: usize,
        interrupt_submission: bool,
        interrupt_withdrawal: bool,
    }

    impl ControlledIntentSink for InterruptingController {
        fn controller_id(&self) -> &ControllerId {
            &self.id
        }

        fn available(&self, _car: &CarId) -> bool {
            true
        }

        fn submit(&mut self, _id: IntentId, _intent: &Intent) -> Submission {
            self.submissions += 1;
            assert!(!self.interrupt_submission, "interrupted controlled send");
            Submission::Submitted
        }

        fn withdraw(&mut self, _id: IntentId) -> WithdrawalOutcome {
            self.withdrawals += 1;
            assert!(
                !self.interrupt_withdrawal,
                "interrupted controlled withdrawal"
            );
            WithdrawalOutcome::Requested
        }
    }

    #[test]
    fn interrupted_controlled_commands_preserve_uncertainty_and_fence_resend() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let car = CarId::new("A");
        let source = ObservationSourceId::new("physical");
        let mut backend = InterruptingController {
            id: ControllerId::new("virtual"),
            submissions: 0,
            withdrawals: 0,
            interrupt_submission: true,
            interrupt_withdrawal: false,
        };
        engine
            .bind_controlled_car(car.clone(), backend.id.clone())
            .unwrap();
        let grant = engine
            .grant_authority(&car, PrincipalId::new("test"))
            .unwrap();
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
        observe(&mut engine, &CarState::new("A", Floor::new(1)), 1);
        let request = engine.issue_controlled_intent(
            Intent::ServeFloor {
                car,
                floor: Floor::new(2),
            },
            None,
        );

        let send = catch_unwind(AssertUnwindSafe(|| {
            let _ = engine.submit_controlled(request.clone(), &grant, &mut backend);
        }));
        assert!(send.is_err());
        assert_eq!(backend.submissions, 1);
        assert_eq!(
            engine.intent_record(request.id()).unwrap().receipt,
            Receipt::Indeterminate
        );

        backend.interrupt_submission = false;
        assert_eq!(
            engine.submit_controlled(request.clone(), &grant, &mut backend),
            Admission::Retry
        );
        assert_eq!(backend.submissions, 1);
        assert_eq!(
            engine.receive_intent_receipt(IntentReceipt {
                controller: backend.id.clone(),
                intent: request.id(),
                status: ReceiptStatus::Accepted,
            }),
            ReceiptResult::Recorded
        );

        backend.interrupt_withdrawal = true;
        let withdrawal = catch_unwind(AssertUnwindSafe(|| {
            let _ = engine.cancel_controlled_intent(request.id(), &grant, &mut backend);
        }));
        assert!(withdrawal.is_err());
        assert_eq!(backend.withdrawals, 1);
        assert_eq!(
            engine.intent_record(request.id()).unwrap().withdrawal,
            Some((WithdrawalReason::Cancelled, WithdrawalOutcome::Requested))
        );

        backend.interrupt_withdrawal = false;
        assert_eq!(
            engine.cancel_controlled_intent(request.id(), &grant, &mut backend),
            Err(WithdrawalError::AlreadyWithdrawn)
        );
        assert_eq!(backend.withdrawals, 1);
    }

    #[test]
    fn unavailable_operational_mode_blocks_controlled_admission() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let car = CarId::new("A");
        let source = ObservationSourceId::new("physical");
        let mut backend = TestController(ControllerId::new("virtual"));
        engine
            .bind_controlled_car(car.clone(), backend.0.clone())
            .unwrap();
        let grant = engine
            .grant_authority(&car, PrincipalId::new("test"))
            .unwrap();
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
        state.mode = OperatingMode::OutOfService;
        observe(&mut engine, &state, 1);

        let request = engine.issue_controlled_intent(
            Intent::ServeFloor {
                car,
                floor: Floor::new(2),
            },
            None,
        );
        assert_eq!(
            engine.submit_controlled(request, &grant, &mut backend),
            Admission::Blocked(WriteBlocker::Mode(OperatingMode::OutOfService))
        );
    }

    #[test]
    fn unsupported_expiry_keeps_the_uncertain_operation_and_blocks_supersession() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let car = CarId::new("A");
        let source = ObservationSourceId::new("physical");
        let mut backend = TestController(ControllerId::new("virtual"));
        engine
            .bind_controlled_car(car.clone(), backend.0.clone())
            .unwrap();
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
        let grant = engine
            .grant_authority(&car, PrincipalId::new("test"))
            .unwrap();
        observe(&mut engine, &CarState::new("A", Floor::new(1)), 1);
        let request = engine.issue_controlled_intent(
            Intent::ParkAt {
                car,
                floor: Floor::new(5),
            },
            None,
        );
        assert_eq!(
            engine.submit_controlled(request.clone(), &grant, &mut backend),
            Admission::Admitted
        );
        assert_eq!(
            engine.expire_controlled_intent(request.id, &mut backend),
            Ok(WithdrawalOutcome::Unsupported)
        );
        assert_eq!(
            engine.intent_record(request.id).unwrap().withdrawal,
            Some((WithdrawalReason::Expired, WithdrawalOutcome::Unsupported))
        );
        assert_eq!(
            engine.submit_controlled(request.clone(), &grant, &mut backend),
            Admission::Retry
        );
        let replacement = engine.issue_controlled_intent(request.intent.clone(), None);
        assert_eq!(
            engine.submit_controlled(replacement, &grant, &mut backend),
            Admission::Blocked(WriteBlocker::ActiveIntent(request.id))
        );
        observe(&mut engine, &CarState::new("A", Floor::new(5)), 2);
        assert_eq!(
            engine.intent_record(request.id).unwrap().reconciliation,
            Reconciliation::Satisfied
        );
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
        let request = engine.issue_controlled_intent(Intent::ServeFloor { car, floor }, Some(call));
        let id = request.id();
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
        let mut request = engine.issue_controlled_intent(
            Intent::ServeFloor {
                car: car.clone(),
                floor: Floor::new(5),
            },
            None,
        );
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

    #[test]
    fn durable_recovery_restores_identity_without_resubmission() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let car = CarId::new("A");
        let controller = ControllerId::new("rmf-gateway");
        let source = ObservationSourceId::new("physical");
        engine
            .bind_controlled_car(car.clone(), controller.clone())
            .unwrap();
        let grant = engine
            .grant_authority(&car, PrincipalId::new("local-gateway-caller"))
            .unwrap();
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
        observe(&mut engine, &CarState::new("A", Floor::new(1)), 1);

        let id = IntentId::new(41);
        engine
            .recover_controlled_intent(
                ControlledIntentRecovery {
                    id,
                    intent: Intent::ServeFloor {
                        car: car.clone(),
                        floor: Floor::new(3),
                    },
                    controller,
                    receipt: Receipt::Indeterminate,
                    started_at: CarLocation::Floor(Floor::new(1)),
                    saw_progress: false,
                    saw_closed_at_target: false,
                },
                &grant,
            )
            .unwrap();
        assert_eq!(
            engine.intent_record(id).unwrap().receipt,
            Receipt::Indeterminate
        );
        assert_eq!(
            engine
                .issue_controlled_intent(
                    Intent::ServeFloor {
                        car: car.clone(),
                        floor: Floor::new(4),
                    },
                    None,
                )
                .id(),
            IntentId::new(42)
        );

        let mut moving = CarState::new("A", Floor::new(2));
        moving.motion = MotionState::Moving(liftassistant_model::Direction::Up);
        observe(&mut engine, &moving, 2);
        assert!(engine.intent_record(id).unwrap().saw_progress());
        observe(&mut engine, &CarState::new("A", Floor::new(3)), 3);
        assert_eq!(
            engine.intent_record(id).unwrap().reconciliation,
            Reconciliation::Satisfied
        );
    }

    #[test]
    fn durable_recovery_requires_current_evidence_and_nonterminal_uncertainty() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let car = CarId::new("A");
        let controller = ControllerId::new("rmf-gateway");
        engine
            .bind_controlled_car(car.clone(), controller.clone())
            .unwrap();
        engine.set_execution_capabilities(car.clone(), ExecutionCapabilities::reference_defaults());
        let grant = engine
            .grant_authority(&car, PrincipalId::new("local-gateway-caller"))
            .unwrap();
        let recovery = ControlledIntentRecovery {
            id: IntentId::new(9),
            intent: Intent::ServeFloor {
                car,
                floor: Floor::new(3),
            },
            controller,
            receipt: Receipt::Rejected,
            started_at: CarLocation::Floor(Floor::new(1)),
            saw_progress: false,
            saw_closed_at_target: false,
        };
        assert_eq!(
            engine.recover_controlled_intent(recovery.clone(), &grant),
            Err(ControlledRecoveryError::TerminalReceipt)
        );
        let mut uncertain = recovery;
        uncertain.receipt = Receipt::Indeterminate;
        assert_eq!(
            engine.recover_controlled_intent(uncertain, &grant),
            Err(ControlledRecoveryError::ObservationUnavailable)
        );
    }

    #[test]
    fn preview_write_readiness_is_side_effect_free_and_preserves_intent_id_sequence() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let car = CarId::new("A");
        let controller = ControllerId::new("virtual");
        engine
            .bind_controlled_car(car.clone(), controller.clone())
            .unwrap();
        assert_eq!(engine.controlled_controller(&car), Some(&controller));
        assert!(engine.active_intent(&car).is_none());
        let grant = engine
            .grant_authority(&car, PrincipalId::new("dev-preview"))
            .unwrap();
        assert_eq!(grant.principal().as_str(), "dev-preview");
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
        engine.set_execution_capabilities(
            car.clone(),
            ExecutionCapabilities::new(true, false, (1..=6).map(Floor::new)),
        );
        observe(&mut engine, &CarState::new("A", Floor::new(1)), 1);

        let before = engine.clone();
        assert_eq!(
            engine.preview_write_readiness(
                &Intent::ServeFloor {
                    car: car.clone(),
                    floor: Floor::new(5),
                },
                None,
                &grant,
                &controller,
                true,
            ),
            Ok(())
        );
        assert_eq!(
            engine.preview_write_readiness(
                &Intent::ServeFloor {
                    car: car.clone(),
                    floor: Floor::new(7),
                },
                None,
                &grant,
                &controller,
                true,
            ),
            Err(WriteBlocker::UnsupportedFloor)
        );
        assert_eq!(
            engine.preview_write_readiness(
                &Intent::ServeFloor {
                    car: car.clone(),
                    floor: Floor::new(5),
                },
                None,
                &grant,
                &controller,
                false,
            ),
            Err(WriteBlocker::ControllerUnavailable)
        );
        assert_eq!(engine, before);

        let first_issued = engine.issue_controlled_intent(
            Intent::ServeFloor {
                car,
                floor: Floor::new(5),
            },
            None,
        );
        assert_eq!(first_issued.id(), IntentId::new(1));
    }
}
