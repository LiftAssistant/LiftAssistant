use liftassistant_model::{
    CarObservationUpdate, Direction, Floor, Intent, ObservationSequence, ObservationSessionId,
    ObservationSourceId,
};
use liftassistant_runtime::RuntimeEvent;

use super::tests::{
    RecordingIntentSink, assignment, call, configured_external, engine, submitted_call,
};
use super::*;
use crate::{ObservationBindingRole, ObservationIngestResult};

fn context(call: CallId, scope: &ExternalDispatchScopeId) -> ExternalCallContext {
    ExternalCallContext {
        call,
        scope: scope.clone(),
        controller: ControllerId::new("controller-X"),
    }
}

fn submit(
    engine: &mut Engine,
    service: HallCall,
    scope: &ExternalDispatchScopeId,
    grant: &ExternalDispatchAuthorityGrant,
    sink: &mut impl ExternalDispatchSink,
) -> CallId {
    let ExternalDispatchSubmission::Created { request, .. } = engine
        .submit_external_call(service, scope, grant, sink)
        .unwrap()
    else {
        panic!("expected new call");
    };
    request.call
}

fn start_service(engine: &mut Engine, context: &ExternalCallContext) {
    assert_eq!(
        engine.receive_external_assignment(assignment(
            context.call,
            &context.scope,
            "native-deck-B"
        )),
        ExternalAssignmentResult::Assigned {
            call: context.call,
            car: CarId::new("B")
        }
    );
    assert_eq!(
        engine.receive_external_serving(context, &NativeAssignmentId::new("native-deck-B")),
        Ok(ExternalServingResult::Started)
    );
}

fn bind_observer(engine: &mut Engine, source: &str, role: ObservationBindingRole) {
    let source = ObservationSourceId::new(source);
    engine
        .bind_observation(source.clone(), CarId::new("B"), role)
        .unwrap();
    engine
        .begin_source_session(source, ObservationSessionId::new("incarnation"))
        .unwrap();
}

fn sample(source: &str, sequence: u64, floor: i16, door: DoorState) -> CarObservationUpdate {
    let mut state = CarState::new("B", Floor::new(floor));
    state.door = door;
    CarObservationUpdate::from_state(
        ObservationProvenance {
            source: ObservationSourceId::new(source),
            session: ObservationSessionId::new("incarnation"),
            sequence: ObservationSequence::new(sequence),
        },
        &state,
    )
}

#[test]
fn only_explicit_authorized_retry_can_send_a_proven_unsent_request() {
    let mut engine = engine();
    let (scope, grant, mut sink) = configured_external(&mut engine);
    sink.delivery = ExternalDispatchDelivery::NotSubmitted;
    let id = submit(&mut engine, call(), &scope, &grant, &mut sink);
    assert!(matches!(
        engine.submit_external_call(call(), &scope, &grant, &mut sink),
        Ok(ExternalDispatchSubmission::Duplicate {
            delivery: ExternalDispatchDelivery::NotSubmitted,
            ..
        })
    ));
    assert_eq!(sink.requests.len(), 1);
    assert_eq!(
        engine
            .receive_external_receipt(&context(id, &scope), ExternalDispatchDisposition::Accepted),
        Err(ExternalEvidenceRejection::NeverSubmitted(id))
    );
    assert_eq!(
        engine.retry_external_call(
            id,
            &ExternalDispatchScopeId::new("another group"),
            &grant,
            &mut sink
        ),
        Err(ExternalCommandRejection::WrongScope)
    );
    sink.controller = ControllerId::new("other");
    assert_eq!(
        engine.retry_external_call(id, &scope, &grant, &mut sink),
        Err(ExternalCommandRejection::WrongController)
    );
    sink.controller = ControllerId::new("controller-X");
    engine
        .revoke_external_dispatch_authority(&scope, &grant)
        .unwrap();
    assert_eq!(
        engine.retry_external_call(id, &scope, &grant, &mut sink),
        Err(ExternalCommandRejection::AuthorityFenced)
    );
    assert_eq!(sink.requests.len(), 1);

    let fresh = engine
        .grant_external_dispatch_authority(&scope, PrincipalId::new("operator"))
        .unwrap();
    sink.delivery = ExternalDispatchDelivery::Submitted;
    assert_eq!(
        engine.retry_external_call(id, &scope, &fresh, &mut sink),
        Ok(ExternalDispatchDelivery::Submitted)
    );
    assert_eq!(
        sink.requests,
        [sink.requests[0].clone(), sink.requests[0].clone()]
    );
    assert_eq!(engine.calls().get(id).unwrap().state, CallState::Pending);
    assert_eq!(
        engine.external_dispatch_record(id).unwrap().authority,
        fresh
    );
    assert_eq!(
        engine.retry_external_call(id, &scope, &fresh, &mut sink),
        Err(ExternalCommandRejection::RetryUnsafe)
    );
}

