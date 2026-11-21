#![cfg(target_os = "linux")]

use std::{
    fs::File,
    io::{Read, Write},
    time::Duration,
};

use liftassistant_lce::{
    LceCaptureDirection, LceCaptureOrigin, LceCapturePayload, LceDisconnectReason,
    LceInterfaceState, LceReadOnlyQuery, LceRuntimeTerminalProfile, LceTerminalSession,
    LceTerminalSessionEvent, LceTerminalSessionState, LceTranscriptEvent,
};
use liftassistant_model::ObservationSessionId;
use rustix::{
    pty::{OpenptFlags, grantpt, ioctl_tiocgptpeer, openpt, unlockpt},
    termios::{OptionalActions, tcgetattr, tcsetattr},
};

const SYNTHETIC_PROVENANCE: &str = "LIFTASSISTANT_SYNTHETIC";
const STATUS: &[u8] = b"STA STATUS_OF_LIFT 02\r\nADV_POS SPD DIR SRV FLT_ID DOOR LVL_NO AT_DZN TERM_LIM SPDA_VID SPDB_VID LOAD CORR_DRV CC_PREFF AT_LEVEL\r\n03 0 0 0 0 3 03 1 0 1 2 0 0 0 FF\r\n";
const COMMAND: &[u8] = b"CMD COMMAND_OF_LIFT 02\nDEST PLN_DIR DOOR_SIDE OPEN_WDTH OPEN_TIME DEL CLS_MDE\n04 0 0 0 40 0 0\n";
const MODE: &[u8] = b"MDE MODE_OF_LIFT 02\r\nLIFT_MODE SIGNL_GROUP REC_LDG REC_CAR FAULT_ID DOOR_STATE ADV_POS\r\n0 1 1 1 0 3 03\r\n";

struct SyntheticFakeLce {
    controller: File,
}

impl SyntheticFakeLce {
    fn send(&mut self, bytes: &[u8]) {
        self.controller.write_all(bytes).unwrap();
        self.controller.flush().unwrap();
    }

    fn expect_query(&mut self, expected: &[u8]) {
        let mut bytes = vec![0; expected.len()];
        self.controller.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, expected, "{SYNTHETIC_PROVENANCE} query mismatch");
    }
}

fn pty_pair() -> (File, SyntheticFakeLce) {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC).unwrap();
    grantpt(&master).unwrap();
    unlockpt(&master).unwrap();
    let slave = ioctl_tiocgptpeer(
        &master,
        OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC,
    )
    .unwrap();
    let mut settings = tcgetattr(&slave).unwrap();
    settings.make_raw();
    tcsetattr(&slave, OptionalActions::Now, &settings).unwrap();
    (
        File::from(slave),
        SyntheticFakeLce {
            controller: File::from(master),
        },
    )
}

fn synthetic_session(id: &str) -> LceTerminalSession {
    LceTerminalSession::new(
        ObservationSessionId::new(id),
        LceInterfaceState::Unknown,
        LceRuntimeTerminalProfile::historical_9600_8n1(),
        Duration::from_millis(100),
        LceCaptureOrigin::LiftAssistantSynthetic,
    )
    .unwrap()
}

fn receive_exact_in_chunks(
    terminal: &mut File,
    session: &mut LceTerminalSession,
    total: usize,
    chunk_sizes: &[usize],
    clock: &mut u64,
) -> Vec<LceTerminalSessionEvent> {
    let mut remaining = total;
    let mut events = Vec::new();
    let mut index = 0;
    while remaining > 0 {
        let size = chunk_sizes[index % chunk_sizes.len()].min(remaining);
        let mut bytes = vec![0; size];
        terminal.read_exact(&mut bytes).unwrap();
        *clock += 100;
        events.extend(
            session
                .receive(Duration::from_micros(*clock), &bytes)
                .unwrap(),
        );
        remaining -= size;
        index += 1;
    }
    events
}

