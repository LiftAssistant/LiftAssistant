use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use liftassistant_engine::{
    Admission, AuthorityGrant, ControlledIntentRecovery, Engine, ObservationBindingRole,
    PrincipalId, Receipt, ReceiptResult, Reconciliation, WriteBlocker,
};
use liftassistant_model::{
    BuildingTopology, CarId, CarLocation, CarObservationUpdate, CarState, Direction, DoorState,
    ExecutionCapabilities, Floor, Intent, IntentId, MotionState, ObservationSessionId,
    ObservationSourceId, OperatingMode, ReportedDestination,
};
use liftassistant_runtime::{
    ControlledIntentSink, ControllerId, IntentReceipt, ReceiptStatus, Submission,
};
use serde::{Deserialize, Serialize};

use crate::{
    LocalControlDomainLease, RmfAdapterError, RmfInteractionModeProjection, RmfLiftBinding,
    RmfLiftCommand, RmfLivePayload, RmfObservationAdapter,
};

const STORE_VERSION: u32 = 2;
const MAX_STORE_BYTES: u64 = 64 * 1024;
const MAX_RECOVERY_AUDIT_RECORDS: usize = 32;
const MAX_RECOVERY_REASON_BYTES: usize = 512;
const HOLDER_PREFIX: &str = "liftassistant-m7";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecoveryOperatorConfig {
    pub allowed_uid: u32,
    pub principal: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GatewayConfig {
    pub socket_path: PathBuf,
    pub state_path: PathBuf,
    pub config_revision: String,
    pub allowed_uid: u32,
    pub principal: String,
    #[serde(default)]
    pub recovery_operator: Option<RecoveryOperatorConfig>,
    pub domain_id: u16,
    pub source_id: String,
    pub logical_car: String,
    pub native_lift: String,
    pub floors: BTreeMap<String, i16>,
    pub max_payload_silence_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GatewayConfigError {
    Empty(&'static str),
    NoFloors,
    DuplicateLogicalFloor(i16),
    ZeroFreshness,
    InvalidTopology,
    Adapter(String),
    Engine(String),
    ControlDomain(String),
    Store(GatewayStoreError),
}

impl fmt::Display for GatewayConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty(field) => write!(f, "gateway configuration field {field} is empty"),
            Self::NoFloors => f.write_str("gateway requires at least one explicit RMF floor map"),
            Self::DuplicateLogicalFloor(floor) => {
                write!(
                    f,
                    "gateway maps more than one RMF label to logical floor {floor}"
                )
            }
            Self::ZeroFreshness => f.write_str("gateway freshness interval must be nonzero"),
            Self::InvalidTopology => f.write_str("gateway floor map produced invalid topology"),
            Self::Adapter(message) => {
                write!(f, "gateway RMF adapter configuration failed: {message}")
            }
            Self::Engine(message) => write!(f, "gateway Engine configuration failed: {message}"),
            Self::ControlDomain(message) => {
                write!(f, "gateway control-domain lease failed: {message}")
            }
            Self::Store(error) => write!(f, "gateway durable state failed: {error}"),
        }
    }
}

impl Error for GatewayConfigError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayReadiness {
    Starting,
    Synchronizing,
    Ready,
    Degraded,
    RecoveryRequired,
    ResumptionRequired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableDelivery {
    ReservedUncertain,
    SentUnconfirmed,
    Correlated,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommissioningState {
    AwaitingObservation,
    Compatible,
    Contradictory,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAuditAction {
    RetiredOutcomeUnknown,
    ResumedWithResidualUncertainty,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteInactivityEvidence {
    Uncertain,
    UncertainAccepted,
}

/// Concrete capability declaration for the first RMF gateway, not a generic DSL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RmfGatewayCapabilities {
    pub ownership: &'static str,
    pub logical_car: CarId,
    pub native_lift: String,
    pub served_floors: Vec<Floor>,
    pub holder_correlation: bool,
    pub observation_source: ObservationSourceId,
    pub withdrawal_supported: bool,
    pub reconnect_without_resend: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DurableGatewayOperation {
    pub intent_id: u64,
    pub logical_car: String,
    pub target_floor: i16,
    pub native_lift: String,
    pub native_floor: String,
    pub rmf_session_id: String,
    pub started_floor: i16,
    pub delivery: DurableDelivery,
    pub saw_progress: bool,
    pub saw_closed_at_target: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayOperationalSnapshot {
    pub floor: i16,
    pub location_precision: String,
    pub motion: String,
    pub door: String,
    pub mode: String,
    pub reported_destination: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryAuditRecord {
    pub sequence: u64,
    pub operation: DurableGatewayOperation,
    pub action: RecoveryAuditAction,
    pub principal: String,
    pub config_revision: String,
    pub control_domain: String,
    pub prior_delivery: DurableDelivery,
    pub prior_saw_progress: bool,
    pub prior_saw_closed_at_target: bool,
    pub operational_state: Option<GatewayOperationalSnapshot>,
    pub observed_holder: Option<String>,
    pub remote_inactivity: RemoteInactivityEvidence,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DurableResumptionRequirement {
    pub operation: DurableGatewayOperation,
    pub retired_config_revision: String,
    pub control_domain: String,
    pub retirement_audit_sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct GatewayDiskState {
    version: u32,
    config_revision: String,
    last_intent_id: u64,
    unresolved: Option<DurableGatewayOperation>,
    resumption_required: Option<DurableResumptionRequirement>,
    audit_sequence: u64,
    recovery_audit: VecDeque<RecoveryAuditRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct GatewayDiskStateV1 {
    version: u32,
    config_revision: String,
    last_intent_id: u64,
    unresolved: Option<DurableGatewayOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GatewayStoreError {
    Io(String),
    Oversized(u64),
    Corrupt(String),
    Version(u32),
    ConfigRevision { stored: String, configured: String },
    Conflict,
    MissingOperation,
    WrongOperation { expected: u64, received: u64 },
    ResumptionAlreadyRequired,
    MissingResumptionRequirement,
    IdentityExhausted,
}

impl fmt::Display for GatewayStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "durable gateway I/O failed: {message}"),
            Self::Oversized(size) => write!(f, "durable gateway state is oversized: {size} bytes"),
            Self::Corrupt(message) => write!(f, "durable gateway state is corrupt: {message}"),
            Self::Version(version) => write!(f, "unsupported durable gateway version {version}"),
            Self::ConfigRevision { stored, configured } => write!(
                f,
                "durable gateway config revision {stored:?} does not match {configured:?}"
            ),
            Self::Conflict => f.write_str("an unresolved gateway operation already exists"),
            Self::MissingOperation => f.write_str("no unresolved gateway operation exists"),
            Self::WrongOperation { expected, received } => write!(
                f,
                "durable gateway operation mismatch: expected {expected}, received {received}"
            ),
            Self::ResumptionAlreadyRequired => {
                f.write_str("gateway already requires an explicit safe-resumption decision")
            }
            Self::MissingResumptionRequirement => {
                f.write_str("gateway has no retired operation awaiting safe resumption")
            }
            Self::IdentityExhausted => f.write_str("gateway recovery audit identity exhausted"),
        }
    }
}

impl Error for GatewayStoreError {}

#[derive(Debug)]
pub enum GatewayIngestError {
    Adapter(RmfAdapterError),
    Commissioning(String),
    Store(GatewayStoreError),
}

impl fmt::Display for GatewayIngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter(error) => write!(f, "RMF observation rejected: {error}"),
            Self::Commissioning(error) => {
                write!(f, "RMF gateway commissioning evidence rejected: {error}")
            }
            Self::Store(error) => write!(f, "gateway recovery state update failed: {error}"),
        }
    }
}

impl Error for GatewayIngestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Adapter(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Commissioning(_) => None,
        }
    }
}

impl From<io::Error> for GatewayStoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

struct GatewayStore {
    path: PathBuf,
    state: GatewayDiskState,
}

impl GatewayStore {
    fn open(path: PathBuf, config_revision: &str) -> Result<Self, GatewayStoreError> {
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(GatewayStoreError::Corrupt(
                "configured state path must not be a symbolic link".into(),
            ));
        }
        let state = match fs::metadata(&path) {
            Ok(metadata) => {
                if !metadata.is_file() {
                    return Err(GatewayStoreError::Corrupt(
                        "configured state path is not a regular file".into(),
                    ));
                }
                if metadata.len() > MAX_STORE_BYTES {
                    return Err(GatewayStoreError::Oversized(metadata.len()));
                }
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(GatewayStoreError::Corrupt(
                        "durable gateway state permissions must not grant group/other access"
                            .into(),
                    ));
                }
                let file = File::open(&path)?;
                let value: serde_json::Value = serde_json::from_reader(BufReader::new(file))
                    .map_err(|error| GatewayStoreError::Corrupt(error.to_string()))?;
                let version = value
                    .get("version")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| {
                        GatewayStoreError::Corrupt("durable state has no numeric version".into())
                    })?;
                match version {
                    1 => {
                        let legacy: GatewayDiskStateV1 = serde_json::from_value(value)
                            .map_err(|error| GatewayStoreError::Corrupt(error.to_string()))?;
                        GatewayDiskState {
                            version: STORE_VERSION,
                            config_revision: legacy.config_revision,
                            last_intent_id: legacy.last_intent_id,
                            unresolved: legacy.unresolved,
                            resumption_required: None,
                            audit_sequence: 0,
                            recovery_audit: VecDeque::new(),
                        }
                    }
                    2 => serde_json::from_value(value)
                        .map_err(|error| GatewayStoreError::Corrupt(error.to_string()))?,
                    other => return Err(GatewayStoreError::Version(other as u32)),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => GatewayDiskState {
                version: STORE_VERSION,
                config_revision: config_revision.into(),
                last_intent_id: 0,
                unresolved: None,
                resumption_required: None,
                audit_sequence: 0,
                recovery_audit: VecDeque::new(),
            },
            Err(error) => return Err(error.into()),
        };
        if state.version != STORE_VERSION {
            return Err(GatewayStoreError::Version(state.version));
        }
        if state.config_revision != config_revision {
            return Err(GatewayStoreError::ConfigRevision {
                stored: state.config_revision,
                configured: config_revision.into(),
            });
        }
        validate_disk_state(&state)?;
        Ok(Self { path, state })
    }

