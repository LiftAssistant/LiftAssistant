#![forbid(unsafe_code)]

use std::{collections::VecDeque, env, process::ExitCode};

use liftassistant_engine::{
    DiagnosticAnomaly, DiagnosticEvent, Engine, ObservationBindingRole, ObservationIngestError,
    ObservationIngestResult, SourceSessionError, SourceSessionTransition,
};
use liftassistant_model::{
    BuildingTopology, CarId, CarLocation, CarObservation, CarObservationUpdate, DoorEvidence,
    DoorState, Floor, MotionEvidence, MotionState, ObservationProvenance, ObservationSequence,
    ObservationSessionId, ObservationSourceId, OperatingMode, OperatingModeEvidence, Position,
    PositionEvidence, ReportedDestination,
};
use liftassistant_runtime::{CarStateResolution, UnresolvedCarStateReason};

const LIFECYCLE_SCENARIO: &str = "headless-read-only-lifecycle";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Readiness {
    NotReady,
    ReadOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DegradedReason {
    ObservationAnomaly(DiagnosticAnomaly),
    ObservationRejected(ObservationIngestError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HostLifecycle {
    Starting,
    Synchronizing,
    ReadyReadOnly,
    Degraded(DegradedReason),
}

impl HostLifecycle {
    fn readiness(&self) -> Readiness {
        match self {
            Self::ReadyReadOnly => Readiness::ReadOnly,
            Self::Starting | Self::Synchronizing | Self::Degraded(_) => Readiness::NotReady,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum HostEvent {
    LifecycleChanged(HostLifecycle),
    ObservationUnresolved {
        car: CarId,
        reasons: Vec<UnresolvedCarStateReason>,
    },
    StateUpdated(liftassistant_model::CarState),
    ObservationRejected(DiagnosticAnomaly),
    IngestionRejected(ObservationIngestError),
    AuxiliaryObservationAccepted {
        source: ObservationSourceId,
        car: CarId,
    },
}

/// Internal in-memory source for the headless proof. It exposes observations only.
#[derive(Debug)]
struct RecordedObservationFeed {
    frames: VecDeque<CarObservationUpdate>,
}

impl RecordedObservationFeed {
    fn new<T>(frames: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<CarObservationUpdate>,
    {
        Self {
            frames: frames.into_iter().map(Into::into).collect(),
        }
    }

    fn next_observation(&mut self) -> Option<CarObservationUpdate> {
        self.frames.pop_front()
    }
}

#[derive(Debug)]
struct ReadOnlyHost {
    engine: Engine,
    lifecycle: HostLifecycle,
    events: Vec<HostEvent>,
}

impl ReadOnlyHost {
    fn new(topology: BuildingTopology) -> Self {
        Self {
            engine: Engine::new(topology),
            lifecycle: HostLifecycle::Starting,
            events: vec![HostEvent::LifecycleChanged(HostLifecycle::Starting)],
        }
    }

    fn lifecycle(&self) -> &HostLifecycle {
        &self.lifecycle
    }

    fn readiness(&self) -> Readiness {
        self.lifecycle.readiness()
    }

    fn poll(&mut self, source: &mut RecordedObservationFeed) -> bool {
        let Some(observation) = source.next_observation() else {
            return false;
        };
        let observation_source = observation.provenance.source.clone();
        let car = observation.car.clone();
        let role = self
            .engine
            .observation_binding_role(&observation_source, &car);
        let is_operational = role == Some(ObservationBindingRole::Operational);

        if is_operational
            && matches!(
                self.lifecycle,
                HostLifecycle::Starting | HostLifecycle::Degraded(_)
            )
        {
            self.transition(HostLifecycle::Synchronizing);
        }

        let incident_marker = latest_incident_marker(&self.engine);
        let resolution = self.engine.ingest_observation_update(observation);
        if latest_incident_marker(&self.engine) != incident_marker {
            let anomaly = self
                .engine
                .diagnostics()
                .incidents()
                .last()
                .expect("a new incident was counted")
                .trigger
                .clone();
            self.events
                .push(HostEvent::ObservationRejected(anomaly.clone()));
            if is_operational {
                self.transition(HostLifecycle::Degraded(DegradedReason::ObservationAnomaly(
                    anomaly,
                )));
            }
            return true;
        }

        if !is_operational {
            match resolution {
                ObservationIngestResult::Rejected(error) => {
                    self.events.push(HostEvent::IngestionRejected(error));
                }
                ObservationIngestResult::Incomplete(_) | ObservationIngestResult::Resolution(_) => {
                    self.events.push(HostEvent::AuxiliaryObservationAccepted {
                        source: observation_source,
                        car,
                    });
                }
            }
            return true;
        }

        match resolution {
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(state)) => {
                self.events.push(HostEvent::StateUpdated(state));
                self.transition(HostLifecycle::ReadyReadOnly);
            }
            ObservationIngestResult::Resolution(CarStateResolution::Unresolved {
                car,
                reasons,
            }) => {
                self.events
                    .push(HostEvent::ObservationUnresolved { car, reasons });
                if !matches!(self.lifecycle, HostLifecycle::Synchronizing) {
                    self.transition(HostLifecycle::Synchronizing);
                }
            }
            ObservationIngestResult::Incomplete(assembled) => {
                if !matches!(self.lifecycle, HostLifecycle::Synchronizing) {
                    self.transition(HostLifecycle::Synchronizing);
                }
                debug_assert!(assembled.snapshot().is_none());
            }
            ObservationIngestResult::Rejected(error) => {
                self.events
                    .push(HostEvent::IngestionRejected(error.clone()));
                self.transition(HostLifecycle::Degraded(
                    DegradedReason::ObservationRejected(error),
                ));
            }
        }
        true
    }

    fn begin_source_session(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
    ) -> Result<SourceSessionTransition, SourceSessionError> {
        let transition = self.engine.begin_source_session(source, session)?;
        let invalidated_operational_state = transition.invalidated_cars.iter().any(|car| {
            self.engine
                .observation_binding_role(&transition.source, car)
                == Some(ObservationBindingRole::Operational)
        });
        if invalidated_operational_state {
            self.transition(HostLifecycle::Synchronizing);
        }
        Ok(transition)
    }

    fn transition(&mut self, lifecycle: HostLifecycle) {
        if self.lifecycle != lifecycle {
            self.lifecycle = lifecycle.clone();
            self.events.push(HostEvent::LifecycleChanged(lifecycle));
        }
    }
}

fn latest_incident_marker(engine: &Engine) -> Option<u64> {
    engine
        .diagnostics()
        .incidents()
        .last()
        .and_then(|incident| {
            incident.evidence.iter().rev().find_map(|entry| {
                matches!(entry.event, DiagnosticEvent::Anomaly(_)).then_some(entry.sequence.value())
            })
        })
}

#[derive(Clone, Debug, PartialEq)]
struct LifecycleSnapshot {
    lifecycle: HostLifecycle,
    readiness: Readiness,
}

#[derive(Clone, Debug, PartialEq)]
struct ProofRun {
    snapshots: Vec<LifecycleSnapshot>,
    events: Vec<HostEvent>,
    latest_state: Option<liftassistant_model::CarState>,
    diagnostic_entries: usize,
    incidents: usize,
}

fn main() -> ExitCode {
    match parse_scenario(env::args().skip(1))
        .and_then(|name| run_scenario(&name))
        .map(|report| {
            println!("{report}");
        }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_scenario(mut args: impl Iterator<Item = String>) -> Result<String, String> {
    match (args.next(), args.next(), args.next()) {
        (Some(flag), Some(name), None) if flag == "--scenario" => Ok(name),
        _ => Err(format!(
            "usage: liftassistant-headless --scenario {LIFECYCLE_SCENARIO}"
        )),
    }
}

fn run_scenario(name: &str) -> Result<String, String> {
    if name != LIFECYCLE_SCENARIO {
        return Err(format!(
            "unknown scenario {name:?}; available scenario: {LIFECYCLE_SCENARIO}"
        ));
    }

    let first = execute_proof();
    let second = execute_proof();
    if first != second {
        return Err("headless lifecycle proof was not deterministic".into());
    }
    verify_proof(&first)?;
    Ok(render_report(&first))
}

fn execute_proof() -> ProofRun {
    let mut source = reference_source();
    let mut host = reference_host();
    let mut snapshots = vec![snapshot(&host)];
    while host.poll(&mut source) {
        snapshots.push(snapshot(&host));
    }

    ProofRun {
        snapshots,
        events: host.events.clone(),
        latest_state: host.engine.car_states().into_iter().next(),
        diagnostic_entries: host.engine.diagnostics().entries().count(),
        incidents: host.engine.diagnostics().incidents().count(),
    }
}

fn reference_host() -> ReadOnlyHost {
    let mut host = ReadOnlyHost::new(
        BuildingTopology::new([1, 2, 3, 4].map(Floor::new))
            .expect("reference topology is non-empty"),
    );
    host.engine
        .bind_observation(
            reference_source_id(),
            reference_car_id(),
            ObservationBindingRole::Operational,
        )
        .expect("headless fixture has one operational observation binding");
    host.begin_source_session(reference_source_id(), reference_session_id())
        .expect("headless fixture starts one source session");
    host
}

fn reference_source_id() -> ObservationSourceId {
    ObservationSourceId::new("fixture/headless-recorded")
}

fn reference_session_id() -> ObservationSessionId {
    ObservationSessionId::new("headless-fixture-session-A")
}

fn reference_car_id() -> CarId {
    CarId::new("lift-A")
}

fn snapshot(host: &ReadOnlyHost) -> LifecycleSnapshot {
    LifecycleSnapshot {
        lifecycle: host.lifecycle().clone(),
        readiness: host.readiness(),
    }
}

fn reference_source() -> RecordedObservationFeed {
    let complete = complete_observation(
        reference_source_id(),
        reference_session_id(),
        2,
        Floor::new(4),
    );
    let duplicate = complete.clone();
    RecordedObservationFeed::new([observation(1), complete, duplicate])
}

fn complete_observation(
    source: ObservationSourceId,
    session: ObservationSessionId,
    sequence: u64,
    floor: Floor,
) -> CarObservation {
    let mut complete = observation(sequence);
    complete.provenance.source = source;
    complete.provenance.session = session;
    complete.position = PositionEvidence::Exact(Position::at_floor(floor));
    complete.motion = MotionEvidence::Known(MotionState::Stopped);
    complete.door = DoorEvidence::Known(DoorState::Open);
    complete.reported_destination = ReportedDestination::None;
    complete
}

fn observation(sequence: u64) -> CarObservation {
    CarObservation {
        provenance: ObservationProvenance {
            source: reference_source_id(),
            session: reference_session_id(),
            sequence: ObservationSequence::new(sequence),
        },
        car: reference_car_id(),
        position: PositionEvidence::FloorReference(Floor::new(3)),
        motion: MotionEvidence::Known(MotionState::Moving(liftassistant_model::Direction::Up)),
        door: DoorEvidence::MovingUnknownDirection,
        mode: OperatingModeEvidence::Known(OperatingMode::FireService),
        reported_destination: ReportedDestination::Known(Floor::new(4)),
    }
}

fn expected_anomaly() -> DiagnosticAnomaly {
    DiagnosticAnomaly::ObservationSequenceNotIncreasing {
        source: ObservationSourceId::new("fixture/headless-recorded"),
        session: reference_session_id(),
        previous: ObservationSequence::new(2),
        received: ObservationSequence::new(2),
    }
}

fn verify_proof(run: &ProofRun) -> Result<(), String> {
    let anomaly = expected_anomaly();
    let expected_snapshots = [
        LifecycleSnapshot {
            lifecycle: HostLifecycle::Starting,
            readiness: Readiness::NotReady,
        },
        LifecycleSnapshot {
            lifecycle: HostLifecycle::Synchronizing,
            readiness: Readiness::NotReady,
        },
        LifecycleSnapshot {
            lifecycle: HostLifecycle::ReadyReadOnly,
            readiness: Readiness::ReadOnly,
        },
        LifecycleSnapshot {
            lifecycle: HostLifecycle::Degraded(DegradedReason::ObservationAnomaly(anomaly.clone())),
            readiness: Readiness::NotReady,
        },
    ];
    if run.snapshots != expected_snapshots {
        return Err("headless lifecycle/readiness transitions did not match the proof".into());
    }
    if run.latest_state.as_ref().map(|state| state.location)
        != Some(CarLocation::Continuous(Position::at_floor(Floor::new(4))))
    {
        return Err("headless host did not retain the last resolved read-only state".into());
    }
    if run.diagnostic_entries != 4 || run.incidents != 1 {
        return Err("headless host did not retain the expected diagnostic evidence".into());
    }
    if !run.events.iter().any(
        |event| matches!(event, HostEvent::ObservationRejected(observed) if observed == &anomaly),
    ) {
        return Err("headless host did not preserve the degradation trigger".into());
    }
    Ok(())
}

fn render_report(run: &ProofRun) -> String {
    let transitions = run
        .snapshots
        .iter()
        .map(|snapshot| format!("{:?}/{:?}", snapshot.lifecycle, snapshot.readiness))
        .collect::<Vec<_>>()
        .join(" -> ");
    let anomaly = run.events.iter().find_map(|event| match event {
        HostEvent::ObservationRejected(anomaly) => Some(anomaly),
        _ => None,
    });
    [
        format!("scenario: {LIFECYCLE_SCENARIO}"),
        "scope: internal headless read-only proof; no passenger write or deployment claim".into(),
        "determinism: PASS (2 identical headless executions)".into(),
        format!("lifecycle/readiness: PASS ({transitions})"),
        "process alive != ready: PASS (Starting and Degraded are NotReady)".into(),
        "write boundary: PASS (recorded source exposes observations only; no IntentSink implementation or simulator dependency)".into(),
        format!(
            "engine diagnostics: PASS ({} entries, {} incident, trigger={anomaly:?})",
            run.diagnostic_entries, run.incidents
        ),
        "outcome: PASS (partial observation synchronized, complete state became read-only ready, duplicate sequence revoked readiness)".into(),
        String::new(),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_started_process_is_not_ready_before_synchronization() {
        let host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());

        assert_eq!(host.lifecycle(), &HostLifecycle::Starting);
        assert_eq!(host.readiness(), Readiness::NotReady);
    }

    #[test]
    fn recorded_observations_drive_read_only_readiness_and_degradation() {
        let run = execute_proof();

        verify_proof(&run).expect("reference proof should satisfy lifecycle invariants");
        assert!(run.events.iter().any(|event| matches!(
            event,
            HostEvent::ObservationUnresolved { reasons, .. }
                if reasons == &[UnresolvedCarStateReason::DoorDirectionUnknown]
        )));
        assert!(run.events.iter().any(|event| matches!(
            event,
            HostEvent::StateUpdated(state)
                if state.location
                    == CarLocation::Continuous(Position::at_floor(Floor::new(4)))
        )));
    }

    #[test]
    fn executable_proof_is_deterministic_and_reports_no_write_boundary() {
        let report = run_scenario(LIFECYCLE_SCENARIO).expect("reference scenario should pass");

        assert!(report.contains("determinism: PASS"));
        assert!(report.contains("process alive != ready: PASS"));
        assert!(report.contains("no IntentSink implementation or simulator dependency"));
        assert!(report.contains("duplicate sequence revoked readiness"));
    }

    #[test]
    fn unknown_scenario_is_rejected() {
        assert!(
            run_scenario("unknown")
                .unwrap_err()
                .contains("unknown scenario")
        );
    }

    #[test]
    fn proof_diagnostics_preserve_machine_readable_anomaly() {
        let mut source = reference_source();
        let mut host = reference_host();
        while host.poll(&mut source) {}

        assert!(host.engine.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            DiagnosticEvent::Anomaly(anomaly) if anomaly == &expected_anomaly()
        )));
    }

    #[test]
    fn new_incidents_still_revoke_readiness_after_incident_retention_is_full() {
        let complete = complete_observation(
            reference_source_id(),
            reference_session_id(),
            2,
            Floor::new(4),
        );
        let mut frames = vec![observation(1), complete.clone()];
        frames.extend(std::iter::repeat_n(complete, 10));
        let mut source = RecordedObservationFeed::new(frames);
        let mut host = reference_host();

        while host.poll(&mut source) {}

        assert_eq!(host.engine.diagnostics().incidents().count(), 8);
        assert_eq!(
            host.events
                .iter()
                .filter(|event| matches!(event, HostEvent::ObservationRejected(_)))
                .count(),
            10
        );
        assert!(matches!(host.lifecycle(), HostLifecycle::Degraded(_)));
        assert_eq!(host.readiness(), Readiness::NotReady);
    }

    #[test]
    fn operational_session_reset_revokes_readiness_until_new_session_state_arrives() {
        let mut host = reference_host();
        let mut initial = RecordedObservationFeed::new([
            observation(1),
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                2,
                Floor::new(4),
            ),
        ]);
        while host.poll(&mut initial) {}
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let next_session = ObservationSessionId::new("headless-fixture-session-B");
        host.begin_source_session(reference_source_id(), next_session.clone())
            .expect("operational source explicitly starts a new session");
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.engine.car_states().is_empty());

        let mut resumed = RecordedObservationFeed::new([complete_observation(
            reference_source_id(),
            next_session,
            1,
            Floor::new(4),
        )]);
        assert!(host.poll(&mut resumed));
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
    }

    #[test]
    fn auxiliary_sequence_anomaly_is_visible_without_revoking_operational_readiness() {
        let mut host = reference_host();
        let mut initial = RecordedObservationFeed::new([complete_observation(
            reference_source_id(),
            reference_session_id(),
            1,
            Floor::new(4),
        )]);
        assert!(host.poll(&mut initial));
        let operational = host.engine.car_states();
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let auxiliary_source = ObservationSourceId::new("fixture/auxiliary-rmf");
        let auxiliary_session = ObservationSessionId::new("auxiliary-session-A");
        host.engine
            .bind_observation(
                auxiliary_source.clone(),
                reference_car_id(),
                ObservationBindingRole::Auxiliary,
            )
            .expect("auxiliary binding is explicit");
        host.begin_source_session(auxiliary_source.clone(), auxiliary_session.clone())
            .expect("auxiliary source starts explicitly");
        let auxiliary = complete_observation(
            auxiliary_source.clone(),
            auxiliary_session.clone(),
            1,
            Floor::new(1),
        );
        let mut feed = RecordedObservationFeed::new([auxiliary.clone(), auxiliary]);
        assert!(host.poll(&mut feed));
        assert!(host.poll(&mut feed));

        let anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source: auxiliary_source,
            session: auxiliary_session,
            previous: ObservationSequence::new(1),
            received: ObservationSequence::new(1),
        };
        assert!(host.engine.diagnostics().entries().any(|entry| {
            matches!(&entry.event, DiagnosticEvent::Anomaly(observed) if observed == &anomaly)
        }));
        assert!(
            host.events
                .contains(&HostEvent::ObservationRejected(anomaly))
        );
        assert_eq!(host.engine.car_states(), operational);
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
    }
}