#[test]
fn interrupted_send_reserves_external_ownership_and_fences_retry() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    for retry in [false, true] {
        let mut engine = engine();
        let (scope, grant, mut sink) = configured_external(&mut engine);
        sink.delivery = ExternalDispatchDelivery::NotSubmitted;
        let existing = retry.then(|| submit(&mut engine, call(), &scope, &grant, &mut sink));
        sink.interrupt_submission = true;
        let send = catch_unwind(AssertUnwindSafe(|| match existing {
            Some(id) => {
                let _ = engine.retry_external_call(id, &scope, &grant, &mut sink);
            }
            None => {
                let _ = engine.submit_external_call(call(), &scope, &grant, &mut sink);
            }
        }));
        assert!(send.is_err());
        let id = sink.requests.last().unwrap().call;
        assert_eq!(
            engine.dispatch_ownership(id),
            Some(DispatchOwnership::ExternalController(scope.clone()))
        );
        assert_eq!(
            engine.external_dispatch_record(id).unwrap().delivery,
            ExternalDispatchDelivery::Indeterminate
        );
        assert_eq!(
            engine.retry_external_call(id, &scope, &grant, &mut sink),
            Err(ExternalCommandRejection::RetryUnsafe)
        );
        assert_eq!(sink.requests.len(), if retry { 2 } else { 1 });
    }
}

#[test]
fn receipt_records_controller_disposition_independently_of_assignment() {
    use ExternalDispatchDisposition::{Accepted, Indeterminate, Rejected};
    let mut engine = engine();
    let (id, scope, grant, mut sink) = submitted_call(&mut engine);
    let ctx = context(id, &scope);
    assert_eq!(
        engine.receive_external_receipt(&ctx, Indeterminate),
        Ok(ExternalReceiptResult::Recorded)
    );
    assert_eq!(
        engine.receive_external_receipt(&ctx, Indeterminate),
        Ok(ExternalReceiptResult::Duplicate)
    );
    assert_eq!(
        engine.receive_external_receipt(&ctx, Accepted),
        Ok(ExternalReceiptResult::Recorded)
    );
    assert_eq!(engine.calls().get(id).unwrap().state, CallState::Pending);
    assert_eq!(
        engine.receive_external_receipt(&ctx, Accepted),
        Ok(ExternalReceiptResult::Duplicate)
    );
    assert_eq!(
        engine.receive_external_receipt(&ctx, Rejected),
        Ok(ExternalReceiptResult::Inconsistent { retained: Accepted })
    );
    assert_eq!(
        engine.receive_external_receipt(&ctx, Indeterminate),
        Ok(ExternalReceiptResult::Stale)
    );
    assert_eq!(
        engine.external_dispatch_record(id).unwrap().receipt,
        Some(Accepted)
    );
    assert_eq!(
        engine.external_dispatch_record(id).unwrap().delivery,
        ExternalDispatchDelivery::Submitted
    );
    start_service(&mut engine, &ctx);

    let other = submit(
        &mut engine,
        HallCall {
            floor: Floor::new(4),
            ..call()
        },
        &scope,
        &grant,
        &mut sink,
    );
    let other_ctx = context(other, &scope);
    assert!(matches!(
        engine.receive_external_assignment(assignment(other, &scope, "native-deck-A")),
        ExternalAssignmentResult::Assigned { .. }
    ));
    assert_eq!(
        engine.receive_external_receipt(&other_ctx, Rejected),
        Ok(ExternalReceiptResult::Recorded)
    );
    assert_eq!(
        engine.calls().get(other).unwrap().state,
        CallState::Assigned(CarId::new("A"))
    );
    assert_eq!(
        engine.external_dispatch_record(other).unwrap().receipt,
        Some(Rejected)
    );
}

