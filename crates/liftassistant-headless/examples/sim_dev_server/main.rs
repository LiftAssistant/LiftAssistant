#![forbid(unsafe_code)]

//! Development-only Simulator server exposing:
//! - Read-only `GET /api/experimental/v0/snapshot` and `GET /api/experimental/v0/events`
//! - Explicitly opted-in, loopback-only, development-only Simulator control surface under
//!   `/api/dev-sim/v0/*` for `ServeFloor` only.
//!
//! This binary is isolated in `examples/` so normal `liftassistant-headless` builds retain zero
//! `liftassistant-sim` dependency and zero write routes. It cannot reach RMF, LCE, or real
//! elevator controllers.

mod control;
mod http;
mod read_projection;

use std::{
    env,
    net::{SocketAddr, TcpListener},
    process::ExitCode,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use signal_hook::{
    consts::{SIGINT, SIGTERM},
    flag,
};

use control::SimDevControl;
use http::ReadModelPublisher;

const DEFAULT_TICK_MS: u64 = 100;
const MIN_TICK_MS: u64 = 10;
const MAX_TICK_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SimProfile {
    Default,
    UnavailableCar,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CliConfig {
    listen: SocketAddr,
    enable_dev_sim_control: bool,
    tick_interval: Duration,
    profile: SimProfile,
}

fn main() -> ExitCode {
    match parse_args(env::args().skip(1)).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> String {
    "usage: sim-dev-server --listen <ip:port> [--enable-dev-sim-control] [--tick-ms <10..=5000>] [--profile <default|unavailable-car>]".into()
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<CliConfig, String> {
    let mut iter = args.into_iter();
    let mut listen: Option<SocketAddr> = None;
    let mut enable_dev_sim_control = false;
    let mut tick_ms = DEFAULT_TICK_MS;
    let mut profile = SimProfile::Default;

    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--listen" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| format!("missing value for --listen\n{}", usage()))?;
                let parsed = raw
                    .parse::<SocketAddr>()
                    .map_err(|error| format!("invalid --listen address {raw:?}: {error}"))?;
                if listen.replace(parsed).is_some() {
                    return Err("--listen may be specified only once".into());
                }
            }
            "--enable-dev-sim-control" => {
                enable_dev_sim_control = true;
            }
            "--tick-ms" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| format!("missing value for --tick-ms\n{}", usage()))?;
                let parsed = raw
                    .parse::<u64>()
                    .map_err(|error| format!("invalid --tick-ms value {raw:?}: {error}"))?;
                if !(MIN_TICK_MS..=MAX_TICK_MS).contains(&parsed) {
                    return Err(format!(
                        "--tick-ms must be within {MIN_TICK_MS}..={MAX_TICK_MS}"
                    ));
                }
                tick_ms = parsed;
            }
            "--profile" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| format!("missing value for --profile\n{}", usage()))?;
                profile = match raw.as_str() {
                    "default" => SimProfile::Default,
                    "unavailable-car" => SimProfile::UnavailableCar,
                    other => {
                        return Err(format!(
                            "unknown --profile {other:?}; expected \"default\" or \"unavailable-car\""
                        ));
                    }
                };
            }
            other => {
                return Err(format!("unknown argument {other:?}\n{}", usage()));
            }
        }
    }

    let listen = listen.ok_or_else(|| format!("--listen is required\n{}", usage()))?;
    if enable_dev_sim_control && !listen.ip().is_loopback() {
        return Err(format!(
            "--enable-dev-sim-control requires a loopback --listen address (got {listen})"
        ));
    }

    Ok(CliConfig {
        listen,
        enable_dev_sim_control,
        tick_interval: Duration::from_millis(tick_ms),
        profile,
    })
}

