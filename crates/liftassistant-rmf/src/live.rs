use std::{error::Error, fmt};

use futures::StreamExt;
use ros2_client::{
    Context, ContextOptions, Message, MessageTypeName, Name, Node, NodeName, NodeOptions,
    Subscription, ros2,
};
use serde::{Deserialize, Serialize};

use crate::RmfLiftState;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RmfLiftStateMessage {
    lift_time: ros2_client::builtin_interfaces::Time,
    lift_name: String,
    available_floors: Vec<String>,
    current_floor: String,
    destination_floor: String,
    door_state: u8,
    motion_state: u8,
    available_modes: Vec<u8>,
    current_mode: u8,
    session_id: String,
}

impl Message for RmfLiftStateMessage {}

impl RmfLiftStateMessage {
    fn into_payload(self) -> RmfLivePayload {
        RmfLivePayload {
            source_time_nanos: self.lift_time.to_nanos(),
            state: RmfLiftState {
                lift_name: self.lift_name,
                available_floors: self.available_floors,
                current_floor: self.current_floor,
                destination_floor: self.destination_floor,
                door_state: self.door_state,
                motion_state: self.motion_state,
                available_modes: self.available_modes,
                current_mode: self.current_mode,
                session_id: self.session_id,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RmfLivePayload {
    pub source_time_nanos: i64,
    pub state: RmfLiftState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RmfReceiveControl {
    Continue,
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RmfTransportError {
    Create(String),
    Receive(String),
}

impl fmt::Display for RmfTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create(message) => write!(f, "failed to create Open-RMF subscription: {message}"),
            Self::Receive(message) => write!(f, "Open-RMF subscription receive failed: {message}"),
        }
    }
}

impl Error for RmfTransportError {}

/// Read-only Open-RMF `/lift_states` subscriber.
///
/// This type owns no publisher and implements no LiftAssistant intent/write contract.
pub struct RmfLiveSubscriber {
    node: Node,
    subscription: Subscription<RmfLiftStateMessage>,
}

impl RmfLiveSubscriber {
    pub fn connect(domain_id: u16, node_name: &str) -> Result<Self, RmfTransportError> {
        let context = Context::with_options(ContextOptions::new().domain_id(domain_id))
            .map_err(|error| RmfTransportError::Create(error.to_string()))?;
        let mut node = context
            .new_node(
                NodeName::new("/liftassistant", node_name)
                    .map_err(|error| RmfTransportError::Create(error.to_string()))?,
                NodeOptions::new(),
            )
            .map_err(|error| RmfTransportError::Create(error.to_string()))?;
        let qos = ros2::QosPolicyBuilder::new()
            .history(ros2::policy::History::KeepLast { depth: 100 })
            .reliability(ros2::policy::Reliability::BestEffort)
            .durability(ros2::policy::Durability::Volatile)
            .build();
        let topic = node
            .create_topic(
                &Name::new("/", "lift_states")
                    .map_err(|error| RmfTransportError::Create(error.to_string()))?,
                MessageTypeName::new("rmf_lift_msgs", "LiftState"),
                &qos,
            )
            .map_err(|error| RmfTransportError::Create(error.to_string()))?;
        let subscription = node
            .create_subscription(&topic, Some(qos))
            .map_err(|error| RmfTransportError::Create(error.to_string()))?;

        Ok(Self { node, subscription })
    }

    pub fn receive_until(
        mut self,
        mut handler: impl FnMut(RmfLivePayload) -> RmfReceiveControl,
    ) -> Result<(), RmfTransportError> {
        let spinner = self
            .node
            .spinner()
            .map_err(|error| RmfTransportError::Create(error.to_string()))?;
        let receive = async {
            let stream = self.subscription.async_stream();
            futures::pin_mut!(stream);
            while let Some(result) = stream.next().await {
                let (message, _) =
                    result.map_err(|error| RmfTransportError::Receive(error.to_string()))?;
                if handler(message.into_payload()) == RmfReceiveControl::Stop {
                    return Ok(());
                }
            }
            Err(RmfTransportError::Receive(
                "subscription stream ended unexpectedly".into(),
            ))
        };
        let spin = async {
            spinner
                .spin()
                .await
                .map_err(|error| RmfTransportError::Receive(error.to_string()))
        };
        let executor = smol::Executor::new();
        smol::block_on(executor.run(smol::future::race(receive, spin)))
    }
}
