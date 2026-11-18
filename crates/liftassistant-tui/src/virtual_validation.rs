use std::time::Duration;

use liftassistant_engine::{
    Admission, AuthorityError, AuthorityGrant, ControlledIssue, ControllerId, DiagnosticAnomaly,
    Engine, IntentRequest, ObservationBindingRole, ObservationIngestResult, PrincipalId, Receipt,
    ReceiptResult, Reconciliation, WithdrawalError, WithdrawalReason, WriteBlocker,
};
use liftassistant_model::{
    BuildingTopology, CarId, CarObservationUpdate, CarState, ExecutionCapabilities, Floor, Intent,
    ObservationProvenance, ObservationSequence, ObservationSessionId, ObservationSourceId,
};
use liftassistant_runtime::{
    ControlledIntentSink, IntentReceipt, ReceiptStatus, WithdrawalOutcome,
};
use liftassistant_sim::{
    SimRuntime, VirtualBehavior, VirtualController, VirtualExecution, VirtualReceipt,
};

fn check(condition: bool, context: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| context.to_owned())
}

struct Lab {
    engine: Engine,
    controller: VirtualController,
    a: CarId,
    b: CarId,
    a_grant: AuthorityGrant,
    b_grant: AuthorityGrant,
}

impl Lab {
    fn new(can_withdraw: bool) -> Self {
        let a = CarId::new("A");
        let b = CarId::new("B");
        let controller_id = ControllerId::new("virtual-lab");
        let world = SimRuntime::new(
            [
                (
                    CarState::new("A", Floor::new(3)),
                    ExecutionCapabilities::reference_defaults(),
                ),
                (
                    CarState::new("B", Floor::new(1)),
                    ExecutionCapabilities::new(true, true, (1..=6).map(Floor::new)),
                ),
            ],
            2.0,
        );
        let mut controller = VirtualController::new(controller_id.clone(), world, can_withdraw);
        let (source, session) = controller.observation_source();
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        for (car, capabilities) in controller.execution_capabilities() {
            engine
                .bind_observation(
                    source.clone(),
                    car.clone(),
                    ObservationBindingRole::Operational,
                )
                .unwrap();
            engine
                .bind_controlled_car(car.clone(), controller_id.clone())
                .unwrap();
            engine.set_execution_capabilities(car, capabilities);
        }
        engine.begin_source_session(source, session).unwrap();
        let a_grant = engine
            .grant_authority(&a, PrincipalId::new("writer-A"))
            .unwrap();
        let b_grant = engine
            .grant_authority(&b, PrincipalId::new("writer-B"))
            .unwrap();
        for update in controller.observe_current() {
            engine.ingest_observation_update(update);
        }
        Self {
            engine,
            controller,
            a,
            b,
            a_grant,
            b_grant,
        }
    }

    fn issue(&mut self, floor: i16) -> IntentRequest {
        self.engine.issue_controlled_intent(
            Intent::ServeFloor {
                car: self.a.clone(),
                floor: Floor::new(floor),
            },
            None,
        )
    }

    fn submit(&mut self, request: &IntentRequest) -> Admission {
        self.engine
            .submit_controlled(request.clone(), &self.a_grant, &mut self.controller)
    }

    fn tick(&mut self, elapsed_ms: u64) -> Result<(), String> {
        let step = self.controller.advance(Duration::from_millis(elapsed_ms));
        for receipt in step.receipts {
            let result = self.engine.receive_intent_receipt(receipt.clone());
            check(
                matches!(result, ReceiptResult::Recorded | ReceiptResult::Duplicate),
                &format!("virtual controller returned {result:?} for {receipt:?}"),
            )?;
        }
        for observation in step.observations {
            check(
                matches!(
                    self.engine.ingest_observation_update(observation),
                    ObservationIngestResult::Resolution(_)
                ),
                "virtual physics bypassed canonical resolution",
            )?;
        }
        Ok(())
    }
}

