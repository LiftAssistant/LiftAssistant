//! Executable developer scenarios; internal and intentionally not a public file format.

use std::{collections::BTreeMap, time::Duration};

use liftassistant_engine::Engine;
use liftassistant_model::{
    BuildingTopology, CallId, CallState, Capabilities, CarId, CarState, Direction, DoorEvidence,
    DoorState, Floor, HallCall, MotionEvidence, MotionState, ObservationProvenance,
    ObservationSequence, ObservationSourceId, OperatingMode, OperatingModeEvidence, Position,
    PositionEvidence,
};
use liftassistant_runtime::{
    CarStateResolution, ObservationSource, RuntimeEvent, SimulationDriver, UnresolvedCarStateReason,
};
use liftassistant_sim::{
    MappedRmfObservation, RecordedFrame, RecordedRuntime, RmfLiftState, map_rmf_lift_state,
};

use crate::{
    app::{call_state_label, direction_name, event_label},
    replay::{Command, Replay, SemanticEvent, run_with_topology},
};

pub const REFERENCE_SCENARIO: &str = "m2-call-lifecycle";
pub const REJECTION_SCENARIO: &str = "intent-rejection-recovery";
pub const RECORDED_SCENARIO: &str = "recorded-read-only-observation";
pub const RMF_RECORDED_SCENARIO: &str = "rmf-recorded-read-only";
pub const SCENARIOS: [&str; 4] = [
    REFERENCE_SCENARIO,
    REJECTION_SCENARIO,
    RECORDED_SCENARIO,
    RMF_RECORDED_SCENARIO,
];

pub fn run(name: &str) -> Result<String, String> {
    match name {
        REFERENCE_SCENARIO => run_reference_scenario(),
        REJECTION_SCENARIO => run_rejection_scenario(),
        RECORDED_SCENARIO => run_recorded_scenario(),
        RMF_RECORDED_SCENARIO => run_rmf_recorded_scenario(),
        _ => Err(format!(
            "unknown scenario {name:?}; available scenarios: {}",
            SCENARIOS.join(", ")
        )),
    }
}

#[derive(Debug, PartialEq)]
struct RecordedBatch {
    elapsed_total: Duration,
    events: Vec<RuntimeEvent>,
    states: Vec<CarState>,
}

#[derive(Debug, PartialEq)]
struct RecordedPlayback {
    source_name: String,
    capabilities: Capabilities,
    batches: Vec<RecordedBatch>,
    remaining_frames: usize,
}

#[derive(Debug, PartialEq)]
struct RmfRecordedExperiment {
    incomplete: MappedRmfObservation,
    incomplete_resolution: CarStateResolution,
    final_observation: MappedRmfObservation,
    final_resolution: CarStateResolution,
    source_specific_mode: MappedRmfObservation,
    source_specific_mode_resolution: CarStateResolution,
    final_states: Vec<CarState>,
}

fn run_reference_scenario() -> Result<String, String> {
    let commands = reference_commands();
    let first = run_with_topology(&commands, BuildingTopology::reference_eight_floor());
    let second = run_with_topology(&commands, BuildingTopology::reference_eight_floor());
    verify_determinism(&first, &second)?;
    verify_reference_outcome(&first)?;
    Ok(render_report(
        REFERENCE_SCENARIO,
        "3 calls completed",
        &commands,
        &first,
    ))
}

fn run_rejection_scenario() -> Result<String, String> {
    let commands = rejection_commands();
    let first = run_with_topology(&commands, BuildingTopology::reference_eight_floor());
    let second = run_with_topology(&commands, BuildingTopology::reference_eight_floor());
    verify_determinism(&first, &second)?;
    verify_rejection_outcome(&first)?;
    Ok(render_report(
        REJECTION_SCENARIO,
        "one injected rejection recovered; call #1 completed",
        &commands,
        &first,
    ))
}

fn run_recorded_scenario() -> Result<String, String> {
    let first = play_recording()?;
    let second = play_recording()?;
    if first != second {
        return Err("recorded runtime produced different observations on replay".into());
    }
    verify_recorded_outcome(&first)?;
    Ok(render_recorded_report(&first))
}

fn run_rmf_recorded_scenario() -> Result<String, String> {
    let first = run_rmf_experiment()?;
    let second = run_rmf_experiment()?;
    if first != second {
        return Err("Open-RMF recorded mapping produced different results on replay".into());
    }
    verify_rmf_experiment(&first)?;
    Ok(render_rmf_report(&first))
}

