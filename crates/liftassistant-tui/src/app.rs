use std::{collections::VecDeque, time::Duration};

use liftassistant_core::dispatch;
use liftassistant_model::{Direction, Floor, HallCall, Intent};
use liftassistant_runtime::{ElevatorRuntime, RuntimeEvent};
use liftassistant_sim::SimRuntime;

pub const FIXED_STEP: Duration = Duration::from_millis(500);
const TRACE_LIMIT: usize = 100;

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
    pub trace: VecDeque<String>,
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
            trace: VecDeque::new(),
        }
    }

    pub fn select_floor(&mut self, delta: i16) {
        self.selected_floor = Floor::new((self.selected_floor.index() + delta).clamp(1, 8));
    }

    pub fn call(&mut self, direction: Direction) {
        let call = HallCall {
            floor: self.selected_floor,
            direction,
        };
        self.last_call = Some(call);
        self.decision = None;
        self.log(format!(
            "HallCall {} {}",
            call.floor,
            direction_name(direction)
        ));
        if !valid_call(call) {
            self.log("Invalid hall call: 1F only UP; 8F only DOWN".into());
            return;
        }
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
            self.log("No eligible car".into());
            return;
        };
        let Intent::ServeFloor { car, .. } = &intent else {
            return;
        };
        self.log(format!("Dispatch -> {car}"));
        self.log(format!("Intent -> {}", intent_label(&intent)));
        self.decision = Some(intent.clone());
        if let Err(error) = self.runtime.submit_intent(intent) {
            self.log(format!("Runtime error: {error}"));
        }
    }

    pub fn advance(&mut self) {
        if !self.paused {
            self.step();
        }
    }

    pub fn step(&mut self) {
        self.steps += 1;
        for event in self.runtime.tick(FIXED_STEP) {
            self.log(event_label(&event));
        }
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
    use liftassistant_model::DoorState;

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
                car: liftassistant_model::CarId::new("A"),
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
        assert_eq!(app.steps, 0);
        assert_eq!(app.trace.len(), 1);
        assert_eq!(app.selected_floor, Floor::new(7));
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