#[test]
fn withdrawal_and_revocation_leave_correlated_remote_progress_eligible() {
    let mut engine = engine();
    let (id, scope, grant, mut sink) = submitted_call(&mut engine);
    let ctx = context(id, &scope);
    let native = NativeAssignmentId::new("native-deck-B");
    assert_eq!(
        engine.receive_external_serving(&ctx, &native),
        Err(ExternalEvidenceRejection::NotAssigned(id))
    );
    sink.withdrawal = WithdrawalOutcome::Requested;
    assert_eq!(
        engine.withdraw_external_call(id, &scope, &grant, &mut sink),
        Ok(WithdrawalOutcome::Requested)
    );
    assert_eq!(
        engine.withdraw_external_call(id, &scope, &grant, &mut sink),
        Err(ExternalCommandRejection::AlreadyWithdrawn)
    );
    engine
        .revoke_external_dispatch_authority(&scope, &grant)
        .unwrap();
    assert_eq!(
        engine.withdraw_external_call(id, &scope, &grant, &mut sink),
        Err(ExternalCommandRejection::AuthorityFenced)
    );
    assert_eq!(sink.withdrawals, [id]);
    start_service(&mut engine, &ctx);
    assert_eq!(
        engine.receive_external_serving(&ctx, &native),
        Ok(ExternalServingResult::Duplicate)
    );
    engine
        .map_external_assignment(
            scope.clone(),
            NativeAssignmentId::new("another-native-entity"),
            CarId::new("B"),
        )
        .unwrap();
    assert!(matches!(
        engine.receive_external_assignment(assignment(id, &scope, "another-native-entity")),
        ExternalAssignmentResult::Rejected(ExternalEvidenceRejection::ConflictingAssignment { .. })
    ));
    assert!(matches!(
        engine.receive_external_serving(&ctx, &NativeAssignmentId::new("native-deck-A")),
        Err(ExternalEvidenceRejection::WrongAssignment { .. })
    ));
    let wrong_scope = ExternalCallContext {
        scope: ExternalDispatchScopeId::new("other"),
        ..ctx.clone()
    };
    assert!(matches!(
        engine.receive_external_serving(&wrong_scope, &native),
        Err(ExternalEvidenceRejection::WrongScope { .. })
    ));
    let wrong_controller = ExternalCallContext {
        controller: ControllerId::new("other"),
        ..ctx.clone()
    };
    assert!(matches!(
        engine.receive_external_receipt(&wrong_controller, ExternalDispatchDisposition::Accepted),
        Err(ExternalEvidenceRejection::WrongController { .. })
    ));
    assert_eq!(
        engine.receive_external_receipt(&ctx, ExternalDispatchDisposition::Accepted),
        Ok(ExternalReceiptResult::Recorded)
    );
    assert_eq!(
        engine.calls().get(id).unwrap().state,
        CallState::Serving(CarId::new("B"))
    );
    assert_eq!(
        engine.external_dispatch_record(id).unwrap().withdrawal,
        Some(WithdrawalOutcome::Requested)
    );
}

#[test]
fn one_observed_stop_does_not_choose_between_two_serving_calls() {
    let mut engine = engine();
    let (up, scope, grant, mut sink) = submitted_call(&mut engine);
    let down = submit(
        &mut engine,
        HallCall {
            direction: Direction::Down,
            ..call()
        },
        &scope,
        &grant,
        &mut sink,
    );
    bind_observer(&mut engine, "primary", ObservationBindingRole::Operational);
    for id in [up, down] {
        start_service(&mut engine, &context(id, &scope));
    }
    engine.ingest_observation_update(sample("primary", 1, 3, DoorState::Closed));
    engine.ingest_observation_update(sample("primary", 2, 3, DoorState::Open));
    for id in [up, down] {
        assert_eq!(
            engine.calls().get(id).unwrap().state,
            CallState::Serving(CarId::new("B"))
        );
    }
}

