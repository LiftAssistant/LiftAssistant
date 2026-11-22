#![cfg(target_os = "linux")]

#[path = "../examples/support/passive_linux.rs"]
mod passive_linux;
#[path = "../examples/support/passive_runner.rs"]
mod passive_runner;

use std::{
    ffi::CStr,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use liftassistant_lce::{
    FinalizedLcePassiveCapture, LINUX_N_TTY_LINE_DISCIPLINE, LceCaptureOrigin,
    LcePassiveAccessMode, LcePassiveCaptureEndReason, LcePassiveCaptureError,
    LcePassiveExclusiveModeStatus, LcePassiveInputQueueObservation, LceTerminalSessionEvent,
};
use passive_linux::{PassiveLinuxRequest, PassiveLinuxTty};
use rustix::{
    fs::{Mode, OFlags, fcntl_getfl, fcntl_setfl, fstat, major, minor, open},
    io::Errno,
    pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt},
    termios::{InputModes, LocalModes, OptionalActions, tcgetattr, tcsetattr},
};

const SYNTHETIC_RX: &[u8] =
    b"synthetic banner\r\n?\r\nVTEST1\r\n\x00\xff\x11\x13binary\rline\nlast\r\n";
static TEST_PATH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn unique_test_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "liftassistant-lce-{label}-{}-{}",
        std::process::id(),
        TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn runner_arguments() -> Vec<String> {
    [
        "--assurance",
        "software-rx-only",
        "--profile",
        "historical-9600-8n1",
        "--device",
        "/nonexistent/liftassistant-lce-tty",
        "--output",
        "/tmp/liftassistant-lce-capture.json",
        "--duration-seconds",
        "1",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn remove_argument_pair(arguments: &mut Vec<String>, name: &str) {
    let index = arguments
        .iter()
        .position(|argument| argument == name)
        .unwrap();
    arguments.drain(index..=index + 1);
}

fn replace_argument_value(arguments: &mut [String], name: &str, value: &str) {
    let index = arguments
        .iter()
        .position(|argument| argument == name)
        .unwrap();
    arguments[index + 1] = value.to_owned();
}

#[test]
fn runner_requires_the_single_software_assurance_and_historical_profile() {
    let mut missing_assurance = runner_arguments();
    remove_argument_pair(&mut missing_assurance, "--assurance");
    assert!(
        passive_runner::parse_arguments(missing_assurance)
            .unwrap_err()
            .to_string()
            .contains("missing --assurance")
    );

    let mut invalid_assurance = runner_arguments();
    replace_argument_value(&mut invalid_assurance, "--assurance", "physical-rx-only");
    assert!(
        passive_runner::parse_arguments(invalid_assurance)
            .unwrap_err()
            .to_string()
            .contains("unsupported --assurance")
    );

    let mut missing_profile = runner_arguments();
    remove_argument_pair(&mut missing_profile, "--profile");
    assert!(
        passive_runner::parse_arguments(missing_profile)
            .unwrap_err()
            .to_string()
            .contains("missing --profile")
    );

    let mut invalid_profile = runner_arguments();
    replace_argument_value(&mut invalid_profile, "--profile", "115200-8n1");
    assert!(
        passive_runner::parse_arguments(invalid_profile)
            .unwrap_err()
            .to_string()
            .contains("unsupported --profile")
    );
}

#[test]
fn runner_keeps_absolute_device_and_duration_bounds() {
    let mut relative_device = runner_arguments();
    replace_argument_value(&mut relative_device, "--device", "ttyUSB0");
    assert!(
        passive_runner::parse_arguments(relative_device)
            .unwrap_err()
            .to_string()
            .contains("absolute path")
    );

    for invalid_duration in ["0", "301"] {
        let mut arguments = runner_arguments();
        replace_argument_value(&mut arguments, "--duration-seconds", invalid_duration);
        assert!(
            passive_runner::parse_arguments(arguments)
                .unwrap_err()
                .to_string()
                .contains("1..=300")
        );
    }
}

#[test]
fn runner_plan_validates_configuration_without_opening_or_creating_files() {
    let output_path = unique_test_path("plan");
    let mut arguments = runner_arguments();
    replace_argument_value(
        &mut arguments,
        "--device",
        "/deliberately/nonexistent/lce-plan-tty",
    );
    replace_argument_value(&mut arguments, "--output", output_path.to_str().unwrap());
    arguments.push("--plan".to_owned());

    let parsed = passive_runner::parse_arguments(arguments.clone()).unwrap();
    let plan = passive_runner::plan_summary(&parsed);
    assert!(plan.contains("mode: PLAN_ONLY"));
    assert!(plan.contains("profile: historical-9600-8n1"));
    assert!(plan.contains("assurance: SOFTWARE_RX_ONLY"));
    assert!(plan.contains("active_protocol_tx: false"));
    assert!(plan.contains("physically_passive: false/unproven"));
    assert!(plan.contains("real_hardware_opened: false"));

    passive_runner::run_with_arguments(arguments).unwrap();
    assert!(!output_path.exists());
}

#[test]
fn runner_removes_reserved_output_after_normal_acquisition_error() {
    let output_path = unique_test_path("cleanup");
    let mut arguments = runner_arguments();
    replace_argument_value(
        &mut arguments,
        "--device",
        "/deliberately/nonexistent/lce-execution-tty",
    );
    replace_argument_value(&mut arguments, "--output", output_path.to_str().unwrap());

    assert!(passive_runner::run_with_arguments(arguments).is_err());
    assert!(!output_path.exists());
}

#[test]
fn runner_preserves_existing_output_before_acquisition() {
    let output_path = unique_test_path("no-overwrite");
    fs::write(&output_path, b"existing evidence").unwrap();
    let mut arguments = runner_arguments();
    replace_argument_value(
        &mut arguments,
        "--device",
        "/deliberately/nonexistent/lce-execution-tty",
    );
    replace_argument_value(&mut arguments, "--output", output_path.to_str().unwrap());

    assert!(passive_runner::run_with_arguments(arguments).is_err());
    assert_eq!(fs::read(&output_path).unwrap(), b"existing evidence");
    fs::remove_file(output_path).unwrap();
}

fn write_all(fd: &impl std::os::fd::AsFd, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let written = rustix::io::write(fd, bytes).unwrap();
        assert_ne!(written, 0);
        bytes = &bytes[written..];
    }
}

fn pty_master_and_path() -> (std::os::fd::OwnedFd, PathBuf) {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC).unwrap();
    let master_flags = fcntl_getfl(&master).unwrap();
    fcntl_setfl(&master, master_flags | OFlags::NONBLOCK).unwrap();
    grantpt(&master).unwrap();
    unlockpt(&master).unwrap();
    let path = ptsname(&master, Vec::new()).unwrap();
    let path = CStr::from_bytes_with_nul(path.as_bytes_with_nul())
        .unwrap()
        .to_string_lossy()
        .into_owned();
    (master, PathBuf::from(path))
}