fn delivery() -> Result<Engine, String> {
    let mut lab = Lab::new(true);
    let operation = lab.issue(6);
    let id = operation.id();
    check(
        lab.submit(&operation) == Admission::Admitted,
        "admit first operation",
    )?;
    for _ in 0..10 {
        check(
            lab.submit(&operation) == Admission::Retry,
            "same-ID duplicate must reuse operation",
        )?;
    }
    let mut conflict = operation.clone();
    conflict.intent = Intent::ServeFloor {
        car: lab.a.clone(),
        floor: Floor::new(7),
    };
    check(
        lab.submit(&conflict) == Admission::IdentityConflict,
        "same-ID changed floor",
    )?;
    check(
        lab.controller.submissions() == 1,
        "duplicate or conflict reached controller",
    )?;
    check(
        lab.engine.receive_intent_receipt(IntentReceipt {
            controller: ControllerId::new("unbound"),
            intent: id,
            status: ReceiptStatus::Accepted,
        }) == ReceiptResult::Unexpected,
        "different controller supplied receipt",
    )?;
    lab.tick(0)?;
    let record = lab.engine.intent_record(id).unwrap();
    check(
        record.receipt == Receipt::Accepted && record.reconciliation == Reconciliation::Pending,
        "acceptance is not arrival",
    )?;
    lab.tick(2_000)?;
    check(
        lab.engine.intent_record(id).unwrap().reconciliation == Reconciliation::Satisfied,
        "operational arrival must satisfy",
    )?;

    lab.controller.set_next_behavior(VirtualBehavior {
        reject: true,
        ..VirtualBehavior::default()
    });
    let rejected = lab.issue(4);
    check(
        lab.submit(&rejected) == Admission::Admitted,
        "admission precedes controller rejection",
    )?;
    lab.tick(0)?;
    let record = lab.engine.intent_record(rejected.id()).unwrap();
    check(
        record.receipt == Receipt::Rejected && record.reconciliation == Reconciliation::Pending,
        "rejected operation changed physical outcome",
    )?;

    lab.controller.set_next_behavior(VirtualBehavior {
        receipt: VirtualReceipt::Lost,
        ..VirtualBehavior::default()
    });
    let lost = lab.issue(4);
    check(
        lab.submit(&lost) == Admission::Admitted,
        "admit with uncertain delivery",
    )?;
    check(
        lab.engine.intent_record(lost.id()).unwrap().receipt == Receipt::Indeterminate,
        "lost receipt must become indeterminate",
    )?;
    lab.tick(0)?;
    lab.tick(1_000)?;
    let record = lab.engine.intent_record(lost.id()).unwrap();
    check(
        record.receipt == Receipt::Indeterminate
            && record.reconciliation == Reconciliation::Satisfied,
        "observation must resolve a lost receipt's physical outcome",
    )?;
    check(
        lab.engine.receive_intent_receipt(IntentReceipt {
            controller: lab.controller.controller_id().clone(),
            intent: lost.id(),
            status: ReceiptStatus::Rejected,
        }) == ReceiptResult::Inconsistent,
        "contradictory late receipt must be surfaced",
    )?;
    let incidents: Vec<_> = lab.engine.diagnostics().incidents().collect();
    for (intent, issue) in [
        (id, ControlledIssue::IdentityConflict),
        (id, ControlledIssue::UnexpectedReceipt),
        (lost.id(), ControlledIssue::InconsistentReceipt),
    ] {
        let anomaly = DiagnosticAnomaly::ControlledIntent { intent, issue };
        check(
            incidents.iter().any(|incident| {
                incident.trigger == anomaly && incident.replay_anomalies().contains(&anomaly)
            }),
            "missing bounded replay evidence for intent incident",
        )?;
    }
    Ok(lab.engine)
}

