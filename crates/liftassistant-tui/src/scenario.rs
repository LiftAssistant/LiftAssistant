//! Executable developer scenarios; internal and intentionally not a public file format.

use liftassistant_model::{
    BuildingTopology, CallId, CallState, Direction, DoorState, Floor, HallCall, MotionState,
};
use liftassistant_runtime::ElevatorRuntime;

use crate::{
    app::{call_state_label, direction_name},
    replay::{Command, Replay, SemanticEvent, run_with_topology},
};

pub const REFERENCE_SCENARIO: &str = "m2-call-lifecycle";
pub const REJECTION_SCENARIO: &str = "intent-rejection-recovery";
pub const SCENARIOS: [&str; 2] = [REFERENCE_SCENARIO, REJECTION_SCENARIO];

pub fn run(name: &str) -> Result<String, String> {
    match name {
        REFERENCE_SCENARIO => run_reference_scenario(),
        REJECTION_SCENARIO => run_rejection_scenario(),
        _ => Err(format!(
            "unknown scenario {name:?}; available scenarios: {}",
            SCENARIOS.join(", ")
        )),
    }
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
