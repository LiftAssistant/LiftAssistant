#![forbid(unsafe_code)]

mod adapter;
mod gateway;
mod live;
mod mapping;
mod private_ipc;

pub use adapter::{
    MappedRmfUpdate, RmfAdapterConfigError, RmfAdapterError, RmfLiftBinding, RmfObservationAdapter,
};
pub use gateway::{
    CommissioningState, DurableDelivery, DurableGatewayOperation, DurableResumptionRequirement,
    GatewayCommandRequest, GatewayCommandResponse, GatewayConfig, GatewayConfigError,
    GatewayIngestError, GatewayOperationalSnapshot, GatewayReadiness, GatewayRecoveryInspection,
    GatewayStoreError, RecoveryAuditAction, RecoveryAuditRecord, RecoveryOperatorConfig,
    RemoteInactivityEvidence, RmfCommandTransport, RmfGatewayCapabilities, RmfIntentGateway,
    load_gateway_config,
};
pub use live::{
    RmfFreshnessPolicy, RmfFreshnessPolicyError, RmfLiftCommand, RmfLiftRequestPublisher,
    RmfLivePayload, RmfLiveSubscriber, RmfReceiveControl, RmfReceiveOutcome, RmfTransportError,
};
pub use mapping::{
    MappedRmfObservation, RmfInteractionModeProjection, RmfLiftState, RmfMappingError,
    RmfSourceContext, map_rmf_lift_state, map_rmf_lift_state_with_projection,
};
pub use private_ipc::{
    LocalControlDomainLease, PrivateIngress, PrivateIngressConnection, PrivateIngressError,
};
