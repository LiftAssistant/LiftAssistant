use std::collections::{BTreeMap, VecDeque};

use liftassistant_core::{CallRegistration, HallCallRejection, validate_hall_call};
use liftassistant_model::{
    CallId, CallRecord, CallState, CarId, CarLocation, CarState, DoorState, HallCall, MotionState,
    ObservationProvenance,
};
use liftassistant_runtime::{AssembledCarObservation, ControllerId, WithdrawalOutcome};

use crate::{Engine, EngineEvent, PrincipalId};

const RECENT_EXTERNAL_TERMINALS: usize = 128;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExternalDispatchScopeId(String);

impl ExternalDispatchScopeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeAssignmentId(String);

impl NativeAssignmentId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExternalDispatchAuthorityGrantId(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalDispatchAuthorityGrant {
    id: ExternalDispatchAuthorityGrantId,
    scope: ExternalDispatchScopeId,
    principal: PrincipalId,
}

impl ExternalDispatchAuthorityGrant {
    pub const fn id(&self) -> ExternalDispatchAuthorityGrantId {
        self.id
    }

    pub fn scope(&self) -> &ExternalDispatchScopeId {
        &self.scope
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalDispatchConfigurationError {
    ScopeAlreadyBound {
        scope: ExternalDispatchScopeId,
        controller: ControllerId,
    },
    ScopeUnbound(ExternalDispatchScopeId),
    NativeAssignmentAlreadyMapped {
        scope: ExternalDispatchScopeId,
        native: NativeAssignmentId,
        existing: CarId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalDispatchAuthorityError {
    ScopeUnbound,
    AlreadyGranted,
    Fenced,
    IdentityExhausted,
}

/// Delivery at the external-dispatch boundary, not assignment or physical outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalDispatchDelivery {
    /// The sink can establish that the request did not reach the external boundary.
    NotSubmitted,
    Submitted,
    Indeterminate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalDispatchRequest {
    pub call: CallId,
    pub service: HallCall,
    pub scope: ExternalDispatchScopeId,
}

/// Separate from `ControlledIntentSink`: this request legitimately has no car yet.
pub trait ExternalDispatchSink {
    fn controller_id(&self) -> &ControllerId;

    fn submit(&mut self, request: &ExternalDispatchRequest) -> ExternalDispatchDelivery;

    fn withdraw(&mut self, _request: &ExternalDispatchRequest) -> WithdrawalOutcome {
        WithdrawalOutcome::Unsupported
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchOwnership {
    LiftAssistant,
    ExternalController(ExternalDispatchScopeId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalDispatchSubmission {
    Created {
        request: ExternalDispatchRequest,
        delivery: ExternalDispatchDelivery,
    },
    Duplicate {
        request: ExternalDispatchRequest,
        delivery: ExternalDispatchDelivery,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalDispatchSubmissionError {
    InvalidCall(HallCallRejection),
    ScopeUnbound,
    AuthorityFenced,
    WrongController,
    OwnershipConflict {
        call: CallId,
        owner: DispatchOwnership,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalAssignmentEvidence {
    pub call: CallId,
    pub scope: ExternalDispatchScopeId,
    pub controller: ControllerId,
    pub native_assignment: NativeAssignmentId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalAssignmentResult {
    Assigned { call: CallId, car: CarId },
    Duplicate { call: CallId, car: CarId },
    Rejected(ExternalAssignmentRejection),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalEvidenceRejection {
    UnknownCall(CallId),
    NotExternallyOwned(CallId),
    WrongScope {
        expected: ExternalDispatchScopeId,
        received: ExternalDispatchScopeId,
    },
    WrongController {
        expected: ControllerId,
        received: ControllerId,
    },
    UnmappedNativeAssignment {
        scope: ExternalDispatchScopeId,
        native: NativeAssignmentId,
    },
    ConflictingAssignment {
        call: CallId,
        existing: CarId,
        attempted: CarId,
        existing_native: NativeAssignmentId,
        attempted_native: NativeAssignmentId,
    },
    InactiveCall(CallId),
    NeverSubmitted(CallId),
    NotAssigned(CallId),
    WrongAssignment {
        expected: NativeAssignmentId,
        received: NativeAssignmentId,
    },
}

pub type ExternalAssignmentRejection = ExternalEvidenceRejection;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalDispatchDisposition {
    Accepted,
    Rejected,
    Indeterminate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalCallContext {
    pub call: CallId,
    pub scope: ExternalDispatchScopeId,
    pub controller: ControllerId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalReceiptResult {
    Recorded,
    Duplicate,
    Stale,
    Inconsistent {
        retained: ExternalDispatchDisposition,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalServingResult {
    Started,
    Duplicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalCommandRejection {
    InactiveCall,
    WrongScope,
    WrongController,
    AuthorityFenced,
    RetryUnsafe,
    NeverSubmitted,
    AlreadyWithdrawn,
    InconsistentPrevention,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalDispatchTerminal {
    Completed,
    Prevented,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalDispatchRecord {
    pub request: ExternalDispatchRequest,
    pub controller: ControllerId,
    pub authority: ExternalDispatchAuthorityGrant,
    pub delivery: ExternalDispatchDelivery,
    pub receipt: Option<ExternalDispatchDisposition>,
    pub withdrawal: Option<WithdrawalOutcome>,
    native_assignment: Option<NativeAssignmentId>,
    service_observation: ServiceObservation,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ServiceObservation {
    baseline: Option<ObservationProvenance>,
    saw_travel: bool,
    saw_closed_at_target: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ExternalDispatches {
    controllers: BTreeMap<ExternalDispatchScopeId, ControllerId>,
    grants: BTreeMap<ExternalDispatchScopeId, ExternalDispatchAuthorityGrant>,
    mappings: BTreeMap<(ExternalDispatchScopeId, NativeAssignmentId), CarId>,
    active: BTreeMap<CallId, ExternalDispatchRecord>,
    // Keep separate capacity so prevented calls cannot evict ownership while
    // the shared registry still retains a completed external CallId.
    completed: VecDeque<(CallId, ExternalDispatchScopeId)>,
    prevented: VecDeque<(CallId, ExternalDispatchScopeId)>,
    last_grant: u64,
}

impl ExternalDispatches {
    pub(crate) fn owns(&self, call: CallId) -> bool {
        self.active.contains_key(&call)
    }

    fn record(&self, call: CallId) -> Option<&ExternalDispatchRecord> {
        self.active.get(&call)
    }

    fn retired(
        &self,
        call: CallId,
    ) -> Option<(&ExternalDispatchScopeId, ExternalDispatchTerminal)> {
        if let Some((_, scope)) = self.completed.iter().find(|(id, _)| *id == call) {
            Some((scope, ExternalDispatchTerminal::Completed))
        } else {
            self.prevented
                .iter()
                .find(|(id, _)| *id == call)
                .map(|(_, scope)| (scope, ExternalDispatchTerminal::Prevented))
        }
    }

    fn retire(&mut self, call: CallId, terminal: ExternalDispatchTerminal) {
        let record = self
            .active
            .remove(&call)
            .expect("retiring active external work");
        let history = match terminal {
            ExternalDispatchTerminal::Completed => &mut self.completed,
            ExternalDispatchTerminal::Prevented => &mut self.prevented,
        };
        if history.len() == RECENT_EXTERNAL_TERMINALS {
            history.pop_front();
        }
        history.push_back((call, record.request.scope));
    }

    pub(crate) fn fence_grants(&mut self) {
        self.grants.clear();
    }

    fn check_grant(
        &self,
        scope: &ExternalDispatchScopeId,
        grant: &ExternalDispatchAuthorityGrant,
    ) -> Result<(), ExternalDispatchAuthorityError> {
        if grant.scope != *scope || self.grants.get(scope) != Some(grant) {
            return Err(ExternalDispatchAuthorityError::Fenced);
        }
        Ok(())
    }
}

impl Engine {
    pub fn bind_external_dispatch_scope(
        &mut self,
        scope: ExternalDispatchScopeId,
        controller: ControllerId,
    ) -> Result<(), ExternalDispatchConfigurationError> {
        if let Some(existing) = self.external_dispatch.controllers.get(&scope) {
            return Err(ExternalDispatchConfigurationError::ScopeAlreadyBound {
                scope,
                controller: existing.clone(),
            });
        }
        self.external_dispatch.controllers.insert(scope, controller);
        Ok(())
    }

    pub fn map_external_assignment(
        &mut self,
        scope: ExternalDispatchScopeId,
        native: NativeAssignmentId,
        car: CarId,
    ) -> Result<(), ExternalDispatchConfigurationError> {
        if !self.external_dispatch.controllers.contains_key(&scope) {
            return Err(ExternalDispatchConfigurationError::ScopeUnbound(scope));
        }
        let key = (scope.clone(), native.clone());
        if let Some(existing) = self.external_dispatch.mappings.get(&key) {
            return Err(
                ExternalDispatchConfigurationError::NativeAssignmentAlreadyMapped {
                    scope,
                    native,
                    existing: existing.clone(),
                },
            );
        }
        self.external_dispatch.mappings.insert(key, car);
        Ok(())
    }

    pub fn grant_external_dispatch_authority(
        &mut self,
        scope: &ExternalDispatchScopeId,
        principal: PrincipalId,
    ) -> Result<ExternalDispatchAuthorityGrant, ExternalDispatchAuthorityError> {
        if !self.external_dispatch.controllers.contains_key(scope) {
            return Err(ExternalDispatchAuthorityError::ScopeUnbound);
        }
        if self.external_dispatch.grants.contains_key(scope) {
            return Err(ExternalDispatchAuthorityError::AlreadyGranted);
        }
        let id = self
            .external_dispatch
            .last_grant
            .checked_add(1)
            .ok_or(ExternalDispatchAuthorityError::IdentityExhausted)?;
        self.external_dispatch.last_grant = id;
        let grant = ExternalDispatchAuthorityGrant {
            id: ExternalDispatchAuthorityGrantId(id),
            scope: scope.clone(),
            principal,
        };
        self.external_dispatch
            .grants
            .insert(scope.clone(), grant.clone());
        Ok(grant)
    }

    pub fn revoke_external_dispatch_authority(
        &mut self,
        scope: &ExternalDispatchScopeId,
        grant: &ExternalDispatchAuthorityGrant,
    ) -> Result<(), ExternalDispatchAuthorityError> {
        self.external_dispatch.check_grant(scope, grant)?;
        self.external_dispatch.grants.remove(scope);
        Ok(())
    }

    pub fn dispatch_ownership(&self, call: CallId) -> Option<DispatchOwnership> {
        if let Some(record) = self.external_dispatch.record(call) {
            return Some(DispatchOwnership::ExternalController(
                record.request.scope.clone(),
            ));
        }
        if let Some((scope, _)) = self.external_dispatch.retired(call) {
            return Some(DispatchOwnership::ExternalController(scope.clone()));
        }
        self.calls
            .get(call)
            .map(|_| DispatchOwnership::LiftAssistant)
    }

    pub fn external_dispatch_record(&self, call: CallId) -> Option<&ExternalDispatchRecord> {
        self.external_dispatch.record(call)
    }

    pub fn external_dispatch_terminal(&self, call: CallId) -> Option<ExternalDispatchTerminal> {
        self.external_dispatch
            .retired(call)
            .map(|(_, terminal)| terminal)
    }

    pub fn submit_external_call(
        &mut self,
        call: HallCall,
        scope: &ExternalDispatchScopeId,
        grant: &ExternalDispatchAuthorityGrant,
        sink: &mut impl ExternalDispatchSink,
    ) -> Result<ExternalDispatchSubmission, ExternalDispatchSubmissionError> {
        validate_hall_call(&call, &self.topology)
            .map_err(ExternalDispatchSubmissionError::InvalidCall)?;
        let expected_controller = self
            .external_dispatch
            .controllers
            .get(scope)
            .ok_or(ExternalDispatchSubmissionError::ScopeUnbound)?
            .clone();
        self.external_dispatch
            .check_grant(scope, grant)
            .map_err(|_| ExternalDispatchSubmissionError::AuthorityFenced)?;
        if sink.controller_id() != &expected_controller {
            return Err(ExternalDispatchSubmissionError::WrongController);
        }

        self.last_call = Some(call);
        self.decision = None;
        match self.calls.register(call) {
            CallRegistration::Created(id) => {
                let request = ExternalDispatchRequest {
                    call: id,
                    service: call,
                    scope: scope.clone(),
                };
                self.external_dispatch.active.insert(
                    id,
                    ExternalDispatchRecord {
                        request: request.clone(),
                        controller: expected_controller,
                        authority: grant.clone(),
                        delivery: ExternalDispatchDelivery::Indeterminate,
                        receipt: None,
                        withdrawal: None,
                        native_assignment: None,
                        service_observation: ServiceObservation::default(),
                    },
                );
                let delivery = self.send_external_request(id, grant, sink);
                Ok(ExternalDispatchSubmission::Created { request, delivery })
            }
            CallRegistration::Duplicate(id) => {
                let Some(record) = self.external_dispatch.record(id) else {
                    return Err(ExternalDispatchSubmissionError::OwnershipConflict {
                        call: id,
                        owner: DispatchOwnership::LiftAssistant,
                    });
                };
                if &record.request.scope != scope {
                    return Err(ExternalDispatchSubmissionError::OwnershipConflict {
                        call: id,
                        owner: DispatchOwnership::ExternalController(record.request.scope.clone()),
                    });
                }
                Ok(ExternalDispatchSubmission::Duplicate {
                    request: record.request.clone(),
                    delivery: record.delivery,
                })
            }
        }
    }

    fn send_external_request(
        &mut self,
        call: CallId,
        grant: &ExternalDispatchAuthorityGrant,
        sink: &mut impl ExternalDispatchSink,
    ) -> ExternalDispatchDelivery {
        let record = self
            .external_dispatch
            .active
            .get_mut(&call)
            .expect("reserved external request");
        // If the sink unwinds after possible transmission, ownership remains
        // external and a caller that catches the unwind cannot retry the send.
        record.delivery = ExternalDispatchDelivery::Indeterminate;
        record.authority = grant.clone();
        record.delivery = sink.submit(&record.request);
        record.delivery
    }

    pub fn receive_external_assignment(
        &mut self,
        evidence: ExternalAssignmentEvidence,
    ) -> ExternalAssignmentResult {
        let context = ExternalCallContext {
            call: evidence.call,
            scope: evidence.scope,
            controller: evidence.controller,
        };
        let (record, call) = match self.external_evidence(&context) {
            Ok(pair) => pair,
            Err(error) => return ExternalAssignmentResult::Rejected(error),
        };

        let Some(car) = self
            .external_dispatch
            .mappings
            .get(&(context.scope.clone(), evidence.native_assignment.clone()))
            .cloned()
        else {
            return ExternalAssignmentResult::Rejected(
                ExternalAssignmentRejection::UnmappedNativeAssignment {
                    scope: context.scope,
                    native: evidence.native_assignment,
                },
            );
        };

        match &call.state {
            CallState::Pending => {
                assert!(self.calls.assign(evidence.call, car.clone()));
                self.external_dispatch
                    .active
                    .get_mut(&evidence.call)
                    .expect("active assignment")
                    .native_assignment = Some(evidence.native_assignment);
                ExternalAssignmentResult::Assigned {
                    call: evidence.call,
                    car,
                }
            }
            CallState::Assigned(_) | CallState::Serving(_)
                if record.native_assignment.as_ref() == Some(&evidence.native_assignment) =>
            {
                ExternalAssignmentResult::Duplicate {
                    call: evidence.call,
                    car,
                }
            }
            CallState::Assigned(existing) | CallState::Serving(existing) => {
                ExternalAssignmentResult::Rejected(
                    ExternalAssignmentRejection::ConflictingAssignment {
                        call: evidence.call,
                        existing: existing.clone(),
                        attempted: car,
                        existing_native: record
                            .native_assignment
                            .clone()
                            .expect("mapped assignment"),
                        attempted_native: evidence.native_assignment,
                    },
                )
            }
            CallState::Completed => unreachable!("evidence requires an active call"),
        }
    }

    fn external_evidence(
        &self,
        context: &ExternalCallContext,
    ) -> Result<(&ExternalDispatchRecord, &CallRecord), ExternalEvidenceRejection> {
        let id = context.call;
        let call = self.calls.get(id);
        let Some(record) = self.external_dispatch.record(id) else {
            return Err(if self.external_dispatch.retired(id).is_some() {
                ExternalEvidenceRejection::InactiveCall(id)
            } else if call.is_some() {
                ExternalEvidenceRejection::NotExternallyOwned(id)
            } else {
                ExternalEvidenceRejection::UnknownCall(id)
            });
        };
        let call = call
            .filter(|call| call.state.is_active())
            .ok_or(ExternalEvidenceRejection::InactiveCall(id))?;
        if record.request.scope != context.scope {
            return Err(ExternalEvidenceRejection::WrongScope {
                expected: record.request.scope.clone(),
                received: context.scope.clone(),
            });
        }
        if record.controller != context.controller {
            return Err(ExternalEvidenceRejection::WrongController {
                expected: record.controller.clone(),
                received: context.controller.clone(),
            });
        }
        if record.delivery == ExternalDispatchDelivery::NotSubmitted {
            return Err(ExternalEvidenceRejection::NeverSubmitted(id));
        }
        Ok((record, call))
    }

    fn external_command(
        &self,
        call: CallId,
        scope: &ExternalDispatchScopeId,
        grant: &ExternalDispatchAuthorityGrant,
        controller: &ControllerId,
    ) -> Result<(&ExternalDispatchRecord, &CallRecord), ExternalCommandRejection> {
        let record = self
            .external_dispatch
            .record(call)
            .ok_or(ExternalCommandRejection::InactiveCall)?;
        let state = self
            .calls
            .get(call)
            .filter(|call| call.state.is_active())
            .ok_or(ExternalCommandRejection::InactiveCall)?;
        if &record.request.scope != scope {
            return Err(ExternalCommandRejection::WrongScope);
        }
        self.external_dispatch
            .check_grant(scope, grant)
            .map_err(|_| ExternalCommandRejection::AuthorityFenced)?;
        if &record.controller != controller
            || self.external_dispatch.controllers.get(scope) != Some(controller)
        {
            return Err(ExternalCommandRejection::WrongController);
        }
        Ok((record, state))
    }

    pub fn retry_external_call(
        &mut self,
        call: CallId,
        scope: &ExternalDispatchScopeId,
        grant: &ExternalDispatchAuthorityGrant,
        sink: &mut impl ExternalDispatchSink,
    ) -> Result<ExternalDispatchDelivery, ExternalCommandRejection> {
        let (record, state) = self.external_command(call, scope, grant, sink.controller_id())?;
        if record.delivery != ExternalDispatchDelivery::NotSubmitted
            || state.state != CallState::Pending
        {
            return Err(ExternalCommandRejection::RetryUnsafe);
        }
        Ok(self.send_external_request(call, grant, sink))
    }

    pub fn receive_external_receipt(
        &mut self,
        context: &ExternalCallContext,
        disposition: ExternalDispatchDisposition,
    ) -> Result<ExternalReceiptResult, ExternalEvidenceRejection> {
        let (record, _) = self.external_evidence(context)?;
        use ExternalDispatchDisposition::Indeterminate;
        match record.receipt {
            Some(prior) if prior == disposition => Ok(ExternalReceiptResult::Duplicate),
            None | Some(Indeterminate) => {
                self.external_dispatch
                    .active
                    .get_mut(&context.call)
                    .expect("active receipt")
                    .receipt = Some(disposition);
                Ok(ExternalReceiptResult::Recorded)
            }
            Some(_) if disposition == Indeterminate => Ok(ExternalReceiptResult::Stale),
            Some(retained) => Ok(ExternalReceiptResult::Inconsistent { retained }),
        }
    }

    pub fn receive_external_serving(
        &mut self,
        context: &ExternalCallContext,
        native: &NativeAssignmentId,
    ) -> Result<ExternalServingResult, ExternalEvidenceRejection> {
        let (record, call) = self.external_evidence(context)?;
        let expected = record
            .native_assignment
            .as_ref()
            .ok_or(ExternalEvidenceRejection::NotAssigned(context.call))?;
        if expected != native {
            return Err(ExternalEvidenceRejection::WrongAssignment {
                expected: expected.clone(),
                received: native.clone(),
            });
        }
        let car = match &call.state {
            CallState::Assigned(car) => car.clone(),
            CallState::Serving(_) => return Ok(ExternalServingResult::Duplicate),
            _ => return Err(ExternalEvidenceRejection::NotAssigned(context.call)),
        };
        let baseline = self
            .operational_observation_source(&car)
            .and_then(|source| self.evidence(source, &car))
            .map(|evidence| evidence.latest_provenance.clone());
        assert!(self.calls.mark_serving(context.call, &car));
        self.external_dispatch
            .active
            .get_mut(&context.call)
            .expect("active service")
            .service_observation
            .baseline = baseline;
        self.diagnostics
            .record_engine_events(&[EngineEvent::CallStateChanged {
                id: context.call,
                state: CallState::Serving(car),
            }]);
        Ok(ExternalServingResult::Started)
    }

    pub fn withdraw_external_call(
        &mut self,
        call: CallId,
        scope: &ExternalDispatchScopeId,
        grant: &ExternalDispatchAuthorityGrant,
        sink: &mut impl ExternalDispatchSink,
    ) -> Result<WithdrawalOutcome, ExternalCommandRejection> {
        let (record, state) = self.external_command(call, scope, grant, sink.controller_id())?;
        if record.delivery == ExternalDispatchDelivery::NotSubmitted {
            return Err(ExternalCommandRejection::NeverSubmitted);
        }
        if record.withdrawal.is_some() {
            return Err(ExternalCommandRejection::AlreadyWithdrawn);
        }
        let outcome = sink.withdraw(&record.request);
        if outcome == WithdrawalOutcome::Prevented {
            if matches!(state.state, CallState::Serving(_)) {
                return Err(ExternalCommandRejection::InconsistentPrevention);
            }
            assert!(self.calls.remove_unstarted(call));
            self.external_dispatch
                .retire(call, ExternalDispatchTerminal::Prevented);
        } else {
            self.external_dispatch
                .active
                .get_mut(&call)
                .expect("active withdrawal")
                .withdrawal = Some(outcome);
        }
        Ok(outcome)
    }

    pub(crate) fn reset_external_service_observation(
        &mut self,
        car: &CarId,
        baseline: Option<&ObservationProvenance>,
    ) {
        if self.external_dispatch.active.is_empty() {
            return;
        }
        for call in self.calls.active_calls() {
            if matches!(&call.state, CallState::Serving(assigned) if assigned == car) {
                if let Some(record) = self.external_dispatch.active.get_mut(&call.id) {
                    record.service_observation = ServiceObservation {
                        baseline: baseline.cloned(),
                        ..ServiceObservation::default()
                    };
                }
            }
        }
    }

    pub(crate) fn observe_external_service(
        &mut self,
        state: &CarState,
        evidence: &AssembledCarObservation,
    ) {
        if self.external_dispatch.active.is_empty() {
            return;
        }
        let floor = state.location.floor_reference();
        let at_floor = match state.location {
            CarLocation::Floor(_) => true,
            CarLocation::Continuous(position) => {
                position.floor_position == f64::from(floor.index())
            }
        };
        let stopped = state.motion == MotionState::Stopped;
        let mut candidates = 0;
        let mut ready = None;
        for call in self.calls.active_calls() {
            if !matches!(&call.state, CallState::Serving(car) if car == &state.id) {
                continue;
            }
            let at_target = at_floor && stopped && floor == call.call.floor;
            candidates += usize::from(at_target);
            let Some(record) = self.external_dispatch.active.get_mut(&call.id) else {
                continue;
            };
            let progress = &mut record.service_observation;
            let newer = |provenance: &ObservationProvenance| {
                progress.baseline.as_ref().is_none_or(|baseline| {
                    provenance.source == baseline.source
                        && (provenance.session != baseline.session
                            || provenance.sequence > baseline.sequence)
                })
            };
            let (Some(position), Some(motion)) = (&evidence.position, &evidence.motion) else {
                continue;
            };
            if !newer(&position.provenance) || !newer(&motion.provenance) {
                continue;
            }
            let fresh_door = evidence.door.as_ref().is_some_and(|sample| {
                newer(&sample.provenance)
                    && sample.provenance.sequence >= position.provenance.sequence
                    && sample.provenance.sequence >= motion.provenance.sequence
            });
            progress.saw_travel |= !stopped || !at_floor || floor != call.call.floor;
            if at_target && fresh_door {
                progress.saw_closed_at_target |= state.door == DoorState::Closed;
                if state.door == DoorState::Open
                    && (progress.saw_travel || progress.saw_closed_at_target)
                {
                    ready = Some(call.id);
                }
            }
        }
        // A car/floor observation cannot distinguish two serving calls at the same landing.
        if let Some(id) = ready.filter(|_| candidates == 1) {
            assert!(self.calls.note_call_arrival(id, &state.id, floor));
            assert!(self.calls.complete_arrived_call(id, &state.id));
            self.external_dispatch
                .retire(id, ExternalDispatchTerminal::Completed);
            self.diagnostics
                .record_engine_events(&[EngineEvent::CallStateChanged {
                    id,
                    state: CallState::Completed,
                }]);
        }
    }
}

#[cfg(test)]
#[path = "external_dispatch/lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    use liftassistant_model::{
        BuildingTopology, CarState, Direction, ExecutionCapabilities, Floor, Intent,
    };
    use liftassistant_runtime::{IntentSink, RuntimeError};

    use super::*;
    use crate::{AuthorityGrant, EngineEvent};

    pub(super) struct RecordingExternalSink {
        pub(super) controller: ControllerId,
        pub(super) delivery: ExternalDispatchDelivery,
        pub(super) requests: Vec<ExternalDispatchRequest>,
        pub(super) withdrawal: WithdrawalOutcome,
        pub(super) withdrawals: Vec<CallId>,
        pub(super) interrupt_submission: bool,
    }

    impl RecordingExternalSink {
        fn new(controller: &str) -> Self {
            Self {
                controller: ControllerId::new(controller),
                delivery: ExternalDispatchDelivery::Submitted,
                requests: Vec::new(),
                withdrawal: WithdrawalOutcome::Unsupported,
                withdrawals: Vec::new(),
                interrupt_submission: false,
            }
        }
    }

    impl ExternalDispatchSink for RecordingExternalSink {
        fn controller_id(&self) -> &ControllerId {
            &self.controller
        }

        fn submit(&mut self, request: &ExternalDispatchRequest) -> ExternalDispatchDelivery {
            self.requests.push(request.clone());
            assert!(!self.interrupt_submission, "interrupted external send");
            self.delivery
        }

        fn withdraw(&mut self, request: &ExternalDispatchRequest) -> WithdrawalOutcome {
            self.withdrawals.push(request.call);
            self.withdrawal
        }
    }

    #[derive(Default)]
    pub(super) struct RecordingIntentSink(pub(super) Vec<Intent>);

    impl IntentSink for RecordingIntentSink {
        fn submit_intent(&mut self, intent: Intent) -> Result<(), RuntimeError> {
            self.0.push(intent);
            Ok(())
        }
    }

    pub(super) fn engine() -> Engine {
        Engine::new(BuildingTopology::reference_eight_floor())
    }

    pub(super) fn call() -> HallCall {
        HallCall {
            floor: Floor::new(3),
            direction: Direction::Up,
        }
    }

    pub(super) fn configured_external(
        engine: &mut Engine,
    ) -> (
        ExternalDispatchScopeId,
        ExternalDispatchAuthorityGrant,
        RecordingExternalSink,
    ) {
        let scope = ExternalDispatchScopeId::new("group-X");
        engine
            .bind_external_dispatch_scope(scope.clone(), ControllerId::new("controller-X"))
            .unwrap();
        engine
            .map_external_assignment(
                scope.clone(),
                NativeAssignmentId::new("native-deck-A"),
                CarId::new("A"),
            )
            .unwrap();
        engine
            .map_external_assignment(
                scope.clone(),
                NativeAssignmentId::new("native-deck-B"),
                CarId::new("B"),
            )
            .unwrap();
        let grant = engine
            .grant_external_dispatch_authority(&scope, PrincipalId::new("operator"))
            .unwrap();
        (scope, grant, RecordingExternalSink::new("controller-X"))
    }

    pub(super) fn submitted_call(
        engine: &mut Engine,
    ) -> (
        CallId,
        ExternalDispatchScopeId,
        ExternalDispatchAuthorityGrant,
        RecordingExternalSink,
    ) {
        let (scope, grant, mut sink) = configured_external(engine);
        let submission = engine
            .submit_external_call(call(), &scope, &grant, &mut sink)
            .unwrap();
        let ExternalDispatchSubmission::Created { request, .. } = submission else {
            panic!("first submission must create a call");
        };
        (request.call, scope, grant, sink)
    }

    pub(super) fn assignment(
        call: CallId,
        scope: &ExternalDispatchScopeId,
        native: &str,
    ) -> ExternalAssignmentEvidence {
        ExternalAssignmentEvidence {
            call,
            scope: scope.clone(),
            controller: ControllerId::new("controller-X"),
            native_assignment: NativeAssignmentId::new(native),
        }
    }

    #[test]
    fn liftassistant_owned_call_keeps_car_specific_intent_semantics() {
        let mut engine = engine();
        let car = CarId::new("A");
        engine
            .operational_states
            .insert(car.clone(), CarState::new("A", Floor::new(1)));
        engine.set_execution_capabilities(car.clone(), ExecutionCapabilities::reference_defaults());
        let mut sink = RecordingIntentSink::default();

        let events = engine.submit_call(call(), &mut sink);

        assert_eq!(
            sink.0,
            vec![Intent::ServeFloor {
                car: car.clone(),
                floor: Floor::new(3),
            }]
        );
        let id = events
            .iter()
            .find_map(|event| match event {
                EngineEvent::CallRegistered(id) => Some(*id),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            engine.dispatch_ownership(id),
            Some(DispatchOwnership::LiftAssistant)
        );
        assert_eq!(
            engine.calls().get(id).unwrap().state,
            CallState::Assigned(car)
        );
    }

    #[test]
    fn external_submission_has_no_intent_and_remains_pending_until_mapped_assignment() {
        let mut engine = engine();
        let (id, scope, _grant, sink) = submitted_call(&mut engine);

        assert_eq!(sink.requests.len(), 1);
        assert_eq!(sink.requests[0].call, id);
        assert_eq!(engine.decision(), None);
        assert_eq!(engine.calls().get(id).unwrap().state, CallState::Pending);
        assert_eq!(
            engine.dispatch_ownership(id),
            Some(DispatchOwnership::ExternalController(scope.clone()))
        );

        assert_eq!(
            engine.receive_external_assignment(assignment(id, &scope, "native-deck-B")),
            ExternalAssignmentResult::Assigned {
                call: id,
                car: CarId::new("B")
            }
        );
        assert_eq!(
            engine.calls().get(id).unwrap().state,
            CallState::Assigned(CarId::new("B"))
        );
    }

    #[test]
    fn unmapped_native_assignment_is_explicit_and_non_destructive() {
        let mut engine = engine();
        let (id, scope, _, _) = submitted_call(&mut engine);

        assert_eq!(
            engine.receive_external_assignment(assignment(id, &scope, "B")),
            ExternalAssignmentResult::Rejected(
                ExternalAssignmentRejection::UnmappedNativeAssignment {
                    scope,
                    native: NativeAssignmentId::new("B"),
                }
            )
        );
        assert_eq!(engine.calls().get(id).unwrap().state, CallState::Pending);
    }

    #[test]
    fn duplicate_assignment_is_idempotent_and_conflict_does_not_overwrite() {
        let mut engine = engine();
        let (id, scope, _, _) = submitted_call(&mut engine);
        let first = assignment(id, &scope, "native-deck-B");

        assert!(matches!(
            engine.receive_external_assignment(first.clone()),
            ExternalAssignmentResult::Assigned { .. }
        ));
        assert_eq!(
            engine.receive_external_assignment(first),
            ExternalAssignmentResult::Duplicate {
                call: id,
                car: CarId::new("B")
            }
        );
        assert_eq!(
            engine.receive_external_assignment(assignment(id, &scope, "native-deck-A")),
            ExternalAssignmentResult::Rejected(
                ExternalAssignmentRejection::ConflictingAssignment {
                    call: id,
                    existing: CarId::new("B"),
                    attempted: CarId::new("A"),
                    existing_native: NativeAssignmentId::new("native-deck-B"),
                    attempted_native: NativeAssignmentId::new("native-deck-A"),
                }
            )
        );
        assert_eq!(
            engine.calls().get(id).unwrap().state,
            CallState::Assigned(CarId::new("B"))
        );
    }

    #[test]
    fn late_assignment_cannot_mutate_completed_call() {
        let mut engine = engine();
        let (id, scope, _, _) = submitted_call(&mut engine);
        assert!(matches!(
            engine.receive_external_assignment(assignment(id, &scope, "native-deck-B")),
            ExternalAssignmentResult::Assigned { .. }
        ));
        assert!(engine.calls.mark_serving(id, &CarId::new("B")));
        assert!(engine.calls.complete(id, &CarId::new("B")));

        assert_eq!(
            engine.receive_external_assignment(assignment(id, &scope, "native-deck-B")),
            ExternalAssignmentResult::Rejected(ExternalAssignmentRejection::InactiveCall(id))
        );
        assert_eq!(engine.calls().get(id).unwrap().state, CallState::Completed);

        // Evict the original terminal record from the bounded Core history. The
        // external operation record still must not make the retired CallId live.
        for _ in 0..128 {
            let CallRegistration::Created(other) = engine.calls.register(call()) else {
                panic!("completed calls must not deduplicate new service");
            };
            assert!(engine.calls.assign(other, CarId::new("A")));
            assert!(engine.calls.mark_serving(other, &CarId::new("A")));
            assert!(engine.calls.complete(other, &CarId::new("A")));
        }
        assert_eq!(engine.calls().get(id), None);
        assert_eq!(
            engine.receive_external_assignment(assignment(id, &scope, "native-deck-B")),
            ExternalAssignmentResult::Rejected(ExternalAssignmentRejection::InactiveCall(id))
        );
    }

    #[test]
    fn wrong_controller_cannot_assign_or_submit() {
        let mut engine = engine();
        let (scope, grant, mut wrong_sink) = configured_external(&mut engine);
        wrong_sink.controller = ControllerId::new("controller-Y");
        assert_eq!(
            engine.submit_external_call(call(), &scope, &grant, &mut wrong_sink),
            Err(ExternalDispatchSubmissionError::WrongController)
        );

        let mut correct_sink = RecordingExternalSink::new("controller-X");
        let ExternalDispatchSubmission::Created { request, .. } = engine
            .submit_external_call(call(), &scope, &grant, &mut correct_sink)
            .unwrap()
        else {
            panic!("expected created call");
        };
        let mut wrong_assignment = assignment(request.call, &scope, "native-deck-B");
        wrong_assignment.controller = ControllerId::new("controller-Y");
        assert_eq!(
            engine.receive_external_assignment(wrong_assignment),
            ExternalAssignmentResult::Rejected(ExternalAssignmentRejection::WrongController {
                expected: ControllerId::new("controller-X"),
                received: ControllerId::new("controller-Y"),
            })
        );
        assert_eq!(
            engine.calls().get(request.call).unwrap().state,
            CallState::Pending
        );
    }

    #[test]
    fn revoked_dispatch_authority_is_fenced() {
        let mut engine = engine();
        let (scope, old_grant, mut sink) = configured_external(&mut engine);
        engine
            .revoke_external_dispatch_authority(&scope, &old_grant)
            .unwrap();
        let replacement = engine
            .grant_external_dispatch_authority(&scope, PrincipalId::new("replacement"))
            .unwrap();

        assert_eq!(
            engine.submit_external_call(call(), &scope, &old_grant, &mut sink),
            Err(ExternalDispatchSubmissionError::AuthorityFenced)
        );
        assert!(matches!(
            engine.submit_external_call(call(), &scope, &replacement, &mut sink),
            Ok(ExternalDispatchSubmission::Created { .. })
        ));
    }

    #[test]
    fn per_car_grant_and_vendor_identity_do_not_authorize_dispatch_scope() {
        let mut engine = engine();
        let scope = ExternalDispatchScopeId::new("group-X");
        let controller = ControllerId::new("controller-X");
        engine
            .bind_external_dispatch_scope(scope.clone(), controller.clone())
            .unwrap();

        let car = CarId::new("A");
        engine
            .bind_controlled_car(car.clone(), ControllerId::new("car-controller"))
            .unwrap();
        let per_car: AuthorityGrant = engine
            .grant_authority(&car, PrincipalId::new("per-car-principal"))
            .unwrap();
        assert_eq!(per_car.principal(), &PrincipalId::new("per-car-principal"));

        let vendor_credential_is_not_authority = ExternalDispatchAuthorityGrant {
            id: ExternalDispatchAuthorityGrantId(7),
            scope: scope.clone(),
            principal: PrincipalId::new("oauth-client-id"),
        };
        let mut sink = RecordingExternalSink::new("controller-X");
        assert_eq!(
            engine.submit_external_call(
                call(),
                &scope,
                &vendor_credential_is_not_authority,
                &mut sink,
            ),
            Err(ExternalDispatchSubmissionError::AuthorityFenced)
        );
        assert!(sink.requests.is_empty());
        assert_eq!(engine.calls().active_calls().count(), 0);
    }

    #[test]
    fn external_call_is_not_dispatched_or_advanced_by_car_specific_runtime_feedback() {
        let mut engine = engine();
        let (id, scope, _, _) = submitted_call(&mut engine);
        assert!(matches!(
            engine.receive_external_assignment(assignment(id, &scope, "native-deck-B")),
            ExternalAssignmentResult::Assigned { .. }
        ));

        let mut intent_sink = RecordingIntentSink::default();
        engine.consume_runtime_events(
            vec![liftassistant_runtime::RuntimeEvent::IntentAccepted(
                Intent::ServeFloor {
                    car: CarId::new("B"),
                    floor: Floor::new(3),
                },
            )],
            &mut intent_sink,
        );

        assert_eq!(
            engine.calls().get(id).unwrap().state,
            CallState::Assigned(CarId::new("B"))
        );
        assert!(intent_sink.0.is_empty());
    }

    #[test]
    fn duplicate_external_submission_does_not_resend() {
        let mut engine = engine();
        let (scope, grant, mut sink) = configured_external(&mut engine);
        let first = engine
            .submit_external_call(call(), &scope, &grant, &mut sink)
            .unwrap();
        let second = engine
            .submit_external_call(call(), &scope, &grant, &mut sink)
            .unwrap();

        assert!(matches!(first, ExternalDispatchSubmission::Created { .. }));
        assert!(matches!(
            second,
            ExternalDispatchSubmission::Duplicate { .. }
        ));
        assert_eq!(sink.requests.len(), 1);
    }
}
