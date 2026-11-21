//! Development-only Simulator control surface bound strictly to `VirtualController(SimRuntime)`.
//!
//! Architectural invariants:
//! - Engine remains the sole owner of Principal/grant authority, `IntentId`, receipt,
//!   reconciliation, and canonical operational state.
//! - `submission_key` is strictly transport/UI submission correlation — NOT a canonical domain
//!   identity and NOT a replacement for `IntentId`.
//! - `WireProjectedOperationStatus` is a read-side projection derived directly from Engine
//!   `IntentRecord` facts, NOT a second operation state machine.
//! - Unreachable from RMF, LCE, real controllers, or passenger equipment.

use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};

use liftassistant_engine::{
    Admission, AuthorityGrant, ControllerId, IntentRecord, ObservationBindingRole, PrincipalId,
    Receipt, Reconciliation, WriteBlocker,
};
use liftassistant_model::{
    BuildingTopology, CarId, CarState, ExecutionCapabilities, Floor, Intent, IntentId,
    ObservationSessionId, ObservationSourceId, Position,
};
use liftassistant_runtime::ControlledIntentSink;
use liftassistant_sim::{SimRuntime, VirtualController};
use serde::Serialize;

use crate::read_projection::{SimReadHost, SimReadModel, WireOperatingMode};

