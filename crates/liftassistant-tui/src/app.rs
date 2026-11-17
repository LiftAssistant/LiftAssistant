use std::{collections::VecDeque, time::Duration};

use liftassistant_core::{DispatchRejection, HallCallRejection};
use liftassistant_engine::{Engine, ObservationBindingRole};
use liftassistant_model::{
    BuildingTopology, CallState, CarId, Direction, Floor, HallCall, Intent, ObservationSourceId,
    OperatingMode, PositionEvidence,
};
use liftassistant_runtime::{RuntimeEvent, SimulationDriver};
use liftassistant_sim::SimRuntime;

use crate::replay::{Command, SemanticEvent, run_with_topology};

pub const FIXED_STEP: Duration = Duration::from_millis(500);
const TRACE_LIMIT: usize = 100;
const RECORDING_LIMIT: usize = 2048;

pub struct App {
    pub runtime: SimRuntime,
    pub engine: Engine,
    pub selected_floor: Floor,
    pub paused: bool,
    pub steps: u64,
    pub trace: VecDeque<String>,
    pub recording: Vec<Command>,
    pub recording_full: bool,
}

impl App {
    pub fn new() -> Self {
        Self::with_topology(BuildingTopology::reference_eight_floor())
    }

    pub(crate) fn with_topology(topology: BuildingTopology) -> Self {
        let floors = topology.served_floors();
        let selected_floor = floors[floors.len().saturating_sub(2)];
        let mut runtime = SimRuntime::reference_two_car();
        let mut engine = Engine::new(topology);
        let source = ObservationSourceId::new(runtime.source_name());
        let source_session = runtime.source_session().clone();
        for (car, capabilities) in runtime.all_capabilities() {
            engine
                .bind_observation(
                    source.clone(),
                    car.clone(),
                    ObservationBindingRole::Operational,
                )
                .expect("simulator cars have unique operational observation bindings");
            engine.set_capabilities(car, capabilities);
        }
        engine
            .begin_source_session(source, source_session)
            .expect("simulator starts one configured observation session");
        for update in runtime.take_observation_updates() {
            engine.ingest_observation_update(update);
        }
        Self {
            runtime,
            engine,
            selected_floor,
            paused: false,
            steps: 0,
            trace: VecDeque::new(),
            recording: Vec::new(),
            recording_full: false,
        }
    }

    pub fn select_floor(&mut self, delta: i16) {
        let floors = self.engine.topology().served_floors();
        let current = floors
            .binary_search(&self.selected_floor)
            .expect("selected floor belongs to the building topology");
        let selected = (current as i32 + i32::from(delta)).clamp(0, floors.len() as i32 - 1);
        self.selected_floor = floors[selected as usize];
    }

    pub fn call(&mut self, direction: Direction) {
        self.execute(Command::HallCall(HallCall {
            floor: self.selected_floor,
            direction,
        }));
    }

    // Both live input and replay use this deterministic path; UI scheduling stays in main.rs.
    pub fn execute(&mut self, command: Command) -> Vec<SemanticEvent> {
        if self.recording.len() < RECORDING_LIMIT {
            self.recording.push(command.clone());
        } else if !self.recording_full {
            self.recording_full = true;
            self.log("Recording full: replay unavailable until R reset".into());
        }
        let events = match command {
            Command::HallCall(call) => {
                let events = self
                    .engine
                    .submit_call(call, &mut self.runtime)
                    .into_iter()
                    .map(Into::into)
                    .collect();
                self.sync_pending_observations();
                events
            }
            Command::SetMode { car, mode } => match self.runtime.set_mode(&car, mode) {
                Ok(()) => {
                    self.sync_pending_observations();
                    let mut events = vec![SemanticEvent::ModeChanged { car, mode }];
                    events.extend(
                        self.engine
                            .retry_pending(&mut self.runtime)
                            .into_iter()
                            .map(SemanticEvent::from),
                    );
                    self.sync_pending_observations();
                    events
                }
                Err(error) => vec![SemanticEvent::RuntimeError(error.to_string())],
            },
            Command::RejectNextIntent => {
                self.runtime.reject_next_intent();
                vec![SemanticEvent::SimulatorIntentRejectionArmed]
            }
            Command::Step => {
                self.steps += 1;
                let runtime_events = self.runtime.advance(FIXED_STEP);
                let events = self
                    .engine
                    .consume_runtime_events(runtime_events, &mut self.runtime)
                    .into_iter()
                    .map(Into::into)
                    .collect();
                self.sync_pending_observations();
                events
            }
        };
        for event in &events {
            self.log(semantic_label(event));
        }
        events
    }

