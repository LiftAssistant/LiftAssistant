use std::{
    error::Error,
    fmt,
    time::{Duration, Instant},
};

use futures::StreamExt;
use ros2_client::{
    Context, ContextOptions, Message, MessageTypeName, Name, Node, NodeName, NodeOptions,
    Publisher, Subscription, builtin_interfaces::Time, ros2,
};
use serde::{Deserialize, Serialize};

use crate::RmfLiftState;

const MAX_DDS_DOMAIN_ID: u16 = 232;

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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RmfLiftRequestMessage {
    lift_name: String,
    request_time: Time,
    session_id: String,
    request_type: u8,
    destination_floor: String,
    door_state: u8,
}

impl Message for RmfLiftRequestMessage {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RmfLiftCommand {
    pub lift_name: String,
    pub request_time_nanos: i64,
    pub session_id: String,
    pub destination_floor: String,
}

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

/// Explicit source-specific payload-silence policy.
///
/// This is an adapter/runtime setting, not a universal LiftAssistant freshness deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RmfFreshnessPolicy {
    max_payload_silence: Duration,
}

impl RmfFreshnessPolicy {
    pub fn new(max_payload_silence: Duration) -> Result<Self, RmfFreshnessPolicyError> {
        if max_payload_silence.is_zero() {
            return Err(RmfFreshnessPolicyError::ZeroPayloadSilence);
        }
        Ok(Self {
            max_payload_silence,
        })
    }

    pub const fn max_payload_silence(self) -> Duration {
        self.max_payload_silence
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RmfFreshnessPolicyError {
    ZeroPayloadSilence,
}

impl fmt::Display for RmfFreshnessPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroPayloadSilence => {
                f.write_str("Open-RMF max payload silence must be greater than zero")
            }
        }
    }
}

impl Error for RmfFreshnessPolicyError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RmfReceiveOutcome {
    HandlerStopped,
    PayloadStale { max_silence: Duration },
    ShutdownRequested,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RmfTransportError {
    Create(String),
    Receive(String),
    Publish(String),
}

impl fmt::Display for RmfTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create(message) => write!(f, "failed to create Open-RMF subscription: {message}"),
            Self::Receive(message) => write!(f, "Open-RMF subscription receive failed: {message}"),
            Self::Publish(message) => write!(f, "Open-RMF request publish failed: {message}"),
        }
    }
}

impl Error for RmfTransportError {}

/// Open-RMF `/lift_requests` publisher used only by the private M7 gateway process.
pub struct RmfLiftRequestPublisher {
    node: Node,
    publisher: Publisher<RmfLiftRequestMessage>,
}

impl RmfLiftRequestPublisher {
    pub fn connect(domain_id: u16, node_name: &str) -> Result<Self, RmfTransportError> {
        if domain_id > MAX_DDS_DOMAIN_ID {
            return Err(RmfTransportError::Create(format!(
                "ROS 2 DDS domain ID {domain_id} exceeds the maximum {MAX_DDS_DOMAIN_ID}"
            )));
        }
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
            .history(ros2::policy::History::KeepLast { depth: 1 })
            .reliability(ros2::policy::Reliability::Reliable {
                max_blocking_time: ros2::Duration::from_millis(100),
            })
            .durability(ros2::policy::Durability::Volatile)
            .build();
        let topic = node
            .create_topic(
                &Name::new("/", "lift_requests")
                    .map_err(|error| RmfTransportError::Create(error.to_string()))?,
                MessageTypeName::new("rmf_lift_msgs", "LiftRequest"),
                &qos,
            )
            .map_err(|error| RmfTransportError::Create(error.to_string()))?;
        let publisher = node
            .create_publisher(&topic, Some(qos))
            .map_err(|error| RmfTransportError::Create(error.to_string()))?;
        let spinner = node
            .spinner()
            .map_err(|error| RmfTransportError::Create(error.to_string()))?;
        smol::spawn(async move {
            let _ = spinner.spin().await;
        })
        .detach();
        Ok(Self { node, publisher })
    }

    pub fn has_subscriber(&self) -> bool {
        self.publisher.get_subscription_count(&self.node) > 0
    }

    pub fn publish(&self, command: RmfLiftCommand) -> Result<(), RmfTransportError> {
        self.publisher
            .publish(RmfLiftRequestMessage {
                lift_name: command.lift_name,
                request_time: Time::from_nanos(command.request_time_nanos),
                session_id: command.session_id,
                request_type: 1, // REQUEST_AGV_MODE
                destination_floor: command.destination_floor,
                door_state: 2, // DOOR_OPEN
            })
            .map_err(|error| RmfTransportError::Publish(format!("{error:?}")))
    }
}

/// Read-only Open-RMF `/lift_states` subscriber.
///
/// This type owns no publisher and implements no LiftAssistant intent/write contract.
pub struct RmfLiveSubscriber {
    node: Node,
    subscription: Subscription<RmfLiftStateMessage>,
}

impl RmfLiveSubscriber {
    pub fn connect(domain_id: u16, node_name: &str) -> Result<Self, RmfTransportError> {
        if domain_id > MAX_DDS_DOMAIN_ID {
            return Err(RmfTransportError::Create(format!(
                "ROS 2 DDS domain ID {domain_id} exceeds the maximum {MAX_DDS_DOMAIN_ID}"
            )));
        }
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
        self,
        mut handler: impl FnMut(RmfLivePayload) -> RmfReceiveControl,
    ) -> Result<(), RmfTransportError> {
        match self.receive(None, None, None, |payload| {
            payload.map_or(RmfReceiveControl::Continue, &mut handler)
        })? {
            RmfReceiveOutcome::HandlerStopped => Ok(()),
            RmfReceiveOutcome::PayloadStale { .. } | RmfReceiveOutcome::ShutdownRequested => {
                unreachable!("receive_until has no payload-silence policy")
            }
        }
    }