#[test]
fn prevention_retires_without_completion_and_unsupported_preserves_work() {
    let mut engine = engine();
    let (id, scope, grant, mut sink) = submitted_call(&mut engine);
    sink.controller = ControllerId::new("other");
    assert_eq!(
        engine.withdraw_external_call(id, &scope, &grant, &mut sink),
        Err(ExternalCommandRejection::WrongController)
    );
    sink.controller = ControllerId::new("controller-X");
    sink.withdrawal = WithdrawalOutcome::Prevented;
    assert_eq!(
        engine.withdraw_external_call(id, &scope, &grant, &mut sink),
        Ok(WithdrawalOutcome::Prevented)
    );
    assert_eq!(engine.calls().get(id), None);
    assert_eq!(engine.external_dispatch_record(id), None);
    assert_eq!(
        engine.external_dispatch_terminal(id),
        Some(ExternalDispatchTerminal::Prevented)
    );
    let new_id = submit(&mut engine, call(), &scope, &grant, &mut sink);
    assert!(new_id > id);
    assert_eq!(
        engine.receive_external_assignment(assignment(id, &scope, "native-deck-B")),
        ExternalAssignmentResult::Rejected(ExternalEvidenceRejection::InactiveCall(id))
    );
    sink.withdrawal = WithdrawalOutcome::Unsupported;
    assert_eq!(
        engine.withdraw_external_call(new_id, &scope, &grant, &mut sink),
        Ok(WithdrawalOutcome::Unsupported)
    );
    start_service(&mut engine, &context(new_id, &scope));

    let other = submit(
        &mut engine,
        HallCall {
            floor: Floor::new(4),
            ..call()
        },
        &scope,
        &grant,
        &mut sink,
    );
    start_service(&mut engine, &context(other, &scope));
    sink.withdrawal = WithdrawalOutcome::Prevented;
    assert_eq!(
        engine.withdraw_external_call(other, &scope, &grant, &mut sink),
        Err(ExternalCommandRejection::InconsistentPrevention)
    );
    assert_eq!(
        engine.calls().get(other).unwrap().state,
        CallState::Serving(CarId::new("B"))
    );
    assert_eq!(engine.external_dispatch_terminal(other), None);
}

#[test]
fn completion_requires_progress_and_fresh_operational_arrival_then_open_evidence() {
    let mut engine = engine();
    let (id, scope, _, _) = submitted_call(&mut engine);
    let ctx = context(id, &scope);
    bind_observer(&mut engine, "primary", ObservationBindingRole::Operational);
    bind_observer(&mut engine, "diagnostic", ObservationBindingRole::Auxiliary);
    engine.ingest_observation_update(sample("primary", 1, 3, DoorState::Open));
    start_service(&mut engine, &ctx);
    engine.ingest_observation_update(sample("primary", 2, 3, DoorState::Open));
    engine.ingest_observation_update(sample("diagnostic", 1, 3, DoorState::Closed));
    engine.ingest_observation_update(sample("diagnostic", 2, 3, DoorState::Open));
    let mut local_sink = RecordingIntentSink::default();
    let car = CarId::new("B");
    engine.consume_runtime_events(
        vec![
            RuntimeEvent::IntentAccepted(Intent::ServeFloor {
                car: car.clone(),
                floor: Floor::new(3),
            }),
            RuntimeEvent::Arrived {
                car: car.clone(),
                floor: Floor::new(3),
            },
            RuntimeEvent::DoorStateChanged {
                car: car.clone(),
                state: DoorState::Open,
            },
        ],
        &mut local_sink,
    );
    assert!(local_sink.0.is_empty());
    assert_eq!(
        engine.calls().get(id).unwrap().state,
        CallState::Serving(car.clone())
    );

    engine.ingest_observation_update(sample("primary", 3, 2, DoorState::Open));
    let mut arrival = sample("primary", 4, 3, DoorState::Open);
    arrival.door = None;
    engine.ingest_observation_update(arrival);
    assert_eq!(
        engine.calls().get(id).unwrap().state,
        CallState::Serving(car)
    );
    assert!(matches!(
        engine.ingest_observation_update(sample("primary", 3, 3, DoorState::Open)),
        ObservationIngestResult::Rejected(_)
    ));
    engine.ingest_observation_update(sample("primary", 5, 3, DoorState::Open));
    assert_eq!(engine.calls().get(id).unwrap().state, CallState::Completed);
    assert_eq!(engine.external_dispatch_record(id), None);
    assert_eq!(
        engine.external_dispatch_terminal(id),
        Some(ExternalDispatchTerminal::Completed)
    );
    assert_eq!(
        engine.receive_external_receipt(&ctx, ExternalDispatchDisposition::Accepted),
        Err(ExternalEvidenceRejection::InactiveCall(id))
    );
    assert_eq!(
        engine.receive_external_serving(&ctx, &NativeAssignmentId::new("native-deck-B")),
        Err(ExternalEvidenceRejection::InactiveCall(id))
    );
}

