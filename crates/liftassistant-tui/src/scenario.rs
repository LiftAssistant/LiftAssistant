//! Executable developer scenarios; internal and intentionally not a public file format.

use std::{collections::BTreeMap, time::Duration};

use liftassistant_engine::{
    DiagnosticAnomaly, DiagnosticEntry, DiagnosticEvent, Engine, IncidentSnapshot,
    ObservationBindingRole, ObservationIngestResult,
};
use liftassistant_model::{
    BuildingTopology, CallId, CallState, CarId, CarLocation, CarObservation, CarObservationUpdate,
    CarState, Direction, DoorEvidence, DoorState, Floor, HallCall, MotionEvidence, MotionState,
    ObservationProvenance, ObservationSequence, ObservationSessionId, ObservationSourceId,
    OperatingMode, OperatingModeEvidence, Position, PositionEvidence, ReportedDestination,
};
use liftassistant_runtime::{
    CarStateResolution, RuntimeEvent, SimulationDriver, UnresolvedCarStateReason,
};
use liftassistant_sim::{
    BacnetDoorStatus, BacnetLiftCarDirection, BacnetLiftCarMode, MappedBacnetObservation,
    MappedRmfObservation, ObservationCoverage, RecordedBacnetLiftProperties, RecordedFrame,
    RecordedRuntime, RmfLiftState, map_bacnet_lift_properties, map_rmf_lift_state,
};

use crate::{
    app::{call_state_label, direction_name, event_label},
    replay::{Command, Replay, SemanticEvent, run_with_topology},
};

pub const REFERENCE_SCENARIO: &str = "m2-call-lifecycle";
pub const REJECTION_SCENARIO: &str = "intent-rejection-recovery";
pub const RECORDED_SCENARIO: &str = "recorded-read-only-observation";
pub const RMF_RECORDED_SCENARIO: &str = "rmf-recorded-read-only";
pub const BACNET_RECORDED_SCENARIO: &str = "bacnet-recorded-read-only";
pub const DIAGNOSTIC_SCENARIO: &str = "diagnostic-incident-replay";
pub const SCENARIOS: [&str; 6] = [
    REFERENCE_SCENARIO,
    REJECTION_SCENARIO,
    RECORDED_SCENARIO,
    RMF_RECORDED_SCENARIO,
    BACNET_RECORDED_SCENARIO,
    DIAGNOSTIC_SCENARIO,
];

