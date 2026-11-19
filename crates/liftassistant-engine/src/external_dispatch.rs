use std::collections::BTreeMap;

use liftassistant_core::{CallRegistration, HallCallRejection, validate_hall_call};
use liftassistant_model::{CallId, CallState, CarId, HallCall};
use liftassistant_runtime::ControllerId;

use crate::{Engine, PrincipalId};

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
pub enum ExternalAssignmentRejection {
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
    },
    InactiveCall(CallId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalDispatchRecord {
    pub request: ExternalDispatchRequest,
    pub controller: ControllerId,
    pub authority: ExternalDispatchAuthorityGrant,
    pub delivery: ExternalDispatchDelivery,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ExternalDispatches {
    controllers: BTreeMap<ExternalDispatchScopeId, ControllerId>,
    grants: BTreeMap<ExternalDispatchScopeId, ExternalDispatchAuthorityGrant>,
    mappings: BTreeMap<(ExternalDispatchScopeId, NativeAssignmentId), CarId>,
    active: BTreeMap<CallId, ExternalDispatchRecord>,
    last_grant: u64,
}

impl ExternalDispatches {
    pub(crate) fn owns(&self, call: CallId) -> bool {
        self.active.contains_key(&call)
    }

    fn record(&self, call: CallId) -> Option<&ExternalDispatchRecord> {
        self.active.get(&call)
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
        self.calls.get(call)?;
        Some(match self.external_dispatch.record(call) {
            Some(record) => DispatchOwnership::ExternalController(record.request.scope.clone()),
            None => DispatchOwnership::LiftAssistant,
        })
    }

    pub fn external_dispatch_record(&self, call: CallId) -> Option<&ExternalDispatchRecord> {
        self.external_dispatch.record(call)
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
                let delivery = sink.submit(&request);
                self.external_dispatch.active.insert(
                    id,
                    ExternalDispatchRecord {
                        request: request.clone(),
                        controller: expected_controller,
                        authority: grant.clone(),
                        delivery,
                    },
                );
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

    pub fn receive_external_assignment(
        &mut self,
        evidence: ExternalAssignmentEvidence,
    ) -> ExternalAssignmentResult {
        let Some(record) = self.external_dispatch.record(evidence.call) else {
            return ExternalAssignmentResult::Rejected(
                if self.calls.get(evidence.call).is_some() {
                    ExternalAssignmentRejection::NotExternallyOwned(evidence.call)
                } else {
                    ExternalAssignmentRejection::UnknownCall(evidence.call)
                },
            );
        };
        if record.request.scope != evidence.scope {
            return ExternalAssignmentResult::Rejected(ExternalAssignmentRejection::WrongScope {
                expected: record.request.scope.clone(),
                received: evidence.scope,
            });
        }
        if record.controller != evidence.controller {
            return ExternalAssignmentResult::Rejected(
                ExternalAssignmentRejection::WrongController {
                    expected: record.controller.clone(),
                    received: evidence.controller,
                },
            );
        }

        let Some(call) = self.calls.get(evidence.call) else {
            return ExternalAssignmentResult::Rejected(ExternalAssignmentRejection::InactiveCall(
                evidence.call,
            ));
        };
        if matches!(call.state, CallState::Serving(_) | CallState::Completed) {
            return ExternalAssignmentResult::Rejected(ExternalAssignmentRejection::InactiveCall(
                evidence.call,
            ));
        }

        let Some(car) = self
            .external_dispatch
            .mappings
            .get(&(evidence.scope.clone(), evidence.native_assignment.clone()))
            .cloned()
        else {
            return ExternalAssignmentResult::Rejected(
                ExternalAssignmentRejection::UnmappedNativeAssignment {
                    scope: evidence.scope,
                    native: evidence.native_assignment,
                },
            );
        };

        match &call.state {
            CallState::Pending => {
                assert!(self.calls.assign(evidence.call, car.clone()));
                ExternalAssignmentResult::Assigned {
                    call: evidence.call,
                    car,
                }
            }
            CallState::Assigned(existing) if existing == &car => {
                ExternalAssignmentResult::Duplicate {
                    call: evidence.call,
                    car,
                }
            }
            CallState::Assigned(existing) => ExternalAssignmentResult::Rejected(
                ExternalAssignmentRejection::ConflictingAssignment {
                    call: evidence.call,
                    existing: existing.clone(),
                    attempted: car,
                },
            ),
            CallState::Serving(_) | CallState::Completed => unreachable!("checked above"),
        }
    }
}

#[cfg(test)]
mod tests {
    use liftassistant_model::{
        BuildingTopology, CarState, Direction, ExecutionCapabilities, Floor, Intent,
    };
    use liftassistant_runtime::{IntentSink, RuntimeError};

    use super::*;
    use crate::{AuthorityGrant, EngineEvent};

    struct RecordingExternalSink {
        controller: ControllerId,
        delivery: ExternalDispatchDelivery,
        requests: Vec<ExternalDispatchRequest>,
    }

    impl RecordingExternalSink {
        fn new(controller: &str) -> Self {
            Self {
                controller: ControllerId::new(controller),
                delivery: ExternalDispatchDelivery::Submitted,
                requests: Vec::new(),
            }
        }
    }

    impl ExternalDispatchSink for RecordingExternalSink {
        fn controller_id(&self) -> &ControllerId {
            &self.controller
        }

        fn submit(&mut self, request: &ExternalDispatchRequest) -> ExternalDispatchDelivery {
            self.requests.push(request.clone());
            self.delivery
        }
    }

    #[derive(Default)]
    struct RecordingIntentSink(Vec<Intent>);

    impl IntentSink for RecordingIntentSink {
        fn submit_intent(&mut self, intent: Intent) -> Result<(), RuntimeError> {
            self.0.push(intent);
            Ok(())
        }
    }

    fn engine() -> Engine {
        Engine::new(BuildingTopology::reference_eight_floor())
    }

    fn call() -> HallCall {
        HallCall {
            floor: Floor::new(3),
            direction: Direction::Up,
        }
    }

    fn configured_external(
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

    fn submitted_call(
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

    fn assignment(
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