fn reference_commands() -> Vec<Command> {
    let mut commands = vec![
        Command::HallCall(HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        }),
        Command::HallCall(HallCall {
            floor: Floor::new(5),
            direction: Direction::Down,
        }),
        Command::HallCall(HallCall {
            floor: Floor::new(2),
            direction: Direction::Up,
        }),
    ];
    commands.extend(std::iter::repeat_n(Command::Step, 13));
    commands
}

fn rejection_commands() -> Vec<Command> {
    let call = Command::HallCall(HallCall {
        floor: Floor::new(7),
        direction: Direction::Up,
    });
    let mut commands = vec![Command::RejectNextIntent, call.clone(), call];
    commands.extend(std::iter::repeat_n(Command::Step, 6));
    commands
}

fn play_recording() -> Result<RecordedPlayback, String> {
    let mut runtime = recorded_runtime()?;
    let source_name = runtime.source_name().to_owned();
    let capabilities = runtime
        .capabilities(&CarId::new("A"))
        .ok_or_else(|| "recorded car A has no capabilities".to_owned())?;

    let mut elapsed_total = Duration::ZERO;
    let mut batches = Vec::new();
    for elapsed in [
        Duration::from_millis(250),
        Duration::from_millis(250),
        Duration::from_millis(499),
        Duration::from_millis(1),
    ] {
        elapsed_total += elapsed;
        batches.push(RecordedBatch {
            elapsed_total,
            events: runtime.advance(elapsed),
            states: runtime.car_states(),
        });
    }
    Ok(RecordedPlayback {
        source_name,
        capabilities,
        batches,
        remaining_frames: runtime.remaining_frames(),
    })
}

fn recorded_runtime() -> Result<RecordedRuntime, String> {
    let moving = CarState {
        id: CarId::new("A"),
        position: Position {
            floor_position: 3.5,
            nearest_floor: Floor::new(4),
        },
        motion: MotionState::Moving(Direction::Up),
        door: DoorState::Closed,
        mode: OperatingMode::Normal,
        target: Some(Floor::new(4)),
    };
    RecordedRuntime::new(
        [CarState::new("A", Floor::new(3))],
        [
            RecordedFrame::after(
                Duration::from_millis(500),
                vec![RuntimeEvent::PositionChanged(moving)],
            ),
            RecordedFrame::after(
                Duration::from_millis(500),
                vec![
                    RuntimeEvent::Arrived {
                        car: CarId::new("A"),
                        floor: Floor::new(4),
                    },
                    RuntimeEvent::DoorStateChanged {
                        car: CarId::new("A"),
                        state: DoorState::Open,
                    },
                ],
            ),
        ],
    )
    .map_err(|error| error.to_string())
}

fn run_rmf_experiment() -> Result<RmfRecordedExperiment, String> {
    let labels = rmf_floor_labels();
    let incomplete_source = rmf_source_state();
    let incomplete = map_rmf_lift_state(&incomplete_source, &labels, rmf_provenance(1))
        .map_err(|error| error.to_string())?;
    let engine = Engine::new(incomplete.topology.clone());
    let incomplete_resolution = engine.resolve_observation(&incomplete.observation);

    let mut final_source = incomplete_source.clone();
    final_source.current_floor = "L4".into();
    final_source.destination_floor.clear();
    final_source.door_state = RmfLiftState::DOOR_OPEN;
    final_source.motion_state = RmfLiftState::MOTION_STOPPED;
    let final_observation = map_rmf_lift_state(&final_source, &labels, rmf_provenance(2))
        .map_err(|error| error.to_string())?;
    let final_resolution = engine.resolve_observation(&final_observation.observation);
    let final_state = match &final_resolution {
        CarStateResolution::Resolved(state) => state.clone(),
        CarStateResolution::Unresolved { reasons, .. } => {
            return Err(format!(
                "sufficient Open-RMF observation stayed unresolved: {reasons:?}"
            ));
        }
    };

    let mut source_specific_mode_source = final_source;
    source_specific_mode_source.current_mode = RmfLiftState::MODE_AGV;
    let source_specific_mode =
        map_rmf_lift_state(&source_specific_mode_source, &labels, rmf_provenance(3))
            .map_err(|error| error.to_string())?;
    let source_specific_mode_resolution =
        engine.resolve_observation(&source_specific_mode.observation);

    let runtime =
        RecordedRuntime::new([final_state.clone()], []).map_err(|error| error.to_string())?;
    let final_states = runtime.car_states();

    Ok(RmfRecordedExperiment {
        incomplete,
        incomplete_resolution,
        final_observation,
        final_resolution,
        source_specific_mode,
        source_specific_mode_resolution,
        final_states,
    })
}