    pub fn replay(&mut self) {
        if self.recording_full {
            self.log("Replay unavailable: recording full; R reset to record a new session".into());
            return;
        }
        let topology = self.engine.topology().clone();
        let first = run_with_topology(&self.recording, topology.clone());
        let second = run_with_topology(&self.recording, topology);
        if first.events != second.events
            || first.app.runtime.car_states() != second.app.runtime.car_states()
            || first.app.runtime.car_states() != self.runtime.car_states()
            || first.app.steps != self.steps
            || first.app.engine != self.engine
        {
            self.log("REPLAY mismatch: live state preserved".into());
            return;
        }
        let message = format!(
            "REPLAY verified: {} commands / {} semantic events",
            self.recording.len(),
            first.events.len()
        );
        let selected = self.selected_floor;
        *self = first.app;
        self.selected_floor = selected;
        self.paused = true;
        self.log(message);
    }

    pub fn advance(&mut self) {
        if !self.paused {
            self.step();
        }
    }

    pub fn toggle_availability(&mut self, id: &str) {
        let car = CarId::new(id);
        let mode = self
            .engine
            .car_states()
            .iter()
            .find(|state| state.id == car)
            .map_or(OperatingMode::OutOfService, |state| {
                if state.mode == OperatingMode::Normal {
                    OperatingMode::OutOfService
                } else {
                    OperatingMode::Normal
                }
            });
        self.execute(Command::SetMode { car, mode });
    }

    pub fn step(&mut self) {
        self.execute(Command::Step);
    }

    pub fn reset(&mut self) {
        let topology = self.engine.topology().clone();
        *self = Self::with_topology(topology);
        self.log("RESET: two-car simulator with configured topology".into());
    }

    fn sync_pending_observations(&mut self) {
        for update in self.runtime.take_observation_updates() {
            self.engine.ingest_observation_update(update);
        }
    }

    fn log(&mut self, message: String) {
        if self.trace.len() == TRACE_LIMIT {
            self.trace.pop_front();
        }
        self.trace
            .push_back(format!("[{:04}] {message}", self.steps));
    }
}

pub fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "UP",
        Direction::Down => "DOWN",
    }
}

pub fn intent_label(intent: &Intent) -> String {
    match intent {
        Intent::ServeFloor { car, floor } => format!("ServeFloor({car}, {floor})"),
        Intent::ParkAt { car, floor } => format!("ParkAt({car}, {floor})"),
    }
}

pub fn call_state_label(state: &CallState) -> String {
    match state {
        CallState::Pending => "Pending".into(),
        CallState::Assigned(car) => format!("Assigned {car}"),
        CallState::Serving(car) => format!("Serving {car}"),
        CallState::Completed => "Completed".into(),
    }
}