fn restart() -> Result<(), String> {
    let mut lab = Lab::new(true);
    lab.controller.set_next_behavior(VirtualBehavior {
        receipt: VirtualReceipt::After(Duration::from_secs(5)),
        execution: VirtualExecution::After(Duration::from_secs(2)),
        ..VirtualBehavior::default()
    });
    let request = lab.issue(7);
    check(
        lab.submit(&request) == Admission::Admitted,
        "submit before restart",
    )?;
    let previous_observation = lab
        .controller
        .observe_current()
        .into_iter()
        .find(|obs| obs.car == lab.a)
        .unwrap();
    lab.engine
        .ingest_observation_update(previous_observation.clone());
    lab.controller = lab.controller.restart();
    lab.engine
        .controlled_transport_lost(lab.controller.controller_id());
    let saved = lab.engine.into_virtual_checkpoint();
    lab.engine = Engine::restore_virtual(saved);
    let record = lab.engine.intent_record(request.id()).unwrap();
    check(
        record.receipt == Receipt::Indeterminate
            && record.reconciliation == Reconciliation::Pending,
        "restart must preserve unresolved delivery",
    )?;
    check(
        lab.engine.operational_car_state(&lab.a).is_none(),
        "restart retained old operational state",
    )?;
    check(
        matches!(
            lab.engine.ingest_observation_update(previous_observation),
            ObservationIngestResult::Rejected(_)
        ),
        "restart accepted stale session sequence",
    )?;
    check(
        lab.submit(&request) == Admission::Blocked(WriteBlocker::AuthorityFenced),
        "restore revived previous grant",
    )?;
    check(
        lab.engine
            .resynchronize_intent(request.id(), &lab.controller)
            .is_none(),
        "disconnected controller query returned a receipt",
    )?;
    lab.controller.set_connected(true);
    let fresh = lab
        .engine
        .grant_authority(&lab.a, PrincipalId::new("writer-A"))
        .unwrap();
    check(fresh.id() != lab.a_grant.id(), "restore reused grant ID")?;
    lab.a_grant = fresh;
    check(
        lab.engine
            .resynchronize_intent(request.id(), &lab.controller)
            == Some(ReceiptResult::Recorded),
        "reconnect failed to query original ID",
    )?;
    check(
        lab.submit(&request) == Admission::Retry,
        "restart must deduplicate under new authority",
    )?;
    let replacement = lab.issue(8);
    check(
        lab.submit(&replacement) == Admission::Blocked(WriteBlocker::ObservationUnavailable),
        "receipt or authority alone enabled writes",
    )?;
    lab.tick(2_000)?;
    check(
        lab.submit(&replacement) == Admission::Blocked(WriteBlocker::ActiveIntent(request.id())),
        "fresh observations bypassed the active conflict guard",
    )?;
    lab.controller.set_connected(false);
    lab.engine
        .controlled_transport_lost(lab.controller.controller_id());
    lab.tick(2_000)?;
    let record = lab.engine.intent_record(request.id()).unwrap();
    check(
        record.receipt == Receipt::Accepted && record.reconciliation == Reconciliation::Satisfied,
        "disconnect after acceptance erased receipt or physical progress",
    )?;
    check(
        lab.controller.submissions() == 1,
        "restart or disconnect caused resend",
    )
}

fn isolation() -> Result<(), String> {
    let mut lab = Lab::new(false);
    let another = PrincipalId::new("competing-writer");
    check(
        lab.engine.grant_authority(&lab.a, another) == Err(AuthorityError::AlreadyGranted),
        "principal stole an active grant",
    )?;
    let denied = lab.issue(5);
    check(
        lab.engine
            .submit_controlled(denied, &lab.b_grant, &mut lab.controller)
            == Admission::Blocked(WriteBlocker::AuthorityFenced),
        "other principal submitted before authorization",
    )?;
    let unsupported = lab.engine.issue_controlled_intent(
        Intent::ServeFloor {
            car: lab.b.clone(),
            floor: Floor::new(7),
        },
        None,
    );
    check(
        lab.engine
            .submit_controlled(unsupported, &lab.b_grant, &mut lab.controller)
            == Admission::Blocked(WriteBlocker::UnsupportedFloor),
        "global topology was mistaken for B's service set",
    )?;
    check(
        lab.controller.submissions() == 0,
        "blocked requests reached controller",
    )?;
    lab.controller.set_next_behavior(VirtualBehavior {
        receipt: VirtualReceipt::Lost,
        execution: VirtualExecution::NoEffect,
        ..VirtualBehavior::default()
    });
    let request = lab.issue(7);
    check(
        lab.submit(&request) == Admission::Admitted,
        "admit a lost no-effect operation",
    )?;
    lab.tick(4_000)?;
    let auxiliary = ObservationSourceId::new("auxiliary");
    let session = ObservationSessionId::new("aux-session");
    lab.engine
        .bind_observation(
            auxiliary.clone(),
            lab.a.clone(),
            ObservationBindingRole::Auxiliary,
        )
        .unwrap();
    lab.engine
        .begin_source_session(auxiliary.clone(), session.clone())
        .unwrap();
    let target = CarState::new("A", Floor::new(7));
    lab.engine
        .ingest_observation_update(CarObservationUpdate::from_state(
            ObservationProvenance {
                source: auxiliary,
                session,
                sequence: ObservationSequence::new(1),
            },
            &target,
        ));
    let record = lab.engine.intent_record(request.id()).unwrap();
    check(
        record.receipt == Receipt::Indeterminate
            && record.reconciliation == Reconciliation::Pending,
        "auxiliary or no-effect evidence completed lost intent",
    )?;
    let (source, session) = lab.controller.observation_source();
    lab.engine
        .invalidate_car_observations(&source, &session, &lab.a)
        .unwrap();
    check(
        lab.engine.operational_car_state(&lab.b).is_some(),
        "A's source loss also revoked B",
    )?;
    let replacement = lab.issue(6);
    check(
        lab.submit(&replacement) == Admission::Blocked(WriteBlocker::ObservationUnavailable),
        "stale authority still admitted new execution",
    )?;
    lab.engine.revoke_authority(&lab.a, &lab.a_grant).unwrap();
    check(
        lab.submit(&request) == Admission::Blocked(WriteBlocker::AuthorityFenced),
        "old principal can issue a retry after revocation",
    )?;
    check(
        lab.controller.submissions() == 1,
        "fault isolation checks caused blind retry",
    )?;
    let anomaly = DiagnosticAnomaly::ControlledIntent {
        intent: request.id(),
        issue: ControlledIssue::WriteReadinessLost,
    };
    check(
        lab.engine
            .diagnostics()
            .incidents()
            .any(|evidence| evidence.trigger == anomaly),
        "active intent readiness loss invisible",
    )
}