pub fn run(name: &str) -> Result<String, String> {
    match name {
        REFERENCE_SCENARIO => run_reference_scenario(),
        REJECTION_SCENARIO => run_rejection_scenario(),
        RECORDED_SCENARIO => run_recorded_scenario(),
        RMF_RECORDED_SCENARIO => run_rmf_recorded_scenario(),
        BACNET_RECORDED_SCENARIO => run_bacnet_recorded_scenario(),
        DIAGNOSTIC_SCENARIO => run_diagnostic_scenario(),
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
    observation_coverage: ObservationCoverage,
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

#[derive(Debug, PartialEq)]
struct BacnetRecordedExperiment {
    partial: MappedBacnetObservation,
    partial_resolution: CarStateResolution,
    final_observation: MappedBacnetObservation,
    final_resolution: CarStateResolution,
    decoupled: MappedBacnetObservation,
    decoupled_resolution: CarStateResolution,
    mixed_doors: MappedBacnetObservation,
    mixed_doors_resolution: CarStateResolution,
    final_states: Vec<CarState>,
}

#[derive(Debug, PartialEq)]
struct DiagnosticExperiment {
    recorder_entries: Vec<DiagnosticEntry>,
    incident: IncidentSnapshot,
    replayed_anomalies: Vec<DiagnosticAnomaly>,
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

fn run_bacnet_recorded_scenario() -> Result<String, String> {
    let first = run_bacnet_experiment()?;
    let second = run_bacnet_experiment()?;
    if first != second {
        return Err("BACnet recorded mapping produced different results on replay".into());
    }
    verify_bacnet_experiment(&first)?;
    Ok(render_bacnet_report(&first))
}

fn run_diagnostic_scenario() -> Result<String, String> {
    let first = run_diagnostic_experiment()?;
    let second = run_diagnostic_experiment()?;
    if first != second {
        return Err("diagnostic incident replay produced different results on replay".into());
    }
    verify_diagnostic_experiment(&first)?;
    Ok(render_diagnostic_report(&first))
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
    let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
    let source_name = runtime.source_name().to_owned();
    let observation_coverage = runtime
        .observation_coverage(&CarId::new("A"))
        .ok_or_else(|| "recorded car A has no observation coverage".to_owned())?;
    engine
        .bind_observation(
            ObservationSourceId::new(&source_name),
            CarId::new("A"),
            ObservationBindingRole::Operational,
        )
        .map_err(|error| format!("recorded observation binding failed: {error:?}"))?;
    engine
        .begin_source_session(
            ObservationSourceId::new(&source_name),
            runtime.source_session().clone(),
        )
        .map_err(|error| format!("recorded source session failed: {error:?}"))?;
    for update in runtime.take_observation_updates() {
        engine.ingest_observation_update(update);
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
        let events = runtime.advance(elapsed);
        for event in &events {
            if let RuntimeEvent::Observation(update) = event {
                engine.ingest_observation_update(update.clone());
            }
        }
        batches.push(RecordedBatch {
            elapsed_total,
            events,
            states: engine.car_states(),
        });
    }
    Ok(RecordedPlayback {
        source_name,
        observation_coverage,
        batches,
        remaining_frames: runtime.remaining_frames(),
    })
}

fn recorded_runtime() -> Result<RecordedRuntime, String> {
    let moving = CarState {
        id: CarId::new("A"),
        location: CarLocation::Continuous(Position {
            floor_position: 3.5,
            nearest_floor: Floor::new(4),
        }),
        motion: MotionState::Moving(Direction::Up),
        door: DoorState::Closed,
        mode: OperatingMode::Normal,
        reported_destination: ReportedDestination::Known(Floor::new(4)),
    };
    let mut final_state = CarState::new("A", Floor::new(4));
    final_state.door = DoorState::Open;
    RecordedRuntime::new(
        [CarState::new("A", Floor::new(3))],
        [
            RecordedFrame::after(
                Duration::from_millis(500),
                vec![RuntimeEvent::Observation(CarObservationUpdate::from_state(
                    recorded_provenance(2),
                    &moving,
                ))],
            ),
            RecordedFrame::after(
                Duration::from_millis(500),
                vec![
                    RuntimeEvent::Observation(CarObservationUpdate::from_state(
                        recorded_provenance(3),
                        &final_state,
                    )),
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
    let car = CarId::new("lift-A");
    let incomplete_source = rmf_source_state();
    let incomplete =
        map_rmf_lift_state(&incomplete_source, car.clone(), &labels, rmf_provenance(1))
            .map_err(|error| error.to_string())?;
    let mut engine = Engine::new(incomplete.topology.clone());
    engine
        .bind_observation(
            rmf_provenance(1).source,
            car.clone(),
            ObservationBindingRole::Operational,
        )
        .map_err(|error| format!("Open-RMF observation binding failed: {error:?}"))?;
    engine
        .begin_source_session(rmf_provenance(1).source, rmf_provenance(1).session)
        .map_err(|error| format!("Open-RMF source session failed: {error:?}"))?;
    let incomplete_resolution = ingest_resolution(&mut engine, &incomplete.observation)?;

    let mut final_source = incomplete_source.clone();
    final_source.current_floor = "L4".into();
    final_source.destination_floor.clear();
    final_source.door_state = RmfLiftState::DOOR_OPEN;
    final_source.motion_state = RmfLiftState::MOTION_STOPPED;
    let final_observation =
        map_rmf_lift_state(&final_source, car.clone(), &labels, rmf_provenance(2))
            .map_err(|error| error.to_string())?;
    let final_resolution = ingest_resolution(&mut engine, &final_observation.observation)?;
    match &final_resolution {
        CarStateResolution::Resolved(_) => {}
        CarStateResolution::Unresolved { reasons, .. } => {
            return Err(format!(
                "sufficient Open-RMF observation stayed unresolved: {reasons:?}"
            ));
        }
    }

    let final_states = engine.car_states();
    let mut source_specific_mode_source = final_source;
    source_specific_mode_source.current_mode = RmfLiftState::MODE_AGV;
    let source_specific_mode = map_rmf_lift_state(
        &source_specific_mode_source,
        car,
        &labels,
        rmf_provenance(3),
    )
    .map_err(|error| error.to_string())?;
    let source_specific_mode_resolution =
        ingest_resolution(&mut engine, &source_specific_mode.observation)?;

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

fn run_bacnet_experiment() -> Result<BacnetRecordedExperiment, String> {
    let floors = bacnet_floor_numbers();
    let partial_source = bacnet_source_properties();
    let partial = map_bacnet_lift_properties(
        &partial_source,
        CarId::new("bacnet-lift-1"),
        &floors,
        bacnet_provenance(1),
    )
    .map_err(|error| error.to_string())?;
    let mut engine = Engine::new(partial.topology.clone());
    engine
        .bind_observation(
            bacnet_provenance(1).source,
            CarId::new("bacnet-lift-1"),
            ObservationBindingRole::Operational,
        )
        .map_err(|error| format!("BACnet observation binding failed: {error:?}"))?;
    engine
        .begin_source_session(bacnet_provenance(1).source, bacnet_provenance(1).session)
        .map_err(|error| format!("BACnet source session failed: {error:?}"))?;
    let partial_resolution = ingest_resolution(&mut engine, &partial.observation)?;

    let mut final_source = partial_source.clone();
    final_source.car_position = 4;
    final_source.car_moving_direction = BacnetLiftCarDirection::Stopped;
    final_source.car_door_status = vec![BacnetDoorStatus::Opened];
    final_source.car_mode = Some(BacnetLiftCarMode::Normal);
    final_source.next_stopping_floor = None;
    let final_observation = map_bacnet_lift_properties(
        &final_source,
        CarId::new("bacnet-lift-1"),
        &floors,
        bacnet_provenance(2),
    )
    .map_err(|error| error.to_string())?;
    let final_resolution = ingest_resolution(&mut engine, &final_observation.observation)?;
    match &final_resolution {
        CarStateResolution::Resolved(_) => {}
        CarStateResolution::Unresolved { reasons, .. } => {
            return Err(format!(
                "sufficient BACnet observation stayed unresolved: {reasons:?}"
            ));
        }
    }

    let final_states = engine.car_states();
    let mut decoupled_source = final_source.clone();
    decoupled_source.out_of_service = true;
    decoupled_source.car_mode = Some(BacnetLiftCarMode::OutOfService);
    let decoupled = map_bacnet_lift_properties(
        &decoupled_source,
        CarId::new("bacnet-lift-1"),
        &floors,
        bacnet_provenance(3),
    )
    .map_err(|error| error.to_string())?;
    let decoupled_resolution = ingest_resolution(&mut engine, &decoupled.observation)?;

    let mut mixed_doors_source = final_source;
    mixed_doors_source.car_door_status = vec![BacnetDoorStatus::Opened, BacnetDoorStatus::Closed];
    let mixed_doors = map_bacnet_lift_properties(
        &mixed_doors_source,
        CarId::new("bacnet-lift-1"),
        &floors,
        bacnet_provenance(4),
    )
    .map_err(|error| error.to_string())?;
    let mixed_doors_resolution = ingest_resolution(&mut engine, &mixed_doors.observation)?;

    Ok(BacnetRecordedExperiment {
        partial,
        partial_resolution,
        final_observation,
        final_resolution,
        decoupled,
        decoupled_resolution,
        mixed_doors,
        mixed_doors_resolution,
        final_states,
    })
}

fn run_diagnostic_experiment() -> Result<DiagnosticExperiment, String> {
    let labels = rmf_floor_labels();
    let car = CarId::new("lift-A");
    let source = rmf_source_state();
    let first = map_rmf_lift_state(&source, car.clone(), &labels, rmf_provenance(1))
        .map_err(|error| error.to_string())?;
    let duplicate = map_rmf_lift_state(&source, car.clone(), &labels, rmf_provenance(1))
        .map_err(|error| error.to_string())?;
    let next = map_rmf_lift_state(&source, car.clone(), &labels, rmf_provenance(2))
        .map_err(|error| error.to_string())?;
    let after = map_rmf_lift_state(&source, car.clone(), &labels, rmf_provenance(3))
        .map_err(|error| error.to_string())?;

    let mut engine = Engine::with_diagnostic_capacity(first.topology.clone(), 4);
    engine
        .bind_observation(
            rmf_provenance(1).source,
            car,
            ObservationBindingRole::Auxiliary,
        )
        .map_err(|error| format!("diagnostic observation binding failed: {error:?}"))?;
    engine
        .begin_source_session(rmf_provenance(1).source, rmf_provenance(1).session)
        .map_err(|error| format!("diagnostic source session failed: {error:?}"))?;
    engine.ingest_observation_update((&first.observation).into());
    engine.ingest_observation_update((&duplicate.observation).into());
    engine.ingest_observation_update((&next.observation).into());
    engine.ingest_observation_update((&after.observation).into());

    let recorder_entries = engine.diagnostics().entries().cloned().collect();
    let incident = engine
        .diagnostics()
        .incidents()
        .next()
        .cloned()
        .ok_or_else(|| "non-increasing observation sequence produced no incident".to_owned())?;
    let replayed_anomalies = incident.replay_anomalies();
    Ok(DiagnosticExperiment {
        recorder_entries,
        incident,
        replayed_anomalies,
    })
}

fn ingest_resolution(
    engine: &mut Engine,
    observation: &CarObservation,
) -> Result<CarStateResolution, String> {
    match engine.ingest_observation_update(observation.into()) {
        ObservationIngestResult::Resolution(resolution) => Ok(resolution),
        ObservationIngestResult::Incomplete(_) => {
            Err("whole observation unexpectedly remained incomplete after assembly".into())
        }
        ObservationIngestResult::Rejected(error) => {
            Err(format!("observation update was rejected: {error:?}"))
        }
    }
}

fn recorded_provenance(sequence: u64) -> ObservationProvenance {
    ObservationProvenance {
        source: ObservationSourceId::new("recorded-read-only"),
        session: ObservationSessionId::new("recorded-session-1"),
        sequence: ObservationSequence::new(sequence),
    }
}

fn rmf_provenance(sequence: u64) -> ObservationProvenance {
    ObservationProvenance {
        source: ObservationSourceId::new("fixture/open-rmf"),
        session: ObservationSessionId::new("rmf-adapter-session-A"),
        sequence: ObservationSequence::new(sequence),
    }
}

fn bacnet_provenance(sequence: u64) -> ObservationProvenance {
    ObservationProvenance {
        source: ObservationSourceId::new("fixture/bacnet-lift"),
        session: ObservationSessionId::new("bacnet-adapter-session-A"),
        sequence: ObservationSequence::new(sequence),
    }
}

fn bacnet_floor_numbers() -> BTreeMap<u8, Floor> {
    [(1, 1), (2, 2), (3, 3), (4, 4)]
        .into_iter()
        .map(|(universal, floor)| (universal, Floor::new(floor)))
        .collect()
}

fn bacnet_source_properties() -> RecordedBacnetLiftProperties {
    RecordedBacnetLiftProperties {
        object_name: "Lift 1".into(),
        car_position: 3,
        car_moving_direction: BacnetLiftCarDirection::Up,
        car_door_status: vec![BacnetDoorStatus::Closing],
        car_mode: Some(BacnetLiftCarMode::FirefighterControl),
        next_stopping_floor: Some(4),
        out_of_service: false,
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
        lift_name: "rmf-lift-east".into(),
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
        || first.app.engine.car_states() != second.app.engine.car_states()
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

    let states = replay.app.engine.car_states();
    let expected_positions = [("A", Floor::new(2)), ("B", Floor::new(5))];
    if states.len() != expected_positions.len()
        || expected_positions.iter().any(|(id, floor)| {
            states
                .iter()
                .find(|state| state.id.as_str() == *id)
                .is_none_or(|state| {
                    state.location != CarLocation::Continuous(Position::at_floor(*floor))
                        || state.motion != MotionState::Stopped
                        || state.door != DoorState::Open
                        || state.reported_destination != ReportedDestination::None
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
    let states = replay.app.engine.car_states();
    let recovered = states.iter().any(|state| {
        state.id.as_str() == "A"
            && state.location == CarLocation::Continuous(Position::at_floor(Floor::new(7)))
            && state.motion == MotionState::Stopped
            && state.door == DoorState::Open
            && state.reported_destination == ReportedDestination::None
    });
    if !recovered {
        return Err("rejection scenario ended before recovered service completed".into());
    }
    if replay
        .app
        .engine
        .diagnostics()
        .entries()
        .any(|entry| matches!(entry.event, DiagnosticEvent::Anomaly(_)))
    {
        return Err("intent rejection/retry produced a false diagnostic anomaly".into());
    }
    Ok(())
}

fn verify_recorded_outcome(playback: &RecordedPlayback) -> Result<(), String> {
    if playback.source_name != "recorded-read-only"
        || !playback.observation_coverage.position
        || !playback.observation_coverage.door
    {
        return Err("recorded runtime did not preserve its explicit read-only boundary".into());
    }
    if playback.batches.len() != 4
        || !playback.batches[0].events.is_empty()
        || playback.batches[1].events.len() != 1
        || !playback.batches[2].events.is_empty()
        || playback.batches[3].events.len() != 3
        || playback.remaining_frames != 0
    {
        return Err("recorded frames were not released at their explicit elapsed times".into());
    }
    let partial = &playback.batches[1].states;
    if partial.len() != 1
        || partial[0].location
            != CarLocation::Continuous(Position {
                floor_position: 3.5,
                nearest_floor: Floor::new(4),
            })
        || partial[0].motion != MotionState::Moving(Direction::Up)
    {
        return Err("first recorded observation did not update the intermediate state".into());
    }
    let final_states = &playback.batches[3].states;
    if final_states.len() != 1
        || final_states[0].location != CarLocation::Floor(Floor::new(4))
        || final_states[0].motion != MotionState::Stopped
        || final_states[0].door != DoorState::Open
        || final_states[0].reported_destination != ReportedDestination::None
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
        || experiment.incomplete.observation.reported_destination
            != ReportedDestination::Known(Floor::new(4))
        || experiment.incomplete.source_context.lift_name != "rmf-lift-east"
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
            reasons: vec![UnresolvedCarStateReason::DoorDirectionUnknown],
        })
    {
        return Err("incomplete Open-RMF evidence was forced into operational state".into());
    }

    let expected_final = CarState {
        id: CarId::new("lift-A"),
        location: CarLocation::Floor(Floor::new(4)),
        motion: MotionState::Stopped,
        door: DoorState::Open,
        mode: OperatingMode::FireService,
        reported_destination: ReportedDestination::Unknown,
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

fn verify_bacnet_experiment(experiment: &BacnetRecordedExperiment) -> Result<(), String> {
    if experiment.partial.observation.provenance != bacnet_provenance(1)
        || experiment.partial.observation.position
            != PositionEvidence::FloorReference(Floor::new(3))
        || experiment.partial.observation.motion
            != MotionEvidence::Known(MotionState::Moving(Direction::Up))
        || experiment.partial.observation.door != DoorEvidence::Known(DoorState::Closing)
        || experiment.partial.observation.mode
            != OperatingModeEvidence::Known(OperatingMode::FireService)
        || experiment.partial.observation.reported_destination
            != ReportedDestination::Known(Floor::new(4))
        || experiment.partial.source_context.object_name != "Lift 1"
        || experiment.partial.source_context.universal_car_position != 3
    {
        return Err("BACnet required properties did not preserve their exact evidence".into());
    }
    let expected_partial = CarState {
        id: CarId::new("bacnet-lift-1"),
        location: CarLocation::Floor(Floor::new(3)),
        motion: MotionState::Moving(Direction::Up),
        door: DoorState::Closing,
        mode: OperatingMode::FireService,
        reported_destination: ReportedDestination::Known(Floor::new(4)),
    };
    if experiment.partial_resolution != CarStateResolution::Resolved(expected_partial) {
        return Err(
            "BACnet floor-level location did not resolve without fabricated precision".into(),
        );
    }

    let expected_final = CarState {
        id: CarId::new("bacnet-lift-1"),
        location: CarLocation::Floor(Floor::new(4)),
        motion: MotionState::Stopped,
        door: DoorState::Open,
        mode: OperatingMode::Normal,
        reported_destination: ReportedDestination::Unknown,
    };
    if experiment.final_observation.observation.provenance != bacnet_provenance(2)
        || experiment.final_resolution != CarStateResolution::Resolved(expected_final.clone())
        || experiment.final_states != [expected_final]
    {
        return Err("sufficient BACnet evidence did not resolve at the no-write boundary".into());
    }

    if !experiment.decoupled.source_context.object_out_of_service
        || experiment.decoupled.observation.position != PositionEvidence::Unknown
        || experiment.decoupled.observation.motion != MotionEvidence::Unknown
        || experiment.decoupled.observation.door != DoorEvidence::Unknown
        || experiment.decoupled.observation.mode != OperatingModeEvidence::Unknown
        || experiment.decoupled_resolution
            != (CarStateResolution::Unresolved {
                car: CarId::new("bacnet-lift-1"),
                reasons: vec![
                    UnresolvedCarStateReason::PositionUnknown,
                    UnresolvedCarStateReason::MotionUnknown,
                    UnresolvedCarStateReason::DoorUnknown,
                    UnresolvedCarStateReason::OperatingModeUnknown,
                ],
            })
    {
        return Err("decoupled BACnet object values leaked into operational state".into());
    }

    if experiment.mixed_doors.source_context.door_statuses
        != [BacnetDoorStatus::Opened, BacnetDoorStatus::Closed]
        || experiment.mixed_doors.observation.door != DoorEvidence::Unknown
        || experiment.mixed_doors_resolution
            != (CarStateResolution::Unresolved {
                car: CarId::new("bacnet-lift-1"),
                reasons: vec![UnresolvedCarStateReason::DoorUnknown],
            })
    {
        return Err("multiple BACnet car doors were collapsed into one false state".into());
    }
    Ok(())
}

fn verify_diagnostic_experiment(experiment: &DiagnosticExperiment) -> Result<(), String> {
    let anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
        source: ObservationSourceId::new("fixture/open-rmf"),
        session: ObservationSessionId::new("rmf-adapter-session-A"),
        previous: ObservationSequence::new(1),
        received: ObservationSequence::new(1),
    };
    if experiment.recorder_entries.len() != 4 {
        return Err("diagnostic flight recorder exceeded or missed its configured bound".into());
    }
    if experiment.incident.trigger != anomaly
        || !experiment.incident.after_window_complete
        || experiment.replayed_anomalies != [anomaly]
    {
        return Err("diagnostic incident did not preserve and replay the expected anomaly".into());
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
    lines.extend(replay.app.engine.car_states().into_iter().map(|state| {
        format!(
            "  {} location={} motion={:?} door={:?} mode={:?} reported_destination={}",
            state.id,
            state.location,
            state.motion,
            state.door,
            state.mode,
            state.reported_destination
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
                "             state {} location={} motion={:?} door={:?} reported_destination={}",
                state.id, state.location, state.motion, state.door, state.reported_destination
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
            "incomplete observation: source={} sequence={} car={} position={:?} motion={:?} door={:?} mode={:?} reported_destination={} session={}",
            incomplete.provenance.source,
            incomplete.provenance.sequence.value(),
            incomplete.car,
            incomplete.position,
            incomplete.motion,
            incomplete.door,
            incomplete.mode,
            incomplete.reported_destination,
            experiment.incomplete.source_context.source_session_id
        ),
        format!(
            "incomplete resolution: {:?}",
            experiment.incomplete_resolution
        ),
        format!(
            "resolved final: sequence={} {} location={} motion={:?} door={:?} mode={:?} reported_destination={}",
            experiment
                .final_observation
                .observation
                .provenance
                .sequence
                .value(),
            final_state.id,
            final_state.location,
            final_state.motion,
            final_state.door,
            final_state.mode,
            final_state.reported_destination
        ),
        format!(
            "source-specific mode: raw={} canonical={:?} resolution={:?}",
            experiment.source_specific_mode.source_context.source_mode,
            experiment.source_specific_mode.observation.mode,
            experiment.source_specific_mode_resolution
        ),
        "outcome: PASS (floor precision preserved; only sufficient complete evidence became CarState)".into(),
        String::new(),
    ]
    .join("\n")
}

fn render_bacnet_report(experiment: &BacnetRecordedExperiment) -> String {
    let partial = &experiment.partial.observation;
    let CarStateResolution::Resolved(final_state) = &experiment.final_resolution else {
        unreachable!("verified experiment has a resolved final observation")
    };
    [
        format!("scenario: {BACNET_RECORDED_SCENARIO}"),
        "source: recorded decoded BACnet Lift object properties; no BACnet transport or network connection".into(),
        "determinism: PASS (2 identical mappings and resolutions)".into(),
        "write boundary: PASS (property projection and recorded runtime expose no IntentSink)".into(),
        format!(
            "partial observation: source={} sequence={} car={} position={:?} motion={:?} door={:?} mode={:?} reported_destination={}",
            partial.provenance.source,
            partial.provenance.sequence.value(),
            partial.car,
            partial.position,
            partial.motion,
            partial.door,
            partial.mode,
            partial.reported_destination
        ),
        format!("partial resolution: {:?}", experiment.partial_resolution),
        format!(
            "resolved final: sequence={} {} location={} motion={:?} door={:?} mode={:?} reported_destination={}",
            experiment.final_observation.observation.provenance.sequence.value(),
            final_state.id,
            final_state.location,
            final_state.motion,
            final_state.door,
            final_state.mode,
            final_state.reported_destination
        ),
        format!(
            "decoupled object: raw_out_of_service={} canonical_position={:?} canonical_mode={:?} resolution={:?}",
            experiment.decoupled.source_context.object_out_of_service,
            experiment.decoupled.observation.position,
            experiment.decoupled.observation.mode,
            experiment.decoupled_resolution
        ),
        format!(
            "mixed car doors: raw={:?} canonical={:?} resolution={:?}",
            experiment.mixed_doors.source_context.door_statuses,
            experiment.mixed_doors.observation.door,
            experiment.mixed_doors_resolution
        ),
        "outcome: PASS (BACnet floor-level/decoupled/multi-door evidence stayed honest; floor-level location resolved without continuous precision)".into(),
        String::new(),
    ]
    .join("\n")
}

fn render_diagnostic_report(experiment: &DiagnosticExperiment) -> String {
    [
        format!("scenario: {DIAGNOSTIC_SCENARIO}"),
        "scope: bounded in-memory diagnostics; no equipment fault or root-cause claim".into(),
        "determinism: PASS (2 identical recordings and incident replays)".into(),
        format!(
            "flight recorder: PASS ({} entries within configured capacity 4)",
            experiment.recorder_entries.len()
        ),
        format!("anomaly: {:?}", experiment.incident.trigger),
        format!(
            "incident: PASS ({} evidence entries; after-window complete={})",
            experiment.incident.evidence.len(),
            experiment.incident.after_window_complete
        ),
        "outcome: PASS (incident replay reproduced the expected sequence anomaly)".into(),
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
        assert!(report.contains("A location=2.00F"));
        assert!(report.contains("B location=5.00F"));
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
        assert!(report.contains("[0500 ms] Observation A 3.50F"));
        assert!(report.contains("[0999 ms] no events"));
        assert!(report.contains("[1000 ms] Arrived A at 4F"));
        assert!(report.contains(
            "state A location=floor 4F motion=Stopped door=Open reported_destination=none"
        ));
    }

    #[test]
    fn rmf_recorded_scenario_preserves_partial_evidence_before_resolution() {
        let report = run(RMF_RECORDED_SCENARIO).expect("recorded RMF scenario should pass");

        assert!(report.contains("determinism: PASS (2 identical mappings and resolutions)"));
        assert!(report.contains("source=fixture/open-rmf sequence=1 car=lift-A"));
        assert!(report.contains("position=FloorReference(Floor(3))"));
        assert!(report.contains("motion=Known(Moving(Up)) door=MovingUnknownDirection"));
        assert!(report.contains("reasons: [DoorDirectionUnknown]"));
        assert!(
            report.contains("resolved final: sequence=2 lift-A location=floor 4F motion=Stopped")
        );
        assert!(report.contains("source-specific mode: raw=2 canonical=SourceSpecific"));
        assert!(report.contains("only sufficient complete evidence became CarState"));
    }

    #[test]
    fn bacnet_recorded_scenario_preserves_decoupled_and_multi_door_evidence() {
        let report = run(BACNET_RECORDED_SCENARIO).expect("recorded BACnet scenario should pass");

        assert!(report.contains("determinism: PASS (2 identical mappings and resolutions)"));
        assert!(report.contains("source=fixture/bacnet-lift sequence=1 car=bacnet-lift-1"));
        assert!(report.contains("position=FloorReference(Floor(3))"));
        assert!(report.contains("door=Known(Closing) mode=Known(FireService)"));
        assert!(report.contains("partial resolution: Resolved"));
        assert!(report.contains("location: Floor(Floor(3))"));
        assert!(report.contains("resolved final: sequence=2 bacnet-lift-1 location=floor 4F"));
        assert!(report.contains("raw_out_of_service=true canonical_position=Unknown"));
        assert!(report.contains("raw=[Opened, Closed] canonical=Unknown"));
        assert!(report.contains("floor-level location resolved without continuous precision"));
    }

    #[test]
    fn diagnostic_scenario_bounds_and_replays_incident_evidence() {
        let report = run(DIAGNOSTIC_SCENARIO).expect("diagnostic scenario should pass");

        assert!(report.contains("determinism: PASS (2 identical recordings and incident replays)"));
        assert!(report.contains("4 entries within configured capacity 4"));
        assert!(report.contains("ObservationSequenceNotIncreasing"));
        assert!(report.contains("after-window complete=true"));
        assert!(report.contains("incident replay reproduced the expected sequence anomaly"));
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
