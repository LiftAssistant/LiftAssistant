//! Executable developer scenarios; internal and intentionally not a public file format.

use std::{collections::BTreeMap, time::Duration};

use liftassistant_model::{
    BuildingTopology, CallId, CallState, Capabilities, CarId, CarState, Direction, DoorState,
    Floor, HallCall, Intent, MotionState, OperatingMode, Position,
};
use liftassistant_runtime::{ElevatorRuntime, RuntimeEvent};
use liftassistant_sim::{
    MappedRmfLiftState, RecordedFrame, RecordedRuntime, RmfLiftState, RmfMappingError,
    map_rmf_lift_state,
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
    runtime_name: String,
    write_error: String,
    capabilities: Capabilities,
    batches: Vec<RecordedBatch>,
    remaining_frames: usize,
}

#[derive(Debug, PartialEq)]
struct RmfRecordedExperiment {
    initial: MappedRmfLiftState,
    final_observation: MappedRmfLiftState,
    write_error: String,
    ambiguous_door_error: RmfMappingError,
    motion_error: RmfMappingError,
    control_mode_error: RmfMappingError,
    emitted: Vec<RuntimeEvent>,
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
    let runtime_name = runtime.runtime_name().to_owned();
    let capabilities = runtime
        .capabilities(&CarId::new("A"))
        .ok_or_else(|| "recorded car A has no capabilities".to_owned())?;
    let initial = runtime.car_states();
    let write_error = match runtime.submit_intent(Intent::ServeFloor {
        car: CarId::new("A"),
        floor: Floor::new(7),
    }) {
        Ok(()) => return Err("recorded runtime unexpectedly accepted an intent".into()),
        Err(error) => error.to_string(),
    };
    if runtime.car_states() != initial {
        return Err("recorded runtime changed state after rejecting an intent".into());
    }

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
            events: runtime.tick(elapsed),
            states: runtime.car_states(),
        });
    }
    Ok(RecordedPlayback {
        runtime_name,
        write_error,
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
    let initial_source = rmf_source_state();
    let initial =
        map_rmf_lift_state(&initial_source, &labels).map_err(|error| error.to_string())?;
    let mut final_source = initial_source.clone();
    final_source.current_floor = "L4".into();
    final_source.destination_floor.clear();
    final_source.door_state = RmfLiftState::DOOR_OPEN;
    final_source.motion_state = RmfLiftState::MOTION_STOPPED;
    let final_observation =
        map_rmf_lift_state(&final_source, &labels).map_err(|error| error.to_string())?;

    let mut ambiguous_door = initial_source.clone();
    ambiguous_door.door_state = RmfLiftState::DOOR_MOVING;
    let ambiguous_door_error = match map_rmf_lift_state(&ambiguous_door, &labels) {
        Ok(_) => return Err("Open-RMF DOOR_MOVING was guessed instead of rejected".into()),
        Err(error) => error,
    };
    let mut control_mode = initial_source;
    let mut moving = control_mode.clone();
    moving.motion_state = RmfLiftState::MOTION_UP;
    let motion_error = match map_rmf_lift_state(&moving, &labels) {
        Ok(_) => return Err("Open-RMF moving state was given fabricated precision".into()),
        Err(error) => error,
    };
    control_mode.current_mode = RmfLiftState::MODE_AGV;
    let control_mode_error = match map_rmf_lift_state(&control_mode, &labels) {
        Ok(_) => return Err("Open-RMF AGV control mode was guessed instead of rejected".into()),
        Err(error) => error,
    };

    let mut runtime = RecordedRuntime::new(
        [initial.car_state.clone()],
        [RecordedFrame::after(
            Duration::from_millis(500),
            vec![RuntimeEvent::PositionChanged(
                final_observation.car_state.clone(),
            )],
        )],
    )
    .map_err(|error| error.to_string())?;
    let write_error = match runtime.submit_intent(Intent::ServeFloor {
        car: initial.car_state.id.clone(),
        floor: Floor::new(1),
    }) {
        Ok(()) => return Err("recorded Open-RMF experiment accepted an intent".into()),
        Err(error) => error.to_string(),
    };
    let emitted = runtime.tick(Duration::from_millis(500));
    let final_states = runtime.car_states();

    Ok(RmfRecordedExperiment {
        initial,
        final_observation,
        write_error,
        ambiguous_door_error,
        motion_error,
        control_mode_error,
        emitted,
        final_states,
    })
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
        door_state: RmfLiftState::DOOR_OPEN,
        motion_state: RmfLiftState::MOTION_STOPPED,
        available_modes: vec![RmfLiftState::MODE_HUMAN, RmfLiftState::MODE_FIRE],
        current_mode: RmfLiftState::MODE_FIRE,
        session_id: "recorded-session".into(),
    }
}

