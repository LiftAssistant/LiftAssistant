#![forbid(unsafe_code)]

use std::{collections::VecDeque, env, process::ExitCode};

use liftassistant_engine::{DiagnosticAnomaly, DiagnosticEvent, Engine};
use liftassistant_model::{
    BuildingTopology, CarId, CarObservation, DoorEvidence, DoorState, Floor, MotionEvidence,
    MotionState, ObservationProvenance, ObservationSequence, ObservationSourceId, OperatingMode,
    OperatingModeEvidence, Position, PositionEvidence,
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
}

/// Internal in-memory source for the headless proof. It exposes observations only.
#[derive(Debug)]
struct RecordedObservationSource {
    frames: VecDeque<CarObservation>,
}

impl RecordedObservationSource {
    fn new(frames: impl IntoIterator<Item = CarObservation>) -> Self {
        Self {
            frames: frames.into_iter().collect(),
        }
    }

    fn next_observation(&mut self) -> Option<CarObservation> {
        self.frames.pop_front()
    }
}

#[derive(Debug)]
struct ReadOnlyHost {
    engine: Engine,
    lifecycle: HostLifecycle,
    latest_state: Option<liftassistant_model::CarState>,
    events: Vec<HostEvent>,
}

impl ReadOnlyHost {
    fn new(topology: BuildingTopology) -> Self {
        Self {
            engine: Engine::new(topology),
            lifecycle: HostLifecycle::Starting,
            latest_state: None,
            events: vec![HostEvent::LifecycleChanged(HostLifecycle::Starting)],
        }
    }

    fn lifecycle(&self) -> &HostLifecycle {
        &self.lifecycle
    }

    fn readiness(&self) -> Readiness {
        self.lifecycle.readiness()
    }

    fn poll(&mut self, source: &mut RecordedObservationSource) -> bool {
        let Some(observation) = source.next_observation() else {
            return false;
        };

        if matches!(
            self.lifecycle,
            HostLifecycle::Starting | HostLifecycle::Degraded(_)
        ) {
            self.transition(HostLifecycle::Synchronizing);
        }

        let incident_marker = latest_incident_marker(&self.engine);
        let resolution = self.engine.resolve_observation(&observation);
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
            self.transition(HostLifecycle::Degraded(DegradedReason::ObservationAnomaly(
                anomaly,
            )));
            return true;
        }

        match resolution {
            CarStateResolution::Resolved(state) => {
                self.latest_state = Some(state.clone());
                self.events.push(HostEvent::StateUpdated(state));
                self.transition(HostLifecycle::ReadyReadOnly);
            }
            CarStateResolution::Unresolved { car, reasons } => {
                self.events
                    .push(HostEvent::ObservationUnresolved { car, reasons });
                if !matches!(self.lifecycle, HostLifecycle::Synchronizing) {
                    self.transition(HostLifecycle::Synchronizing);
                }
            }
        }
        true
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
    let mut host = ReadOnlyHost::new(
        BuildingTopology::new([1, 2, 3, 4].map(Floor::new))
            .expect("reference topology is non-empty"),
    );
    let mut snapshots = vec![snapshot(&host)];
    while host.poll(&mut source) {
        snapshots.push(snapshot(&host));
    }

    ProofRun {
        snapshots,
        events: host.events.clone(),
        latest_state: host.latest_state.clone(),
        diagnostic_entries: host.engine.diagnostics().entries().count(),
        incidents: host.engine.diagnostics().incidents().count(),
    }
}

fn snapshot(host: &ReadOnlyHost) -> LifecycleSnapshot {
    LifecycleSnapshot {
        lifecycle: host.lifecycle().clone(),
        readiness: host.readiness(),
    }
}

fn reference_source() -> RecordedObservationSource {
    let mut complete = observation(2);
    complete.position = PositionEvidence::Exact(Position::at_floor(Floor::new(4)));
    complete.motion = MotionEvidence::Known(MotionState::Stopped);
    complete.door = DoorEvidence::Known(DoorState::Open);
    complete.target = None;

    let duplicate = complete.clone();
    RecordedObservationSource::new([observation(1), complete, duplicate])
}

fn observation(sequence: u64) -> CarObservation {
    CarObservation {
        provenance: ObservationProvenance {
            source: ObservationSourceId::new("fixture/headless-recorded"),
            sequence: ObservationSequence::new(sequence),
        },
        car: CarId::new("lift-A"),
        position: PositionEvidence::FloorReference(Floor::new(3)),
        motion: MotionEvidence::Known(MotionState::Moving(liftassistant_model::Direction::Up)),
        door: DoorEvidence::MovingUnknownDirection,
        mode: OperatingModeEvidence::Known(OperatingMode::FireService),
        target: Some(Floor::new(4)),
    }
}

fn expected_anomaly() -> DiagnosticAnomaly {
    DiagnosticAnomaly::ObservationSequenceNotIncreasing {
        source: ObservationSourceId::new("fixture/headless-recorded"),
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
    if run.latest_state.as_ref().map(|state| state.position)
        != Some(Position::at_floor(Floor::new(4)))
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
                if reasons == &[
                    UnresolvedCarStateReason::ExactPositionUnavailable,
                    UnresolvedCarStateReason::DoorDirectionUnknown,
                ]
        )));
        assert!(run.events.iter().any(|event| matches!(
            event,
            HostEvent::StateUpdated(state)
                if state.position == Position::at_floor(Floor::new(4))
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
        let mut host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());
        while host.poll(&mut source) {}

        assert!(host.engine.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            DiagnosticEvent::Anomaly(anomaly) if anomaly == &expected_anomaly()
        )));
    }

    #[test]
    fn new_incidents_still_revoke_readiness_after_incident_retention_is_full() {
        let mut complete = observation(2);
        complete.position = PositionEvidence::Exact(Position::at_floor(Floor::new(4)));
        complete.motion = MotionEvidence::Known(MotionState::Stopped);
        complete.door = DoorEvidence::Known(DoorState::Open);
        complete.target = None;
        let mut frames = vec![observation(1), complete.clone()];
        frames.extend(std::iter::repeat_n(complete, 10));
        let mut source = RecordedObservationSource::new(frames);
        let mut host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());

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
}