    /// Receives payloads until the handler stops or an active data plane becomes stale.
    ///
    /// The silence timer arms only after this subscription receives its first payload. Before
    /// that point the reader is still awaiting discovery/data and cannot truthfully be called
    /// stale.
    pub fn receive_until_stale(
        self,
        policy: RmfFreshnessPolicy,
        mut handler: impl FnMut(RmfLivePayload) -> RmfReceiveControl,
    ) -> Result<RmfReceiveOutcome, RmfTransportError> {
        self.receive(Some(policy), None, None, |payload| {
            payload.map_or(RmfReceiveControl::Continue, &mut handler)
        })
    }

    /// Receives payloads with freshness and cooperative process-shutdown supervision.
    ///
    /// The nonzero poll interval bounds idle supervision ticks. The handler receives
    /// `None` on a tick and `Some` when a payload arrives.
    pub fn receive_until_stale_or_shutdown(
        self,
        policy: RmfFreshnessPolicy,
        shutdown_poll_interval: Duration,
        mut should_shutdown: impl FnMut() -> bool,
        handler: impl FnMut(Option<RmfLivePayload>) -> RmfReceiveControl,
    ) -> Result<RmfReceiveOutcome, RmfTransportError> {
        if shutdown_poll_interval.is_zero() {
            return Err(RmfTransportError::Create(
                "Open-RMF shutdown poll interval must be greater than zero".into(),
            ));
        }
        self.receive(
            Some(policy),
            Some(shutdown_poll_interval),
            Some(&mut should_shutdown),
            handler,
        )
    }

    fn receive(
        mut self,
        freshness: Option<RmfFreshnessPolicy>,
        shutdown_poll_interval: Option<Duration>,
        mut should_shutdown: Option<&mut dyn FnMut() -> bool>,
        mut handler: impl FnMut(Option<RmfLivePayload>) -> RmfReceiveControl,
    ) -> Result<RmfReceiveOutcome, RmfTransportError> {
        let spinner = self
            .node
            .spinner()
            .map_err(|error| RmfTransportError::Create(error.to_string()))?;
        let receive = async {
            let stream = self.subscription.async_stream();
            futures::pin_mut!(stream);
            let mut last_payload_at = None;
            loop {
                if should_shutdown.as_mut().is_some_and(|check| check()) {
                    return Ok(RmfReceiveOutcome::ShutdownRequested);
                }
                let freshness_remaining = last_payload_at.and_then(|last: Instant| {
                    freshness
                        .map(|policy| policy.max_payload_silence().saturating_sub(last.elapsed()))
                });
                let wait = match (freshness_remaining, shutdown_poll_interval) {
                    (Some(freshness), Some(shutdown)) => Some(freshness.min(shutdown)),
                    (Some(freshness), None) => Some(freshness),
                    (None, Some(shutdown)) => Some(shutdown),
                    (None, None) => None,
                };
                let next = if let Some(wait) = wait {
                    smol::future::race(async { Some(stream.next().await) }, async {
                        smol::Timer::after(wait).await;
                        None
                    })
                    .await
                } else {
                    Some(stream.next().await)
                };
                let Some(next) = next else {
                    if should_shutdown.as_mut().is_some_and(|check| check()) {
                        return Ok(RmfReceiveOutcome::ShutdownRequested);
                    }
                    if let Some((policy, _)) = freshness
                        .zip(last_payload_at)
                        .filter(|(policy, last)| last.elapsed() >= policy.max_payload_silence())
                    {
                        return Ok(RmfReceiveOutcome::PayloadStale {
                            max_silence: policy.max_payload_silence(),
                        });
                    }
                    if handler(None) == RmfReceiveControl::Stop {
                        return Ok(RmfReceiveOutcome::HandlerStopped);
                    }
                    continue;
                };
                let Some(result) = next else {
                    return Err(RmfTransportError::Receive(
                        "subscription stream ended unexpectedly".into(),
                    ));
                };
                let (message, _) =
                    result.map_err(|error| RmfTransportError::Receive(error.to_string()))?;
                last_payload_at = Some(Instant::now());
                if handler(Some(message.into_payload())) == RmfReceiveControl::Stop {
                    return Ok(RmfReceiveOutcome::HandlerStopped);
                }
            }
        };
        let spin = async {
            spinner
                .spin()
                .await
                .map_err(|error| RmfTransportError::Receive(error.to_string()))?;
            Err(RmfTransportError::Receive(
                "Open-RMF node spinner ended unexpectedly".into(),
            ))
        };
        let executor = smol::Executor::new();
        smol::block_on(executor.run(smol::future::race(receive, spin)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_policy_is_explicit_and_rejects_a_disabled_deadline() {
        assert_eq!(
            RmfFreshnessPolicy::new(Duration::ZERO),
            Err(RmfFreshnessPolicyError::ZeroPayloadSilence)
        );
        let policy = RmfFreshnessPolicy::new(Duration::from_secs(3))
            .expect("a source-specific nonzero silence limit is valid");
        assert_eq!(policy.max_payload_silence(), Duration::from_secs(3));
    }

    #[test]
    fn invalid_dds_domain_is_rejected_before_transport_creation() {
        let error = RmfLiveSubscriber::connect(233, "invalid_domain_fixture")
            .err()
            .expect("DDS domains above 232 must not reach RustDDS port arithmetic");
        assert_eq!(
            error,
            RmfTransportError::Create("ROS 2 DDS domain ID 233 exceeds the maximum 232".into())
        );
    }
}