#[test]
fn virtual_recovery_keeps_call_identity_delivery_progress_and_command_fences() {
    for delivery in [
        ExternalDispatchDelivery::NotSubmitted,
        ExternalDispatchDelivery::Submitted,
        ExternalDispatchDelivery::Indeterminate,
    ] {
        let mut engine = engine();
        let (scope, grant, mut sink) = configured_external(&mut engine);
        sink.delivery = delivery;
        let id = submit(&mut engine, call(), &scope, &grant, &mut sink);
        let ctx = context(id, &scope);
        if delivery == ExternalDispatchDelivery::Indeterminate {
            assert_eq!(
                engine.receive_external_receipt(&ctx, ExternalDispatchDisposition::Indeterminate),
                Ok(ExternalReceiptResult::Recorded)
            );
        }
        let mut engine = Engine::restore_virtual(engine.into_virtual_checkpoint());
        assert_eq!(
            engine.external_dispatch_record(id).unwrap().delivery,
            delivery
        );
        if delivery == ExternalDispatchDelivery::Indeterminate {
            assert_eq!(
                engine.external_dispatch_record(id).unwrap().receipt,
                Some(ExternalDispatchDisposition::Indeterminate)
            );
        }
        assert_eq!(
            engine.retry_external_call(id, &scope, &grant, &mut sink),
            Err(ExternalCommandRejection::AuthorityFenced)
        );
        let fresh = engine
            .grant_external_dispatch_authority(&scope, PrincipalId::new("operator"))
            .unwrap();
        assert!(
            matches!(engine.submit_external_call(call(), &scope, &fresh, &mut sink),
            Ok(ExternalDispatchSubmission::Duplicate { request, .. }) if request.call == id)
        );
        assert_eq!(sink.requests.len(), 1);
        if delivery == ExternalDispatchDelivery::NotSubmitted {
            sink.delivery = ExternalDispatchDelivery::Submitted;
            assert_eq!(
                engine.retry_external_call(id, &scope, &fresh, &mut sink),
                Ok(ExternalDispatchDelivery::Submitted)
            );
            assert_eq!(sink.requests[0], sink.requests[1]);
        } else {
            assert_eq!(
                engine.retry_external_call(id, &scope, &fresh, &mut sink),
                Err(ExternalCommandRejection::RetryUnsafe)
            );
            assert_eq!(sink.requests.len(), 1);
        }
    }

    let mut engine = engine();
    let (id, scope, _, sink) = submitted_call(&mut engine);
    let ctx = context(id, &scope);
    bind_observer(&mut engine, "primary", ObservationBindingRole::Operational);
    engine.receive_external_assignment(assignment(id, &scope, "native-deck-B"));
    let mut engine = Engine::restore_virtual(engine.into_virtual_checkpoint());
    assert_eq!(
        engine.calls().get(id).unwrap().state,
        CallState::Assigned(CarId::new("B"))
    );
    assert_eq!(
        engine.receive_external_serving(&ctx, &NativeAssignmentId::new("native-deck-B")),
        Ok(ExternalServingResult::Started)
    );
    engine.ingest_observation_update(sample("primary", 1, 3, DoorState::Closed));
    let mut engine = Engine::restore_virtual(engine.into_virtual_checkpoint());
    assert_eq!(
        engine.calls().get(id).unwrap().state,
        CallState::Serving(CarId::new("B"))
    );
    assert!(matches!(
        engine.ingest_observation_update(sample("primary", 1, 3, DoorState::Open)),
        ObservationIngestResult::Rejected(_)
    ));
    engine.ingest_observation_update(sample("primary", 2, 3, DoorState::Open));
    assert_eq!(
        engine.calls().get(id).unwrap().state,
        CallState::Serving(CarId::new("B"))
    );
    engine.ingest_observation_update(sample("primary", 3, 3, DoorState::Closed));
    engine.ingest_observation_update(sample("primary", 4, 3, DoorState::Open));
    assert_eq!(engine.calls().get(id).unwrap().state, CallState::Completed);
    let mut engine = Engine::restore_virtual(engine.into_virtual_checkpoint());
    assert_eq!(
        engine.receive_external_serving(&ctx, &NativeAssignmentId::new("native-deck-B")),
        Err(ExternalEvidenceRejection::InactiveCall(id))
    );
    assert_eq!(engine.decision(), None);
    assert_eq!(sink.requests.len(), 1);
}