fn rmf_provenance(sequence: u64) -> ObservationProvenance {
    ObservationProvenance {
        source: ObservationSourceId::new("fixture/open-rmf"),
        sequence: ObservationSequence::new(sequence),
    }
}

fn rmf_floor_labels() -> BTreeMap<String, Floor> {
    [("L1", 1), ("L2", 2), ("L3", 3), ("L4", 4)]
        .into_iter()
        .map(|(label, floor)| (label.to_owned(), Floor::new(floor)))
        .collect()
}

fn rmf_source_state() -> RmfLiftState {
    RmfLiftState {
        lift_name: "lift-A".into(),
        available_floors: vec!["L1".into(), "L2".into(), "L3".into(), "L4".into()],
        current_floor: "L3".into(),
        destination_floor: "L4".into(),
        door_state: RmfLiftState::DOOR_MOVING,
        motion_state: RmfLiftState::MOTION_UP,
        available_modes: vec![RmfLiftState::MODE_HUMAN, RmfLiftState::MODE_FIRE],
        current_mode: RmfLiftState::MODE_FIRE,
        session_id: "recorded-session".into(),
    }
}

fn verify_determinism(first: &Replay, second: &Replay) -> Result<(), String> {
    if first.events != second.events
        || first.app.runtime.car_states() != second.app.runtime.car_states()
        || first.app.steps != second.app.steps
        || first.app.engine != second.app.engine
        || first.app.recording != second.app.recording
    {
        return Err("scenario produced different semantic or physical results on replay".into());
    }
    Ok(())
}

fn verify_reference_outcome(replay: &Replay) -> Result<(), String> {
    let expected_calls = [
        (
            CallId::new(1),
            HallCall {
                floor: Floor::new(7),
                direction: Direction::Up,
            },
        ),
        (
            CallId::new(2),
            HallCall {
                floor: Floor::new(5),
                direction: Direction::Down,
            },
        ),
        (
            CallId::new(3),
            HallCall {
                floor: Floor::new(2),
                direction: Direction::Up,
            },
        ),
    ];
    if replay.app.engine.calls().calls().count() != expected_calls.len()
        || expected_calls.iter().any(|(id, call)| {
            replay
                .app
                .engine
                .calls()
                .get(*id)
                .is_none_or(|record| record.call != *call || record.state != CallState::Completed)
        })
    {
        return Err("reference scenario did not complete the three expected calls".into());
    }
    if replay
        .events
        .iter()
        .any(|event| matches!(event, SemanticEvent::RuntimeError(_)))
    {
        return Err("reference scenario emitted a runtime error".into());
    }
    if replay.app.steps != 13 {
        return Err(format!(
            "reference scenario executed {} steps instead of 13",
            replay.app.steps
        ));
    }

    let states = replay.app.runtime.car_states();
    let expected_positions = [("A", Floor::new(2)), ("B", Floor::new(5))];
    if states.len() != expected_positions.len()
        || expected_positions.iter().any(|(id, floor)| {
            states
                .iter()
                .find(|state| state.id.as_str() == *id)
                .is_none_or(|state| {
                    state.position.floor_position != f64::from(floor.index())
                        || state.position.nearest_floor != *floor
                        || state.motion != MotionState::Stopped
                        || state.door != DoorState::Open
                        || state.target.is_some()
                })
        })
    {
        return Err("reference scenario ended in an unexpected physical state".into());
    }
    Ok(())
}