pub(crate) const DEV_CONTROL_API_VERSION: &str = "dev-sim/v0";
pub(crate) const DEV_SIM_PRINCIPAL: &str = "dev-sim-operator";
pub(crate) const DEV_SIM_CONTROLLER_ID: &str = "sim-dev-controller";
pub(crate) const MAX_SUBMISSION_RECORDS: usize = 128;
const MIN_SUBMISSION_KEY_LEN: usize = 8;
const MAX_SUBMISSION_KEY_LEN: usize = 64;
const MAX_CAR_ID_LEN: usize = 64;
const MAX_INSTANCE_ID_LEN: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidationError {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl ValidationError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SubmitError {
    Validation(ValidationError),
    Conflict {
        submission_key: String,
        message: String,
    },
}

pub(crate) fn validate_operation(operation: &str) -> Result<(), ValidationError> {
    if operation != "serve_floor" {
        return Err(ValidationError::new(
            "unsupported_operation",
            format!("only explicit operation \"serve_floor\" is accepted (got {operation:?})"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_submission_key(key: &str) -> Result<(), ValidationError> {
    if key.len() < MIN_SUBMISSION_KEY_LEN || key.len() > MAX_SUBMISSION_KEY_LEN {
        return Err(ValidationError::new(
            "invalid_submission_key",
            format!(
                "submission_key length must be between {MIN_SUBMISSION_KEY_LEN} and {MAX_SUBMISSION_KEY_LEN} ASCII characters"
            ),
        ));
    }
    if !key
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ValidationError::new(
            "invalid_submission_key",
            "submission_key may contain only ASCII letters, digits, '-' and '_'",
        ));
    }
    Ok(())
}

pub(crate) fn validate_car_id(car: &str) -> Result<CarId, ValidationError> {
    if car.is_empty() || car.len() > MAX_CAR_ID_LEN {
        return Err(ValidationError::new(
            "invalid_car",
            format!("car id must be 1..={MAX_CAR_ID_LEN} characters"),
        ));
    }
    if car.chars().any(char::is_control) {
        return Err(ValidationError::new(
            "invalid_car",
            "car id must not contain control characters",
        ));
    }
    Ok(CarId::new(car))
}

pub(crate) fn validate_instance_id(instance_id: &str) -> Result<(), ValidationError> {
    if instance_id.is_empty() || instance_id.len() > MAX_INSTANCE_ID_LEN {
        return Err(ValidationError::new(
            "invalid_instance_id",
            format!("expected_instance_id must be 1..={MAX_INSTANCE_ID_LEN} hex characters"),
        ));
    }
    if !instance_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ValidationError::new(
            "invalid_instance_id",
            "expected_instance_id must be hexadecimal",
        ));
    }
    Ok(())
}

pub(crate) fn generate_control_instance_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to generate control_instance_id entropy: {error}"))?;
    let mut hex = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok(hex)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ImmutableServeFloorRequest {
    car: CarId,
    floor: Floor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SubmissionOutcome {
    Admitted { intent_id: IntentId },
    Rejected { blocker: WriteBlocker },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SubmissionEntry {
    request: ImmutableServeFloorRequest,
    outcome: SubmissionOutcome,
}

#[derive(Debug)]
struct SubmissionRegistry {
    capacity: usize,
    entries: BTreeMap<String, SubmissionEntry>,
    order: VecDeque<String>,
    evicted_count: u64,
}

impl SubmissionRegistry {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: BTreeMap::new(),
            order: VecDeque::new(),
            evicted_count: 0,
        }
    }

    fn get(&self, key: &str) -> Option<&SubmissionEntry> {
        self.entries.get(key)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    fn evicted_count(&self) -> u64 {
        self.evicted_count
    }

    fn insert(
        &mut self,
        key: String,
        entry: SubmissionEntry,
        is_active: impl Fn(&SubmissionEntry) -> bool,
    ) {
        if self.entries.contains_key(&key) {
            return;
        }
        if self.entries.len() == self.capacity {
            let evict_index = self
                .order
                .iter()
                .position(|candidate| {
                    self.entries
                        .get(candidate)
                        .is_none_or(|existing| !is_active(existing))
                })
                .unwrap_or(0);
            if let Some(evicted_key) = self.order.remove(evict_index) {
                self.entries.remove(&evicted_key);
                self.evicted_count = self.evicted_count.saturating_add(1);
            }
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, entry);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireOperationSupport {
    Unknown,
    Unsupported,
    Supported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireControlPathAvailability {
    Available,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireAuthorizationContext {
    DevProcessLocalGrant,
    Fenced,
    Unbound,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct WireAuthorityMetadata {
    pub(crate) kind: &'static str,
    pub(crate) multi_user_auth: bool,
    pub(crate) principal: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct WireCarServeFloorCapability {
    pub(crate) support: WireOperationSupport,
    pub(crate) control_path_availability: WireControlPathAvailability,
    pub(crate) control_path_reason: Option<&'static str>,
    pub(crate) authorization: WireAuthorizationContext,
    pub(crate) executable_floors: Vec<i16>,
    pub(crate) active_intent_id: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct WireCarCapabilities {
    pub(crate) car: String,
    pub(crate) serve_floor: WireCarServeFloorCapability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct WireCapabilitiesResponse {
    pub(crate) control_api_version: &'static str,
    pub(crate) development_only: bool,
    pub(crate) control_instance_id: String,
    pub(crate) ephemeral_process_memory: bool,
    pub(crate) authority_context: WireAuthorityMetadata,
    pub(crate) cars: Vec<WireCarCapabilities>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct WireWriteBlocker {
    pub(crate) code: &'static str,
    pub(crate) detail: String,
    pub(crate) blocking_intent_id: Option<u64>,
    pub(crate) observed_mode: Option<WireOperatingMode>,
}

impl From<WriteBlocker> for WireWriteBlocker {
    fn from(blocker: WriteBlocker) -> Self {
        match blocker {
            WriteBlocker::AuthorityFenced => Self {
                code: "authority_fenced",
                detail: "development process-local authority grant is fenced or revoked".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::ControllerUnbound => Self {
                code: "controller_unbound",
                detail: "no controller is bound for this car".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::WrongController => Self {
                code: "wrong_controller",
                detail: "configured controller does not match car binding".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::ControllerUnavailable => Self {
                code: "controller_unavailable",
                detail: "simulator controller path is currently unavailable".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::ObservationUnavailable => Self {
                code: "observation_unavailable",
                detail: "current valid Operational observation state is unavailable".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::Mode(mode) => {
                let wire_mode = WireOperatingMode::from(mode);
                Self {
                    code: "mode_not_normal",
                    detail: format!("car operating mode is {mode:?}, not Normal"),
                    blocking_intent_id: None,
                    observed_mode: Some(wire_mode),
                }
            }
            WriteBlocker::Busy => Self {
                code: "busy",
                detail: "car is currently busy or moving".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::CapabilitiesUnavailable => Self {
                code: "capabilities_unavailable",
                detail: "execution capabilities are currently unavailable".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::OperationUnsupported => Self {
                code: "operation_unsupported",
                detail: "ServeFloor is not supported by car execution capabilities".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::UnsupportedFloor => Self {
                code: "unsupported_floor",
                detail: "requested floor is outside car execution capabilities or topology".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::InvalidCall => Self {
                code: "invalid_call",
                detail: "associated hall call is invalid for controlled admission".into(),
                blocking_intent_id: None,
                observed_mode: None,
            },
            WriteBlocker::ActiveIntent(id) => Self {
                code: "active_intent",
                detail: format!(
                    "another controlled intent (#{}) is already active",
                    id.value()
                ),
                blocking_intent_id: Some(id.value()),
                observed_mode: None,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct WireServeFloorPreview {
    pub(crate) control_api_version: &'static str,
    pub(crate) development_only: bool,
    pub(crate) control_instance_id: String,
    pub(crate) advisory: bool,
    pub(crate) operation: &'static str,
    pub(crate) car: String,
    pub(crate) floor: i16,
    pub(crate) support: WireOperationSupport,
    pub(crate) control_path_availability: WireControlPathAvailability,
    pub(crate) control_path_reason: Option<&'static str>,
    pub(crate) authorization: WireAuthorizationContext,
    pub(crate) eligible: bool,
    pub(crate) blocker: Option<WireWriteBlocker>,
    pub(crate) observed_car_available: bool,
    pub(crate) observed_floor_reference: Option<i16>,
    pub(crate) observed_operating_mode: Option<WireOperatingMode>,
}

/// Read-side UI presentation projection derived from Engine `IntentRecord` facts.
/// This is NOT a canonical domain operation lifecycle enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireProjectedOperationStatus {
    NotFound,
    Rejected,
    Accepted,
    InProgress,
    Completed,
    Diverged,
    KnowledgeUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireReceipt {
    NotSubmitted,
    Awaiting,
    Accepted,
    Rejected,
    Indeterminate,
}

impl From<Receipt> for WireReceipt {
    fn from(receipt: Receipt) -> Self {
        match receipt {
            Receipt::NotSubmitted => Self::NotSubmitted,
            Receipt::Awaiting => Self::Awaiting,
            Receipt::Accepted => Self::Accepted,
            Receipt::Rejected => Self::Rejected,
            Receipt::Indeterminate => Self::Indeterminate,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireReconciliation {
    Pending,
    Satisfied,
    Diverged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct WireProjectedSubmission {
    pub(crate) control_api_version: &'static str,
    pub(crate) development_only: bool,
    pub(crate) control_instance_id: String,
    pub(crate) ephemeral_process_memory: bool,
    pub(crate) submission_key: String,
    pub(crate) operation: &'static str,
    pub(crate) car: Option<String>,
    pub(crate) floor: Option<i16>,
    pub(crate) status: WireProjectedOperationStatus,
    pub(crate) duplicate_submission: bool,
    pub(crate) intent_id: Option<u64>,
    pub(crate) receipt: Option<WireReceipt>,
    pub(crate) reconciliation: Option<WireReconciliation>,
    pub(crate) diverged_floor: Option<i16>,
    pub(crate) saw_progress: bool,
    pub(crate) is_active_in_engine: bool,
    pub(crate) blocker: Option<WireWriteBlocker>,
    pub(crate) knowledge_reason: Option<&'static str>,
}

#[derive(Debug)]
pub(crate) struct SimDevControl {
    host: SimReadHost,
    controller: VirtualController,
    grants: BTreeMap<CarId, AuthorityGrant>,
    control_instance_id: String,
    submissions: SubmissionRegistry,
}

impl SimDevControl {
    /// Default Simulator world:
    /// - Building topology: floors 1..=8.
    /// - Car A: Operational, Normal, continuous at 3F, ServeFloor execution floors 1..=8.
    /// - Car B: Operational, Normal, continuous at 1F, ServeFloor execution floors 1..=6
    ///   (deliberately narrower than BuildingTopology to prove floor selection comes from
    ///   `ExecutionCapabilities` rather than building topology).
    pub(crate) fn default_simulator() -> Result<Self, String> {
        let world = SimRuntime::new(
            [
                (
                    CarState::with_continuous_position("A", Position::at_floor(Floor::new(3))),
                    ExecutionCapabilities::new(true, false, (1..=8).map(Floor::new)),
                ),
                (
                    CarState::with_continuous_position("B", Position::at_floor(Floor::new(1))),
                    ExecutionCapabilities::new(true, false, (1..=6).map(Floor::new)),
                ),
            ],
            1.5,
        );
        Self::from_sim_runtime(
            BuildingTopology::reference_eight_floor(),
            world,
            generate_control_instance_id()?,
            MAX_SUBMISSION_RECORDS,
        )
    }

    /// Simulator fixture with Car A operational and Car B configured Operational whose
    /// observation is revoked (unavailable), plus an Auxiliary binding on Car B to prove
    /// Auxiliary evidence never restores command eligibility.
    pub(crate) fn with_unavailable_car_profile() -> Result<Self, String> {
        let mut sim = Self::default_simulator()?;
        let (source, session) = sim.controller.observation_source();
        let aux_source = ObservationSourceId::new("simulator-aux");
        let aux_session = ObservationSessionId::new("simulator-aux-session-1");
        let car_b = CarId::new("B");
        sim.host
            .bind_observation(
                aux_source.clone(),
                car_b.clone(),
                ObservationBindingRole::Auxiliary,
            )
            .map_err(|error| format!("failed to bind auxiliary fixture: {error:?}"))?;
        sim.host
            .begin_source_session(aux_source.clone(), aux_session.clone())
            .map_err(|error| format!("failed to begin auxiliary session: {error:?}"))?;
        sim.host
            .revoke_car_observations(&source, &session, &car_b)
            .map_err(|error| format!("failed to revoke operational car B: {error:?}"))?;
        sim.host
            .ingest_observation(liftassistant_model::CarObservationUpdate::from_state(
                liftassistant_model::ObservationProvenance {
                    source: aux_source,
                    session: aux_session,
                    sequence: liftassistant_model::ObservationSequence::new(1),
                },
                &CarState::with_continuous_position("B", Position::at_floor(Floor::new(1))),
            ));
        Ok(sim)
    }

    pub(crate) fn from_sim_runtime(
        topology: BuildingTopology,
        world: SimRuntime,
        control_instance_id: String,
        registry_capacity: usize,
    ) -> Result<Self, String> {
        let controller_id = ControllerId::new(DEV_SIM_CONTROLLER_ID);
        let mut controller = VirtualController::new(controller_id.clone(), world, true);
        let (source, session) = controller.observation_source();
        let mut host = SimReadHost::new(topology);
        let mut grants = BTreeMap::new();

        for (car, capabilities) in controller.execution_capabilities() {
            host.bind_observation(
                source.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .map_err(|error| format!("failed to bind simulator observation: {error:?}"))?;
            host.engine_mut()
                .bind_controlled_car(car.clone(), controller_id.clone())
                .map_err(|error| format!("failed to bind controlled car: {error:?}"))?;
            host.engine_mut()
                .set_execution_capabilities(car.clone(), capabilities);
            let grant = host
                .engine_mut()
                .grant_authority(&car, PrincipalId::new(DEV_SIM_PRINCIPAL))
                .map_err(|error| format!("failed to grant dev simulator authority: {error:?}"))?;
            grants.insert(car, grant);
        }

        host.begin_source_session(source, session)
            .map_err(|error| format!("failed to activate simulator source session: {error:?}"))?;
        for update in controller.observe_current() {
            host.ingest_observation(update);
        }

        Ok(Self {
            host,
            controller,
            grants,
            control_instance_id,
            submissions: SubmissionRegistry::new(registry_capacity),
        })
    }

    pub(crate) fn control_instance_id(&self) -> &str {
        &self.control_instance_id
    }

    pub(crate) fn read_model(&self) -> SimReadModel {
        self.host.read_model()
    }

    pub(crate) fn advance(&mut self, elapsed: Duration) -> bool {
        let step = self.controller.advance(elapsed);
        let mut changed = false;
        for receipt in step.receipts {
            let _ = self.host.engine_mut().receive_intent_receipt(receipt);
            changed = true;
        }
        for observation in step.observations {
            self.host.ingest_observation(observation);
            changed = true;
        }
        changed
    }

    pub(crate) fn capabilities(&self) -> WireCapabilitiesResponse {
        let cars = self
            .host
            .configured_cars()
            .into_iter()
            .map(|car| WireCarCapabilities {
                serve_floor: self.project_car_capability(&car),
                car: car.as_str().to_owned(),
            })
            .collect();
        WireCapabilitiesResponse {
            control_api_version: DEV_CONTROL_API_VERSION,
            development_only: true,
            control_instance_id: self.control_instance_id.clone(),
            ephemeral_process_memory: true,
            authority_context: WireAuthorityMetadata {
                kind: "dev_simulator_process_local_grant",
                multi_user_auth: false,
                principal: DEV_SIM_PRINCIPAL,
            },
            cars,
        }
    }

    pub(crate) fn preview_serve_floor(
        &self,
        car_raw: &str,
        floor_raw: i16,
    ) -> Result<WireServeFloorPreview, ValidationError> {
        let car = validate_car_id(car_raw)?;
        let floor = Floor::new(floor_raw);
        let cap = self.project_car_capability(&car);
        let engine = self.host.engine();

        // Observed car state is strictly live Operational evidence — never stale values.
        let observed = engine.operational_car_state(&car);
        let (observed_car_available, observed_floor_reference, observed_operating_mode) =
            match observed {
                Some(state) => (
                    true,
                    Some(state.location.floor_reference().index()),
                    Some(WireOperatingMode::from(state.mode)),
                ),
                None => (false, None, None),
            };

        let readiness = self.evaluate_preview_readiness(&car, floor);
        Ok(WireServeFloorPreview {
            control_api_version: DEV_CONTROL_API_VERSION,
            development_only: true,
            control_instance_id: self.control_instance_id.clone(),
            advisory: true,
            operation: "serve_floor",
            car: car.as_str().to_owned(),
            floor: floor_raw,
            support: cap.support,
            control_path_availability: cap.control_path_availability,
            control_path_reason: cap.control_path_reason,
            authorization: cap.authorization,
            eligible: readiness.is_ok(),
            blocker: readiness.err().map(WireWriteBlocker::from),
            observed_car_available,
            observed_floor_reference,
            observed_operating_mode,
        })
    }

    pub(crate) fn submit_serve_floor(
        &mut self,
        submission_key: &str,
        car_raw: &str,
        floor_raw: i16,
    ) -> Result<WireProjectedSubmission, SubmitError> {
        validate_submission_key(submission_key).map_err(SubmitError::Validation)?;
        let car = validate_car_id(car_raw).map_err(SubmitError::Validation)?;
        let floor = Floor::new(floor_raw);
        let immutable_request = ImmutableServeFloorRequest {
            car: car.clone(),
            floor,
        };

        if let Some(existing) = self.submissions.get(submission_key) {
            if existing.request != immutable_request {
                return Err(SubmitError::Conflict {
                    submission_key: submission_key.to_owned(),
                    message: format!(
                        "submission_key {submission_key:?} is already bound to ServeFloor(car={}, floor={})",
                        existing.request.car, existing.request.floor
                    ),
                });
            }
            return Ok(self.project_submission_entry(submission_key, existing, true));
        }

        // Brand-new submission_key: re-evaluate real Engine admission (preview was advisory only).
        let outcome = self.execute_engine_admission(&car, floor);
        let entry = SubmissionEntry {
            request: immutable_request,
            outcome,
        };

        let active_checker = |candidate: &SubmissionEntry| match candidate.outcome {
            SubmissionOutcome::Admitted { intent_id } => self
                .host
                .engine()
                .active_intent(&candidate.request.car)
                .is_some_and(|record| record.request.id() == intent_id),
            SubmissionOutcome::Rejected { .. } => false,
        };
        let projected = self.project_submission_entry(submission_key, &entry, false);
        self.submissions
            .insert(submission_key.to_owned(), entry, active_checker);
        Ok(projected)
    }

    pub(crate) fn query_submission(
        &self,
        submission_key: &str,
        expected_instance_id: Option<&str>,
    ) -> Result<WireProjectedSubmission, ValidationError> {
        validate_submission_key(submission_key)?;
        if let Some(expected) = expected_instance_id {
            validate_instance_id(expected)?;
            if expected != self.control_instance_id {
                return Ok(WireProjectedSubmission {
                    control_api_version: DEV_CONTROL_API_VERSION,
                    development_only: true,
                    control_instance_id: self.control_instance_id.clone(),
                    ephemeral_process_memory: true,
                    submission_key: submission_key.to_owned(),
                    operation: "serve_floor",
                    car: None,
                    floor: None,
                    status: WireProjectedOperationStatus::KnowledgeUnavailable,
                    duplicate_submission: false,
                    intent_id: None,
                    receipt: None,
                    reconciliation: None,
                    diverged_floor: None,
                    saw_progress: false,
                    is_active_in_engine: false,
                    blocker: None,
                    knowledge_reason: Some("control_process_restarted"),
                });
            }
        }

        if let Some(entry) = self.submissions.get(submission_key) {
            return Ok(self.project_submission_entry(submission_key, entry, false));
        }

        let (status, reason) = if self.submissions.evicted_count() > 0 {
            (
                WireProjectedOperationStatus::KnowledgeUnavailable,
                "submission_evicted_or_unknown",
            )
        } else {
            (
                WireProjectedOperationStatus::NotFound,
                "no_known_submission_in_current_process",
            )
        };
        Ok(WireProjectedSubmission {
            control_api_version: DEV_CONTROL_API_VERSION,
            development_only: true,
            control_instance_id: self.control_instance_id.clone(),
            ephemeral_process_memory: true,
            submission_key: submission_key.to_owned(),
            operation: "serve_floor",
            car: None,
            floor: None,
            status,
            duplicate_submission: false,
            intent_id: None,
            receipt: None,
            reconciliation: None,
            diverged_floor: None,
            saw_progress: false,
            is_active_in_engine: false,
            blocker: None,
            knowledge_reason: Some(reason),
        })
    }

    fn evaluate_preview_readiness(&self, car: &CarId, floor: Floor) -> Result<(), WriteBlocker> {
        let engine = self.host.engine();
        let Some(controller_id) = engine.controlled_controller(car) else {
            return Err(WriteBlocker::ControllerUnbound);
        };
        let Some(grant) = self.grants.get(car) else {
            return Err(WriteBlocker::AuthorityFenced);
        };
        engine.preview_write_readiness(
            &Intent::ServeFloor {
                car: car.clone(),
                floor,
            },
            None,
            grant,
            controller_id,
            self.controller.available(car),
        )
    }

    fn execute_engine_admission(&mut self, car: &CarId, floor: Floor) -> SubmissionOutcome {
        if self.host.engine().controlled_controller(car).is_none() {
            return SubmissionOutcome::Rejected {
                blocker: WriteBlocker::ControllerUnbound,
            };
        }
        let Some(grant) = self.grants.get(car).cloned() else {
            return SubmissionOutcome::Rejected {
                blocker: WriteBlocker::AuthorityFenced,
            };
        };
        let request = self.host.engine_mut().issue_controlled_intent(
            Intent::ServeFloor {
                car: car.clone(),
                floor,
            },
            None,
        );
        let id = request.id();
        match self
            .host
            .engine_mut()
            .submit_controlled(request, &grant, &mut self.controller)
        {
            Admission::Admitted => SubmissionOutcome::Admitted { intent_id: id },
            Admission::Blocked(blocker) => SubmissionOutcome::Rejected { blocker },
            Admission::Retry | Admission::IdentityConflict | Admission::RetiredIdentity => {
                // Freshly issued IntentId cannot legitimately collide; fail closed.
                SubmissionOutcome::Rejected {
                    blocker: WriteBlocker::ControllerUnavailable,
                }
            }
        }
    }

    fn project_car_capability(&self, car: &CarId) -> WireCarServeFloorCapability {
        let engine = self.host.engine();
        let active_intent_id = engine
            .active_intent(car)
            .map(|record| record.request.id().value());

        let Some(controller_id) = engine.controlled_controller(car) else {
            return WireCarServeFloorCapability {
                support: WireOperationSupport::Unsupported,
                control_path_availability: WireControlPathAvailability::Unavailable,
                control_path_reason: Some("controller_unbound"),
                authorization: WireAuthorizationContext::Unbound,
                executable_floors: Vec::new(),
                active_intent_id,
            };
        };

        let authorization = match self.grants.get(car) {
            None => WireAuthorizationContext::Fenced,
            Some(grant) => {
                let probe = engine.preview_write_readiness(
                    &Intent::ServeFloor {
                        car: car.clone(),
                        floor: engine.topology().lowest_floor(),
                    },
                    None,
                    grant,
                    controller_id,
                    true,
                );
                if probe == Err(WriteBlocker::AuthorityFenced) {
                    WireAuthorizationContext::Fenced
                } else {
                    WireAuthorizationContext::DevProcessLocalGrant
                }
            }
        };

        let (support, executable_floors) = match engine.execution_capabilities(car) {
            None => (WireOperationSupport::Unknown, Vec::new()),
            Some(caps) if !caps.serve_floor => (WireOperationSupport::Unsupported, Vec::new()),
            Some(caps) => {
                let floors = engine
                    .topology()
                    .served_floors()
                    .iter()
                    .copied()
                    .filter(|floor| caps.supports_floor(*floor))
                    .map(Floor::index)
                    .collect();
                (WireOperationSupport::Supported, floors)
            }
        };

        let (control_path_availability, control_path_reason) =
            if authorization == WireAuthorizationContext::Fenced {
                (
                    WireControlPathAvailability::Unavailable,
                    Some("authority_fenced"),
                )
            } else if !self.controller.available(car) {
                (
                    WireControlPathAvailability::Unavailable,
                    Some("controller_unavailable"),
                )
            } else if engine.execution_capabilities(car).is_none() {
                (
                    WireControlPathAvailability::Unavailable,
                    Some("capabilities_unavailable"),
                )
            } else if support == WireOperationSupport::Unsupported {
                (
                    WireControlPathAvailability::Unavailable,
                    Some("operation_unsupported"),
                )
            } else {
                (WireControlPathAvailability::Available, None)
            };

        WireCarServeFloorCapability {
            support,
            control_path_availability,
            control_path_reason,
            authorization,
            executable_floors,
            active_intent_id,
        }
    }

    fn project_submission_entry(
        &self,
        submission_key: &str,
        entry: &SubmissionEntry,
        duplicate_submission: bool,
    ) -> WireProjectedSubmission {
        let car = Some(entry.request.car.as_str().to_owned());
        let floor = Some(entry.request.floor.index());

        match entry.outcome {
            SubmissionOutcome::Rejected { blocker } => WireProjectedSubmission {
                control_api_version: DEV_CONTROL_API_VERSION,
                development_only: true,
                control_instance_id: self.control_instance_id.clone(),
                ephemeral_process_memory: true,
                submission_key: submission_key.to_owned(),
                operation: "serve_floor",
                car,
                floor,
                status: WireProjectedOperationStatus::Rejected,
                duplicate_submission,
                intent_id: None,
                receipt: None,
                reconciliation: None,
                diverged_floor: None,
                saw_progress: false,
                is_active_in_engine: false,
                blocker: Some(WireWriteBlocker::from(blocker)),
                knowledge_reason: None,
            },
            SubmissionOutcome::Admitted { intent_id } => {
                let engine = self.host.engine();
                let is_active = engine
                    .active_intent(&entry.request.car)
                    .is_some_and(|active| active.request.id() == intent_id);
                let Some(record) = engine.intent_record(intent_id) else {
                    return WireProjectedSubmission {
                        control_api_version: DEV_CONTROL_API_VERSION,
                        development_only: true,
                        control_instance_id: self.control_instance_id.clone(),
                        ephemeral_process_memory: true,
                        submission_key: submission_key.to_owned(),
                        operation: "serve_floor",
                        car,
                        floor,
                        status: WireProjectedOperationStatus::KnowledgeUnavailable,
                        duplicate_submission,
                        intent_id: Some(intent_id.value()),
                        receipt: None,
                        reconciliation: None,
                        diverged_floor: None,
                        saw_progress: false,
                        is_active_in_engine: false,
                        blocker: None,
                        knowledge_reason: Some("engine_history_evicted"),
                    };
                };
                self.project_engine_record(
                    submission_key,
                    entry,
                    record,
                    is_active,
                    duplicate_submission,
                )
            }
        }
    }

    fn project_engine_record(
        &self,
        submission_key: &str,
        entry: &SubmissionEntry,
        record: &IntentRecord,
        is_active: bool,
        duplicate_submission: bool,
    ) -> WireProjectedSubmission {
        let (reconciliation, diverged_floor) = match record.reconciliation {
            Reconciliation::Pending => (WireReconciliation::Pending, None),
            Reconciliation::Satisfied => (WireReconciliation::Satisfied, None),
            Reconciliation::Diverged { observed } => {
                (WireReconciliation::Diverged, Some(observed.index()))
            }
        };

        let (status, blocker, knowledge_reason) = match record.reconciliation {
            Reconciliation::Satisfied => (WireProjectedOperationStatus::Completed, None, None),
            Reconciliation::Diverged { .. } => (WireProjectedOperationStatus::Diverged, None, None),
            Reconciliation::Pending => match record.receipt {
                Receipt::Rejected => (
                    WireProjectedOperationStatus::Rejected,
                    Some(WireWriteBlocker {
                        code: "controller_rejected",
                        detail: "lower simulator controller rejected the submitted intent".into(),
                        blocking_intent_id: None,
                        observed_mode: None,
                    }),
                    None,
                ),
                Receipt::NotSubmitted => (
                    WireProjectedOperationStatus::Rejected,
                    Some(WireWriteBlocker {
                        code: "not_submitted",
                        detail: "controller sink did not submit the operation".into(),
                        blocking_intent_id: None,
                        observed_mode: None,
                    }),
                    None,
                ),
                Receipt::Indeterminate if !record.saw_progress() => (
                    WireProjectedOperationStatus::KnowledgeUnavailable,
                    None,
                    Some("delivery_or_receipt_indeterminate"),
                ),
                Receipt::Awaiting | Receipt::Accepted | Receipt::Indeterminate => {
                    if record.saw_progress() {
                        (WireProjectedOperationStatus::InProgress, None, None)
                    } else {
                        (WireProjectedOperationStatus::Accepted, None, None)
                    }
                }
            },
        };

        WireProjectedSubmission {
            control_api_version: DEV_CONTROL_API_VERSION,
            development_only: true,
            control_instance_id: self.control_instance_id.clone(),
            ephemeral_process_memory: true,
            submission_key: submission_key.to_owned(),
            operation: "serve_floor",
            car: Some(entry.request.car.as_str().to_owned()),
            floor: Some(entry.request.floor.index()),
            status,
            duplicate_submission,
            intent_id: Some(record.request.id().value()),
            receipt: Some(WireReceipt::from(record.receipt)),
            reconciliation: Some(reconciliation),
            diverged_floor,
            saw_progress: record.saw_progress(),
            is_active_in_engine: is_active,
            blocker,
            knowledge_reason,
        }
    }

    #[cfg(test)]
    pub(crate) fn controller_submissions(&self) -> u64 {
        self.controller.submissions()
    }

    #[cfg(test)]
    pub(crate) fn registry_len(&self) -> usize {
        self.submissions.len()
    }

    #[cfg(test)]
    pub(crate) fn set_controller_connected(&mut self, connected: bool) {
        self.controller.set_connected(connected);
    }

    #[cfg(test)]
    pub(crate) fn set_next_virtual_behavior(
        &mut self,
        behavior: liftassistant_sim::VirtualBehavior,
    ) {
        self.controller.set_next_behavior(behavior);
    }

    #[cfg(test)]
    pub(crate) fn set_car_mode(&mut self, car: &CarId, mode: liftassistant_model::OperatingMode) {
        self.controller
            .set_mode(car, mode)
            .expect("test car exists in simulator");
        for update in self.controller.observe_current() {
            self.host.ingest_observation(update);
        }
    }

    #[cfg(test)]
    pub(crate) fn revoke_car_observation(&mut self, car: &CarId) {
        let (source, session) = self.controller.observation_source();
        self.host
            .revoke_car_observations(&source, &session, car)
            .expect("operational car observation revokes");
    }

    #[cfg(test)]
    pub(crate) fn revoke_grant(&mut self, car: &CarId) {
        let grant = self
            .grants
            .get(car)
            .cloned()
            .expect("grant exists for test car");
        self.host
            .engine_mut()
            .revoke_authority(car, &grant)
            .expect("grant revokes in engine");
    }

    #[cfg(test)]
    pub(crate) fn bind_unbound_car_only_for_observation(&mut self, car: CarId, floor: Floor) {
        let source = ObservationSourceId::new("simulator-unbound");
        let session = ObservationSessionId::new("simulator-unbound-session-1");
        self.host
            .bind_observation(
                source.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("bind unbound car");
        self.host
            .begin_source_session(source.clone(), session.clone())
            .expect("begin unbound session");
        self.host
            .ingest_observation(liftassistant_model::CarObservationUpdate::from_state(
                liftassistant_model::ObservationProvenance {
                    source,
                    session,
                    sequence: liftassistant_model::ObservationSequence::new(1),
                },
                &CarState::with_continuous_position(car.as_str(), Position::at_floor(floor)),
            ));
    }

    #[cfg(test)]
    pub(crate) fn from_sim_runtime_without_caps_for(
        topology: BuildingTopology,
        world: SimRuntime,
        control_instance_id: String,
        omit_caps_car: &CarId,
    ) -> Result<Self, String> {
        let controller_id = ControllerId::new(DEV_SIM_CONTROLLER_ID);
        let mut controller = VirtualController::new(controller_id.clone(), world, true);
        let (source, session) = controller.observation_source();
        let mut host = SimReadHost::new(topology);
        let mut grants = BTreeMap::new();

        for (car, capabilities) in controller.execution_capabilities() {
            host.bind_observation(
                source.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .map_err(|error| format!("failed to bind simulator observation: {error:?}"))?;
            host.engine_mut()
                .bind_controlled_car(car.clone(), controller_id.clone())
                .map_err(|error| format!("failed to bind controlled car: {error:?}"))?;
            if &car != omit_caps_car {
                host.engine_mut()
                    .set_execution_capabilities(car.clone(), capabilities);
            }
            let grant = host
                .engine_mut()
                .grant_authority(&car, PrincipalId::new(DEV_SIM_PRINCIPAL))
                .map_err(|error| format!("failed to grant dev simulator authority: {error:?}"))?;
            grants.insert(car, grant);
        }

        host.begin_source_session(source, session)
            .map_err(|error| format!("failed to activate simulator source session: {error:?}"))?;
        for update in controller.observe_current() {
            host.ingest_observation(update);
        }

        Ok(Self {
            host,
            controller,
            grants,
            control_instance_id,
            submissions: SubmissionRegistry::new(MAX_SUBMISSION_RECORDS),
        })
    }
}