#[test]
fn recorded_landing_call_partial_mapping_keeps_remote_uncertainty_pending() {
    let mut engine = engine();
    let (scope, grant, mut sink) = configured_external(&mut engine);
    sink.delivery = ExternalDispatchDelivery::Submitted;

    let up = submit(&mut engine, call(), &scope, &grant, &mut sink);
    let up_context = context(up, &scope);
    assert_eq!(
        engine.receive_external_receipt(&up_context, ExternalDispatchDisposition::Indeterminate),
        Ok(ExternalReceiptResult::Recorded)
    );
    assert!(matches!(
        engine.submit_external_call(call(), &scope, &grant, &mut sink),
        Ok(ExternalDispatchSubmission::Duplicate {
            request,
            delivery: ExternalDispatchDelivery::Submitted,
        }) if request.call == up
    ));
    assert_eq!(sink.requests.len(), 1, "resume must not resubmit Call A");

    let down = submit(
        &mut engine,
        HallCall {
            direction: Direction::Down,
            ..call()
        },
        &scope,
        &grant,
        &mut sink,
    );
    assert_eq!(
        engine.receive_external_receipt(
            &context(down, &scope),
            ExternalDispatchDisposition::Indeterminate
        ),
        Ok(ExternalReceiptResult::Recorded)
    );
    sink.withdrawal = WithdrawalOutcome::Requested;
    assert_eq!(
        engine.withdraw_external_call(down, &scope, &grant, &mut sink),
        Ok(WithdrawalOutcome::Requested)
    );

    for id in [up, down] {
        assert_eq!(engine.calls().get(id).unwrap().state, CallState::Pending);
        assert_eq!(engine.external_dispatch_terminal(id), None);
        let record = engine.external_dispatch_record(id).unwrap();
        assert_eq!(record.delivery, ExternalDispatchDelivery::Submitted);
        assert_eq!(
            record.receipt,
            Some(ExternalDispatchDisposition::Indeterminate)
        );
    }
    assert_eq!(
        engine.external_dispatch_record(up).unwrap().withdrawal,
        None
    );
    assert_eq!(
        engine.external_dispatch_record(down).unwrap().withdrawal,
        Some(WithdrawalOutcome::Requested)
    );
    assert_eq!(sink.requests.len(), 2);
    assert_eq!(sink.withdrawals, vec![down]);
}

#[test]
fn completed_and_prevented_history_stay_bounded_and_evicted_ids_stay_fenced() {
    let mut engine = engine();
    let (scope, grant, mut sink) = configured_external(&mut engine);
    bind_observer(&mut engine, "primary", ObservationBindingRole::Operational);
    let mut sequence = 1;
    for i in 0..=RECENT_EXTERNAL_TERMINALS {
        let completed = submit(&mut engine, call(), &scope, &grant, &mut sink);
        start_service(&mut engine, &context(completed, &scope));
        engine.ingest_observation_update(sample("primary", sequence, 3, DoorState::Closed));
        engine.ingest_observation_update(sample("primary", sequence + 1, 3, DoorState::Open));
        sequence += 2;
        let prevented = submit(
            &mut engine,
            HallCall {
                direction: Direction::Down,
                ..call()
            },
            &scope,
            &grant,
            &mut sink,
        );
        sink.withdrawal = WithdrawalOutcome::Prevented;
        engine
            .withdraw_external_call(prevented, &scope, &grant, &mut sink)
            .unwrap();
        assert_eq!(completed.sequence(), (i as u64) * 2 + 1);
        assert_eq!(prevented.sequence(), completed.sequence() + 1);
        assert!(engine.external_dispatch.active.is_empty());
    }
    assert_eq!(
        engine.external_dispatch.completed.len(),
        RECENT_EXTERNAL_TERMINALS
    );
    assert_eq!(
        engine.external_dispatch.prevented.len(),
        RECENT_EXTERNAL_TERMINALS
    );
    let mut engine = Engine::restore_virtual(engine.into_virtual_checkpoint());
    for id in [CallId::new(1), CallId::new(2)] {
        assert_eq!(engine.external_dispatch_terminal(id), None);
        assert_eq!(engine.calls().get(id), None);
        assert_eq!(
            engine.receive_external_receipt(
                &context(id, &scope),
                ExternalDispatchDisposition::Accepted
            ),
            Err(ExternalEvidenceRejection::UnknownCall(id))
        );
    }
    assert_eq!(engine.calls().active_calls().count(), 0);
}
