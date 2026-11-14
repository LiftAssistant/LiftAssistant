use std::{collections::VecDeque, time::Duration};

use liftassistant_core::{CallRegistration, CallRegistry, dispatch};
use liftassistant_model::{CallState, CarId, Direction, Floor, HallCall, Intent, OperatingMode};
use liftassistant_runtime::{ElevatorRuntime, RuntimeEvent};
use liftassistant_sim::SimRuntime;

use crate::replay::{Command, SemanticEvent, run};

pub const FIXED_STEP: Duration = Duration::from_millis(500);
const TRACE_LIMIT: usize = 100;
const RECORDING_LIMIT: usize = 2048;

pub fn valid_call(call: HallCall) -> bool {
    (1..=8).contains(&call.floor.index())
        && !matches!(
            (call.floor.index(), call.direction),
            (1, Direction::Down) | (8, Direction::Up)
        )
}

pub struct App {
    pub runtime: SimRuntime,
    pub selected_floor: Floor,
    pub paused: bool,
    pub steps: u64,
    pub last_call: Option<HallCall>,
    pub decision: Option<Intent>,
    pub calls: CallRegistry,
    pub trace: VecDeque<String>,
    pub recording: Vec<Command>,
    pub recording_full: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            runtime: SimRuntime::reference_two_car(),
            selected_floor: Floor::new(7),
            paused: false,
            steps: 0,
            last_call: None,
            decision: None,
            calls: CallRegistry::new(),
            trace: VecDeque::new(),
            recording: Vec::new(),
            recording_full: false,
        }
    }

    pub fn select_floor(&mut self, delta: i16) {
        self.selected_floor = Floor::new((self.selected_floor.index() + delta).clamp(1, 8));
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
            Command::HallCall(call) => self.submit_call(call),
            Command::SetMode { car, mode } => match self.runtime.set_mode(&car, mode) {
                Ok(()) => {
                    let mut events = vec![SemanticEvent::ModeChanged { car, mode }];
                    events.extend(self.dispatch_pending());
                    events
                }
                Err(error) => vec![SemanticEvent::RuntimeError(error.to_string())],
            },
            Command::Step => {
                self.steps += 1;
                self.runtime
                    .tick(FIXED_STEP)
                    .into_iter()
                    .map(SemanticEvent::Runtime)
                    .collect()
            }
        };
        for event in &events {
            self.log(semantic_label(event));
        }
        events
    }

    fn submit_call(&mut self, call: HallCall) -> Vec<SemanticEvent> {
        let mut events = vec![SemanticEvent::HallCall(call)];
        self.last_call = Some(call);
        if !valid_call(call) {
            self.decision = None;
            events.push(SemanticEvent::InvalidCall);
            return events;
        }
        match self.calls.register(call) {
            CallRegistration::Created(id) => {
                self.decision = None;
                events.push(SemanticEvent::CallRegistered(id));
            }
            CallRegistration::Duplicate(id) => {
                events.push(SemanticEvent::CallDeduplicated(id));
                if self
                    .calls
                    .get(id)
                    .is_some_and(|record| record.state != CallState::Pending)
                {
                    return events;
                }
                self.decision = None;
            }
        }
        events.extend(self.dispatch_pending());
        events
    }

    fn dispatch_pending(&mut self) -> Vec<SemanticEvent> {
        let pending: Vec<_> = self.calls.pending_calls().collect();
        let mut events = Vec::new();

        for (id, call) in pending {
            let states = self.runtime.car_states();
            let capabilities = states
                .iter()
                .filter_map(|car| {
                    self.runtime
                        .capabilities(&car.id)
                        .map(|caps| (car.id.clone(), caps))
                })
                .collect();
            let Some(intent) = dispatch(&call, &states, &capabilities) else {
                events.push(SemanticEvent::NoEligibleCar(id));
                continue;
            };
            let (Intent::ServeFloor { car, .. } | Intent::ParkAt { car, .. }) = &intent;
            let car = car.clone();
            events.push(SemanticEvent::DispatchSelected {
                id,
                car: car.clone(),
            });
            events.push(SemanticEvent::IntentGenerated {
                id,
                intent: intent.clone(),
            });
            self.decision = Some(intent.clone());
            match self.runtime.submit_intent(intent) {
                Ok(()) => {
                    let assigned = CallState::Assigned(car.clone());
                    assert!(self.calls.assign(id, car));
                    events.push(SemanticEvent::CallStateChanged {
                        id,
                        state: assigned,
                    });
                }
                Err(error) => events.push(SemanticEvent::RuntimeError(error.to_string())),
            }
        }
        events
    }

    pub fn replay(&mut self) {
        if self.recording_full {
            self.log("Replay unavailable: recording full; R reset to record a new session".into());
            return;
        }
        let first = run(&self.recording);
        let second = run(&self.recording);
        if first.events != second.events
            || first.app.runtime.car_states() != second.app.runtime.car_states()
            || first.app.runtime.car_states() != self.runtime.car_states()
            || first.app.steps != self.steps
            || first.app.last_call != self.last_call
            || first.app.decision != self.decision
            || first.app.calls != self.calls
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
            .runtime
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
        *self = Self::new();
        self.log("RESET: reference two-car simulator".into());
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
        SemanticEvent::InvalidCall => "Invalid hall call: 1F only UP; 8F only DOWN".into(),
        SemanticEvent::CallRegistered(id) => format!("Call {id} -> Pending"),
        SemanticEvent::CallDeduplicated(id) => format!("Active duplicate -> {id}"),
        SemanticEvent::CallStateChanged { id, state } => {
            format!("Call {id} -> {}", call_state_label(state))
        }
        SemanticEvent::NoEligibleCar(id) => format!("Call {id}: no eligible idle car"),
        SemanticEvent::DispatchSelected { id, car } => format!("Call {id}: dispatch -> {car}"),
        SemanticEvent::IntentGenerated { id, intent } => {
            format!("Call {id}: intent -> {}", intent_label(intent))
        }
        SemanticEvent::ModeChanged { car, mode } => format!("Car {car} -> {mode:?}"),
        SemanticEvent::RuntimeError(error) => format!("Runtime error: {error}"),
        SemanticEvent::Runtime(event) => event_label(event),
    }
}