fn verify_rejection_outcome(replay: &Replay) -> Result<(), String> {
    let expected_call = HallCall {
        floor: Floor::new(7),
        direction: Direction::Up,
    };
    if replay.app.engine.calls().calls().count() != 1
        || replay
            .app
            .engine
            .calls()
            .get(CallId::new(1))
            .is_none_or(|record| {
                record.call != expected_call || record.state != CallState::Completed
            })
    {
        return Err("rejection scenario did not recover and complete call #1".into());
    }
    let runtime_errors: Vec<_> = replay
        .events
        .iter()
        .filter_map(|event| match event {
            SemanticEvent::RuntimeError(error) => Some(error.as_str()),
            _ => None,
        })
        .collect();
    if runtime_errors != ["simulator injected one-shot intent rejection"] {
        return Err(format!(
            "rejection scenario emitted unexpected runtime errors: {runtime_errors:?}"
        ));
    }
    let Some(rejection_index) = replay
        .events
        .iter()
        .position(|event| matches!(event, SemanticEvent::RuntimeError(_)))
    else {
        return Err("rejection scenario did not trace the injected rejection".into());
    };
    let Some(retry_index) = replay
        .events
        .iter()
        .position(|event| *event == SemanticEvent::CallDeduplicated(CallId::new(1)))
    else {
        return Err("rejection scenario did not retry the same call identity".into());
    };
    if !replay
        .events
        .contains(&SemanticEvent::SimulatorIntentRejectionArmed)
        || rejection_index >= retry_index
        || replay.events[..retry_index]
            .iter()
            .any(|event| matches!(event, SemanticEvent::CallStateChanged { .. }))
    {
        return Err(
            "rejection scenario did not leave the call pending until its traced same-call retry"
                .into(),
        );
    }
    let states = replay.app.runtime.car_states();
    let recovered = states.iter().any(|state| {
        state.id.as_str() == "A"
            && state.position.nearest_floor == Floor::new(7)
            && state.position.floor_position == 7.0
            && state.motion == MotionState::Stopped
            && state.door == DoorState::Open
            && state.target.is_none()
    });
    if !recovered {
        return Err("rejection scenario ended before recovered service completed".into());
    }
    Ok(())
}

fn verify_recorded_outcome(playback: &RecordedPlayback) -> Result<(), String> {
    if playback.source_name != "recorded-read-only"
        || !playback.capabilities.observe_position
        || !playback.capabilities.observe_door
        || playback.capabilities.serve_floor
        || playback.capabilities.park
    {
        return Err("recorded runtime did not preserve its explicit read-only boundary".into());
    }
    if playback.batches.len() != 4
        || !playback.batches[0].events.is_empty()
        || playback.batches[1].events.len() != 1
        || !playback.batches[2].events.is_empty()
        || playback.batches[3].events.len() != 2
        || playback.remaining_frames != 0
    {
        return Err("recorded frames were not released at their explicit elapsed times".into());
    }
    let partial = &playback.batches[1].states;
    if partial.len() != 1
        || partial[0].position.floor_position != 3.5
        || partial[0].motion != MotionState::Moving(Direction::Up)
    {
        return Err("first recorded observation did not update the intermediate state".into());
    }
    let final_states = &playback.batches[3].states;
    if final_states.len() != 1
        || final_states[0].position != Position::at_floor(Floor::new(4))
        || final_states[0].motion != MotionState::Stopped
        || final_states[0].door != DoorState::Open
        || final_states[0].target.is_some()
    {
        return Err("recorded observation did not reach the expected final state".into());
    }
    Ok(())
}

fn verify_rmf_experiment(experiment: &RmfRecordedExperiment) -> Result<(), String> {
    if experiment.incomplete.observation.provenance != rmf_provenance(1)
        || experiment.incomplete.observation.car != CarId::new("lift-A")
        || experiment.incomplete.observation.position
            != PositionEvidence::FloorReference(Floor::new(3))
        || experiment.incomplete.observation.motion
            != MotionEvidence::Known(MotionState::Moving(Direction::Up))
        || experiment.incomplete.observation.door != DoorEvidence::MovingUnknownDirection
        || experiment.incomplete.observation.mode
            != OperatingModeEvidence::Known(OperatingMode::FireService)
        || experiment.incomplete.observation.target != Some(Floor::new(4))
        || experiment.incomplete.source_context.current_floor_label != "L3"
        || experiment.incomplete.source_context.source_session_id != "recorded-session"
        || experiment.incomplete.topology.served_floors()
            != [Floor::new(1), Floor::new(2), Floor::new(3), Floor::new(4)]
    {
        return Err("incomplete Open-RMF fields did not preserve their exact evidence".into());
    }
    if experiment.incomplete_resolution
        != (CarStateResolution::Unresolved {
            car: CarId::new("lift-A"),
            reasons: vec![
                UnresolvedCarStateReason::ExactPositionUnavailable,
                UnresolvedCarStateReason::DoorDirectionUnknown,
            ],
        })
    {
        return Err("incomplete Open-RMF evidence was forced into operational state".into());
    }

    let expected_final = CarState {
        id: CarId::new("lift-A"),
        position: Position::at_floor(Floor::new(4)),
        motion: MotionState::Stopped,
        door: DoorState::Open,
        mode: OperatingMode::FireService,
        target: None,
    };
    if experiment.final_observation.observation.provenance != rmf_provenance(2)
        || experiment.final_observation.observation.position
            != PositionEvidence::FloorReference(Floor::new(4))
        || experiment.final_resolution != CarStateResolution::Resolved(expected_final.clone())
    {
        return Err("sufficient Open-RMF evidence did not resolve to expected state".into());
    }
    if experiment.source_specific_mode.observation.provenance != rmf_provenance(3)
        || experiment.source_specific_mode.observation.mode != OperatingModeEvidence::SourceSpecific
        || experiment.source_specific_mode.source_context.source_mode != RmfLiftState::MODE_AGV
        || experiment.source_specific_mode_resolution
            != (CarStateResolution::Unresolved {
                car: CarId::new("lift-A"),
                reasons: vec![UnresolvedCarStateReason::OperatingModeSourceSpecific],
            })
    {
        return Err("Open-RMF source mode leaked into operational mode semantics".into());
    }
    if experiment.final_states != [expected_final] {
        return Err(
            "resolved Open-RMF state did not preserve the no-write runtime boundary".into(),
        );
    }
    Ok(())
}