fn withdrawal_and_divergence() -> Result<(), String> {
    let mut lab = Lab::new(true);
    for expire in [false, true] {
        lab.controller.set_next_behavior(VirtualBehavior {
            execution: VirtualExecution::After(Duration::from_secs(2)),
            ..VirtualBehavior::default()
        });
        let request = lab.issue(6);
        check(
            lab.submit(&request) == Admission::Admitted,
            "queue delayed execution",
        )?;
        lab.tick(0)?;
        let outcome = if expire {
            lab.engine
                .expire_controlled_intent(request.id(), &mut lab.controller)
        } else {
            check(
                lab.engine.cancel_controlled_intent(
                    request.id(),
                    &lab.b_grant,
                    &mut lab.controller,
                ) == Err(WithdrawalError::AuthorityFenced),
                "wrong grant withdrew an operation",
            )?;
            lab.engine
                .cancel_controlled_intent(request.id(), &lab.a_grant, &mut lab.controller)
        };
        check(
            outcome == Ok(WithdrawalOutcome::Prevented),
            "queued execution was not prevented",
        )?;
        lab.tick(4_000)?;
        let state = lab.engine.operational_car_state(&lab.a).unwrap();
        check(
            state.location.floor_reference() == Floor::new(3),
            "prevented queue started later",
        )?;
    }
    lab.controller.set_next_behavior(VirtualBehavior {
        execution: VirtualExecution::Diverges(Floor::new(4)),
        ..VirtualBehavior::default()
    });
    let request = lab.issue(6);
    check(
        lab.submit(&request) == Admission::Admitted,
        "admit divergent fixture",
    )?;
    lab.tick(0)?;
    check(
        lab.engine
            .cancel_controlled_intent(request.id(), &lab.a_grant, &mut lab.controller)
            == Ok(WithdrawalOutcome::Requested),
        "moving operation incorrectly claimed prevention",
    )?;
    let other = lab.issue(5);
    check(
        lab.submit(&other) == Admission::Blocked(WriteBlocker::ActiveIntent(request.id())),
        "withdrawal silently superseded active operation",
    )?;
    lab.tick(1_000)?;
    let record = lab.engine.intent_record(request.id()).unwrap();
    check(
        record.request.intent.floor() == Floor::new(6)
            && record.reconciliation
                == Reconciliation::Diverged {
                    observed: Floor::new(4),
                },
        "reported destination overwrote desired intent or masked divergence",
    )?;
    check(
        record.withdrawal == Some((WithdrawalReason::Cancelled, WithdrawalOutcome::Requested)),
        "physical result overwrote withdrawal",
    )?;
    let anomaly = DiagnosticAnomaly::ControlledIntent {
        intent: request.id(),
        issue: ControlledIssue::Divergence,
    };
    check(
        lab.engine.diagnostics().incidents().any(|evidence| {
            evidence.trigger == anomaly && evidence.replay_anomalies().contains(&anomaly)
        }),
        "divergence not replayable",
    )?;

    lab.controller.set_next_behavior(VirtualBehavior {
        execution: VirtualExecution::Stalled,
        ..VirtualBehavior::default()
    });
    let stall = lab.issue(8);
    check(
        lab.submit(&stall) == Admission::Admitted,
        "admit stalled fixture",
    )?;
    lab.tick(0)?;
    let b = lab.engine.issue_controlled_intent(
        Intent::ParkAt {
            car: lab.b.clone(),
            floor: Floor::new(5),
        },
        None,
    );
    check(
        lab.engine
            .submit_controlled(b.clone(), &lab.b_grant, &mut lab.controller)
            == Admission::Admitted,
        "A stall blocked healthy B",
    )?;
    lab.tick(0)?;
    lab.tick(2_000)?;
    check(
        lab.engine.intent_record(stall.id()).unwrap().reconciliation == Reconciliation::Pending
            && lab.engine.intent_record(b.id()).unwrap().reconciliation
                == Reconciliation::Satisfied,
        "stall or unrelated-car isolation failed",
    )
}