    fn last_intent_id(&self) -> u64 {
        self.state.last_intent_id
    }

    fn unresolved(&self) -> Option<&DurableGatewayOperation> {
        self.state.unresolved.as_ref()
    }

    fn resumption_required(&self) -> Option<&DurableResumptionRequirement> {
        self.state.resumption_required.as_ref()
    }

    fn recovery_audit(&self) -> &VecDeque<RecoveryAuditRecord> {
        &self.state.recovery_audit
    }

    fn reserve(&mut self, operation: DurableGatewayOperation) -> Result<(), GatewayStoreError> {
        if self.state.unresolved.is_some() {
            return Err(GatewayStoreError::Conflict);
        }
        if self.state.resumption_required.is_some() {
            return Err(GatewayStoreError::ResumptionAlreadyRequired);
        }
        let mut next = self.state.clone();
        next.last_intent_id = next.last_intent_id.max(operation.intent_id);
        next.unresolved = Some(operation);
        self.commit(next)
    }

    fn update(
        &mut self,
        id: IntentId,
        mutate: impl FnOnce(&mut DurableGatewayOperation),
    ) -> Result<(), GatewayStoreError> {
        let mut next = self.state.clone();
        let operation = next
            .unresolved
            .as_mut()
            .ok_or(GatewayStoreError::MissingOperation)?;
        if operation.intent_id != id.value() {
            return Err(GatewayStoreError::WrongOperation {
                expected: operation.intent_id,
                received: id.value(),
            });
        }
        mutate(operation);
        self.commit(next)
    }

    fn clear(&mut self, id: IntentId) -> Result<(), GatewayStoreError> {
        self.update(id, |_| {})?;
        let mut next = self.state.clone();
        next.unresolved = None;
        self.commit(next)
    }

    fn retire_unknown(
        &mut self,
        principal: &str,
        control_domain: &str,
        operational_state: Option<GatewayOperationalSnapshot>,
        observed_holder: Option<String>,
        reason: String,
    ) -> Result<(), GatewayStoreError> {
        if self.state.resumption_required.is_some() {
            return Err(GatewayStoreError::ResumptionAlreadyRequired);
        }
        let operation = self
            .state
            .unresolved
            .clone()
            .ok_or(GatewayStoreError::MissingOperation)?;
        let sequence = self
            .state
            .audit_sequence
            .checked_add(1)
            .ok_or(GatewayStoreError::IdentityExhausted)?;
        let record = RecoveryAuditRecord {
            sequence,
            operation: operation.clone(),
            action: RecoveryAuditAction::RetiredOutcomeUnknown,
            principal: principal.into(),
            config_revision: self.state.config_revision.clone(),
            control_domain: control_domain.into(),
            prior_delivery: operation.delivery,
            prior_saw_progress: operation.saw_progress,
            prior_saw_closed_at_target: operation.saw_closed_at_target,
            operational_state,
            observed_holder,
            remote_inactivity: RemoteInactivityEvidence::Uncertain,
            reason,
        };
        let mut next = self.state.clone();
        next.unresolved = None;
        next.resumption_required = Some(DurableResumptionRequirement {
            operation,
            retired_config_revision: next.config_revision.clone(),
            control_domain: control_domain.into(),
            retirement_audit_sequence: sequence,
        });
        next.audit_sequence = sequence;
        push_bounded_audit(&mut next.recovery_audit, record);
        self.commit(next)
    }

    fn resume_with_residual_uncertainty(
        &mut self,
        principal: &str,
        operational_state: GatewayOperationalSnapshot,
        observed_holder: Option<String>,
        reason: String,
    ) -> Result<(), GatewayStoreError> {
        let requirement = self
            .state
            .resumption_required
            .clone()
            .ok_or(GatewayStoreError::MissingResumptionRequirement)?;
        let sequence = self
            .state
            .audit_sequence
            .checked_add(1)
            .ok_or(GatewayStoreError::IdentityExhausted)?;
        let operation = requirement.operation;
        let record = RecoveryAuditRecord {
            sequence,
            operation: operation.clone(),
            action: RecoveryAuditAction::ResumedWithResidualUncertainty,
            principal: principal.into(),
            config_revision: self.state.config_revision.clone(),
            control_domain: requirement.control_domain,
            prior_delivery: operation.delivery,
            prior_saw_progress: operation.saw_progress,
            prior_saw_closed_at_target: operation.saw_closed_at_target,
            operational_state: Some(operational_state),
            observed_holder,
            remote_inactivity: RemoteInactivityEvidence::UncertainAccepted,
            reason,
        };
        let mut next = self.state.clone();
        next.resumption_required = None;
        next.audit_sequence = sequence;
        push_bounded_audit(&mut next.recovery_audit, record);
        self.commit(next)
    }

    fn commit(&mut self, next: GatewayDiskState) -> Result<(), GatewayStoreError> {
        validate_disk_state(&next)?;
        let encoded =
            serde_json::to_vec(&next).map_err(|error| GatewayStoreError::Io(error.to_string()))?;
        let encoded_size = encoded.len().saturating_add(1) as u64;
        if encoded_size > MAX_STORE_BYTES {
            return Err(GatewayStoreError::Oversized(encoded_size));
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| GatewayStoreError::Io("state path has no parent directory".into()))?;
        fs::create_dir_all(parent)?;
        let mut entropy = [0_u8; 8];
        getrandom::fill(&mut entropy)
            .map_err(|error| GatewayStoreError::Io(format!("temporary name entropy: {error}")))?;
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| GatewayStoreError::Io("state filename is not valid UTF-8".into()))?;
        let temporary = parent.join(format!(".{name}.{:016x}.tmp", u64::from_be_bytes(entropy)));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        let write_result = (|| -> Result<(), GatewayStoreError> {
            let mut writer = BufWriter::new(file);
            writer.write_all(&encoded)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
            writer.get_ref().sync_all()?;
            fs::rename(&temporary, &self.path)?;
            File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result?;
        self.state = next;
        Ok(())
    }
}

fn validate_disk_state(state: &GatewayDiskState) -> Result<(), GatewayStoreError> {
    if state.config_revision.is_empty() {
        return Err(GatewayStoreError::Corrupt(
            "config_revision must not be empty".into(),
        ));
    }
    if state.unresolved.is_some() && state.resumption_required.is_some() {
        return Err(GatewayStoreError::Corrupt(
            "unresolved and resumption-required states are mutually exclusive".into(),
        ));
    }
    if let Some(operation) = &state.unresolved {
        validate_operation(operation, state.last_intent_id)?;
    }
    if let Some(requirement) = &state.resumption_required {
        validate_operation(&requirement.operation, state.last_intent_id)?;
        if requirement.retired_config_revision.is_empty()
            || requirement.control_domain.is_empty()
            || requirement.retirement_audit_sequence == 0
            || requirement.retirement_audit_sequence > state.audit_sequence
        {
            return Err(GatewayStoreError::Corrupt(
                "resumption requirement violates durable identity invariants".into(),
            ));
        }
    }
    if state.recovery_audit.len() > MAX_RECOVERY_AUDIT_RECORDS {
        return Err(GatewayStoreError::Corrupt(
            "recovery audit exceeds its bounded retention".into(),
        ));
    }
    let mut previous = 0;
    for record in &state.recovery_audit {
        validate_operation(&record.operation, state.last_intent_id)?;
        if record.sequence <= previous
            || record.sequence > state.audit_sequence
            || record.principal.is_empty()
            || record.config_revision.is_empty()
            || record.control_domain.is_empty()
            || record.reason.len() > MAX_RECOVERY_REASON_BYTES
        {
            return Err(GatewayStoreError::Corrupt(
                "recovery audit violates ordering or field invariants".into(),
            ));
        }
        previous = record.sequence;
    }
    Ok(())
}

fn validate_operation(
    operation: &DurableGatewayOperation,
    last_intent_id: u64,
) -> Result<(), GatewayStoreError> {
    if operation.intent_id == 0
        || operation.intent_id > last_intent_id
        || operation.logical_car.is_empty()
        || operation.native_lift.is_empty()
        || operation.native_floor.is_empty()
        || operation.rmf_session_id.is_empty()
    {
        return Err(GatewayStoreError::Corrupt(
            "unresolved operation violates identity/mapping invariants".into(),
        ));
    }
    Ok(())
}

fn push_bounded_audit(audit: &mut VecDeque<RecoveryAuditRecord>, record: RecoveryAuditRecord) {
    if audit.len() == MAX_RECOVERY_AUDIT_RECORDS {
        audit.pop_front();
    }
    audit.push_back(record);
}

fn validate_store_against_config(
    store: &GatewayStore,
    config: &GatewayConfig,
) -> Result<(), GatewayStoreError> {
    if let Some(operation) = store.unresolved() {
        validate_operation_mapping(operation, config)?;
    }
    if let Some(requirement) = store.resumption_required() {
        validate_operation_mapping(&requirement.operation, config)?;
        if requirement.retired_config_revision != config.config_revision
            || requirement.control_domain != config.control_domain_key()
        {
            return Err(GatewayStoreError::Corrupt(
                "retired operation does not match the current control domain/configuration".into(),
            ));
        }
    }
    Ok(())
}

fn validate_operation_mapping(
    operation: &DurableGatewayOperation,
    config: &GatewayConfig,
) -> Result<(), GatewayStoreError> {
    if operation.logical_car != config.logical_car
        || operation.native_lift != config.native_lift
        || config.floors.get(&operation.native_floor) != Some(&operation.target_floor)
    {
        return Err(GatewayStoreError::Corrupt(
            "durable operation does not match the configured logical/native mapping".into(),
        ));
    }
    Ok(())
}

pub trait RmfCommandTransport {
    fn available(&self) -> bool;
    fn publish(&mut self, command: RmfLiftCommand) -> Result<(), String>;
}

impl RmfCommandTransport for crate::RmfLiftRequestPublisher {
    fn available(&self) -> bool {
        self.has_subscriber()
    }