fn event_label(event: &RuntimeEvent) -> String {
    match event {
        RuntimeEvent::IntentAccepted(intent) => format!("IntentAccepted {}", intent_label(intent)),
        RuntimeEvent::PositionChanged(car) => format!(
            "PositionChanged {} {:.2}F {:?} door={:?}",
            car.id, car.position.floor_position, car.motion, car.door
        ),
        RuntimeEvent::Arrived { car, floor } => format!("Arrived {car} at {floor}"),
        RuntimeEvent::DoorStateChanged { car, state } => {
            format!("DoorStateChanged {car} {state:?}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{CallId, CallState, CarId, DoorState};

    #[test]
    fn hall_calls_respect_building_boundaries() {
        for floor in 0..=9 {
            for direction in [Direction::Up, Direction::Down] {
                let expected = matches!(
                    (floor, direction),
                    (1, Direction::Up) | (8, Direction::Down) | (2..=7, _)
                );
                assert_eq!(
                    valid_call(HallCall {
                        floor: Floor::new(floor),
                        direction
                    }),
                    expected
                );
            }
        }
    }

    #[test]
    fn paused_single_steps_arrive_and_reset_clears_application_state() {
        let mut app = App::new();
        app.paused = true;
        app.call(Direction::Up);
        assert_eq!(
            app.decision,
            Some(Intent::ServeFloor {
                car: CarId::new("A"),
                floor: Floor::new(7)
            })
        );
        let before = app.runtime.car_states();
        app.advance();
        assert_eq!(app.steps, 0);
        assert_eq!(app.runtime.car_states(), before);
        app.step();
        assert_eq!(app.steps, 1);
        assert_eq!(app.runtime.car_states()[0].position.floor_position, 3.75);
        for _ in 0..5 {
            app.step();
        }
        let a = &app.runtime.car_states()[0];
        assert_eq!(a.position.floor_position, 7.0);
        assert_eq!(a.door, DoorState::Open);
        assert_eq!(
            app.calls.get(CallId::new(1)).map(|record| &record.state),
            Some(&CallState::Assigned(CarId::new("A")))
        );
        assert!(
            app.trace
                .iter()
                .any(|line| line.contains("Arrived A at 7F"))
        );
        app.reset();
        assert_eq!(
            app.runtime.car_states(),
            SimRuntime::reference_two_car().car_states()
        );
        assert!(app.decision.is_none() && app.last_call.is_none());
        assert!(app.calls.calls().next().is_none());
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
        let states = app.runtime.car_states();

        let events = app.execute(Command::HallCall(HallCall {
            floor: Floor::new(7),
            direction: Direction::Up,
        }));

        assert!(events.contains(&SemanticEvent::CallDeduplicated(CallId::new(1))));
        assert_eq!(app.calls.calls().count(), 1);
        assert_eq!(app.runtime.car_states(), states);
    }

    #[test]
    fn invalid_calls_do_not_submit_and_trace_is_bounded() {
        let mut app = App::new();
        app.select_floor(1);
        for _ in 0..150 {
            app.call(Direction::Up);
        }
        assert!(app.decision.is_none());
        assert!(
            app.runtime
                .car_states()
                .iter()
                .all(|car| car.target.is_none())
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