fn configure_inherited_input_policy(slave_path: &Path) {
    let setup = open(
        slave_path,
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap();
    let mut inherited = tcgetattr(&setup).unwrap();
    inherited.input_modes.insert(
        InputModes::IGNPAR
            | InputModes::INPCK
            | InputModes::IXOFF
            | InputModes::IXANY
            | InputModes::IUCLC,
    );
    inherited
        .local_modes
        .insert(LocalModes::ECHO | LocalModes::ICANON);
    tcsetattr(&setup, OptionalActions::Now, &inherited).unwrap();
}

#[test]
fn linux_pty_proves_software_rx_only_capture_and_offline_context_replay() {
    let (master, slave_path) = pty_master_and_path();
    configure_inherited_input_policy(&slave_path);
    let passive = PassiveLinuxTty::open(&slave_path).unwrap();

    let flags = fcntl_getfl(passive.borrowed_fd_for_test()).unwrap();
    assert_eq!(flags & OFlags::ACCMODE, OFlags::RDONLY);
    assert_eq!(
        rustix::io::write(passive.borrowed_fd_for_test(), b"forbidden synthetic TX"),
        Err(Errno::BADF)
    );
    assert_eq!(
        passive.exclusive_status_for_test(),
        LcePassiveExclusiveModeStatus::ObservedEnabled
    );
    assert_eq!(
        passive.line_disciplines_for_test(),
        (LINUX_N_TTY_LINE_DISCIPLINE, LINUX_N_TTY_LINE_DISCIPLINE)
    );
    assert!(matches!(
        passive.pre_capture_input_queue_for_test(),
        LcePassiveInputQueueObservation::Observed { .. }
    ));
    let opened_stat = fstat(passive.borrowed_fd_for_test()).unwrap();
    let opened_device = passive.opened_device_for_test();
    assert_eq!(opened_device.major(), major(opened_stat.st_rdev));
    assert_eq!(opened_device.minor(), minor(opened_stat.st_rdev));

    match open(
        &slave_path,
        OFlags::RDONLY | OFlags::NOCTTY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Err(Errno::BUSY) => {}
        Ok(second) if unsafe { libc::geteuid() } == 0 => {
            drop(second);
            eprintln!(
                "typed skip: privileged root/CAP_SYS_ADMIN may bypass TIOCEXCL's EBUSY fence"
            );
        }
        Ok(second) => {
            drop(second);
            panic!("TIOCEXCL was observed but a later ordinary open unexpectedly succeeded");
        }
        Err(error) => panic!("second open failed with {error}, expected EBUSY"),
    }

    let before = passive.before_for_test().clone();
    assert!(before.ignpar && before.inpck && before.ixoff && before.ixany);
    assert!(before.iuclc && before.echo && before.icanon);

    let after = passive.after_for_test().clone();
    assert_eq!(after.input_speed, 9600);
    assert_eq!(after.output_speed, 9600);
    assert_eq!(after.character_size_bits, 8);
    assert!(after.cs8 && after.cread && after.clocal);
    assert!(!after.parenb && !after.parodd && !after.cstopb);
    assert!(!after.ignbrk && !after.brkint && !after.ignpar && !after.parmrk && !after.inpck);
    assert!(!after.ixon && !after.ixoff && !after.ixany && !after.crtscts);
    assert!(!after.icanon && !after.echo && !after.isig && !after.iexten);
    assert!(!after.icrnl && !after.inlcr && !after.igncr && !after.istrip && !after.iuclc);
    assert!(!after.opost && !after.hupcl);
    assert_eq!((after.vmin, after.vtime), (0, 0));

    let capture_path = slave_path.clone();
    let capture_thread = thread::spawn(move || {
        passive
            .capture(PassiveLinuxRequest {
                device: &capture_path,
                session_id: "LIFTASSISTANT_SYNTHETIC/r3a-pty-A",
                origin: LceCaptureOrigin::LiftAssistantSynthetic,
                duration_seconds: 1,
            })
            .unwrap()
    });

    thread::sleep(Duration::from_millis(20));
    write_all(&master, &SYNTHETIC_RX[..17]);
    thread::sleep(Duration::from_millis(10));
    write_all(&master, &SYNTHETIC_RX[17..32]);
    thread::sleep(Duration::from_millis(10));
    write_all(&master, &SYNTHETIC_RX[32..]);
    thread::sleep(Duration::from_millis(50));

    let mut unexpected_tx = [0_u8; 256];
    assert_eq!(
        rustix::io::read(&master, &mut unexpected_tx),
        Err(Errno::AGAIN)
    );

    let capture = capture_thread.join().unwrap();
    assert_eq!(
        capture.end_reason(),
        LcePassiveCaptureEndReason::DurationElapsed
    );
    assert_eq!(
        capture.normalized_open_policy().access_mode(),
        LcePassiveAccessMode::ReadOnly
    );
    assert!(capture.normalized_open_policy().no_controlling_tty());
    assert!(capture.normalized_open_policy().nonblocking());
    assert!(capture.normalized_open_policy().close_on_exec());
    assert!(capture.normalized_open_policy().exclusive_requested());
    assert!(capture.normalized_open_policy().exclusive_observed());
    assert_eq!(capture.normalized_termios_before(), &before);
    assert_eq!(capture.normalized_termios_after(), &after);
    assert_eq!(
        capture.requested_device_path_bytes(),
        slave_path.as_os_str().as_bytes()
    );
    assert_eq!(capture.opened_device(), opened_device);
    assert_eq!(
        (
            capture.line_discipline_before(),
            capture.line_discipline_after()
        ),
        (LINUX_N_TTY_LINE_DISCIPLINE, LINUX_N_TTY_LINE_DISCIPLINE)
    );
    assert!(matches!(
        capture.pre_capture_input_queue(),
        LcePassiveInputQueueObservation::Observed { .. }
    ));
    assert_eq!(
        capture
            .entries()
            .iter()
            .flat_map(|entry| entry.bytes().iter().copied())
            .collect::<Vec<_>>(),
        SYNTHETIC_RX
    );
    let mut previous = 0;
    for entry in capture.entries() {
        assert!(entry.read_observed_at_micros() >= previous);
        assert!(entry.read_observed_at_micros() <= capture.ended_at_micros());
        assert!(
            entry.read_observed_at_micros()
                <= u64::from(capture.requested_duration_seconds()) * 1_000_000
        );
        previous = entry.read_observed_at_micros();
    }

    let encoded = capture.to_json_bytes().unwrap();
    let encoded_value = serde_json::from_slice::<serde_json::Value>(&encoded).unwrap();
    let encoded_entry = &encoded_value["capture"]["entries"][0];
    assert!(encoded_entry.get("read_observed_at_micros").is_some());
    assert!(encoded_entry.get("offset_micros").is_none());
    let decoded = FinalizedLcePassiveCapture::from_json_bytes(&encoded).unwrap();
    assert_eq!(capture, decoded);
    assert_eq!(capture.sha256().len(), 64);
    assert!(!capture.replay_r1().is_empty());
    let replay = capture.replay_r2_context().unwrap();
    assert!(
        replay
            .iter()
            .any(|event| matches!(event, LceTerminalSessionEvent::MainLevelBoundary))
    );
    assert!(
        replay
            .iter()
            .any(|event| matches!(event, LceTerminalSessionEvent::VTestBoundary))
    );

    let mut tampered_path = encoded_value.clone();
    tampered_path["capture"]["requested_device_path_bytes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::Value::from(u64::from(b'X')));
    assert_eq!(
        FinalizedLcePassiveCapture::from_json_bytes(&serde_json::to_vec(&tampered_path).unwrap()),
        Err(LcePassiveCaptureError::IntegrityMismatch)
    );

    let mut tampered_device = encoded_value;
    let major = tampered_device["capture"]["opened_device"]["major"]
        .as_u64()
        .unwrap();
    tampered_device["capture"]["opened_device"]["major"] = serde_json::Value::from(major + 1);
    assert_eq!(
        FinalizedLcePassiveCapture::from_json_bytes(&serde_json::to_vec(&tampered_device).unwrap()),
        Err(LcePassiveCaptureError::IntegrityMismatch)
    );
}

#[test]
fn linux_pty_peer_disconnect_ends_the_one_shot_capture_early() {
    const BEFORE_DISCONNECT: &[u8] = b"synthetic evidence before disconnect\r\n";

    let (master, slave_path) = pty_master_and_path();
    let passive = PassiveLinuxTty::open(&slave_path).unwrap();
    let capture_path = slave_path.clone();
    let capture_thread = thread::spawn(move || {
        passive
            .capture(PassiveLinuxRequest {
                device: &capture_path,
                session_id: "LIFTASSISTANT_SYNTHETIC/r3a-pty-disconnect",
                origin: LceCaptureOrigin::LiftAssistantSynthetic,
                duration_seconds: 2,
            })
            .unwrap()
    });

    thread::sleep(Duration::from_millis(20));
    write_all(&master, BEFORE_DISCONNECT);
    thread::sleep(Duration::from_millis(50));
    drop(master);

    let capture = capture_thread.join().unwrap();
    assert!(
        capture.ended_at_micros() < 2_000_000,
        "peer disconnect must terminate this acquisition instead of waiting for its deadline"
    );
    assert!(
        matches!(
            capture.end_reason(),
            LcePassiveCaptureEndReason::PeerHangup
                | LcePassiveCaptureEndReason::EndOfFile
                | LcePassiveCaptureEndReason::ReadError { .. }
        ),
        "Linux PTYs may surface peer closure as HUP, EOF, or EIO"
    );
    assert_eq!(
        capture
            .entries()
            .iter()
            .flat_map(|entry| entry.bytes().iter().copied())
            .collect::<Vec<_>>(),
        BEFORE_DISCONNECT
    );
}
