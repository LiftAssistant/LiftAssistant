use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use liftassistant_model::ObservationSessionId;
use liftassistant_rmf::{
    GatewayCommandResponse, GatewayConfig, PrivateIngress, RecoveryOperatorConfig,
    RmfCommandTransport, RmfIntentGateway, RmfLiftCommand, RmfLiftState, RmfLivePayload,
};
use serde_json::Value;
use tempfile::TempDir;

struct ProcessTransport {
    log: PathBuf,
    crash_after_possible_send: bool,
}

impl RmfCommandTransport for ProcessTransport {
    fn available(&self) -> bool {
        true
    }

    fn publish(&mut self, command: RmfLiftCommand) -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .map_err(|error| error.to_string())?;
        writeln!(
            file,
            "{} {} {}",
            command.lift_name, command.destination_floor, command.session_id
        )
        .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        if self.crash_after_possible_send {
            std::process::abort();
        }
        Ok(())
    }
}

fn config(root: &Path) -> GatewayConfig {
    let domain_id = root.to_string_lossy().bytes().fold(0_u16, |value, byte| {
        value.wrapping_mul(31).wrapping_add(u16::from(byte))
    }) % 200
        + 1;
    GatewayConfig {
        socket_path: root.join("gateway.sock"),
        state_path: root.join("gateway-state.json"),
        config_revision: "process-fixture-v1".into(),
        allowed_uid: rustix::process::getuid().as_raw(),
        principal: "process-fixture-caller".into(),
        recovery_operator: Some(RecoveryOperatorConfig {
            allowed_uid: rustix::process::getuid().as_raw(),
            principal: "process-recovery-operator".into(),
        }),
        domain_id,
        source_id: "process-rmf-fixture".into(),
        logical_car: "A".into(),
        native_lift: "Lift1".into(),
        floors: BTreeMap::from([("L1".into(), 1), ("L2".into(), 2)]),
        max_payload_silence_ms: 4000,
    }
}