fn render_report(name: &str, outcome: &str, commands: &[Command], replay: &Replay) -> String {
    let mut lines = vec![
        format!("scenario: {name}"),
        "scope: developer-only simulator research; not conformance or certification".into(),
        "determinism: PASS (2 identical executions)".into(),
        format!("outcome: PASS ({outcome})"),
        format!("commands: {}", commands.len()),
        format!("semantic events: {}", replay.events.len()),
        String::new(),
        "trace:".into(),
    ];
    lines.extend(replay.app.trace.iter().map(|line| format!("  {line}")));
    lines.push(String::new());
    lines.push("final calls:".into());
    lines.extend(replay.app.engine.calls().calls().map(|record| {
        format!(
            "  {} {} {} -> {}",
            record.id,
            record.call.floor,
            direction_name(record.call.direction),
            call_state_label(&record.state)
        )
    }));
    lines.push(String::new());
    lines.push("final cars:".into());
    lines.extend(replay.app.runtime.car_states().into_iter().map(|state| {
        let target = state
            .target
            .map_or_else(|| "-".into(), |floor| floor.to_string());
        format!(
            "  {} position={:.2}F nearest={} motion={:?} door={:?} mode={:?} target={target}",
            state.id,
            state.position.floor_position,
            state.position.nearest_floor,
            state.motion,
            state.door,
            state.mode
        )
    }));
    lines.push(String::new());
    lines.join("\n")
}