fn semantic_label(event: &SemanticEvent) -> String {
    match event {
        SemanticEvent::HallCall(call) => {
            format!("HallCall {} {}", call.floor, direction_name(call.direction))
        }
        SemanticEvent::InvalidCall(reason) => match reason {
            HallCallRejection::FloorNotServed(floor) => {
                format!("Invalid hall call: {floor} is not served")
            }
            HallCallRejection::DirectionUnavailableAtTerminal { floor, direction } => format!(
                "Invalid hall call: {floor} has no {} service",
                direction_name(*direction)
            ),
        },
        SemanticEvent::CallRegistered(id) => format!("Call {id} -> Pending"),
        SemanticEvent::CallDeduplicated(id) => format!("Active duplicate -> {id}"),
        SemanticEvent::CallStateChanged { id, state } => {
            format!("Call {id} -> {}", call_state_label(state))
        }
        SemanticEvent::DispatchCandidate { id, candidate } => {
            let status = if candidate.rejections.is_empty() {
                "eligible".into()
            } else {
                format!(
                    "reject[{}]",
                    candidate
                        .rejections
                        .iter()
                        .map(rejection_label)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            let serve_floor = candidate
                .serve_floor
                .map_or("unknown".into(), |supported| supported.to_string());
            format!(
                "Call {id}: {} {status} d={:.2} mode={:?} busy={} serve={serve_floor}",
                candidate.car, candidate.distance_floors, candidate.mode, candidate.busy
            )
        }
        SemanticEvent::NoEligibleCar(id) => format!("Call {id}: no eligible idle car"),
        SemanticEvent::DispatchSelected { id, selection } => format!(
            "Call {id}: selected {} {:?} d={:.2}",
            selection.car, selection.reason, selection.distance_floors
        ),
        SemanticEvent::IntentGenerated { id, intent } => {
            format!("Call {id}: intent -> {}", intent_label(intent))
        }
        SemanticEvent::ModeChanged { car, mode } => format!("Car {car} -> {mode:?}"),
        SemanticEvent::SimulatorIntentRejectionArmed => {
            "Simulator armed: reject next intent once".into()
        }
        SemanticEvent::RuntimeError(error) => format!("Runtime error: {error}"),
        SemanticEvent::Runtime(event) => event_label(event),
    }
}

fn rejection_label(rejection: &DispatchRejection) -> &'static str {
    match rejection {
        DispatchRejection::Mode(_) => "Mode",
        DispatchRejection::Busy => "Busy",
        DispatchRejection::CapabilitiesUnavailable => "CapabilitiesUnknown",
        DispatchRejection::ServeFloorUnsupported => "ServeFloorUnsupported",
    }
}

pub(crate) fn event_label(event: &RuntimeEvent) -> String {
    match event {
        RuntimeEvent::IntentAccepted(intent) => format!("IntentAccepted {}", intent_label(intent)),
        RuntimeEvent::Observation(update) => {
            let position = match update.position {
                Some(PositionEvidence::Exact(position)) => {
                    format!("{:.2}F", position.floor_position)
                }
                Some(PositionEvidence::FloorReference(floor)) => format!("floor {floor}"),
                Some(PositionEvidence::Unknown) => "unknown".into(),
                None => "omitted".into(),
            };
            format!(
                "Observation {} {position} motion={:?} door={:?}",
                update.car, update.motion, update.door
            )
        }
        RuntimeEvent::Arrived { car, floor } => format!("Arrived {car} at {floor}"),
        RuntimeEvent::DoorStateChanged { car, state } => {
            format!("DoorStateChanged {car} {state:?}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{
        CallId, CallState, CarId, CarLocation, DoorState, Position, ReportedDestination,
    };

    #[test]
    fn floor_selection_and_call_validation_use_the_configured_topology() {
        let topology = BuildingTopology::new([Floor::new(1), Floor::new(3), Floor::new(5)])
            .expect("nonempty topology");
        let mut app = App::with_topology(topology.clone());

        assert_eq!(app.selected_floor, Floor::new(3));
        app.select_floor(1);
        assert_eq!(app.selected_floor, Floor::new(5));
        app.select_floor(1);
        assert_eq!(app.selected_floor, Floor::new(5));
        app.select_floor(-2);
        assert_eq!(app.selected_floor, Floor::new(1));

        let unknown = app.execute(Command::HallCall(HallCall {
            floor: Floor::new(2),
            direction: Direction::Up,
        }));
        assert!(unknown.contains(&SemanticEvent::InvalidCall(
            HallCallRejection::FloorNotServed(Floor::new(2))
        )));
        let terminal = app.execute(Command::HallCall(HallCall {
            floor: Floor::new(1),
            direction: Direction::Down,
        }));
        assert!(terminal.contains(&SemanticEvent::InvalidCall(
            HallCallRejection::DirectionUnavailableAtTerminal {
                floor: Floor::new(1),
                direction: Direction::Down,
            }
        )));
        assert!(app.engine.calls().calls().next().is_none());

        app.reset();
        assert_eq!(app.engine.topology(), &topology);
        assert_eq!(app.selected_floor, Floor::new(3));
    }

    #[test]
    fn paused_single_steps_arrive_and_reset_clears_application_state() {
        let mut app = App::new();
        app.paused = true;
        app.call(Direction::Up);
        assert_eq!(
            app.engine.decision(),
            Some(&Intent::ServeFloor {
                car: CarId::new("A"),
                floor: Floor::new(7)
            })
        );
        let before = app.engine.car_states();
        app.advance();
        assert_eq!(app.steps, 0);
        assert_eq!(app.engine.car_states(), before);
        app.step();
        assert_eq!(app.steps, 1);
        assert_eq!(
            app.engine.car_states()[0].location,
            CarLocation::Continuous(Position {
                floor_position: 3.75,
                nearest_floor: Floor::new(4),
            })
        );
        for _ in 0..5 {
            app.step();
        }
        let states = app.engine.car_states();
        let a = &states[0];
        assert_eq!(
            a.location,
            CarLocation::Continuous(Position::at_floor(Floor::new(7)))
        );
        assert_eq!(a.door, DoorState::Open);
        assert_eq!(
            app.engine
                .calls()
                .get(CallId::new(1))
                .map(|record| &record.state),
            Some(&CallState::Completed)
        );
        assert!(
            app.trace
                .iter()
                .any(|line| line.contains("Arrived A at 7F"))
        );
        app.reset();
        assert_eq!(app.engine.car_states(), App::new().engine.car_states());
        assert!(app.engine.decision().is_none() && app.engine.last_call().is_none());
        assert!(app.engine.calls().calls().next().is_none());
        assert_eq!(app.steps, 0);
        assert_eq!(app.trace.len(), 1);
        assert_eq!(app.selected_floor, Floor::new(7));
        assert!(app.recording.is_empty());
    }

    #[test]
    fn active_duplicate_call_reuses_identity_without_resubmitting() {
        let mut app = App::new();
        app.paused = true;
        app.call(Direction::Up);
        let states = app.engine.car_states();

        let events = app.execute(Command::HallCall(HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        }));

        assert!(events.contains(&SemanticEvent::CallDeduplicated(CallId::new(1))));
        assert_eq!(app.engine.calls().calls().count(), 1);
        assert_eq!(app.engine.car_states(), states);
    }

    #[test]
    fn invalid_calls_do_not_submit_and_trace_is_bounded() {
        let mut app = App::new();
        app.select_floor(1);
        for _ in 0..150 {
            app.call(Direction::Up);
        }
        assert!(app.engine.decision().is_none());
        assert!(
            app.runtime
                .car_states()
                .iter()
                .all(|car| car.reported_destination == ReportedDestination::None)
        );
        assert_eq!(app.trace.len(), TRACE_LIMIT);
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;

    #[test]
    fn a_truncated_recording_cannot_be_silently_replayed() {
        let mut app = App::new();
        for _ in 0..=RECORDING_LIMIT {
            app.step();
        }
        assert_eq!(app.recording.len(), RECORDING_LIMIT);
        assert!(app.recording_full);
        let steps = app.steps;
        app.replay();
        assert_eq!(app.steps, steps);
        assert!(app.trace.back().unwrap().contains("Replay unavailable"));
        app.reset();
        assert!(!app.recording_full);
    }
}
