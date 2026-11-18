use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};

use liftassistant_model::{
    CarId, CarObservationUpdate, ExecutionCapabilities, Floor, Intent, IntentId,
    ObservationSessionId, ObservationSourceId,
};
use liftassistant_runtime::{
    ControlledIntentSink, ControllerId, IntentReceipt, ReceiptStatus, RuntimeEvent,
    SimulationDriver, Submission, WithdrawalOutcome,
};

use crate::SimRuntime;

const RECENT_RECEIPTS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VirtualReceipt {
    After(Duration),
    Lost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VirtualExecution {
    After(Duration),
    Stalled,
    Diverges(Floor),
    NoEffect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtualBehavior {
    pub reject: bool,
    pub receipt: VirtualReceipt,
    pub execution: VirtualExecution,
}

impl Default for VirtualBehavior {
    fn default() -> Self {
        Self {
            reject: false,
            receipt: VirtualReceipt::After(Duration::ZERO),
            execution: VirtualExecution::After(Duration::ZERO),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Execution {
    Queued,
    Moving,
    Finished,
}

#[derive(Debug)]
struct Operation {
    id: IntentId,
    intent: Intent,
    behavior: VirtualBehavior,
    execution: Execution,
    receipt_sent: bool,
    indeterminate_reported: bool,
}

impl Operation {
    fn status(&self) -> ReceiptStatus {
        if self.behavior.reject {
            ReceiptStatus::Rejected
        } else {
            ReceiptStatus::Accepted
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct VirtualStep {
    pub receipts: Vec<IntentReceipt>,
    pub observations: Vec<CarObservationUpdate>,
}

/// Deterministic lower controller for the simulated world. It owns delivery and accepted
/// jobs. SimRuntime owns physical movement and produces all canonical observations.
#[derive(Debug)]
pub struct VirtualController {
    id: ControllerId,
    world: SimRuntime,
    connected: bool,
    can_withdraw: bool,
    next_behavior: VirtualBehavior,
    operations: BTreeMap<CarId, Operation>,
    recent: VecDeque<IntentReceipt>,
    last_id: Option<IntentId>,
    submissions: u64,
}

impl VirtualController {
    pub fn new(id: ControllerId, world: SimRuntime, can_withdraw: bool) -> Self {
        Self {
            id,
            world,
            connected: true,
            can_withdraw,
            next_behavior: VirtualBehavior::default(),
            operations: BTreeMap::new(),
            recent: VecDeque::new(),
            last_id: None,
            submissions: 0,
        }
    }

    pub fn set_next_behavior(&mut self, behavior: VirtualBehavior) {
        self.next_behavior = behavior;
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }

    /// Replace the endpoint, losing its notification queue and volatile fault
    /// settings. The accepted jobs, receipt lookup, and world survive this fixture.
    pub fn restart(mut self) -> Self {
        self.connected = false;
        self.next_behavior = VirtualBehavior::default();
        for operation in self
            .operations
            .values_mut()
            .filter(|operation| !operation.receipt_sent)
        {
            operation.behavior.receipt = VirtualReceipt::Lost;
        }
        self
    }

    pub fn submissions(&self) -> u64 {
        self.submissions
    }

    pub fn observation_source(&self) -> (ObservationSourceId, ObservationSessionId) {
        (self.world.source.clone(), self.world.source_session.clone())
    }

    pub fn execution_capabilities(&self) -> Vec<(CarId, ExecutionCapabilities)> {
        self.world.all_execution_capabilities()
    }

    pub fn observe_current(&mut self) -> Vec<CarObservationUpdate> {
        self.world.take_observation_updates();
        for state in self.world.car_states() {
            self.world.queue_observation(state);
        }
        self.world.take_observation_updates()
    }

    /// Advance the world for this interval, then release queued jobs at the tick
    /// boundary. Newly released jobs move on the following interval.
    pub fn advance(&mut self, elapsed: Duration) -> VirtualStep {
        let mut step = VirtualStep::default();
        for event in self.world.advance(elapsed) {
            match event {
                RuntimeEvent::Observation(observation) => step.observations.push(observation),
                RuntimeEvent::Arrived { car, .. } => {
                    if let Some(operation) = self.operations.get_mut(&car) {
                        operation.execution = Execution::Finished;
                    }
                }
                _ => {}
            }
        }
        for operation in self.operations.values_mut() {
            if !operation.receipt_sent {
                match &mut operation.behavior.receipt {
                    VirtualReceipt::After(remaining) => {
                        *remaining = remaining.saturating_sub(elapsed);
                        if remaining.is_zero() && self.connected {
                            operation.receipt_sent = true;
                            step.receipts.push(IntentReceipt {
                                controller: self.id.clone(),
                                intent: operation.id,
                                status: operation.status(),
                            });
                        } else if !self.connected && !operation.indeterminate_reported {
                            operation.indeterminate_reported = true;
                            step.receipts.push(IntentReceipt {
                                controller: self.id.clone(),
                                intent: operation.id,
                                status: ReceiptStatus::Indeterminate,
                            });
                        }
                    }
                    VirtualReceipt::Lost => {}
                }
            }
            if operation.execution != Execution::Queued {
                continue;
            }
            operation.execution = Self::start_when_due(&mut self.world, operation, elapsed);
        }
        step.observations
            .extend(self.world.take_observation_updates());
        self.operations.retain(|_, operation| {
            let retire = operation.execution == Execution::Finished
                && (operation.receipt_sent || operation.behavior.receipt == VirtualReceipt::Lost);
            if retire {
                if self.recent.len() == RECENT_RECEIPTS {
                    self.recent.pop_front();
                }
                self.recent.push_back(IntentReceipt {
                    controller: self.id.clone(),
                    intent: operation.id,
                    status: operation.status(),
                });
            }
            !retire
        });
        step
    }

    fn start_when_due(
        world: &mut SimRuntime,
        operation: &mut Operation,
        elapsed: Duration,
    ) -> Execution {
        let mut intent = operation.intent.clone();
        match &mut operation.behavior.execution {
            VirtualExecution::After(remaining) => {
                *remaining = remaining.saturating_sub(elapsed);
                if !remaining.is_zero() {
                    return Execution::Queued;
                }
            }
            VirtualExecution::NoEffect => return Execution::Finished,
            VirtualExecution::Diverges(target) => match &mut intent {
                Intent::ServeFloor { floor, .. } | Intent::ParkAt { floor, .. } => *floor = *target,
            },
            VirtualExecution::Stalled => {}
        }
        if world.set_target(&intent).is_err() {
            return Execution::Finished;
        }
        if operation.behavior.execution == VirtualExecution::Stalled {
            // Retain the moving observation and suppress further physical progress.
            world
                .cars
                .get_mut(intent.car())
                .expect("simulated car")
                .desired_target = None;
        }
        Execution::Moving
    }
}

impl ControlledIntentSink for VirtualController {
    fn controller_id(&self) -> &ControllerId {
        &self.id
    }

    fn available(&self, car: &CarId) -> bool {
        self.connected && self.world.cars.contains_key(car)
    }

    fn submit(&mut self, id: IntentId, intent: &Intent) -> Submission {
        let car = intent.car();
        if !self.available(car)
            || self.operations.contains_key(car)
            || self.last_id.is_some_and(|last| id <= last)
        {
            return Submission::NotSubmitted;
        }
        self.last_id = Some(id);
        self.submissions = self
            .submissions
            .checked_add(1)
            .expect("virtual submission count exhausted");
        let behavior = std::mem::take(&mut self.next_behavior);
        self.operations.insert(
            car.clone(),
            Operation {
                id,
                intent: intent.clone(),
                behavior,
                execution: if behavior.reject {
                    Execution::Finished
                } else {
                    Execution::Queued
                },
                receipt_sent: false,
                indeterminate_reported: false,
            },
        );
        if behavior.receipt == VirtualReceipt::Lost {
            Submission::Indeterminate
        } else {
            Submission::Submitted
        }
    }

    fn withdraw(&mut self, id: IntentId) -> WithdrawalOutcome {
        if !self.can_withdraw || !self.connected {
            return WithdrawalOutcome::Unsupported;
        }
        if let Some(operation) = self
            .operations
            .values_mut()
            .find(|operation| operation.id == id)
        {
            if operation.execution == Execution::Queued {
                operation.execution = Execution::Finished;
                return WithdrawalOutcome::Prevented;
            }
        }
        WithdrawalOutcome::Requested
    }

    fn receipt(&self, id: IntentId) -> Option<IntentReceipt> {
        if !self.connected {
            return None;
        }
        self.operations
            .values()
            .find(|operation| operation.id == id)
            .map(|operation| IntentReceipt {
                controller: self.id.clone(),
                intent: id,
                status: operation.status(),
            })
            .or_else(|| {
                self.recent
                    .iter()
                    .find(|receipt| receipt.intent == id)
                    .cloned()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_and_lost_delivery_are_independent_of_simulated_movement() {
        let mut controller = VirtualController::new(
            ControllerId::new("virtual"),
            SimRuntime::reference_two_car(),
            true,
        );
        controller.observe_current();
        let operation = Intent::ParkAt {
            car: CarId::new("A"),
            floor: Floor::new(6),
        };
        let id = IntentId::new(1);
        controller.set_next_behavior(VirtualBehavior {
            receipt: VirtualReceipt::Lost,
            execution: VirtualExecution::After(Duration::from_secs(2)),
            ..VirtualBehavior::default()
        });
        assert_eq!(controller.submit(id, &operation), Submission::Indeterminate);
        assert_eq!(controller.submit(id, &operation), Submission::NotSubmitted);
        assert_eq!(controller.submissions(), 1);
        assert_eq!(
            controller.advance(Duration::from_secs(1)),
            VirtualStep::default()
        );
        let starts = controller.advance(Duration::from_secs(1));
        assert!(starts.receipts.is_empty());
        assert!(!starts.observations.is_empty());
        let finishes = controller.advance(Duration::from_secs(3));
        assert!(finishes.receipts.is_empty());
        assert_eq!(
            controller.receipt(id).unwrap().status,
            ReceiptStatus::Accepted
        );
        assert!(matches!(
            controller.world.car_states()[0].motion,
            liftassistant_model::MotionState::Stopped
        ));
        assert_eq!(
            controller.world.car_states()[0].location.floor_reference(),
            operation.floor()
        );
    }
}