fn payload(time: i64, floor: &str, motion: u8, door: u8, holder: &str) -> RmfLivePayload {
    RmfLivePayload {
        source_time_nanos: time,
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

#[test]
fn gateway_child_process() {
    let Ok(mode) = env::var("LIFTASSISTANT_GATEWAY_CHILD_MODE") else {
        return;
    };
    let root = PathBuf::from(env::var_os("LIFTASSISTANT_GATEWAY_CHILD_ROOT").unwrap());
    let config = config(&root);
    let transport = ProcessTransport {
        log: root.join("send.log"),
        crash_after_possible_send: mode == "crash-after-send",
    };
    let mut gateway = RmfIntentGateway::new(
        config.clone(),
        ObservationSessionId::new(format!("child-{mode}")),
        transport,
    )
    .unwrap();
    gateway
        .ingest(payload(
            1,
            "L1",
            RmfLiftState::MOTION_STOPPED,
            RmfLiftState::DOOR_CLOSED,
            "",
        ))
        .unwrap();
    let ingress = PrivateIngress::bind(&config.socket_path).unwrap();
    let requests = match mode.as_str() {
        "recovery-retire" => 4,
        "recovery-resume" => 3,
        _ => 1,
    };
    let mut responses = Vec::new();
    for _ in 0..requests {
        let mut connection = ingress.accept().unwrap();
        let request = connection.read_request().unwrap();
        let response = gateway.handle_command(connection.peer_uid(), request);
        connection.respond(&response).unwrap();
        responses.push(response);
    }

    if mode == "recover" {
        let response = &responses[0];
        assert_eq!(response.code, "recovery_required");
        let holder = gateway.unresolved().unwrap().rmf_session_id.clone();
        gateway
            .ingest(payload(
                2,
                "L1",
                RmfLiftState::MOTION_UP,
                RmfLiftState::DOOR_CLOSED,
                &holder,
            ))
            .unwrap();
        gateway
            .ingest(payload(
                3,
                "L2",
                RmfLiftState::MOTION_STOPPED,
                RmfLiftState::DOOR_OPEN,
                &holder,
            ))
            .unwrap();
        assert!(gateway.unresolved().is_none());
    }
}

#[test]
fn process_restart_recovers_same_operation_without_resend() {
    if env::var_os("LIFTASSISTANT_GATEWAY_CHILD_MODE").is_some() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let first = spawn_child(temp.path(), "crash-after-send");
    wait_for_socket(&temp.path().join("gateway.sock"));
    let first_response = send_request(&temp.path().join("gateway.sock"));
    assert!(
        first_response.is_err(),
        "aborted send must not claim a response"
    );
    let first_status = first.wait_with_output().unwrap();
    assert!(!first_status.status.success());

    let state: Value =
        serde_json::from_slice(&fs::read(temp.path().join("gateway-state.json")).unwrap()).unwrap();
    assert_eq!(state["last_intent_id"], 1);
    assert_eq!(state["unresolved"]["intent_id"], 1);
    assert_eq!(state["unresolved"]["delivery"], "reserved_uncertain");
    assert_eq!(send_count(&temp.path().join("send.log")), 1);

    let restarted = spawn_child(temp.path(), "recover");
    wait_for_socket(&temp.path().join("gateway.sock"));
    let response = send_request(&temp.path().join("gateway.sock")).unwrap();
    assert!(!response.accepted);
    assert_eq!(response.code, "recovery_required");
    let restarted_status = restarted.wait_with_output().unwrap();
    assert!(
        restarted_status.status.success(),
        "recovery child failed: {}",
        String::from_utf8_lossy(&restarted_status.stderr)
    );
    assert_eq!(send_count(&temp.path().join("send.log")), 1);
    let recovered: Value =
        serde_json::from_slice(&fs::read(temp.path().join("gateway-state.json")).unwrap()).unwrap();
    assert_eq!(recovered["last_intent_id"], 1);
    assert!(recovered["unresolved"].is_null());
}

#[test]
fn private_ipc_recovery_retirement_and_resumption_survive_process_restart() {
    if env::var_os("LIFTASSISTANT_GATEWAY_CHILD_MODE").is_some() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let first = spawn_child(temp.path(), "crash-after-send");
    wait_for_socket(&temp.path().join("gateway.sock"));
    assert!(send_request(&temp.path().join("gateway.sock")).is_err());
    assert!(!first.wait_with_output().unwrap().status.success());
    assert_eq!(send_count(&temp.path().join("send.log")), 1);

    let retiring = spawn_child(temp.path(), "recovery-retire");
    wait_for_socket(&temp.path().join("gateway.sock"));
    let wrong = send_json(
        &temp.path().join("gateway.sock"),
        serde_json::json!({
            "version": 1,
            "operation": "inspect_recovery",
            "principal": "process-fixture-caller"
        }),
    )
    .unwrap();
    assert_eq!(wrong.code, "unauthorized_recovery_principal");
    let normal = send_request(&temp.path().join("gateway.sock")).unwrap();
    assert_eq!(normal.code, "recovery_required");
    let inspection = send_json(
        &temp.path().join("gateway.sock"),
        serde_json::json!({
            "version": 1,
            "operation": "inspect_recovery",
            "principal": "process-recovery-operator"
        }),
    )
    .unwrap();
    assert!(inspection.accepted);
    assert_eq!(
        inspection.recovery.unwrap().unresolved.unwrap().intent_id,
        1
    );
    let retired = send_json(
        &temp.path().join("gateway.sock"),
        serde_json::json!({
            "version": 1,
            "operation": "retire_unknown",
            "principal": "process-recovery-operator",
            "reason": "process crash recovery outcome remains unknown"
        }),
    )
    .unwrap();
    assert_eq!(retired.code, "retired_outcome_unknown");
    assert_eq!(
        retired.readiness,
        liftassistant_rmf::GatewayReadiness::ResumptionRequired
    );
    assert!(retiring.wait_with_output().unwrap().status.success());
    assert_eq!(send_count(&temp.path().join("send.log")), 1);

    let state: Value =
        serde_json::from_slice(&fs::read(temp.path().join("gateway-state.json")).unwrap()).unwrap();
    assert!(state["unresolved"].is_null());
    assert_eq!(state["resumption_required"]["operation"]["intent_id"], 1);

    let resuming = spawn_child(temp.path(), "recovery-resume");
    wait_for_socket(&temp.path().join("gateway.sock"));
    let bypass = send_request(&temp.path().join("gateway.sock")).unwrap();
    assert_eq!(bypass.code, "resumption_required");
    let resumed = send_json(
        &temp.path().join("gateway.sock"),
        serde_json::json!({
            "version": 1,
            "operation": "resume_control",
            "principal": "process-recovery-operator",
            "reason": "resume despite explicitly retained historical uncertainty",
            "accept_residual_uncertainty": true
        }),
    )
    .unwrap();
    assert_eq!(resumed.code, "control_resumed_with_residual_uncertainty");
    let next = send_request(&temp.path().join("gateway.sock")).unwrap();
    assert_eq!(next.intent_id, Some(2));
    assert!(resuming.wait_with_output().unwrap().status.success());
    assert_eq!(send_count(&temp.path().join("send.log")), 2);
}

fn spawn_child(root: &Path, mode: &str) -> std::process::Child {
    Command::new(env::current_exe().unwrap())
        .arg("--exact")
        .arg("gateway_child_process")
        .arg("--nocapture")
        .env("LIFTASSISTANT_GATEWAY_CHILD_MODE", mode)
        .env("LIFTASSISTANT_GATEWAY_CHILD_ROOT", root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn wait_for_socket(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "gateway child did not bind socket"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn send_request(path: &Path) -> Result<GatewayCommandResponse, String> {
    send_json(
        path,
        serde_json::json!({
            "version": 1,
            "operation": "serve_floor",
            "logical_car": "A",
            "floor": 2
        }),
    )
}

fn send_json(path: &Path, request: Value) -> Result<GatewayCommandResponse, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match UnixStream::connect(path) {
            Ok(stream) => break stream,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.to_string()),
        }
    };
    serde_json::to_writer(&mut stream, &request).map_err(|error| error.to_string())?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&response).map_err(|error| error.to_string())
}

fn send_count(path: &Path) -> usize {
    fs::read_to_string(path).unwrap().lines().count()
}
