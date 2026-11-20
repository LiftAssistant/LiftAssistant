use std::{
    collections::{BTreeMap, BTreeSet},
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
    BuildingTopology, CarId, CarLocation, ExecutionCapabilities, Floor, Intent, IntentId,
    ObservationSessionId, ObservationSourceId,
};
use liftassistant_runtime::{
    ControlledIntentSink, ControllerId, IntentReceipt, ReceiptStatus, Submission,
};
use serde::{Deserialize, Serialize};

use crate::{
    RmfAdapterError, RmfInteractionModeProjection, RmfLiftBinding, RmfLiftCommand, RmfLivePayload,
    RmfObservationAdapter,
};

const STORE_VERSION: u32 = 1;
const MAX_STORE_BYTES: u64 = 64 * 1024;
const HOLDER_PREFIX: &str = "liftassistant-m7";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GatewayConfig {
    pub socket_path: PathBuf,
    pub state_path: PathBuf,
    pub config_revision: String,
    pub allowed_uid: u32,
    pub principal: String,
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
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableDelivery {
    ReservedUncertain,
    SentUnconfirmed,
    Correlated,
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
struct GatewayDiskState {
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
        }
    }
}

impl Error for GatewayStoreError {}

#[derive(Debug)]
pub enum GatewayIngestError {
    Adapter(RmfAdapterError),
    Store(GatewayStoreError),
}

impl fmt::Display for GatewayIngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter(error) => write!(f, "RMF observation rejected: {error}"),
            Self::Store(error) => write!(f, "gateway recovery state update failed: {error}"),
        }
    }
}

impl Error for GatewayIngestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Adapter(error) => Some(error),
            Self::Store(error) => Some(error),
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
                serde_json::from_reader::<_, GatewayDiskState>(BufReader::new(file))
                    .map_err(|error| GatewayStoreError::Corrupt(error.to_string()))?
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => GatewayDiskState {
                version: STORE_VERSION,
                config_revision: config_revision.into(),
                last_intent_id: 0,
                unresolved: None,
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

    fn reserve(&mut self, operation: DurableGatewayOperation) -> Result<(), GatewayStoreError> {
        if self.state.unresolved.is_some() {
            return Err(GatewayStoreError::Conflict);
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

    fn commit(&mut self, next: GatewayDiskState) -> Result<(), GatewayStoreError> {
        validate_disk_state(&next)?;
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
            serde_json::to_writer(&mut writer, &next)
                .map_err(|error| GatewayStoreError::Io(error.to_string()))?;
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
    if let Some(operation) = &state.unresolved {
        if operation.intent_id == 0
            || operation.intent_id > state.last_intent_id
            || operation.logical_car.is_empty()
            || operation.native_lift.is_empty()
            || operation.native_floor.is_empty()
            || operation.rmf_session_id.is_empty()
        {
            return Err(GatewayStoreError::Corrupt(
                "unresolved operation violates identity/mapping invariants".into(),
            ));
        }
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
    pub logical_car: String,
    pub floor: i16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GatewayCommandResponse {
    pub accepted: bool,
    pub code: String,
    pub intent_id: Option<u64>,
    pub readiness: GatewayReadiness,
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
        let topology = BuildingTopology::new(floor_map.values().copied())
            .map_err(|_| GatewayConfigError::InvalidTopology)?;
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
        let controller = ControllerId::new("rmf-private-gateway");
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
            ExecutionCapabilities::new(true, false, config.floors.values().map(|f| Floor::new(*f))),
        );
        engine.restore_controlled_intent_watermark(IntentId::new(store.last_intent_id()));
        let grant = engine
            .grant_authority(&car, PrincipalId::new(&config.principal))
            .map_err(|error| GatewayConfigError::Engine(format!("{error:?}")))?;
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
        })
    }

    pub fn readiness(&self) -> GatewayReadiness {
        if self.store.unresolved().is_some() {
            return GatewayReadiness::RecoveryRequired;
        }
        let car = CarId::new(&self.config.logical_car);
        if self.engine.operational_car_state(&car).is_none() {
            return GatewayReadiness::Synchronizing;
        }
        if self.grant.is_none() || !self.transport.available() {
            return GatewayReadiness::Degraded;
        }
        GatewayReadiness::Ready
    }

    pub fn unresolved(&self) -> Option<&DurableGatewayOperation> {
        self.store.unresolved()
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
        if peer_uid != self.config.allowed_uid {
            return self.reject("unauthorized_peer");
        }
        if request.version != 1 || request.operation != "serve_floor" {
            return self.reject("unsupported_operation");
        }
        if request.logical_car != self.config.logical_car {
            return self.reject("unsupported_car");
        }
        if self.store.unresolved().is_some() {
            return self.reject("recovery_required");
        }
        let Some(native_floor) = self
            .config
            .floors
            .iter()
            .find_map(|(native, floor)| (*floor == request.floor).then(|| native.clone()))
        else {
            return self.reject("unsupported_floor");
        };
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
        }
    }

    pub fn ingest(&mut self, payload: RmfLivePayload) -> Result<(), GatewayIngestError> {
        self.latest_source_time_nanos = payload.source_time_nanos;
        let native_session = payload.state.session_id.clone();
        let mapped = match self.adapter.map(&payload.state) {
            Ok(mapped) => mapped,
            Err(error @ RmfAdapterError::Mapping { .. }) => {
                let car = CarId::new(&self.config.logical_car);
                let _ = self
                    .engine
                    .invalidate_car_observations(&self.source, &self.session, &car);
                return Err(GatewayIngestError::Adapter(error));
            }
            Err(error) => return Err(GatewayIngestError::Adapter(error)),
        };
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
        let _ = self
            .engine
            .invalidate_source_observations(self.source.clone(), self.session.clone());
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

    fn reject(&self, code: &str) -> GatewayCommandResponse {
        GatewayCommandResponse {
            accepted: false,
            code: code.into(),
            intent_id: None,
            readiness: self.readiness(),
        }
    }
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
        GatewayConfig {
            socket_path: temp.path().join("gateway.sock"),
            state_path: temp.path().join("gateway-state.json"),
            config_revision: "fixture-v1".into(),
            allowed_uid: 1000,
            principal: "fixture-caller".into(),
            domain_id: 47,
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
            "observation_unavailable"
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
        assert_eq!(gateway.readiness(), GatewayReadiness::Synchronizing);
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
}