#[test]
fn real_pty_runs_only_typed_queries_and_replays_exact_evidence() {
    let (mut terminal, mut fake) = pty_pair();
    let mut session = synthetic_session("LIFTASSISTANT_SYNTHETIC/pty-A");
    let mut clock = 0;
    session.connect(Duration::ZERO).unwrap();
    session.transport_ready(Duration::ZERO).unwrap();

    let startup = b"synthetic CPU561-shaped banner only\r\n?\r\nVTEST1\r\n";
    fake.send(startup);
    let startup_events = receive_exact_in_chunks(
        &mut terminal,
        &mut session,
        startup.len(),
        &[1, 3, 2],
        &mut clock,
    );
    assert!(
        startup_events
            .iter()
            .any(|event| matches!(event, LceTerminalSessionEvent::MainLevelBoundary))
    );
    assert!(
        startup_events
            .iter()
            .any(|event| matches!(event, LceTerminalSessionEvent::VTestBoundary))
    );
    assert_eq!(session.state(), &LceTerminalSessionState::VTestReady);
    assert_eq!(session.interface_state(), LceInterfaceState::Available);

    let fixtures = [
        (LceReadOnlyQuery::Status, STATUS, true, vec![1]),
        (
            LceReadOnlyQuery::CommandState,
            COMMAND,
            false,
            vec![COMMAND.len()],
        ),
        (LceReadOnlyQuery::Mode, MODE, true, vec![7, 2, 1, 5]),
    ];
    let mut decoded = 0;
    for (query, response, echo, chunks) in fixtures {
        clock += 100;
        let tx = session
            .issue_query(query, Duration::from_micros(clock))
            .unwrap();
        let accepted = terminal.write(tx.as_bytes()).unwrap();
        assert_eq!(accepted, tx.as_bytes().len());
        session
            .transport_write_accepted(&tx, accepted, Duration::from_micros(clock))
            .unwrap();
        terminal.flush().unwrap();
        fake.expect_query(tx.as_bytes());

        if query == LceReadOnlyQuery::Mode {
            clock += 20_000;
            assert!(
                session
                    .advance_time(Duration::from_micros(clock))
                    .unwrap()
                    .is_empty()
            );
        }
        let mut output = Vec::new();
        if echo {
            output.extend_from_slice(&tx.as_bytes()[..3]);
            output.extend_from_slice(b"\r\n");
        }
        output.extend_from_slice(response);
        fake.send(&output);
        let events = receive_exact_in_chunks(
            &mut terminal,
            &mut session,
            output.len(),
            &chunks,
            &mut clock,
        );
        decoded += events
            .iter()
            .filter(|event| matches!(event, LceTerminalSessionEvent::NativeFrame(_)))
            .count();
        assert_eq!(session.state(), &LceTerminalSessionState::VTestReady);
    }
    assert_eq!(decoded, 3);

    session
        .disconnect(
            Duration::from_micros(clock + 1),
            LceDisconnectReason::SyntheticTestComplete,
        )
        .unwrap();
    let capture = session
        .finalize_capture(Duration::from_micros(clock + 1))
        .unwrap();
    assert_eq!(
        capture.initial_interface_state(),
        LceInterfaceState::Unknown
    );
    assert!(capture.entries().iter().any(|entry| matches!(
        entry.payload(),
        LceCapturePayload::Marker {
            marker: liftassistant_lce::LceCaptureMarker::MainLevelBoundary
        }
    )));
    assert!(capture.entries().iter().any(|entry| matches!(
        entry.payload(),
        LceCapturePayload::Marker {
            marker: liftassistant_lce::LceCaptureMarker::VTestBoundary
        }
    )));
    let tx_bytes = capture
        .entries()
        .iter()
        .filter_map(|entry| match entry.payload() {
            LceCapturePayload::Bytes {
                direction: LceCaptureDirection::TransportAcceptedTx,
                bytes,
            } => Some(bytes.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tx_bytes, [b"STA\r", b"CMD\r", b"MDE\r"]);
    let replay = capture.replay_r1();
    assert_eq!(
        replay
            .iter()
            .filter(|event| matches!(event, LceTranscriptEvent::NativeFrame(_)))
            .count(),
        3
    );
    let encoded = capture.to_json_bytes().unwrap();
    let decoded_capture =
        liftassistant_lce::FinalizedLceEvidenceCapture::from_json_bytes(&encoded).unwrap();
    assert_eq!(capture.sha256(), decoded_capture.sha256());
    assert_eq!(capture.replay_r1(), decoded_capture.replay_r1());
}

#[test]
fn pty_disconnect_fences_old_session_and_reconnect_uses_new_incarnation() {
    let (mut old_terminal, mut old_fake) = pty_pair();
    let mut old = synthetic_session("LIFTASSISTANT_SYNTHETIC/pty-old");
    let mut clock = 0;
    old.connect(Duration::ZERO).unwrap();
    old.transport_ready(Duration::ZERO).unwrap();
    old_fake.send(b"?\r\nVTEST1\r\n");
    receive_exact_in_chunks(&mut old_terminal, &mut old, 11, &[1], &mut clock);
    let tx = old
        .issue_query(LceReadOnlyQuery::Status, Duration::from_micros(clock + 1))
        .unwrap();
    let accepted = old_terminal.write(tx.as_bytes()).unwrap();
    assert_eq!(accepted, tx.as_bytes().len());
    old.transport_write_accepted(&tx, accepted, Duration::from_micros(clock + 2))
        .unwrap();
    old_fake.expect_query(b"STA\r");
    let partial = &STATUS[..STATUS.len() / 2];
    old_fake.send(partial);
    receive_exact_in_chunks(
        &mut old_terminal,
        &mut old,
        partial.len(),
        &[11, 1],
        &mut clock,
    );
    drop(old_fake);
    old.disconnect(
        Duration::from_micros(clock + 1),
        LceDisconnectReason::PeerClosed,
    )
    .unwrap();
    assert!(
        old.receive(Duration::from_micros(clock + 2), &STATUS[partial.len()..])
            .is_err()
    );

    let (mut new_terminal, mut new_fake) = pty_pair();
    let mut new = synthetic_session("LIFTASSISTANT_SYNTHETIC/pty-new");
    let mut new_clock = 0;
    new.connect(Duration::ZERO).unwrap();
    new.transport_ready(Duration::ZERO).unwrap();
    new_fake.send(b"?\r\nVTEST1\r\n");
    receive_exact_in_chunks(&mut new_terminal, &mut new, 11, &[2, 1], &mut new_clock);
    let tx = new
        .issue_query(
            LceReadOnlyQuery::Status,
            Duration::from_micros(new_clock + 1),
        )
        .unwrap();
    let accepted = new_terminal.write(tx.as_bytes()).unwrap();
    assert_eq!(accepted, tx.as_bytes().len());
    new.transport_write_accepted(&tx, accepted, Duration::from_micros(new_clock + 2))
        .unwrap();
    new_fake.expect_query(b"STA\r");
    new_fake.send(STATUS);
    let events = receive_exact_in_chunks(
        &mut new_terminal,
        &mut new,
        STATUS.len(),
        &[13, 2, 5],
        &mut new_clock,
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LceTerminalSessionEvent::NativeFrame(_)))
    );
    assert_ne!(old.session_id(), new.session_id());
}