fn verify_determinism(first: &Replay, second: &Replay) -> Result<(), String> {
    if first.events != second.events
        || first.app.runtime.car_states() != second.app.runtime.car_states()
        || first.app.steps != second.app.steps
        || first.app.last_call != second.app.last_call
        || first.app.decision != second.app.decision
        || first.app.calls != second.app.calls
        || first.app.topology != second.app.topology
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
    if replay.app.calls.calls().count() != expected_calls.len()
        || expected_calls.iter().any(|(id, call)| {
            replay
                .app
                .calls
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
    if replay.app.calls.calls().count() != 1
        || replay.app.calls.get(CallId::new(1)).is_none_or(|record| {
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
    if playback.runtime_name != "recorded-read-only"
        || playback.write_error != "recorded runtime is read-only and rejects all intents"
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
    if experiment.initial.car_state.id != CarId::new("lift-A")
        || experiment.initial.car_state.position != Position::at_floor(Floor::new(3))
        || experiment.initial.car_state.motion != MotionState::Stopped
        || experiment.initial.car_state.door != DoorState::Open
        || experiment.initial.car_state.mode != OperatingMode::FireService
        || experiment.initial.car_state.target != Some(Floor::new(4))
        || experiment.initial.source_session_id != "recorded-session"
        || experiment.initial.topology.served_floors()
            != [Floor::new(1), Floor::new(2), Floor::new(3), Floor::new(4)]
    {
        return Err("unambiguous Open-RMF fields did not map as expected".into());
    }
    if experiment.ambiguous_door_error != RmfMappingError::AmbiguousDoorMoving
        || experiment.motion_error
            != RmfMappingError::MotionLacksContinuousPosition(RmfLiftState::MOTION_UP)
        || experiment.control_mode_error
            != RmfMappingError::ControlModeIsNotOperatingMode(RmfLiftState::MODE_AGV)
    {
        return Err("ambiguous Open-RMF fields were not rejected explicitly".into());
    }
    if experiment.write_error != "recorded runtime is read-only and rejects all intents"
        || experiment.emitted
            != [RuntimeEvent::PositionChanged(
                experiment.final_observation.car_state.clone(),
            )]
        || experiment.final_states != [experiment.final_observation.car_state.clone()]
    {
        return Err("mapped Open-RMF recording did not preserve the no-write playback path".into());
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
    lines.extend(replay.app.calls.calls().map(|record| {
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
        format!("write boundary: PASS ({})", playback.write_error),
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
    let initial = &experiment.initial.car_state;
    let final_state = &experiment.final_observation.car_state;
    [
        format!("scenario: {RMF_RECORDED_SCENARIO}"),
        "source: recorded Open-RMF LiftState fields; no ROS or network connection".into(),
        "determinism: PASS (2 identical mappings and playbacks)".into(),
        format!("write boundary: PASS ({})", experiment.write_error),
        format!(
            "mapped initial: {} position={} motion={:?} door={:?} mode={:?} target={} session={}",
            initial.id,
            initial.position.nearest_floor,
            initial.motion,
            initial.door,
            initial.mode,
            initial
                .target
                .map_or_else(|| "-".into(), |floor| floor.to_string()),
            experiment.initial.source_session_id
        ),
        format!(
            "mapped final: {} position={} motion={:?} door={:?} mode={:?} target=-",
            final_state.id,
            final_state.position.nearest_floor,
            final_state.motion,
            final_state.door,
            final_state.mode
        ),
        format!(
            "rejected ambiguous door: {}",
            experiment.ambiguous_door_error
        ),
        format!("rejected imprecise motion: {}", experiment.motion_error),
        format!("rejected control mode: {}", experiment.control_mode_error),
        "outcome: PASS (unambiguous observations mapped; ambiguous semantics preserved as errors)"
            .into(),
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
        assert!(report.contains(
            "write boundary: PASS (recorded runtime is read-only and rejects all intents)"
        ));
        assert!(report.contains("[0250 ms] no events"));
        assert!(report.contains("[0500 ms] PositionChanged A 3.50F"));
        assert!(report.contains("[0999 ms] no events"));
        assert!(report.contains("[1000 ms] Arrived A at 4F"));
        assert!(report.contains("state A position=4.00F motion=Stopped door=Open target=-"));
    }

    #[test]
    fn rmf_recorded_scenario_maps_only_unambiguous_read_only_state() {
        let report = run(RMF_RECORDED_SCENARIO).expect("recorded RMF scenario should pass");

        assert!(report.contains("determinism: PASS (2 identical mappings and playbacks)"));
        assert!(report.contains("mapped initial: lift-A position=3F motion=Stopped door=Open"));
        assert!(report.contains("mode=FireService target=4F session=recorded-session"));
        assert!(report.contains("mapped final: lift-A position=4F motion=Stopped door=Open"));
        assert!(report.contains(
            "rejected ambiguous door: Open-RMF DOOR_MOVING does not distinguish opening from closing"
        ));
        assert!(report.contains(
            "rejected imprecise motion: Open-RMF motion_state 1 has no continuous position to map"
        ));
        assert!(report.contains(
            "rejected control mode: Open-RMF control mode 2 is not a LiftAssistant operating mode"
        ));
        assert!(report.contains("ambiguous semantics preserved as errors"));
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