fn bounded_history() -> Result<(), String> {
    let mut lab = Lab::new(true);
    let first = lab.issue(8);
    let mut latest = first.clone();
    let mut ids = Vec::new();
    for repetition in 0..1_024 {
        lab.controller.set_next_behavior(VirtualBehavior {
            reject: true,
            ..VirtualBehavior::default()
        });
        let request = if repetition == 0 {
            first.clone()
        } else {
            lab.issue(8)
        };
        check(
            lab.submit(&request) == Admission::Admitted,
            "bounded history churn admission",
        )?;
        lab.tick(0)?;
        ids.push(request.id());
        latest = request;
    }
    check(
        ids.iter()
            .filter(|id| lab.engine.intent_record(**id).is_some())
            .count()
            == 128
            && ids
                .iter()
                .filter(|id| lab.controller.receipt(**id).is_some())
                .count()
                == 128,
        "historical terminal state exceeded fixed retention",
    )?;
    check(
        lab.submit(&latest) == Admission::Retry && lab.submit(&first) == Admission::RetiredIdentity,
        "retry reopened a terminal identity",
    )?;
    let mut conflict = first;
    conflict.intent = Intent::ServeFloor {
        car: lab.a.clone(),
        floor: Floor::new(4),
    };
    check(
        lab.submit(&conflict) == Admission::IdentityConflict
            && lab.controller.submissions() == 1_024,
        "retired identity lost its original meaning or caused a replay",
    )
}

fn same_floor() -> Result<(), String> {
    let mut lab = Lab::new(true);
    lab.controller.set_next_behavior(VirtualBehavior {
        execution: VirtualExecution::After(Duration::from_secs(2)),
        ..VirtualBehavior::default()
    });
    let request = lab.issue(3);
    check(
        lab.submit(&request) == Admission::Admitted,
        "already-at-floor operation failed admission",
    )?;
    lab.tick(0)?;
    for update in lab.controller.observe_current() {
        lab.engine.ingest_observation_update(update);
    }
    check(
        lab.engine
            .intent_record(request.id())
            .unwrap()
            .reconciliation
            == Reconciliation::Pending,
        "an unchanged idle snapshot satisfied a still-queued operation",
    )?;
    lab.tick(2_000)?;
    lab.tick(1)?;
    check(
        lab.engine
            .intent_record(request.id())
            .unwrap()
            .reconciliation
            == Reconciliation::Satisfied,
        "fresh same-floor service evidence did not settle the operation",
    )
}

pub fn run() -> Result<String, String> {
    check(delivery()? == delivery()?, "independent replay diverged")?;
    restart()?;
    isolation()?;
    withdrawal_and_divergence()?;
    bounded_history()?;
    same_floor()?;
    Ok(concat!(
        "M6A VirtualController failure and restart matrix PASS\n",
        "  identity, receipts, canonical reconciliation, deterministic incident replay\n",
        "  checkpoint, disconnect, resynchronization without resend\n",
        "  authority, readiness, floor coverage, and per-car isolation\n",
        "  cancellation, expiry, stalls, and observed divergence\n",
        "  1,024 operations, 128 retained outcomes per owner, retired sequence fenced\n",
    )
    .to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn virtual_failure_and_restart_matrix() {
        super::run().unwrap();
    }
}