    fn publish(&mut self, command: RmfLiftCommand) -> Result<(), String> {
        crate::RmfLiftRequestPublisher::publish(self, command).map_err(|error| error.to_string())
    }
}

struct PreparedRmfSink<'a, T> {
    id: ControllerId,
    transport: &'a mut T,
    store: &'a mut GatewayStore,
    operation: DurableGatewayOperation,
    request_time_nanos: i64,
}

impl<T: RmfCommandTransport> ControlledIntentSink for PreparedRmfSink<'_, T> {
    fn controller_id(&self) -> &ControllerId {
        &self.id
    }

    fn available(&self, car: &CarId) -> bool {
        car.as_str() == self.operation.logical_car && self.transport.available()
    }

    fn submit(&mut self, id: IntentId, intent: &Intent) -> Submission {
        if id.value() != self.operation.intent_id
            || intent.car().as_str() != self.operation.logical_car
            || intent.floor().index() != self.operation.target_floor
        {
            return Submission::NotSubmitted;
        }
        let command = RmfLiftCommand {
            lift_name: self.operation.native_lift.clone(),
            request_time_nanos: self.request_time_nanos,
            session_id: self.operation.rmf_session_id.clone(),
            destination_floor: self.operation.native_floor.clone(),
        };
        match self.transport.publish(command) {
            Ok(()) => {
                if self
                    .store
                    .update(id, |operation| {
                        operation.delivery = DurableDelivery::SentUnconfirmed;
                    })
                    .is_ok()
                {
                    Submission::Submitted
                } else {
                    Submission::Indeterminate
                }
            }
            Err(_) => Submission::Indeterminate,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GatewayCommandRequest {
    pub version: u32,
    pub operation: String,
    #[serde(default)]
    pub logical_car: String,
    #[serde(default)]
    pub floor: i16,
    #[serde(default)]
    pub principal: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub accept_residual_uncertainty: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayRecoveryInspection {
    pub config_revision: String,
    pub control_domain: String,
    pub unresolved: Option<DurableGatewayOperation>,
    pub resumption_required: Option<DurableResumptionRequirement>,
    pub durable_progress: String,
    pub operational_state: Option<GatewayOperationalSnapshot>,
    pub current_holder: Option<String>,
    pub command_transport_available: bool,
    pub control_domain_owned: bool,
    pub commissioning: CommissioningState,
    pub overloaded: bool,
    pub recent_audit: Vec<RecoveryAuditRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GatewayCommandResponse {
    pub accepted: bool,
    pub code: String,
    pub intent_id: Option<u64>,
    pub readiness: GatewayReadiness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery: Option<GatewayRecoveryInspection>,
}

pub struct RmfIntentGateway<T> {
    config: GatewayConfig,
    engine: Engine,
    adapter: RmfObservationAdapter,
    source: ObservationSourceId,
    session: ObservationSessionId,
    controller: ControllerId,
    grant: Option<AuthorityGrant>,
    store: GatewayStore,
    transport: T,
    latest_source_time_nanos: i64,
    recovered_active: bool,
    commissioning: CommissioningState,
    latest_holder: Option<String>,
    latest_update: Option<CarObservationUpdate>,
    overloaded: bool,
    control_domain_lease: Option<LocalControlDomainLease>,
}

impl<T: RmfCommandTransport> RmfIntentGateway<T> {
    pub fn new(
        config: GatewayConfig,
        session: ObservationSessionId,
        transport: T,
    ) -> Result<Self, GatewayConfigError> {
        validate_config(&config)?;
        let car = CarId::new(&config.logical_car);
        let source = ObservationSourceId::new(&config.source_id);
        let floor_map = config
            .floors
            .iter()
            .map(|(native, logical)| (native.clone(), Floor::new(*logical)))
            .collect::<BTreeMap<_, _>>();
        let adapter = RmfObservationAdapter::new(
            source.clone(),
            session.clone(),
            RmfInteractionModeProjection::conservative().with_agv_as_normal(),
            [(
                config.native_lift.clone(),
                RmfLiftBinding::new(car.clone(), floor_map),
            )],
        )
        .map_err(|error| GatewayConfigError::Adapter(error.to_string()))?;
        let store = GatewayStore::open(config.state_path.clone(), &config.config_revision)
            .map_err(GatewayConfigError::Store)?;
        validate_store_against_config(&store, &config).map_err(GatewayConfigError::Store)?;
        let control_domain_lease =
            LocalControlDomainLease::acquire(&config.control_domain_key())
                .map_err(|error| GatewayConfigError::ControlDomain(error.to_string()))?;
        let controller = ControllerId::new("rmf-private-gateway");
        let (engine, grant) = build_gateway_engine(
            &config,
            &source,
            &session,
            &controller,
            store.last_intent_id(),
        )?;
        Ok(Self {
            config,
            engine,
            adapter,
            source,
            session,
            controller,
            grant: Some(grant),
            store,
            transport,
            latest_source_time_nanos: 0,
            recovered_active: false,
            commissioning: CommissioningState::AwaitingObservation,
            latest_holder: None,
            latest_update: None,
            overloaded: false,
            control_domain_lease: Some(control_domain_lease),
        })
    }

    pub fn readiness(&self) -> GatewayReadiness {
        if self.store.unresolved().is_some() {
            return GatewayReadiness::RecoveryRequired;
        }
        if self.store.resumption_required().is_some() {
            return GatewayReadiness::ResumptionRequired;
        }
        if self.overloaded || self.commissioning == CommissioningState::Contradictory {
            return GatewayReadiness::Degraded;
        }
        if self.commissioning == CommissioningState::AwaitingObservation {
            return GatewayReadiness::Synchronizing;
        }
        let car = CarId::new(&self.config.logical_car);
        if self.engine.operational_car_state(&car).is_none() {
            return GatewayReadiness::Synchronizing;
        }
        if self.grant.is_none()
            || !self.transport.available()
            || self.control_domain_lease.is_none()
        {
            return GatewayReadiness::Degraded;
        }
        GatewayReadiness::Ready
    }

    pub fn unresolved(&self) -> Option<&DurableGatewayOperation> {
        self.store.unresolved()
    }

    pub fn resumption_required(&self) -> Option<&DurableResumptionRequirement> {
        self.store.resumption_required()
    }

    pub fn recovery_audit(&self) -> &VecDeque<RecoveryAuditRecord> {
        self.store.recovery_audit()
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn capabilities(&self) -> RmfGatewayCapabilities {
        RmfGatewayCapabilities {
            ownership: "car_specific_liftassistant_owned",
            logical_car: CarId::new(&self.config.logical_car),
            native_lift: self.config.native_lift.clone(),
            served_floors: self
                .config
                .floors
                .values()
                .map(|floor| Floor::new(*floor))
                .collect(),
            holder_correlation: true,
            observation_source: self.source.clone(),
            withdrawal_supported: false,
            reconnect_without_resend: true,
        }
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn revoke_authority(&mut self) {
        let car = CarId::new(&self.config.logical_car);
        if let Some(grant) = self.grant.take() {
            let _ = self.engine.revoke_authority(&car, &grant);
        }
    }

    pub fn handle_command(
        &mut self,
        peer_uid: u32,
        request: GatewayCommandRequest,
    ) -> GatewayCommandResponse {
        if request.version != 1 {
            return self.reject("unsupported_operation");
        }
        match request.operation.as_str() {
            "serve_floor" => self.handle_serve_floor(peer_uid, request),
            "inspect_recovery" => self.inspect_recovery(peer_uid, &request),
            "retire_unknown" => self.retire_unknown(peer_uid, &request),
            "resume_control" => self.resume_control(peer_uid, &request),
            _ => self.reject("unsupported_operation"),
        }
    }

    fn handle_serve_floor(
        &mut self,
        peer_uid: u32,
        request: GatewayCommandRequest,
    ) -> GatewayCommandResponse {
        if peer_uid != self.config.allowed_uid {
            return self.reject("unauthorized_peer");
        }
        if request.logical_car != self.config.logical_car {
            return self.reject("unsupported_car");
        }
        if self.store.unresolved().is_some() {
            return self.reject("recovery_required");
        }
        if self.store.resumption_required().is_some() {
            return self.reject("resumption_required");
        }
        let Some(native_floor) = self
            .config
            .floors
            .iter()
            .find_map(|(native, floor)| (*floor == request.floor).then(|| native.clone()))
        else {
            return self.reject("unsupported_floor");
        };
        if self.control_domain_lease.is_none() {
            return self.reject("control_domain_unheld");
        }
        if self.overloaded {
            return self.reject("observation_overloaded");
        }
        if self.commissioning != CommissioningState::Compatible {
            return self.reject("commissioning_incomplete");
        }
        let Some(grant) = self.grant.clone() else {
            return self.reject("authority_fenced");
        };
        let car = CarId::new(&self.config.logical_car);
        let intent = Intent::ServeFloor {
            car: car.clone(),
            floor: Floor::new(request.floor),
        };
        let issued = self.engine.issue_controlled_intent(intent, None);
        if let Err(blocker) = self.engine.write_readiness(
            &issued,
            &grant,
            &self.controller,
            self.transport.available(),
        ) {
            return self.reject(write_blocker_code(blocker));
        }
        let started_floor = self
            .engine
            .operational_car_state(&car)
            .expect("write readiness requires operational state")
            .location
            .floor_reference()
            .index();
        let operation = DurableGatewayOperation {
            intent_id: issued.id().value(),
            logical_car: self.config.logical_car.clone(),
            target_floor: request.floor,
            native_lift: self.config.native_lift.clone(),
            native_floor,
            rmf_session_id: format!("{HOLDER_PREFIX}-{}", issued.id().value()),
            started_floor,
            delivery: DurableDelivery::ReservedUncertain,
            saw_progress: false,
            saw_closed_at_target: false,
        };
        if self.store.reserve(operation.clone()).is_err() {
            return self.reject("durable_reservation_failed");
        }
        let mut sink = PreparedRmfSink {
            id: self.controller.clone(),
            transport: &mut self.transport,
            store: &mut self.store,
            operation,
            request_time_nanos: self.latest_source_time_nanos,
        };
        let admission = self
            .engine
            .submit_controlled(issued.clone(), &grant, &mut sink);
        if admission != Admission::Admitted {
            let _ = self.store.clear(issued.id());
            return self.reject(match admission {
                Admission::Blocked(blocker) => write_blocker_code(blocker),
                Admission::Retry => "identity_retry",
                Admission::IdentityConflict => "identity_conflict",
                Admission::RetiredIdentity => "retired_identity",
                Admission::Admitted => unreachable!(),
            });
        }
        self.recovered_active = true;
        GatewayCommandResponse {
            accepted: true,
            code: "admitted_unresolved".into(),
            intent_id: Some(issued.id().value()),
            readiness: self.readiness(),
            recovery: None,
        }
    }

    fn inspect_recovery(
        &self,
        peer_uid: u32,
        request: &GatewayCommandRequest,
    ) -> GatewayCommandResponse {
        if let Err(code) = self.authorize_recovery(peer_uid, request.principal.as_deref()) {
            return self.reject(code);
        }
        GatewayCommandResponse {
            accepted: true,
            code: "recovery_inspection".into(),
            intent_id: self
                .store
                .unresolved()
                .map(|operation| operation.intent_id)
                .or_else(|| {
                    self.store
                        .resumption_required()
                        .map(|requirement| requirement.operation.intent_id)
                }),
            readiness: self.readiness(),
            recovery: Some(self.recovery_inspection()),
        }
    }

    fn retire_unknown(
        &mut self,
        peer_uid: u32,
        request: &GatewayCommandRequest,
    ) -> GatewayCommandResponse {
        let principal = match self.authorize_recovery(peer_uid, request.principal.as_deref()) {
            Ok(principal) => principal.to_owned(),
            Err(code) => return self.reject(code),
        };
        let reason = match recovery_reason(request.reason.as_deref()) {
            Ok(reason) => reason,
            Err(code) => return self.reject(code),
        };
        let operation_id = match self.store.unresolved() {
            Some(operation) => operation.intent_id,
            None if self.store.resumption_required().is_some() => {
                return self.reject("resumption_required");
            }
            None => return self.reject("no_unresolved_operation"),
        };
        let result = self.store.retire_unknown(
            &principal,
            &self.config.control_domain_key(),
            self.current_operational_snapshot(),
            self.latest_holder.clone(),
            reason,
        );
        if result.is_err() {
            return self.reject("retirement_commit_failed");
        }
        // Do not use recovery authority to mint or retain command authority. The
        // old process grant is fenced, and the old in-memory Engine operation is
        // intentionally not rewritten as terminal. A normal process restart must
        // establish the next process-local command grant before resumption can pass.
        self.revoke_authority();
        self.recovered_active = false;
        GatewayCommandResponse {
            accepted: true,
            code: "retired_outcome_unknown".into(),
            intent_id: Some(operation_id),
            readiness: self.readiness(),
            recovery: Some(self.recovery_inspection()),
        }
    }

    fn resume_control(
        &mut self,
        peer_uid: u32,
        request: &GatewayCommandRequest,
    ) -> GatewayCommandResponse {
        let principal = match self.authorize_recovery(peer_uid, request.principal.as_deref()) {
            Ok(principal) => principal.to_owned(),
            Err(code) => return self.reject(code),
        };
        let reason = match recovery_reason(request.reason.as_deref()) {
            Ok(reason) => reason,
            Err(code) => return self.reject(code),
        };
        let Some(requirement) = self.store.resumption_required().cloned() else {
            return self.reject("resumption_not_required");
        };
        if self.control_domain_lease.is_none() {
            return self.reject("control_domain_unheld");
        }
        if requirement.retired_config_revision != self.config.config_revision
            || requirement.control_domain != self.config.control_domain_key()
        {
            return self.reject("recovery_config_mismatch");
        }
        if self.overloaded {
            return self.reject("observation_overloaded");
        }
        if self.commissioning != CommissioningState::Compatible {
            return self.reject("commissioning_incomplete");
        }
        let Some(operational_state) = self.current_operational_snapshot() else {
            return self.reject("observation_unavailable");
        };
        if !self.transport.available() {
            return self.reject("backend_unavailable");
        }
        if self.grant.is_none() {
            return self.reject("authority_fenced");
        }
        if self
            .engine
            .intent_record(IntentId::new(requirement.operation.intent_id))
            .is_some()
        {
            return self.reject("active_intent");
        }
        let Some(holder) = self.latest_holder.clone() else {
            return self.reject("observation_unavailable");
        };
        if !holder.is_empty() {
            return self.reject("backend_holder_active");
        }
        if !request.accept_residual_uncertainty {
            return self.reject("residual_uncertainty_not_accepted");
        }
        if self
            .store
            .resume_with_residual_uncertainty(&principal, operational_state, Some(holder), reason)
            .is_err()
        {
            return self.reject("resumption_commit_failed");
        }
        GatewayCommandResponse {
            accepted: true,
            code: "control_resumed_with_residual_uncertainty".into(),
            intent_id: Some(requirement.operation.intent_id),
            readiness: self.readiness(),
            recovery: Some(self.recovery_inspection()),
        }
    }

    pub fn ingest(&mut self, payload: RmfLivePayload) -> Result<(), GatewayIngestError> {
        let configured_lift = payload.state.lift_name == self.config.native_lift;
        if configured_lift {
            if let Err(error) =
                self.validate_commissioning_evidence(&payload.state.available_floors)
            {
                self.commissioning = CommissioningState::Contradictory;
                self.latest_update = None;
                self.latest_holder = None;
                let car = CarId::new(&self.config.logical_car);
                let _ = self
                    .engine
                    .invalidate_car_observations(&self.source, &self.session, &car);
                return Err(GatewayIngestError::Commissioning(error));
            }
        }
        let native_session = payload.state.session_id.clone();
        let mapped = match self.adapter.map(&payload.state) {
            Ok(mapped) => mapped,
            Err(error @ RmfAdapterError::Mapping { .. }) => {
                self.commissioning = CommissioningState::Contradictory;
                self.latest_update = None;
                self.latest_holder = None;
                let car = CarId::new(&self.config.logical_car);
                let _ = self
                    .engine
                    .invalidate_car_observations(&self.source, &self.session, &car);
                return Err(GatewayIngestError::Adapter(error));
            }
            Err(error) => return Err(GatewayIngestError::Adapter(error)),
        };
        self.latest_source_time_nanos = payload.source_time_nanos;
        self.latest_holder = Some(native_session.clone());
        self.latest_update = Some(mapped.update.clone());
        self.commissioning = CommissioningState::Compatible;
        self.overloaded = false;
        self.engine.ingest_observation_update(mapped.update);
        self.try_recover();
        let Some(operation) = self.store.unresolved().cloned() else {
            return Ok(());
        };
        let id = IntentId::new(operation.intent_id);
        if self.recovered_active && native_session == operation.rmf_session_id {
            let result = self.engine.receive_intent_receipt(IntentReceipt {
                controller: self.controller.clone(),
                intent: id,
                status: ReceiptStatus::Accepted,
            });
            if matches!(result, ReceiptResult::Recorded | ReceiptResult::Duplicate) {
                self.store
                    .update(id, |durable| durable.delivery = DurableDelivery::Correlated)
                    .map_err(GatewayIngestError::Store)?;
            }
        }
        if let Some(record) = self.engine.intent_record(id) {
            let progress = record.saw_progress();
            let closed = record.saw_closed_at_target();
            if progress != operation.saw_progress || closed != operation.saw_closed_at_target {
                self.store
                    .update(id, |durable| {
                        durable.saw_progress = progress;
                        durable.saw_closed_at_target = closed;
                    })
                    .map_err(GatewayIngestError::Store)?;
            }
            if record.reconciliation != Reconciliation::Pending {
                self.store.clear(id).map_err(GatewayIngestError::Store)?;
                self.recovered_active = false;
            }
        }
        Ok(())
    }

    pub fn invalidate_observations(&mut self) {
        self.latest_update = None;
        self.latest_holder = None;
        let _ = self
            .engine
            .invalidate_source_observations(self.source.clone(), self.session.clone());
    }

    pub fn mark_observation_overloaded(&mut self) {
        self.overloaded = true;
        self.invalidate_observations();
    }

    pub fn commissioning(&self) -> CommissioningState {
        self.commissioning
    }

    fn try_recover(&mut self) {
        if self.recovered_active {
            return;
        }
        let Some(operation) = self.store.unresolved().cloned() else {
            return;
        };
        let Some(grant) = &self.grant else {
            return;
        };
        let receipt = match operation.delivery {
            DurableDelivery::Correlated => Receipt::Accepted,
            DurableDelivery::ReservedUncertain | DurableDelivery::SentUnconfirmed => {
                Receipt::Indeterminate
            }
        };
        let recovery = ControlledIntentRecovery {
            id: IntentId::new(operation.intent_id),
            intent: Intent::ServeFloor {
                car: CarId::new(operation.logical_car),
                floor: Floor::new(operation.target_floor),
            },
            controller: self.controller.clone(),
            receipt,
            started_at: CarLocation::Floor(Floor::new(operation.started_floor)),
            saw_progress: operation.saw_progress,
            saw_closed_at_target: operation.saw_closed_at_target,
        };
        self.recovered_active = self
            .engine
            .recover_controlled_intent(recovery, grant)
            .is_ok();
    }

    fn validate_commissioning_evidence(&self, available_floors: &[String]) -> Result<(), String> {
        let observed = available_floors.iter().cloned().collect::<BTreeSet<_>>();
        if observed.len() != available_floors.len() {
            return Err("available_floors contains duplicate native labels".into());
        }
        let configured = self.config.floors.keys().cloned().collect::<BTreeSet<_>>();
        if observed != configured {
            return Err(format!(
                "configured served-floor labels {configured:?} do not match current backend capability {observed:?}"
            ));
        }
        Ok(())
    }

    fn authorize_recovery<'a>(
        &'a self,
        peer_uid: u32,
        requested_principal: Option<&str>,
    ) -> Result<&'a str, &'static str> {
        let Some(operator) = &self.config.recovery_operator else {
            return Err("recovery_permission_required");
        };
        if peer_uid != operator.allowed_uid {
            return Err("unauthorized_recovery_peer");
        }
        if requested_principal != Some(operator.principal.as_str()) {
            return Err("unauthorized_recovery_principal");
        }
        Ok(&operator.principal)
    }

    fn current_operational_snapshot(&self) -> Option<GatewayOperationalSnapshot> {
        self.engine
            .operational_car_state(&CarId::new(&self.config.logical_car))
            .map(operational_snapshot)
    }

    fn recovery_inspection(&self) -> GatewayRecoveryInspection {
        let durable_progress = if let Some(operation) = self.store.unresolved() {
            let reconciliation = self
                .engine
                .intent_record(IntentId::new(operation.intent_id))
                .map(|record| match record.reconciliation {
                    Reconciliation::Pending => "pending",
                    Reconciliation::Satisfied => "satisfied",
                    Reconciliation::Diverged { .. } => "diverged",
                })
                .unwrap_or("awaiting_fresh_operational_evidence");
            format!(
                "{}/{reconciliation}",
                durable_delivery_name(operation.delivery)
            )
        } else if self.store.resumption_required().is_some() {
            "retired_outcome_unknown/resumption_required".into()
        } else {
            "none".into()
        };
        GatewayRecoveryInspection {
            config_revision: self.config.config_revision.clone(),
            control_domain: self.config.control_domain_key(),
            unresolved: self.store.unresolved().cloned(),
            resumption_required: self.store.resumption_required().cloned(),
            durable_progress,
            operational_state: self.current_operational_snapshot(),
            current_holder: self.latest_holder.clone(),
            command_transport_available: self.transport.available(),
            control_domain_owned: self.control_domain_lease.is_some(),
            commissioning: self.commissioning,
            overloaded: self.overloaded,
            recent_audit: self.store.recovery_audit().iter().cloned().collect(),
        }
    }

    fn reject(&self, code: &str) -> GatewayCommandResponse {
        GatewayCommandResponse {
            accepted: false,
            code: code.into(),
            intent_id: None,
            readiness: self.readiness(),
            recovery: None,
        }
    }
}

fn recovery_reason(reason: Option<&str>) -> Result<String, &'static str> {
    let Some(reason) = reason.map(str::trim).filter(|reason| !reason.is_empty()) else {
        return Err("recovery_reason_required");
    };
    if reason.len() > MAX_RECOVERY_REASON_BYTES {
        return Err("recovery_reason_too_long");
    }
    Ok(reason.into())
}

const fn durable_delivery_name(delivery: DurableDelivery) -> &'static str {
    match delivery {
        DurableDelivery::ReservedUncertain => "reserved_uncertain",
        DurableDelivery::SentUnconfirmed => "sent_unconfirmed",
        DurableDelivery::Correlated => "correlated",
    }
}

fn operational_snapshot(state: &CarState) -> GatewayOperationalSnapshot {
    let (floor, location_precision) = match state.location {
        CarLocation::Floor(floor) => (floor.index(), "floor_reference"),
        CarLocation::Continuous(position) => (position.nearest_floor.index(), "continuous"),
    };
    let motion = match state.motion {
        MotionState::Stopped => "stopped",
        MotionState::Moving(Direction::Up) => "moving_up",
        MotionState::Moving(Direction::Down) => "moving_down",
    };
    let door = match state.door {
        DoorState::Closed => "closed",
        DoorState::Opening => "opening",
        DoorState::Open => "open",
        DoorState::Closing => "closing",
        DoorState::Obstructed => "obstructed",
        DoorState::Fault => "fault",
    };
    let mode = match state.mode {
        OperatingMode::Normal => "normal",
        OperatingMode::Independent => "independent",
        OperatingMode::Inspection => "inspection",
        OperatingMode::FireService => "fire_service",
        OperatingMode::OutOfService => "out_of_service",
    };
    let reported_destination = match state.reported_destination {
        ReportedDestination::Known(floor) => format!("known:{}", floor.index()),
        ReportedDestination::None => "none".into(),
        ReportedDestination::Unknown => "unknown".into(),
    };
    GatewayOperationalSnapshot {
        floor,
        location_precision: location_precision.into(),
        motion: motion.into(),
        door: door.into(),
        mode: mode.into(),
        reported_destination,
    }
}

impl GatewayConfig {
    /// Stable local coverage identity for the concrete RMF backend/lift pair.
    ///
    /// Socket paths, logical aliases and process incarnations intentionally do not
    /// participate: two local processes addressing the same native lift in the same
    /// DDS domain must contend for the same lease.
    pub fn control_domain_key(&self) -> String {
        format!("rmf:{}:{}", self.domain_id, self.native_lift)
    }
}

fn build_gateway_engine(
    config: &GatewayConfig,
    source: &ObservationSourceId,
    session: &ObservationSessionId,
    controller: &ControllerId,
    last_intent_id: u64,
) -> Result<(Engine, AuthorityGrant), GatewayConfigError> {
    let topology = BuildingTopology::new(config.floors.values().map(|floor| Floor::new(*floor)))
        .map_err(|_| GatewayConfigError::InvalidTopology)?;
    let car = CarId::new(&config.logical_car);
    let mut engine = Engine::new(topology);
    engine
        .bind_observation(
            source.clone(),
            car.clone(),
            ObservationBindingRole::Operational,
        )
        .map_err(|error| GatewayConfigError::Engine(format!("{error:?}")))?;
    engine
        .begin_source_session(source.clone(), session.clone())
        .map_err(|error| GatewayConfigError::Engine(format!("{error:?}")))?;
    engine
        .bind_controlled_car(car.clone(), controller.clone())
        .map_err(|error| GatewayConfigError::Engine(format!("{error:?}")))?;
    engine.set_execution_capabilities(
        car.clone(),
        ExecutionCapabilities::new(
            true,
            false,
            config.floors.values().map(|floor| Floor::new(*floor)),
        ),
    );
    engine.restore_controlled_intent_watermark(IntentId::new(last_intent_id));
    let grant = engine
        .grant_authority(&car, PrincipalId::new(&config.principal))
        .map_err(|error| GatewayConfigError::Engine(format!("{error:?}")))?;
    Ok((engine, grant))
}

fn validate_config(config: &GatewayConfig) -> Result<(), GatewayConfigError> {
    for (field, value) in [
        ("config_revision", config.config_revision.as_str()),
        ("principal", config.principal.as_str()),
        ("source_id", config.source_id.as_str()),
        ("logical_car", config.logical_car.as_str()),
        ("native_lift", config.native_lift.as_str()),
    ] {
        if value.is_empty() {
            return Err(GatewayConfigError::Empty(field));
        }
    }
    if config.floors.is_empty() {
        return Err(GatewayConfigError::NoFloors);
    }
    let mut logical = BTreeSet::new();
    for floor in config.floors.values() {
        if !logical.insert(*floor) {
            return Err(GatewayConfigError::DuplicateLogicalFloor(*floor));
        }
    }
    if config.max_payload_silence_ms == 0 {
        return Err(GatewayConfigError::ZeroFreshness);
    }
    if config
        .recovery_operator
        .as_ref()
        .is_some_and(|operator| operator.principal.is_empty())
    {
        return Err(GatewayConfigError::Empty("recovery_operator.principal"));
    }
    Ok(())
}

fn write_blocker_code(blocker: WriteBlocker) -> &'static str {
    match blocker {
        WriteBlocker::AuthorityFenced => "authority_fenced",
        WriteBlocker::ControllerUnbound => "controller_unbound",
        WriteBlocker::WrongController => "wrong_controller",
        WriteBlocker::ControllerUnavailable => "backend_unavailable",
        WriteBlocker::ObservationUnavailable => "observation_unavailable",
        WriteBlocker::Mode(_) => "operating_mode",
        WriteBlocker::Busy => "busy",
        WriteBlocker::CapabilitiesUnavailable => "capabilities_unavailable",
        WriteBlocker::OperationUnsupported => "unsupported_operation",
        WriteBlocker::UnsupportedFloor => "unsupported_floor",
        WriteBlocker::InvalidCall => "invalid_call",
        WriteBlocker::ActiveIntent(_) => "active_intent",
    }
}

pub fn load_gateway_config(path: &Path) -> Result<GatewayConfig, GatewayConfigError> {
    let metadata = fs::metadata(path).map_err(|error| {
        GatewayConfigError::Engine(format!("failed to inspect configuration: {error}"))
    })?;
    if !metadata.is_file() || metadata.len() > MAX_STORE_BYTES {
        return Err(GatewayConfigError::Engine(
            "configuration must be a regular file no larger than 64 KiB".into(),
        ));
    }
    let mut input = String::new();
    File::open(path)
        .and_then(|mut file| file.read_to_string(&mut input))
        .map_err(|error| {
            GatewayConfigError::Engine(format!("failed to read configuration: {error}"))
        })?;
    let config = serde_json::from_str(&input).map_err(|error| {
        GatewayConfigError::Engine(format!("invalid configuration JSON: {error}"))
    })?;
    validate_config(&config)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU16, Ordering};

    use super::*;
    use crate::RmfLiftState;
    use tempfile::TempDir;

    #[derive(Default)]
    struct FakeTransport {
        available: bool,
        sent: Vec<RmfLiftCommand>,
        fail: bool,
    }

    impl RmfCommandTransport for FakeTransport {
        fn available(&self) -> bool {
            self.available
        }

        fn publish(&mut self, command: RmfLiftCommand) -> Result<(), String> {
            self.sent.push(command);
            if self.fail {
                Err("uncertain fixture publish".into())
            } else {
                Ok(())
            }
        }
    }

    fn config(temp: &TempDir) -> GatewayConfig {
        static NEXT_DOMAIN: AtomicU16 = AtomicU16::new(1);
        GatewayConfig {
            socket_path: temp.path().join("gateway.sock"),
            state_path: temp.path().join("gateway-state.json"),
            config_revision: "fixture-v1".into(),
            allowed_uid: 1000,
            principal: "fixture-caller".into(),
            recovery_operator: Some(RecoveryOperatorConfig {
                allowed_uid: 2000,
                principal: "fixture-recovery-operator".into(),
            }),
            domain_id: NEXT_DOMAIN.fetch_add(1, Ordering::Relaxed),
            source_id: "rmf-fixture".into(),
            logical_car: "A".into(),
            native_lift: "Lift1".into(),
            floors: [("L1".into(), 1), ("L2".into(), 2)].into(),
            max_payload_silence_ms: 4000,
        }
    }

    fn state(
        sequence_time: i64,
        floor: &str,
        motion: u8,
        door: u8,
        holder: &str,
    ) -> RmfLivePayload {
        RmfLivePayload {
            source_time_nanos: sequence_time,
            state: RmfLiftState {
                lift_name: "Lift1".into(),
                available_floors: vec!["L1".into(), "L2".into()],
                current_floor: floor.into(),
                destination_floor: floor.into(),
                door_state: door,
                motion_state: motion,
                available_modes: vec![RmfLiftState::MODE_AGV],
                current_mode: RmfLiftState::MODE_AGV,
                session_id: holder.into(),
            },
        }
    }

    fn request(floor: i16) -> GatewayCommandRequest {
        GatewayCommandRequest {
            version: 1,
            operation: "serve_floor".into(),
            logical_car: "A".into(),
            floor,
            principal: None,
            reason: None,
            accept_residual_uncertainty: false,
        }
    }

    fn recovery_request(
        operation: &str,
        principal: &str,
        reason: Option<&str>,
        accept_residual_uncertainty: bool,
    ) -> GatewayCommandRequest {
        GatewayCommandRequest {
            version: 1,
            operation: operation.into(),
            logical_car: String::new(),
            floor: 0,
            principal: Some(principal.into()),
            reason: reason.map(str::to_owned),
            accept_residual_uncertainty,
        }
    }

    #[test]
    fn durable_reservation_precedes_send_and_completion_is_observation_driven() {
        let temp = TempDir::new().unwrap();
        let transport = FakeTransport {
            available: true,
            ..FakeTransport::default()
        };
        let mut gateway = RmfIntentGateway::new(
            config(&temp),
            ObservationSessionId::new("process-A"),
            transport,
        )
        .unwrap();
        gateway
            .ingest(state(
                1,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert_eq!(gateway.readiness(), GatewayReadiness::Ready);
        let response = gateway.handle_command(1000, request(2));
        assert!(response.accepted);
        assert_eq!(response.intent_id, Some(1));
        let operation = gateway.unresolved().unwrap().clone();
        assert_eq!(operation.delivery, DurableDelivery::SentUnconfirmed);
        assert_eq!(gateway.transport.sent.len(), 1);

        gateway
            .ingest(state(
                2,
                "L1",
                RmfLiftState::MOTION_UP,
                RmfLiftState::DOOR_CLOSED,
                &operation.rmf_session_id,
            ))
            .unwrap();
        assert_eq!(
            gateway.unresolved().unwrap().delivery,
            DurableDelivery::Correlated
        );
        gateway
            .ingest(state(
                3,
                "L2",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_OPEN,
                &operation.rmf_session_id,
            ))
            .unwrap();
        assert!(gateway.unresolved().is_none());
        assert_eq!(gateway.transport.sent.len(), 1);
    }

    #[test]
    fn restart_restores_same_identity_and_never_resends() {
        let temp = TempDir::new().unwrap();
        let config = config(&temp);
        let mut first = RmfIntentGateway::new(
            config.clone(),
            ObservationSessionId::new("process-A"),
            FakeTransport {
                available: true,
                fail: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        first
            .ingest(state(
                1,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert!(first.handle_command(1000, request(2)).accepted);
        let unresolved = first.unresolved().unwrap().clone();
        assert_eq!(unresolved.delivery, DurableDelivery::ReservedUncertain);
        assert_eq!(first.transport.sent.len(), 1);
        drop(first);

        let mut restarted = RmfIntentGateway::new(
            config,
            ObservationSessionId::new("process-B"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        assert_eq!(restarted.readiness(), GatewayReadiness::RecoveryRequired);
        restarted
            .ingest(state(
                2,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert_eq!(restarted.transport.sent.len(), 0);
        restarted
            .ingest(state(
                3,
                "L1",
                RmfLiftState::MOTION_UP,
                RmfLiftState::DOOR_CLOSED,
                &unresolved.rmf_session_id,
            ))
            .unwrap();
        restarted
            .ingest(state(
                4,
                "L2",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_OPEN,
                &unresolved.rmf_session_id,
            ))
            .unwrap();
        assert!(restarted.unresolved().is_none());
        assert_eq!(restarted.transport.sent.len(), 0);
        let next = restarted.handle_command(1000, request(1));
        assert_eq!(next.intent_id, Some(2));
    }

    #[test]
    fn authority_capability_and_observation_fail_closed_before_send() {
        let temp = TempDir::new().unwrap();
        let mut gateway = RmfIntentGateway::new(
            config(&temp),
            ObservationSessionId::new("process-A"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        assert_eq!(
            gateway.handle_command(999, request(2)).code,
            "unauthorized_peer"
        );
        assert_eq!(
            gateway.handle_command(1000, request(9)).code,
            "unsupported_floor"
        );
        assert_eq!(
            gateway.handle_command(1000, request(2)).code,
            "commissioning_incomplete"
        );
        assert!(gateway.transport.sent.is_empty());
        gateway
            .ingest(state(
                1,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        gateway.transport.available = false;
        assert_eq!(
            gateway.handle_command(1000, request(2)).code,
            "backend_unavailable"
        );
        gateway.transport.available = true;
        gateway.revoke_authority();
        assert_eq!(
            gateway.handle_command(1000, request(2)).code,
            "authority_fenced"
        );
        assert!(gateway.transport.sent.is_empty());
    }

    #[test]
    fn corrupted_or_incompatible_state_fails_closed() {
        let temp = TempDir::new().unwrap();
        let config = config(&temp);
        fs::write(&config.state_path, b"not-json\n").unwrap();
        fs::set_permissions(&config.state_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            RmfIntentGateway::new(
                config.clone(),
                ObservationSessionId::new("process-A"),
                FakeTransport::default()
            ),
            Err(GatewayConfigError::Store(GatewayStoreError::Corrupt(_)))
        ));
        fs::write(
            &config.state_path,
            br#"{"version":1,"config_revision":"other","last_intent_id":0,"unresolved":null}"#,
        )
        .unwrap();
        fs::set_permissions(&config.state_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            RmfIntentGateway::new(
                config,
                ObservationSessionId::new("process-B"),
                FakeTransport::default()
            ),
            Err(GatewayConfigError::Store(
                GatewayStoreError::ConfigRevision { .. }
            ))
        ));
    }

    #[test]
    fn durable_operation_mapping_must_match_current_control_domain_even_at_same_revision() {
        let temp = TempDir::new().unwrap();
        let config = config(&temp);
        fs::write(
            &config.state_path,
            br#"{"version":1,"config_revision":"fixture-v1","last_intent_id":7,"unresolved":{"intent_id":7,"logical_car":"A","target_floor":2,"native_lift":"Lift2","native_floor":"L2","rmf_session_id":"liftassistant-m7-7","started_floor":1,"delivery":"sent_unconfirmed","saw_progress":false,"saw_closed_at_target":false}}"#,
        )
        .unwrap();
        fs::set_permissions(&config.state_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            RmfIntentGateway::new(
                config,
                ObservationSessionId::new("process-A"),
                FakeTransport::default()
            ),
            Err(GatewayConfigError::Store(GatewayStoreError::Corrupt(_)))
        ));
    }

    #[test]
    fn mapping_failure_revokes_only_configured_car_while_unknown_lift_is_isolated() {
        let temp = TempDir::new().unwrap();
        let mut gateway = RmfIntentGateway::new(
            config(&temp),
            ObservationSessionId::new("process-A"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        gateway
            .ingest(state(
                1,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        let car = CarId::new("A");
        assert!(gateway.engine().operational_car_state(&car).is_some());

        let mut unknown = state(
            2,
            "L1",
            RmfLiftState::MOTION_STOPPED,
            RmfLiftState::DOOR_CLOSED,
            "",
        );
        unknown.state.lift_name = "Lift3".into();
        assert!(matches!(
            gateway.ingest(unknown),
            Err(GatewayIngestError::Adapter(
                RmfAdapterError::UnconfiguredLift { .. }
            ))
        ));
        assert!(gateway.engine().operational_car_state(&car).is_some());

        let malformed = state(
            3,
            "OUTSIDE",
            RmfLiftState::MOTION_STOPPED,
            RmfLiftState::DOOR_CLOSED,
            "",
        );
        assert!(matches!(
            gateway.ingest(malformed),
            Err(GatewayIngestError::Adapter(RmfAdapterError::Mapping { .. }))
        ));
        assert!(gateway.engine().operational_car_state(&car).is_none());
        assert_eq!(gateway.readiness(), GatewayReadiness::Degraded);
    }

    #[test]
    fn observation_loss_preserves_unresolved_work_and_blocks_completion() {
        let temp = TempDir::new().unwrap();
        let mut gateway = RmfIntentGateway::new(
            config(&temp),
            ObservationSessionId::new("process-A"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        gateway
            .ingest(state(
                1,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert!(gateway.handle_command(1000, request(2)).accepted);
        let id = IntentId::new(1);
        gateway.invalidate_observations();
        assert_eq!(gateway.readiness(), GatewayReadiness::RecoveryRequired);
        assert_eq!(
            gateway.engine().intent_record(id).unwrap().reconciliation,
            Reconciliation::Pending
        );
        assert!(gateway.unresolved().is_some());
    }

    #[test]
    fn recovery_authority_retirement_restart_and_safe_resumption_are_distinct() {
        let temp = TempDir::new().unwrap();
        let config = config(&temp);
        let mut gateway = RmfIntentGateway::new(
            config.clone(),
            ObservationSessionId::new("process-A"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        gateway
            .ingest(state(
                1,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert!(gateway.handle_command(1000, request(2)).accepted);
        assert_eq!(gateway.unresolved().unwrap().intent_id, 1);

        let inspect =
            recovery_request("inspect_recovery", "fixture-recovery-operator", None, false);
        assert_eq!(
            gateway.handle_command(1000, inspect.clone()).code,
            "unauthorized_recovery_peer"
        );
        let wrong_principal = recovery_request("inspect_recovery", "fixture-caller", None, false);
        assert_eq!(
            gateway.handle_command(2000, wrong_principal).code,
            "unauthorized_recovery_principal"
        );
        let inspection = gateway.handle_command(2000, inspect);
        assert!(inspection.accepted);
        let evidence = inspection.recovery.unwrap();
        assert_eq!(evidence.unresolved.unwrap().intent_id, 1);
        assert_eq!(evidence.current_holder.as_deref(), Some(""));
        assert_eq!(evidence.config_revision, "fixture-v1");

        let retire = recovery_request(
            "retire_unknown",
            "fixture-recovery-operator",
            Some("operator accepts that the historical outcome remains unknown"),
            false,
        );
        let retired = gateway.handle_command(2000, retire);
        assert!(retired.accepted);
        assert_eq!(retired.code, "retired_outcome_unknown");
        assert_eq!(retired.readiness, GatewayReadiness::ResumptionRequired);
        assert!(gateway.unresolved().is_none());
        assert!(gateway.resumption_required().is_some());
        assert!(gateway.engine().intent_record(IntentId::new(1)).is_some());
        assert_eq!(
            gateway.recovery_audit().back().unwrap().action,
            RecoveryAuditAction::RetiredOutcomeUnknown
        );
        assert_eq!(
            gateway.recovery_audit().back().unwrap().remote_inactivity,
            RemoteInactivityEvidence::Uncertain
        );
        assert_eq!(
            gateway.handle_command(1000, request(1)).code,
            "resumption_required"
        );
        assert_eq!(gateway.transport.sent.len(), 1);
        drop(gateway);

        let mut restarted = RmfIntentGateway::new(
            config,
            ObservationSessionId::new("process-B"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        assert_eq!(restarted.readiness(), GatewayReadiness::ResumptionRequired);
        let resume = recovery_request(
            "resume_control",
            "fixture-recovery-operator",
            Some("resume current control while preserving the old unknown outcome"),
            true,
        );
        assert_eq!(
            restarted.handle_command(2000, resume.clone()).code,
            "commissioning_incomplete"
        );
        restarted
            .ingest(state(
                2,
                "L2",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert_eq!(restarted.readiness(), GatewayReadiness::ResumptionRequired);
        let without_acceptance = recovery_request(
            "resume_control",
            "fixture-recovery-operator",
            Some("not yet accepting uncertainty"),
            false,
        );
        assert_eq!(
            restarted.handle_command(2000, without_acceptance).code,
            "residual_uncertainty_not_accepted"
        );
        let resumed = restarted.handle_command(2000, resume);
        assert!(resumed.accepted);
        assert_eq!(resumed.readiness, GatewayReadiness::Ready);
        assert!(restarted.resumption_required().is_none());
        assert_eq!(restarted.transport.sent.len(), 0);
        assert_eq!(
            restarted.recovery_audit().back().unwrap().action,
            RecoveryAuditAction::ResumedWithResidualUncertainty
        );
        assert_eq!(
            restarted.recovery_audit().back().unwrap().remote_inactivity,
            RemoteInactivityEvidence::UncertainAccepted
        );
        let next = restarted.handle_command(1000, request(1));
        assert_eq!(next.intent_id, Some(2));
        assert_eq!(restarted.transport.sent.len(), 1);
    }

    #[test]
    fn safe_resumption_rechecks_transport_holder_freshness_and_overload() {
        let temp = TempDir::new().unwrap();
        let config = config(&temp);
        let mut gateway = RmfIntentGateway::new(
            config.clone(),
            ObservationSessionId::new("process-A"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        gateway
            .ingest(state(
                1,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert!(gateway.handle_command(1000, request(2)).accepted);
        assert!(
            gateway
                .handle_command(
                    2000,
                    recovery_request(
                        "retire_unknown",
                        "fixture-recovery-operator",
                        Some("fixture retirement"),
                        false,
                    ),
                )
                .accepted
        );
        let resume = || {
            recovery_request(
                "resume_control",
                "fixture-recovery-operator",
                Some("fixture resumption"),
                true,
            )
        };

        assert_eq!(
            gateway.handle_command(2000, resume()).code,
            "authority_fenced"
        );
        drop(gateway);
        let mut gateway = RmfIntentGateway::new(
            config,
            ObservationSessionId::new("process-B"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        gateway
            .ingest(state(
                2,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();

        gateway.transport.available = false;
        assert_eq!(
            gateway.handle_command(2000, resume()).code,
            "backend_unavailable"
        );
        gateway.transport.available = true;
        gateway
            .ingest(state(
                3,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "another-holder",
            ))
            .unwrap();
        assert_eq!(
            gateway.handle_command(2000, resume()).code,
            "backend_holder_active"
        );
        gateway.invalidate_observations();
        assert_eq!(
            gateway.handle_command(2000, resume()).code,
            "observation_unavailable"
        );
        gateway
            .ingest(state(
                4,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        gateway.mark_observation_overloaded();
        assert_eq!(
            gateway.handle_command(2000, resume()).code,
            "observation_overloaded"
        );
        gateway
            .ingest(state(
                5,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert!(gateway.handle_command(2000, resume()).accepted);
    }

    #[test]
    fn commissioning_and_overload_fail_closed_until_fresh_compatible_evidence() {
        let temp = TempDir::new().unwrap();
        let mut gateway = RmfIntentGateway::new(
            config(&temp),
            ObservationSessionId::new("process-A"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        let mut subset = state(
            1,
            "L1",
            RmfLiftState::MOTION_STOPPED,
            RmfLiftState::DOOR_CLOSED,
            "",
        );
        subset.state.available_floors = vec!["L1".into()];
        assert!(matches!(
            gateway.ingest(subset),
            Err(GatewayIngestError::Commissioning(_))
        ));
        assert_eq!(gateway.commissioning(), CommissioningState::Contradictory);
        assert_eq!(gateway.readiness(), GatewayReadiness::Degraded);

        gateway
            .ingest(state(
                2,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert_eq!(gateway.readiness(), GatewayReadiness::Ready);
        gateway.mark_observation_overloaded();
        assert_eq!(gateway.readiness(), GatewayReadiness::Degraded);
        assert!(
            gateway
                .engine()
                .operational_car_state(&CarId::new("A"))
                .is_none()
        );
        gateway
            .ingest(state(
                3,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        assert_eq!(gateway.readiness(), GatewayReadiness::Ready);
    }

    #[test]
    fn recovery_audit_is_bounded_while_intent_watermark_remains_monotonic() {
        let temp = TempDir::new().unwrap();
        let config = config(&temp);
        let mut gateway = RmfIntentGateway::new(
            config.clone(),
            ObservationSessionId::new("process-A"),
            FakeTransport {
                available: true,
                ..FakeTransport::default()
            },
        )
        .unwrap();
        gateway
            .ingest(state(
                1,
                "L1",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_CLOSED,
                "",
            ))
            .unwrap();
        for expected in 1..=40 {
            let admitted = gateway.handle_command(1000, request(2));
            assert_eq!(admitted.intent_id, Some(expected));
            assert!(
                gateway
                    .handle_command(
                        2000,
                        recovery_request(
                            "retire_unknown",
                            "fixture-recovery-operator",
                            Some("bounded-retention stress"),
                            false,
                        ),
                    )
                    .accepted
            );
            drop(gateway);
            gateway = RmfIntentGateway::new(
                config.clone(),
                ObservationSessionId::new(format!("process-recovery-{expected}")),
                FakeTransport {
                    available: true,
                    ..FakeTransport::default()
                },
            )
            .unwrap();
            gateway
                .ingest(state(
                    i64::try_from(expected).unwrap() + 1,
                    "L1",
                    RmfLiftState::MOTION_STOPPED,
                    RmfLiftState::DOOR_CLOSED,
                    "",
                ))
                .unwrap();
            assert!(
                gateway
                    .handle_command(
                        2000,
                        recovery_request(
                            "resume_control",
                            "fixture-recovery-operator",
                            Some("bounded-retention stress"),
                            true,
                        ),
                    )
                    .accepted
            );
        }
        assert_eq!(gateway.store.last_intent_id(), 40);
        assert_eq!(gateway.recovery_audit().len(), MAX_RECOVERY_AUDIT_RECORDS);
        assert_eq!(gateway.recovery_audit().front().unwrap().sequence, 49);
        assert_eq!(gateway.recovery_audit().back().unwrap().sequence, 80);
    }

    #[test]
    fn crash_windows_keep_one_bounded_conservative_record() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("state.json");
        let mut store = GatewayStore::open(path.clone(), "fixture-v1").unwrap();

        // A: before reservation, no operation is recoverable and no side effect is claimed.
        assert!(store.unresolved().is_none());
        assert!(!path.exists());

        let operation = DurableGatewayOperation {
            intent_id: 7,
            logical_car: "A".into(),
            target_floor: 2,
            native_lift: "Lift1".into(),
            native_floor: "L2".into(),
            rmf_session_id: "liftassistant-m7-7".into(),
            started_floor: 1,
            delivery: DurableDelivery::ReservedUncertain,
            saw_progress: false,
            saw_closed_at_target: false,
        };
        // B and C intentionally converge: after reservation, both pre-send and
        // possible-send crashes recover as uncertainty and never imply safe resend.
        store.reserve(operation).unwrap();
        assert_eq!(
            GatewayStore::open(path.clone(), "fixture-v1")
                .unwrap()
                .unresolved()
                .unwrap()
                .delivery,
            DurableDelivery::ReservedUncertain
        );

        // D: a correlated backend holder is stronger receipt evidence but remains
        // unresolved until canonical physical observations satisfy Engine.
        store
            .update(IntentId::new(7), |record| {
                record.delivery = DurableDelivery::Correlated
            })
            .unwrap();
        let correlated = GatewayStore::open(path.clone(), "fixture-v1").unwrap();
        assert_eq!(correlated.state.last_intent_id, 7);
        assert_eq!(
            correlated.unresolved().unwrap().delivery,
            DurableDelivery::Correlated
        );

        // E is unsupported by the first RMF gateway. No withdrawal state transition
        // exists, so interruption cannot create a duplicate command or clear work.
        assert_eq!(correlated.state.unresolved.as_ref().unwrap().intent_id, 7);
        assert_eq!(correlated.state.unresolved.iter().count(), 1);
    }

    #[test]
    fn recovery_crash_windows_preserve_either_old_or_fully_committed_state() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("state.json");
        let operation = DurableGatewayOperation {
            intent_id: 7,
            logical_car: "A".into(),
            target_floor: 2,
            native_lift: "Lift1".into(),
            native_floor: "L2".into(),
            rmf_session_id: "liftassistant-m7-7".into(),
            started_floor: 1,
            delivery: DurableDelivery::SentUnconfirmed,
            saw_progress: true,
            saw_closed_at_target: false,
        };
        let mut store = GatewayStore::open(path.clone(), "fixture-v1").unwrap();
        store.reserve(operation.clone()).unwrap();

        // A/B: an interrupted pre-rename write leaves only an unreferenced temporary
        // file. Reopening the exact configured path keeps the unresolved operation.
        fs::write(temp.path().join(".state.json.retire.tmp"), b"partial").unwrap();
        let before_retirement = GatewayStore::open(path.clone(), "fixture-v1").unwrap();
        assert_eq!(before_retirement.unresolved(), Some(&operation));
        drop(before_retirement);

        store
            .retire_unknown(
                "fixture-recovery-operator",
                "rmf:47:Lift1",
                None,
                Some(String::new()),
                "crash-window retirement".into(),
            )
            .unwrap();
        drop(store);

        // C/D: a committed retirement always reopens as ResumptionRequired. A
        // second interrupted pre-rename write cannot silently clear that gate.
        let mut retired = GatewayStore::open(path.clone(), "fixture-v1").unwrap();
        assert!(retired.unresolved().is_none());
        assert!(retired.resumption_required().is_some());
        fs::write(temp.path().join(".state.json.resume.tmp"), b"partial").unwrap();
        let still_retired = GatewayStore::open(path.clone(), "fixture-v1").unwrap();
        assert!(still_retired.resumption_required().is_some());
        drop(still_retired);

        retired
            .resume_with_residual_uncertainty(
                "fixture-recovery-operator",
                GatewayOperationalSnapshot {
                    floor: 2,
                    location_precision: "floor_reference".into(),
                    motion: "stopped".into(),
                    door: "closed".into(),
                    mode: "normal".into(),
                    reported_destination: "known:2".into(),
                },
                Some(String::new()),
                "crash-window resumption".into(),
            )
            .unwrap();
        drop(retired);

        // E: after a committed resumption and before new work, restart remains
        // clear but retains the old watermark and both outcome-unknown audits.
        let mut resumed = GatewayStore::open(path, "fixture-v1").unwrap();
        assert!(resumed.unresolved().is_none());
        assert!(resumed.resumption_required().is_none());
        assert_eq!(resumed.last_intent_id(), 7);
        assert_eq!(resumed.recovery_audit().len(), 2);
        let mut next = operation;
        next.intent_id = 8;
        next.rmf_session_id = "liftassistant-m7-8".into();
        resumed.reserve(next).unwrap();
        assert_eq!(resumed.last_intent_id(), 8);
    }
}
