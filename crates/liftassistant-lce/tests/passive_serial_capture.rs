#![cfg(target_os = "linux")]

#[path = "../examples/support/passive_linux.rs"]
mod passive_linux;

use std::{ffi::CStr, path::PathBuf, thread, time::Duration};

use liftassistant_lce::{
    FinalizedLcePassiveCapture, LceCaptureOrigin, LcePassiveAccessMode, LcePassiveCaptureEndReason,
    LcePassiveExclusiveModeStatus, LceTerminalSessionEvent,
};
use passive_linux::{PassiveLinuxRequest, PassiveLinuxTty};
use rustix::{
    fs::{Mode, OFlags, fcntl_getfl, fcntl_setfl, open},
    io::Errno,
    pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt},
};

const SYNTHETIC_RX: &[u8] =
    b"synthetic banner\r\n?\r\nVTEST1\r\n\x00\xff\x11\x13binary\rline\nlast\r\n";

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

#[test]
fn linux_pty_proves_software_rx_only_capture_and_offline_context_replay() {
    let (master, slave_path) = pty_master_and_path();
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

    let after = passive.after_for_test().clone();
    assert_eq!(after.input_speed, 9600);
    assert_eq!(after.output_speed, 9600);
    assert_eq!(after.character_size_bits, 8);
    assert!(after.cs8 && after.cread && after.clocal);
    assert!(!after.parenb && !after.parodd && !after.cstopb);
    assert!(!after.ixon && !after.ixoff && !after.ixany && !after.crtscts);
    assert!(!after.icanon && !after.echo && !after.isig && !after.iexten);
    assert!(!after.icrnl && !after.inlcr && !after.igncr && !after.istrip);
    assert!(!after.opost && !after.hupcl);
    assert_eq!((after.vmin, after.vtime), (0, 0));

    let before = passive.before_for_test().clone();
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
        capture
            .entries()
            .iter()
            .flat_map(|entry| entry.bytes().iter().copied())
            .collect::<Vec<_>>(),
        SYNTHETIC_RX
    );

    let encoded = capture.to_json_bytes().unwrap();
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