fn render_recorded_report(playback: &RecordedPlayback) -> String {
    let mut lines = vec![
        format!("scenario: {RECORDED_SCENARIO}"),
        "scope: native semantic recording; read-only; no protocol or hardware I/O".into(),
        "determinism: PASS (2 identical executions)".into(),
        "write boundary: PASS (recorded source has no IntentSink implementation)".into(),
        "outcome: PASS (2 frames observed at explicit elapsed times)".into(),
        String::new(),
        "timeline:".into(),
    ];
    for batch in &playback.batches {
        let milliseconds = batch.elapsed_total.as_millis();
        if batch.events.is_empty() {
            lines.push(format!("  [{milliseconds:04} ms] no events"));
        } else {
            lines.extend(
                batch
                    .events
                    .iter()
                    .map(|event| format!("  [{milliseconds:04} ms] {}", event_label(event))),
            );
        }
        lines.extend(batch.states.iter().map(|state| {
            format!(
                "             state {} position={:.2}F motion={:?} door={:?} target={}",
                state.id,
                state.position.floor_position,
                state.motion,
                state.door,
                state
                    .target
                    .map_or_else(|| "-".into(), |floor| floor.to_string())
            )
        }));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_rmf_report(experiment: &RmfRecordedExperiment) -> String {
    let incomplete = &experiment.incomplete.observation;
    let CarStateResolution::Resolved(final_state) = &experiment.final_resolution else {
        unreachable!("verified experiment has a resolved final observation")
    };
    [
        format!("scenario: {RMF_RECORDED_SCENARIO}"),
        "source: recorded Open-RMF LiftState fields; no ROS or network connection".into(),
        "determinism: PASS (2 identical mappings and resolutions)".into(),
        "write boundary: PASS (recorded source has no IntentSink implementation)".into(),
        format!(
            "incomplete observation: source={} sequence={} car={} position={:?} motion={:?} door={:?} mode={:?} target={} session={}",
            incomplete.provenance.source,
            incomplete.provenance.sequence.value(),
            incomplete.car,
            incomplete.position,
            incomplete.motion,
            incomplete.door,
            incomplete.mode,
            incomplete
                .target
                .map_or_else(|| "-".into(), |floor| floor.to_string()),
            experiment.incomplete.source_context.source_session_id
        ),
        format!(
            "incomplete resolution: {:?}",
            experiment.incomplete_resolution
        ),
        format!(
            "resolved final: sequence={} {} position={} motion={:?} door={:?} mode={:?} target=-",
            experiment
                .final_observation
                .observation
                .provenance
                .sequence
                .value(),
            final_state.id,
            final_state.position.nearest_floor,
            final_state.motion,
            final_state.door,
            final_state.mode
        ),
        format!(
            "source-specific mode: raw={} canonical={:?} resolution={:?}",
            experiment.source_specific_mode.source_context.source_mode,
            experiment.source_specific_mode.observation.mode,
            experiment.source_specific_mode_resolution
        ),
        "outcome: PASS (partial evidence preserved; only sufficient evidence became CarState)".into(),
        String::new(),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_scenario_verifies_and_reports_trace_and_final_state() {
        let report = run(REFERENCE_SCENARIO).expect("reference scenario should pass");

        assert!(report.contains("determinism: PASS (2 identical executions)"));
        assert!(report.contains("outcome: PASS (3 calls completed)"));
        assert!(report.contains("Call #3 -> Completed"));
        assert!(report.contains("#3 2F UP -> Completed"));
        assert!(report.contains("A position=2.00F"));
        assert!(report.contains("B position=5.00F"));
    }

    #[test]
    fn rejection_scenario_traces_one_failure_and_deterministic_recovery() {
        let report = run(REJECTION_SCENARIO).expect("rejection scenario should recover");

        assert!(report.contains("determinism: PASS (2 identical executions)"));
        assert!(
            report.contains("outcome: PASS (one injected rejection recovered; call #1 completed)")
        );
        assert!(report.contains("Simulator armed: reject next intent once"));
        assert!(report.contains("Runtime error: simulator injected one-shot intent rejection"));
        assert!(report.contains("Active duplicate -> #1"));
        assert!(report.contains("#1 7F UP -> Completed"));
    }

    #[test]
    fn recorded_scenario_proves_timing_state_and_no_write_boundary() {
        let report = run(RECORDED_SCENARIO).expect("recorded scenario should pass");

        assert!(report.contains("determinism: PASS (2 identical executions)"));
        assert!(
            report.contains(
                "write boundary: PASS (recorded source has no IntentSink implementation)"
            )
        );
        assert!(report.contains("[0250 ms] no events"));
        assert!(report.contains("[0500 ms] PositionChanged A 3.50F"));
        assert!(report.contains("[0999 ms] no events"));
        assert!(report.contains("[1000 ms] Arrived A at 4F"));
        assert!(report.contains("state A position=4.00F motion=Stopped door=Open target=-"));
    }

    #[test]
    fn rmf_recorded_scenario_preserves_partial_evidence_before_resolution() {
        let report = run(RMF_RECORDED_SCENARIO).expect("recorded RMF scenario should pass");

        assert!(report.contains("determinism: PASS (2 identical mappings and resolutions)"));
        assert!(report.contains("source=fixture/open-rmf sequence=1 car=lift-A"));
        assert!(report.contains("position=FloorReference(Floor(3))"));
        assert!(report.contains("motion=Known(Moving(Up)) door=MovingUnknownDirection"));
        assert!(report.contains("reasons: [ExactPositionUnavailable, DoorDirectionUnknown]"));
        assert!(report.contains("resolved final: sequence=2 lift-A position=4F motion=Stopped"));
        assert!(report.contains("source-specific mode: raw=2 canonical=SourceSpecific"));
        assert!(report.contains("only sufficient evidence became CarState"));
    }

    #[test]
    fn unknown_scenario_is_rejected_without_starting_the_tui() {
        assert_eq!(
            run("missing"),
            Err(format!(
                "unknown scenario \"missing\"; available scenarios: {}",
                SCENARIOS.join(", ")
            ))
        );
    }
}