fn run(config: CliConfig) -> Result<(), String> {
    let shutdown = Arc::new(AtomicBool::new(false));
    flag::register(SIGINT, Arc::clone(&shutdown))
        .map_err(|error| format!("failed to register SIGINT handler: {error}"))?;
    flag::register(SIGTERM, Arc::clone(&shutdown))
        .map_err(|error| format!("failed to register SIGTERM handler: {error}"))?;

    let sim = match config.profile {
        SimProfile::Default => SimDevControl::default_simulator()?,
        SimProfile::UnavailableCar => SimDevControl::with_unavailable_car_profile()?,
    };
    let instance_id = sim.control_instance_id().to_owned();
    let publisher = ReadModelPublisher::new(sim.read_model());
    let sim = Arc::new(Mutex::new(sim));

    let listener = TcpListener::bind(config.listen).map_err(|error| {
        format!(
            "failed to bind simulator server on {}: {error}",
            config.listen
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect listener address: {error}"))?;

    let tick_sim = Arc::clone(&sim);
    let tick_publisher = publisher.clone();
    let tick_shutdown = Arc::clone(&shutdown);
    let tick_interval = config.tick_interval;
    let tick_thread = thread::Builder::new()
        .name("liftassistant-sim-dev-tick".into())
        .spawn(move || {
            while !tick_shutdown.load(Ordering::Relaxed) {
                thread::sleep(tick_interval);
                if tick_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                let maybe_model = {
                    let mut guard = tick_sim
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if guard.advance(tick_interval) {
                        Some(guard.read_model())
                    } else {
                        None
                    }
                };
                if let Some(model) = maybe_model {
                    let _ = tick_publisher.publish(model);
                }
            }
        })
        .map_err(|error| format!("failed to spawn simulator tick thread: {error}"))?;

    if config.enable_dev_sim_control {
        println!(
            "DEVELOPMENT-ONLY Simulator server listening on http://{local_addr} (read API: /api/experimental/v0/*, dev-sim control: /api/dev-sim/v0/*, instance={instance_id})"
        );
    } else {
        println!(
            "Simulator read-only server listening on http://{local_addr} (read API: /api/experimental/v0/*, dev-sim control: DISABLED)"
        );
    }

    let serve_result = http::serve(
        listener,
        sim,
        publisher,
        config.enable_dev_sim_control,
        Arc::clone(&shutdown),
    );
    shutdown.store(true, Ordering::Relaxed);
    let _ = tick_thread.join();
    serve_result
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener, TcpStream},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    use liftassistant_model::{
        BuildingTopology, CarId, CarState, ExecutionCapabilities, Floor, OperatingMode, Position,
    };
    use liftassistant_sim::{SimRuntime, VirtualBehavior, VirtualReceipt};
    use serde_json::{Value, json};

    use super::*;
    use crate::control::{
        MAX_SUBMISSION_RECORDS, SubmitError, WireAuthorizationContext, WireControlPathAvailability,
        WireOperationSupport, WireProjectedOperationStatus, WireReceipt, WireReconciliation,
    };
    use crate::http::{
        DEV_CAPABILITIES_PATH, DEV_SERVE_FLOOR_PREVIEW_PATH, DEV_SERVE_FLOOR_SUBMIT_PATH,
        EVENTS_PATH, SNAPSHOT_PATH,
    };

    struct TestServer {
        address: SocketAddr,
        sim: Arc<Mutex<SimDevControl>>,
        shutdown: Arc<AtomicBool>,
        server_thread: Option<thread::JoinHandle<Result<(), String>>>,
        tick_thread: Option<thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn start(sim: SimDevControl, control_enabled: bool, auto_tick_ms: Option<u64>) -> Self {
            let publisher = ReadModelPublisher::new(sim.read_model());
            let sim = Arc::new(Mutex::new(sim));
            let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener binds");
            let address = listener.local_addr().expect("listener address");
            let shutdown = Arc::new(AtomicBool::new(false));

            let server_sim = Arc::clone(&sim);
            let server_publisher = publisher.clone();
            let server_shutdown = Arc::clone(&shutdown);
            let server_thread = thread::spawn(move || {
                http::serve(
                    listener,
                    server_sim,
                    server_publisher,
                    control_enabled,
                    server_shutdown,
                )
            });

            let tick_thread = auto_tick_ms.map(|ms| {
                let tick_sim = Arc::clone(&sim);
                let tick_publisher = publisher;
                let tick_shutdown = Arc::clone(&shutdown);
                let interval = Duration::from_millis(ms);
                thread::spawn(move || {
                    while !tick_shutdown.load(Ordering::Relaxed) {
                        thread::sleep(interval);
                        if tick_shutdown.load(Ordering::Relaxed) {
                            break;
                        }
                        let model = {
                            let mut guard = tick_sim.lock().unwrap();
                            guard.advance(interval).then(|| guard.read_model())
                        };
                        if let Some(model) = model {
                            let _ = tick_publisher.publish(model);
                        }
                    }
                })
            });

            Self {
                address,
                sim,
                shutdown,
                server_thread: Some(server_thread),
                tick_thread,
            }
        }

        fn start_on_address(
            address: SocketAddr,
            sim: SimDevControl,
            control_enabled: bool,
        ) -> Self {
            let publisher = ReadModelPublisher::new(sim.read_model());
            let sim = Arc::new(Mutex::new(sim));
            let listener = (0..50)
                .find_map(|_| match TcpListener::bind(address) {
                    Ok(listener) => Some(listener),
                    Err(_) => {
                        thread::sleep(Duration::from_millis(20));
                        None
                    }
                })
                .expect("loopback listener rebinds");
            let actual = listener.local_addr().expect("listener address");
            let shutdown = Arc::new(AtomicBool::new(false));
            let server_sim = Arc::clone(&sim);
            let server_shutdown = Arc::clone(&shutdown);
            let server_thread = thread::spawn(move || {
                http::serve(
                    listener,
                    server_sim,
                    publisher,
                    control_enabled,
                    server_shutdown,
                )
            });
            Self {
                address: actual,
                sim,
                shutdown,
                server_thread: Some(server_thread),
                tick_thread: None,
            }
        }

        fn stop(mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = self.tick_thread.take() {
                handle.join().expect("tick thread joins");
            }
            if let Some(handle) = self.server_thread.take() {
                handle
                    .join()
                    .expect("server thread joins")
                    .expect("server exits cleanly");
            }
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = self.tick_thread.take() {
                let _ = handle.join();
            }
            if let Some(handle) = self.server_thread.take() {
                let _ = handle.join();
            }
        }
    }

    fn raw_http(address: SocketAddr, request: &str) -> (u16, String, String) {
        let mut stream = (0..50)
            .find_map(|_| match TcpStream::connect(address) {
                Ok(stream) => Some(stream),
                Err(_) => {
                    thread::sleep(Duration::from_millis(10));
                    None
                }
            })
            .expect("connect to loopback server");
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("set read timeout");
        stream
            .write_all(request.as_bytes())
            .expect("write HTTP request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read HTTP response");
        let (head, body) = response
            .split_once("\r\n\r\n")
            .expect("valid HTTP response separator");
        let status_line = head.lines().next().expect("status line");
        let status = status_line
            .split_whitespace()
            .nth(1)
            .expect("status code")
            .parse::<u16>()
            .expect("numeric status");
        (status, head.to_owned(), body.to_owned())
    }

    fn get_json(address: SocketAddr, path: &str) -> (u16, String, Value) {
        let req = format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
        let (status, headers, body) = raw_http(address, &req);
        let json = serde_json::from_str(&body)
            .unwrap_or_else(|error| panic!("invalid JSON ({status}): {error}; body={body:?}"));
        (status, headers, json)
    }

    fn post_json(address: SocketAddr, path: &str, payload: &Value) -> (u16, String, Value) {
        let body = serde_json::to_string(payload).expect("serialize request JSON");
        let req = format!(
            "POST {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (status, headers, raw_body) = raw_http(address, &req);
        let json = serde_json::from_str(&raw_body)
            .unwrap_or_else(|error| panic!("invalid JSON ({status}): {error}; body={raw_body:?}"));
        (status, headers, json)
    }

    #[test]
    fn cli_rejects_non_loopback_when_dev_control_is_enabled_and_defaults_disabled() {
        let default_cfg =
            parse_args(["--listen".into(), "127.0.0.1:8080".into()]).expect("default read-only");
        assert!(!default_cfg.enable_dev_sim_control);

        let non_loopback_err = parse_args([
            "--listen".into(),
            "0.0.0.0:8080".into(),
            "--enable-dev-sim-control".into(),
        ])
        .expect_err("non-loopback control must fail closed");
        assert!(non_loopback_err.contains("loopback"));

        let loopback_ok = parse_args([
            "--listen".into(),
            "127.0.0.1:8080".into(),
            "--enable-dev-sim-control".into(),
        ])
        .expect("loopback opt-in succeeds");
        assert!(loopback_ok.enable_dev_sim_control);
    }

    #[test]
    fn capabilities_preserve_support_availability_authorization_and_execution_floors() {
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
                (
                    CarState::with_continuous_position("C", Position::at_floor(Floor::new(2))),
                    ExecutionCapabilities::new(false, true, (1..=8).map(Floor::new)),
                ),
                (
                    CarState::with_continuous_position("E", Position::at_floor(Floor::new(1))),
                    ExecutionCapabilities::new(true, false, (1..=8).map(Floor::new)),
                ),
            ],
            1.5,
        );
        let mut sim = SimDevControl::from_sim_runtime_without_caps_for(
            BuildingTopology::reference_eight_floor(),
            world,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            &CarId::new("E"),
        )
        .expect("sim builds");
        sim.bind_unbound_car_only_for_observation(CarId::new("D"), Floor::new(1));

        let caps = sim.capabilities();
        assert!(caps.development_only);
        assert!(!caps.authority_context.multi_user_auth);

        let by_id = |id: &str| {
            caps.cars
                .iter()
                .find(|entry| entry.car == id)
                .expect("car present")
                .serve_floor
                .clone()
        };

        let a = by_id("A");
        assert_eq!(a.support, WireOperationSupport::Supported);
        assert_eq!(
            a.control_path_availability,
            WireControlPathAvailability::Available
        );
        assert_eq!(
            a.authorization,
            WireAuthorizationContext::DevProcessLocalGrant
        );
        assert_eq!(a.executable_floors, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        // Car B floors MUST come from ExecutionCapabilities (1..=6), not BuildingTopology (1..=8).
        let b = by_id("B");
        assert_eq!(b.support, WireOperationSupport::Supported);
        assert_eq!(b.executable_floors, vec![1, 2, 3, 4, 5, 6]);

        // Car C explicitly does not support ServeFloor.
        let c = by_id("C");
        assert_eq!(c.support, WireOperationSupport::Unsupported);
        assert_eq!(
            c.control_path_availability,
            WireControlPathAvailability::Unavailable
        );
        assert_eq!(c.control_path_reason, Some("operation_unsupported"));
        assert!(c.executable_floors.is_empty());

        // Car D has no controller bound.
        let d = by_id("D");
        assert_eq!(d.support, WireOperationSupport::Unsupported);
        assert_eq!(
            d.control_path_availability,
            WireControlPathAvailability::Unavailable
        );
        assert_eq!(d.control_path_reason, Some("controller_unbound"));
        assert_eq!(d.authorization, WireAuthorizationContext::Unbound);

        // Car E has a controller binding but no ExecutionCapabilities -> support is UNKNOWN.
        let e = by_id("E");
        assert_eq!(e.support, WireOperationSupport::Unknown);
        assert_eq!(
            e.control_path_availability,
            WireControlPathAvailability::Unavailable
        );
        assert_eq!(e.control_path_reason, Some("capabilities_unavailable"));

        // Disconnect simulator controller -> A remains Supported, but path becomes Unavailable.
        sim.set_controller_connected(false);
        let disconnected = sim.capabilities();
        let a_down = disconnected
            .cars
            .iter()
            .find(|entry| entry.car == "A")
            .unwrap();
        assert_eq!(a_down.serve_floor.support, WireOperationSupport::Supported);
        assert_eq!(
            a_down.serve_floor.control_path_availability,
            WireControlPathAvailability::Unavailable
        );
        assert_eq!(
            a_down.serve_floor.control_path_reason,
            Some("controller_unavailable")
        );

        // Revoke grant on B -> authorization becomes Fenced.
        sim.set_controller_connected(true);
        sim.revoke_grant(&CarId::new("B"));
        let fenced = sim.capabilities();
        let b_fenced = fenced.cars.iter().find(|entry| entry.car == "B").unwrap();
        assert_eq!(
            b_fenced.serve_floor.authorization,
            WireAuthorizationContext::Fenced
        );
        assert_eq!(
            b_fenced.serve_floor.control_path_availability,
            WireControlPathAvailability::Unavailable
        );
        assert_eq!(
            b_fenced.serve_floor.control_path_reason,
            Some("authority_fenced")
        );
    }

    #[test]
    fn preview_is_advisory_and_covers_rejection_matrix_including_unavailable_car_and_auxiliary() {
        let mut sim = SimDevControl::with_unavailable_car_profile().expect("profile builds");

        // Car A is operational and idle -> eligible preview, zero side effects.
        let preview_a = sim.preview_serve_floor("A", 6).expect("preview A");
        assert!(preview_a.advisory);
        assert!(preview_a.eligible);
        assert!(preview_a.blocker.is_none());
        assert!(preview_a.observed_car_available);
        assert_eq!(preview_a.observed_floor_reference, Some(3));
        assert_eq!(sim.controller_submissions(), 0);
        assert_eq!(sim.registry_len(), 0);

        // Car B has revoked Operational observation + active Auxiliary observation:
        // Auxiliary MUST NOT restore command eligibility, and preview MUST NOT expose stale floor/mode.
        let preview_b = sim.preview_serve_floor("B", 4).expect("preview B");
        assert!(!preview_b.eligible);
        assert!(!preview_b.observed_car_available);
        assert!(preview_b.observed_floor_reference.is_none());
        assert!(preview_b.observed_operating_mode.is_none());
        assert_eq!(
            preview_b.blocker.as_ref().map(|b| b.code),
            Some("observation_unavailable")
        );
        let rejected_b = sim
            .submit_serve_floor("sub-unavail-001", "B", 4)
            .expect("structured rejection");
        assert_eq!(rejected_b.status, WireProjectedOperationStatus::Rejected);
        assert!(rejected_b.intent_id.is_none());
        assert_eq!(
            rejected_b.blocker.as_ref().map(|b| b.code),
            Some("observation_unavailable")
        );

        // Unsupported floor on A (outside topology) and on B (after restoring Operational truth, floor 7 > 6).
        let preview_floor_9 = sim.preview_serve_floor("A", 9).expect("preview 9F");
        assert!(!preview_floor_9.eligible);
        assert_eq!(
            preview_floor_9.blocker.as_ref().map(|b| b.code),
            Some("unsupported_floor")
        );

        let mut sim2 = SimDevControl::default_simulator().expect("default sim");
        let preview_b7 = sim2.preview_serve_floor("B", 7).expect("preview B 7F");
        assert!(!preview_b7.eligible);
        assert_eq!(
            preview_b7.blocker.as_ref().map(|b| b.code),
            Some("unsupported_floor")
        );

        // OutOfService mode blocks preview & submit.
        sim2.set_car_mode(&CarId::new("A"), OperatingMode::OutOfService);
        let preview_oos = sim2.preview_serve_floor("A", 5).expect("preview OOS");
        assert!(!preview_oos.eligible);
        assert_eq!(
            preview_oos.blocker.as_ref().map(|b| b.code),
            Some("mode_not_normal")
        );
        let rejected_oos = sim2
            .submit_serve_floor("sub-oos-001", "A", 5)
            .expect("rejected OOS");
        assert_eq!(rejected_oos.status, WireProjectedOperationStatus::Rejected);
        assert!(rejected_oos.intent_id.is_none());

        // Restore Normal mode -> SAME rejected submission_key MUST stay terminal rejected and NOT execute!
        sim2.set_car_mode(&CarId::new("A"), OperatingMode::Normal);
        let still_rejected = sim2
            .submit_serve_floor("sub-oos-001", "A", 5)
            .expect("same key stays terminal rejected");
        assert_eq!(
            still_rejected.status,
            WireProjectedOperationStatus::Rejected
        );
        assert!(still_rejected.duplicate_submission);
        assert!(still_rejected.intent_id.is_none());
        assert_eq!(sim2.controller_submissions(), 0);

        // Brand-new submission_key after recovery succeeds.
        let admitted = sim2
            .submit_serve_floor("sub-oos-002", "A", 5)
            .expect("new key succeeds");
        assert_eq!(admitted.status, WireProjectedOperationStatus::Accepted);
        assert_eq!(admitted.intent_id, Some(2));
        assert_eq!(sim2.controller_submissions(), 1);
    }

    #[test]
    fn full_lifecycle_idempotency_conflict_and_new_key_semantics() {
        let mut sim = SimDevControl::default_simulator().expect("default sim");

        // 1. Submit ServeFloor(A, 6) -> Accepted (not Completed!).
        let accepted = sim
            .submit_serve_floor("sub-trip-0001", "A", 6)
            .expect("submit accepted");
        assert_eq!(accepted.status, WireProjectedOperationStatus::Accepted);
        assert_eq!(accepted.intent_id, Some(1));
        assert_eq!(accepted.receipt, Some(WireReceipt::Awaiting));
        assert_eq!(accepted.reconciliation, Some(WireReconciliation::Pending));
        assert!(!accepted.saw_progress);
        assert!(!accepted.duplicate_submission);
        assert_eq!(sim.controller_submissions(), 1);

        // 2. Duplicate same key + same immutable payload -> idempotent, zero second side effect.
        for _ in 0..5 {
            let dup = sim
                .submit_serve_floor("sub-trip-0001", "A", 6)
                .expect("duplicate idempotent");
            assert_eq!(dup.intent_id, Some(1));
            assert!(dup.duplicate_submission);
            assert_eq!(sim.controller_submissions(), 1);
        }

        // 3. Same key + different floor OR different car -> explicit conflict.
        assert!(matches!(
            sim.submit_serve_floor("sub-trip-0001", "A", 7),
            Err(SubmitError::Conflict { .. })
        ));
        assert!(matches!(
            sim.submit_serve_floor("sub-trip-0001", "B", 6),
            Err(SubmitError::Conflict { .. })
        ));
        assert_eq!(sim.controller_submissions(), 1);

        // 4. New key + same (car, floor) while active -> Engine rejects with ActiveIntent (no (car, floor) dedup).
        let overlapping = sim
            .submit_serve_floor("sub-trip-0002", "A", 6)
            .expect("overlapping rejected");
        assert_eq!(overlapping.status, WireProjectedOperationStatus::Rejected);
        assert_eq!(
            overlapping.blocker.as_ref().map(|b| b.code),
            Some("active_intent")
        );
        assert_eq!(
            overlapping
                .blocker
                .as_ref()
                .and_then(|b| b.blocking_intent_id),
            Some(1)
        );
        assert_eq!(sim.controller_submissions(), 1);

        // 5. Advance simulator one tick -> Accepted receipt + Moving observation -> InProgress!
        assert!(sim.advance(Duration::from_millis(200)));
        let in_progress = sim
            .query_submission("sub-trip-0001", Some(sim.control_instance_id()))
            .expect("query in progress");
        assert_eq!(in_progress.status, WireProjectedOperationStatus::InProgress);
        assert_eq!(in_progress.intent_id, Some(1));
        assert_eq!(in_progress.receipt, Some(WireReceipt::Accepted));
        assert_eq!(
            in_progress.reconciliation,
            Some(WireReconciliation::Pending)
        );
        assert!(in_progress.saw_progress);
        assert!(in_progress.is_active_in_engine);

        // 6. Advance until arrival + door open -> Engine Reconciliation::Satisfied -> Completed!
        for _ in 0..20 {
            sim.advance(Duration::from_millis(200));
        }
        let completed = sim
            .query_submission("sub-trip-0001", Some(sim.control_instance_id()))
            .expect("query completed");
        assert_eq!(completed.status, WireProjectedOperationStatus::Completed);
        assert_eq!(completed.intent_id, Some(1));
        assert_eq!(
            completed.reconciliation,
            Some(WireReconciliation::Satisfied)
        );
        assert!(!completed.is_active_in_engine);

        // 7. New key + same (car, floor) AFTER completion -> new Engine IntentId, NOT deduplicated!
        let second = sim
            .submit_serve_floor("sub-trip-0003", "A", 6)
            .expect("second submission");
        assert_eq!(second.status, WireProjectedOperationStatus::Accepted);
        assert!(second.intent_id.unwrap() > 1);
        assert_eq!(sim.controller_submissions(), 2);
    }

    #[test]
    fn lost_virtual_receipt_projects_unknown_until_canonical_observations_establish_outcome() {
        let mut sim = SimDevControl::default_simulator().expect("default sim");
        sim.set_next_virtual_behavior(VirtualBehavior {
            receipt: VirtualReceipt::Lost,
            ..VirtualBehavior::default()
        });
        let submitted = sim
            .submit_serve_floor("sub-indet-0001", "A", 5)
            .expect("submit with lost receipt");
        assert_eq!(
            submitted.status,
            WireProjectedOperationStatus::KnowledgeUnavailable
        );
        assert_eq!(submitted.intent_id, Some(1));
        assert_eq!(submitted.receipt, Some(WireReceipt::Indeterminate));
        assert_eq!(
            submitted.knowledge_reason,
            Some("delivery_or_receipt_indeterminate")
        );

        // Once Simulator moves and canonical observations arrive, Engine establishes progress & completion.
        sim.advance(Duration::from_millis(200));
        let moving = sim
            .query_submission("sub-indet-0001", None)
            .expect("query moving");
        assert_eq!(moving.status, WireProjectedOperationStatus::InProgress);
        for _ in 0..15 {
            sim.advance(Duration::from_millis(200));
        }
        let done = sim
            .query_submission("sub-indet-0001", None)
            .expect("query done");
        assert_eq!(done.status, WireProjectedOperationStatus::Completed);
        assert_eq!(done.reconciliation, Some(WireReconciliation::Satisfied));
    }

    #[test]
    fn bounded_registry_caps_size_retains_active_operation_and_reports_eviction_honestly() {
        let mut sim = SimDevControl::default_simulator().expect("default sim");

        // Submit one active operation on Car A.
        let active = sim
            .submit_serve_floor("sub-active-0001", "A", 8)
            .expect("active op");
        assert_eq!(active.status, WireProjectedOperationStatus::Accepted);

        // Flood with 140 rejected submissions (> MAX_SUBMISSION_RECORDS = 128).
        for idx in 0..140_usize {
            let key = format!("sub-reject-{idx:04}");
            let res = sim
                .submit_serve_floor(&key, "A", 8)
                .expect("rejected overlapping");
            assert_eq!(res.status, WireProjectedOperationStatus::Rejected);
        }

        assert_eq!(sim.registry_len(), MAX_SUBMISSION_RECORDS);

        // Active operation MUST still be retained and queryable!
        let active_query = sim
            .query_submission("sub-active-0001", Some(sim.control_instance_id()))
            .expect("active retained");
        assert_eq!(active_query.status, WireProjectedOperationStatus::Accepted);
        assert_eq!(active_query.intent_id, Some(1));

        // Evicted oldest rejected key MUST NOT be claimed as "not_found / never happened"!
        let evicted = sim
            .query_submission("sub-reject-0000", Some(sim.control_instance_id()))
            .expect("evicted query");
        assert_eq!(
            evicted.status,
            WireProjectedOperationStatus::KnowledgeUnavailable
        );
        assert_eq!(
            evicted.knowledge_reason,
            Some("submission_evicted_or_unknown")
        );
    }

    #[test]
    fn http_response_loss_reconnect_query_conflict_restart_and_readonly_safety_proof() {
        let sim1 = SimDevControl::default_simulator().expect("sim 1");
        let instance_1 = sim1.control_instance_id().to_owned();
        let server1 = TestServer::start(sim1, true, Some(25));
        let address = server1.address;

        // 1. Existing read API stays strictly read-only GET (POST -> 405) and omits wildcard CORS.
        let (snap_status, snap_headers, snap_json) = get_json(address, SNAPSHOT_PATH);
        assert_eq!(snap_status, 200);
        assert_eq!(snap_json["api_version"], "experimental/v0");
        assert!(
            snap_headers
                .to_ascii_lowercase()
                .contains("cache-control: no-store")
        );
        assert!(
            !snap_headers
                .to_ascii_lowercase()
                .contains("access-control-allow-origin")
        );

        let (post_snap_status, _, _) = raw_http(
            address,
            &format!(
                "POST {SNAPSHOT_PATH} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        );
        assert_eq!(post_snap_status, 405);
        let (post_events_status, _, _) = raw_http(
            address,
            &format!(
                "POST {EVENTS_PATH} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        );
        assert_eq!(post_events_status, 405);

        // 2. Reject unknown fields, invalid operations, PATCH, and oversized payloads.
        let (bad_op_status, _, bad_op) = post_json(
            address,
            DEV_SERVE_FLOOR_PREVIEW_PATH,
            &json!({"operation": "set_mode", "car": "A", "floor": 4}),
        );
        assert_eq!(bad_op_status, 400);
        assert_eq!(bad_op["error_code"], "unsupported_operation");

        let (extra_field_status, _, extra_field_json) = post_json(
            address,
            DEV_SERVE_FLOOR_PREVIEW_PATH,
            &json!({"operation": "serve_floor", "car": "A", "floor": 4, "extra": 1}),
        );
        assert_eq!(extra_field_status, 422);
        assert_eq!(extra_field_json["error_code"], "invalid_request_json");

        let (patch_status, _, _) = raw_http(
            address,
            &format!(
                "PATCH {DEV_SERVE_FLOOR_SUBMIT_PATH} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        );
        assert_eq!(patch_status, 405);

        let oversized_body = format!(
            "{{\"operation\":\"serve_floor\",\"car\":\"{}\",\"floor\":4}}",
            "A".repeat(5_000)
        );
        let (too_large_status, _, _) = raw_http(
            address,
            &format!(
                "POST {DEV_SERVE_FLOOR_PREVIEW_PATH} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{oversized_body}",
                oversized_body.len()
            ),
        );
        assert_eq!(too_large_status, 413);

        // 3. Capabilities & Preview over real HTTP.
        let (caps_status, _, caps_json) = get_json(address, DEV_CAPABILITIES_PATH);
        assert_eq!(caps_status, 200);
        assert_eq!(caps_json["control_instance_id"], instance_1);

        let (prev_status, _, prev_json) = post_json(
            address,
            DEV_SERVE_FLOOR_PREVIEW_PATH,
            &json!({"operation": "serve_floor", "car": "A", "floor": 6}),
        );
        assert_eq!(prev_status, 200);
        assert_eq!(prev_json["advisory"], true);
        assert_eq!(prev_json["eligible"], true);

        // 4. RESPONSE-LOSS PROOF:
        // Send POST /serve-floor/submit and intentionally drop the client socket immediately
        // without reading the server's response, simulating mid-flight response loss.
        let submission_key = "sub-loss-proof-0001";
        {
            let submit_body = serde_json::to_string(&json!({
                "submission_key": submission_key,
                "operation": "serve_floor",
                "car": "A",
                "floor": 6
            }))
            .unwrap();
            let mut stream = TcpStream::connect(address).expect("connect for submit");
            write!(
                stream,
                "POST {DEV_SERVE_FLOOR_SUBMIT_PATH} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{submit_body}",
                submit_body.len()
            )
            .expect("write submit request");
            stream.flush().expect("flush submit request");
            // Wait briefly for server to accept, then drop without reading response!
            thread::sleep(Duration::from_millis(60));
        }

        // Client reconnects and issues ONLY GET /submissions/{key} (NO resend!) until Completed.
        let query_path = format!(
            "/api/dev-sim/v0/submissions/{submission_key}?expected_instance_id={instance_1}"
        );
        let mut recovered_intent_id = None;
        let mut completed = false;
        for _ in 0..80 {
            let (q_status, _, q_json) = get_json(address, &query_path);
            assert_eq!(q_status, 200);
            if let Some(id) = q_json["intent_id"].as_u64() {
                recovered_intent_id = Some(id);
            }
            if q_json["status"] == "completed" {
                assert_eq!(q_json["reconciliation"], "satisfied");
                completed = true;
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
        assert_eq!(recovered_intent_id, Some(1));
        assert!(
            completed,
            "lost-response submission must reconcile to completed"
        );
        assert_eq!(server1.sim.lock().unwrap().controller_submissions(), 1);

        // 5. Same-key conflict over HTTP returns 409 Conflict without creating a second operation.
        let (conflict_status, _, conflict_json) = post_json(
            address,
            DEV_SERVE_FLOOR_SUBMIT_PATH,
            &json!({
                "submission_key": submission_key,
                "operation": "serve_floor",
                "car": "A",
                "floor": 7
            }),
        );
        assert_eq!(conflict_status, 409);
        assert_eq!(conflict_json["error_code"], "submission_key_conflict");
        assert_eq!(server1.sim.lock().unwrap().controller_submissions(), 1);

        // 6. PROCESS RESTART PROOF:
        // Stop server 1 and start fresh server 2 on the same loopback address (new instance ID).
        server1.stop();
        let sim2 = SimDevControl::default_simulator().expect("sim 2");
        let instance_2 = sim2.control_instance_id().to_owned();
        assert_ne!(instance_1, instance_2);
        let server2 = TestServer::start_on_address(address, sim2, true);

        let (restart_status, _, restart_json) = get_json(address, &query_path);
        assert_eq!(restart_status, 200);
        assert_eq!(restart_json["control_instance_id"], instance_2);
        assert_eq!(restart_json["status"], "knowledge_unavailable");
        assert_eq!(
            restart_json["knowledge_reason"],
            "control_process_restarted"
        );
        assert!(restart_json["intent_id"].is_null());
        assert_eq!(server2.sim.lock().unwrap().controller_submissions(), 0);
        server2.stop();

        // 7. Control disabled by default: control routes return 404 while read API still works.
        let sim3 = SimDevControl::default_simulator().expect("sim 3");
        let server3 = TestServer::start(sim3, false, None);
        let (ro_snap, _, _) = get_json(server3.address, SNAPSHOT_PATH);
        assert_eq!(ro_snap, 200);
        let (disabled_caps, _, _) = raw_http(
            server3.address,
            &format!(
                "GET {DEV_CAPABILITIES_PATH} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                server3.address
            ),
        );
        assert_eq!(disabled_caps, 404);
        server3.stop();
    }

    #[test]
    fn live_revocation_clears_preview_truth_and_controller_reject_or_divergence_project_honestly() {
        let mut sim = SimDevControl::default_simulator().expect("default sim");

        // Live Operational observation revocation clears floor/mode from preview immediately.
        sim.revoke_car_observation(&CarId::new("B"));
        let preview_revoked = sim.preview_serve_floor("B", 3).expect("preview revoked B");
        assert!(!preview_revoked.eligible);
        assert!(!preview_revoked.observed_car_available);
        assert!(preview_revoked.observed_floor_reference.is_none());
        assert!(preview_revoked.observed_operating_mode.is_none());
        assert_eq!(
            preview_revoked.blocker.as_ref().map(|b| b.code),
            Some("observation_unavailable")
        );

        // Lower controller receipt rejection projects Rejected with receipt=rejected.
        sim.set_next_virtual_behavior(VirtualBehavior {
            reject: true,
            ..VirtualBehavior::default()
        });
        let admitted = sim
            .submit_serve_floor("sub-ctrl-rej-01", "A", 5)
            .expect("admitted before controller reject");
        assert_eq!(admitted.status, WireProjectedOperationStatus::Accepted);
        sim.advance(Duration::from_millis(200));
        let ctrl_rejected = sim
            .query_submission("sub-ctrl-rej-01", None)
            .expect("query controller reject");
        assert_eq!(ctrl_rejected.status, WireProjectedOperationStatus::Rejected);
        assert_eq!(ctrl_rejected.receipt, Some(WireReceipt::Rejected));
        assert_eq!(
            ctrl_rejected.blocker.as_ref().map(|b| b.code),
            Some("controller_rejected")
        );

        // Diverged physical execution projects Diverged with observed floor (never Completed).
        sim.set_next_virtual_behavior(VirtualBehavior {
            execution: liftassistant_sim::VirtualExecution::Diverges(Floor::new(7)),
            ..VirtualBehavior::default()
        });
        let div_sub = sim
            .submit_serve_floor("sub-diverge-01", "A", 5)
            .expect("admitted diverging trip");
        assert_eq!(div_sub.status, WireProjectedOperationStatus::Accepted);
        for _ in 0..25 {
            sim.advance(Duration::from_millis(200));
        }
        let diverged = sim
            .query_submission("sub-diverge-01", None)
            .expect("query diverged");
        assert_eq!(diverged.status, WireProjectedOperationStatus::Diverged);
        assert_eq!(diverged.reconciliation, Some(WireReconciliation::Diverged));
        assert_eq!(diverged.diverged_floor, Some(7));
    }

    #[test]
    fn evicted_engine_intent_record_projects_knowledge_unavailable_rather_than_completed_or_failed()
    {
        let mut sim = SimDevControl::from_sim_runtime(
            BuildingTopology::reference_eight_floor(),
            SimRuntime::new(
                [(
                    CarState::with_continuous_position("A", Position::at_floor(Floor::new(3))),
                    ExecutionCapabilities::new(true, false, (1..=8).map(Floor::new)),
                )],
                10.0,
            ),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            256,
        )
        .expect("sim builds");

        // Complete 130 ServeFloor operations (> Engine RECENT_INTENTS = 128) while keeping
        // registry capacity at 256 so the first submission_key stays in SubmissionRegistry
        // after Engine evicts IntentId(1).
        for idx in 0..130_usize {
            let floor = if idx % 2 == 0 { 4 } else { 3 };
            let key = format!("sub-churn-{idx:04}");
            let res = sim
                .submit_serve_floor(&key, "A", floor)
                .expect("submit succeeds");
            assert_eq!(res.status, WireProjectedOperationStatus::Accepted);
            for _ in 0..5 {
                sim.advance(Duration::from_millis(200));
            }
            let done = sim.query_submission(&key, None).expect("query done");
            assert_eq!(done.status, WireProjectedOperationStatus::Completed);
        }

        // IntentId(1) was evicted from Engine's bounded 128-entry recent history -> honest unknown!
        let evicted = sim
            .query_submission("sub-churn-0000", Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"))
            .expect("query evicted engine intent");
        assert_eq!(
            evicted.status,
            WireProjectedOperationStatus::KnowledgeUnavailable
        );
        assert_eq!(evicted.intent_id, Some(1));
        assert_eq!(evicted.knowledge_reason, Some("engine_history_evicted"));
    }

    #[test]
    fn concurrent_http_submits_serialize_idempotency_and_conflicts_without_double_intent() {
        let sim = SimDevControl::default_simulator().expect("default sim");
        let server = TestServer::start(sim, true, None);
        let address = server.address;

        // Warm up listener before launching barrier-synchronized threads.
        let (caps_status, _, _) = get_json(address, DEV_CAPABILITIES_PATH);
        assert_eq!(caps_status, 200);

        // Reject unknown query parameters and mutation on GET-only control routes.
        let (bad_query_status, _, bad_query_json) = get_json(
            address,
            "/api/dev-sim/v0/submissions/sub-valid-0001?unknown_param=1",
        );
        assert_eq!(bad_query_status, 400);
        assert_eq!(bad_query_json["error_code"], "invalid_query");

        let (post_caps_status, _, _) = raw_http(
            address,
            &format!(
                "POST {DEV_CAPABILITIES_PATH} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        );
        assert_eq!(post_caps_status, 405);

        let barrier = Arc::new(std::sync::Barrier::new(12));
        let mut handles = Vec::with_capacity(12);
        for idx in 0..12_usize {
            let barrier = Arc::clone(&barrier);
            let floor: i16 = if idx < 8 { 6 } else { 7 };
            handles.push(thread::spawn(move || {
                barrier.wait();
                let (status, _, body) = post_json(
                    address,
                    DEV_SERVE_FLOOR_SUBMIT_PATH,
                    &json!({
                        "submission_key": "sub-race-0001",
                        "operation": "serve_floor",
                        "car": "A",
                        "floor": floor,
                    }),
                );
                (floor, status, body)
            }));
        }

        let results: Vec<(i16, u16, Value)> = handles
            .into_iter()
            .map(|handle| handle.join().expect("client thread completes"))
            .collect();

        // Exactly ONE side effect / IntentId must have been created in Engine & Simulator!
        assert_eq!(server.sim.lock().unwrap().controller_submissions(), 1);

        let originals: Vec<&(i16, u16, Value)> = results
            .iter()
            .filter(|(_, status, body)| *status == 200 && body["duplicate_submission"] == false)
            .collect();
        assert_eq!(originals.len(), 1, "exactly one non-duplicate winner");
        let winning_floor = originals[0].0;
        assert_eq!(originals[0].2["intent_id"], 1);
        assert_eq!(originals[0].2["status"], "accepted");

        for (floor, status, body) in &results {
            if *floor == winning_floor {
                assert_eq!(*status, 200);
                assert_eq!(body["intent_id"], 1);
                assert_eq!(body["floor"], winning_floor);
            } else {
                assert_eq!(*status, 409);
                assert_eq!(body["error_code"], "submission_key_conflict");
            }
        }

        let expected_dups = if winning_floor == 6 { 7 } else { 3 };
        let duplicate_count = results
            .iter()
            .filter(|(_, status, body)| *status == 200 && body["duplicate_submission"] == true)
            .count();
        assert_eq!(duplicate_count, expected_dups);

        server.stop();
    }
}
