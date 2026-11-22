use std::{error::Error, fmt, time::Duration};

use liftassistant_model::ObservationSessionId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    LceCaptureOrigin, LceInterfaceState, LceRuntimeTerminalProfile, LceTerminalSession,
    LceTerminalSessionEvent, LceTerminalSessionState, LceTerminalUseError, LceTranscriptDecoder,
    LceTranscriptEvent, transcript::MAX_INPUT_CHUNK_BYTES,
};

pub const LCE_PASSIVE_CAPTURE_FORMAT_VERSION: u16 = 2;
pub const MAX_PASSIVE_CAPTURE_SESSION_ID_BYTES: usize = 128;
pub const MAX_PASSIVE_DEVICE_PATH_BYTES: usize = 4096;
pub const MAX_PASSIVE_CAPTURE_ENTRIES: usize = 4096;
pub const MAX_PASSIVE_CAPTURE_ENTRY_BYTES: usize = 64 * 1024;
pub const MAX_PASSIVE_CAPTURE_RAW_BYTES: usize = 1024 * 1024;
pub const MAX_SERIALIZED_PASSIVE_CAPTURE_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_PASSIVE_CAPTURE_DURATION_SECONDS: u16 = 300;
pub const LINUX_N_TTY_LINE_DISCIPLINE: i32 = 0;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LcePassiveAcquisitionLayer {
    /// Bytes returned by Linux `read(2)` on the configured tty descriptor.
    /// This is not physical-wire or UART-waveform evidence.
    LinuxTtyDeliveredRx,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LcePassiveAccessMode {
    ReadOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LcePassiveOpenedDeviceKind {
    CharacterDevice,
}

/// Kernel character-device identity reported by `fstat(2)` for the already
/// opened descriptor. It is not a persistent adapter, cable, or controller
/// identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LcePassiveOpenedDevice {
    kind: LcePassiveOpenedDeviceKind,
    major: u32,
    minor: u32,
}

impl LcePassiveOpenedDevice {
    pub const fn character_device(major: u32, minor: u32) -> Self {
        Self {
            kind: LcePassiveOpenedDeviceKind::CharacterDevice,
            major,
            minor,
        }
    }

    pub const fn kind(self) -> LcePassiveOpenedDeviceKind {
        self.kind
    }

    pub const fn major(self) -> u32 {
        self.major
    }

    pub const fn minor(self) -> u32 {
        self.minor
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LcePassiveInputQueueObservation {
    Observed { bytes: u32 },
    Unavailable { raw_os_error: Option<i32> },
}

/// Normalized Linux open policy recorded by the experimental acquisition
/// example. It describes software behavior only and cannot prove physical
/// passivity or absence of another opener before `TIOCEXCL`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LcePassiveOpenPolicy {
    access_mode: LcePassiveAccessMode,
    no_controlling_tty: bool,
    nonblocking: bool,
    close_on_exec: bool,
    exclusive_requested: bool,
    exclusive_observed: bool,
}

impl LcePassiveOpenPolicy {
    pub const fn linux_rx_only(exclusive_observed: bool) -> Self {
        Self {
            access_mode: LcePassiveAccessMode::ReadOnly,
            no_controlling_tty: true,
            nonblocking: true,
            close_on_exec: true,
            exclusive_requested: true,
            exclusive_observed,
        }
    }

    pub const fn access_mode(self) -> LcePassiveAccessMode {
        self.access_mode
    }

    pub const fn no_controlling_tty(self) -> bool {
        self.no_controlling_tty
    }

    pub const fn nonblocking(self) -> bool {
        self.nonblocking
    }

    pub const fn close_on_exec(self) -> bool {
        self.close_on_exec
    }

    pub const fn exclusive_requested(self) -> bool {
        self.exclusive_requested
    }

    pub const fn exclusive_observed(self) -> bool {
        self.exclusive_observed
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LcePassiveExclusiveModeStatus {
    ObservedEnabled,
    RequestedUnverified { raw_os_error: Option<i32> },
}

/// A normalized snapshot of every termios field controlled by the R3A local
/// capture policy. Public fields keep the Linux-only example independent from
/// the crate's normal dependency graph; this remains an internal/experimental
/// artifact rather than a stable public schema.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LceNormalizedTermios {
    pub ignbrk: bool,
    pub brkint: bool,
    pub ignpar: bool,
    pub parmrk: bool,
    pub inpck: bool,
    pub ixon: bool,
    pub ixoff: bool,
    pub ixany: bool,
    pub icrnl: bool,
    pub inlcr: bool,
    pub igncr: bool,
    pub istrip: bool,
    pub iuclc: bool,
    pub icanon: bool,
    pub echo: bool,
    pub echoe: bool,
    pub echonl: bool,
    pub isig: bool,
    pub iexten: bool,
    pub opost: bool,
    pub character_size_bits: u8,
    pub cs8: bool,
    pub parenb: bool,
    pub parodd: bool,
    pub cstopb: bool,
    pub cread: bool,
    pub clocal: bool,
    pub crtscts: bool,
    pub hupcl: bool,
    pub input_speed: u32,
    pub output_speed: u32,
    pub vmin: u8,
    pub vtime: u8,
}

impl LceNormalizedTermios {
    fn is_capture_policy(&self) -> bool {
        !self.ignbrk
            && !self.brkint
            && !self.ignpar
            && !self.parmrk
            && !self.inpck
            && !self.ixon
            && !self.ixoff
            && !self.ixany
            && !self.icrnl
            && !self.inlcr
            && !self.igncr
            && !self.istrip
            && !self.iuclc
            && !self.icanon
            && !self.echo
            && !self.echoe
            && !self.echonl
            && !self.isig
            && !self.iexten
            && !self.opost
            && self.character_size_bits == 8
            && self.cs8
            && !self.parenb
            && !self.parodd
            && !self.cstopb
            && self.cread
            && self.clocal
            && !self.crtscts
            && !self.hupcl
            && self.input_speed == 9600
            && self.output_speed == 9600
            && self.vmin == 0
            && self.vtime == 0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LcePassiveRxEntryKind {
    TtyDeliveredRx,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LcePassiveRxEntry {
    read_observed_at_micros: u64,
    kind: LcePassiveRxEntryKind,
    bytes: Vec<u8>,
}

impl LcePassiveRxEntry {
    /// Relative monotonic time at which the acquisition loop observed this
    /// userspace `read(2)` result. This is not a wire, UART-arrival,
    /// controller-send, or per-byte timestamp.
    pub const fn read_observed_at_micros(&self) -> u64 {
        self.read_observed_at_micros
    }

    pub const fn kind(&self) -> LcePassiveRxEntryKind {
        self.kind
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LcePassiveCaptureEndReason {
    DurationElapsed,
    EndOfFile,
    PeerHangup,
    ReadError { raw_os_error: Option<i32> },
    EntryLimit,
    ChunkLimit,
    AggregateByteLimit,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LcePassiveCaptureMetadata {
    pub session_id: String,
    pub origin: LceCaptureOrigin,
    pub requested_device_path_bytes: Vec<u8>,
    pub opened_device: LcePassiveOpenedDevice,
    pub requested_terminal_profile: LceRuntimeTerminalProfile,
    pub requested_duration_seconds: u16,
    pub normalized_open_policy: LcePassiveOpenPolicy,
    pub normalized_termios_before: LceNormalizedTermios,
    pub normalized_termios_after: LceNormalizedTermios,
    pub exclusive_mode_status: LcePassiveExclusiveModeStatus,
    pub line_discipline_before: i32,
    pub line_discipline_after: i32,
    pub pre_capture_input_queue: LcePassiveInputQueueObservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct LcePassiveCaptureBody {
    format_version: u16,
    session_id: String,
    origin: LceCaptureOrigin,
    acquisition_layer: LcePassiveAcquisitionLayer,
    requested_device_path_bytes: Vec<u8>,
    opened_device: LcePassiveOpenedDevice,
    requested_terminal_profile: LceRuntimeTerminalProfile,
    requested_duration_seconds: u16,
    normalized_open_policy: LcePassiveOpenPolicy,
    normalized_termios_before: LceNormalizedTermios,
    normalized_termios_after: LceNormalizedTermios,
    exclusive_mode_status: LcePassiveExclusiveModeStatus,
    line_discipline_before: i32,
    line_discipline_after: i32,
    pre_capture_input_queue: LcePassiveInputQueueObservation,
    entries: Vec<LcePassiveRxEntry>,
    ended_at_micros: u64,
    end_reason: LcePassiveCaptureEndReason,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct LcePassiveCaptureEnvelope {
    capture: LcePassiveCaptureBody,
    sha256: String,
}

#[derive(Deserialize)]
struct LcePassiveCaptureVersionProbe {
    capture: LcePassiveCaptureVersionBody,
}

#[derive(Deserialize)]
struct LcePassiveCaptureVersionBody {
    format_version: u16,
}

/// A bounded, finalized raw acquisition artifact. Its JSON shape is internal
/// and experimental. SHA-256 detects mutation; it does not authenticate the
/// source or establish chain of custody.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizedLcePassiveCapture {
    body: LcePassiveCaptureBody,
    sha256: String,
}

impl FinalizedLcePassiveCapture {
    pub fn session_id(&self) -> &str {
        &self.body.session_id
    }

    pub const fn origin(&self) -> LceCaptureOrigin {
        self.body.origin
    }

    pub const fn acquisition_layer(&self) -> LcePassiveAcquisitionLayer {
        self.body.acquisition_layer
    }

    pub fn requested_device_path_bytes(&self) -> &[u8] {
        &self.body.requested_device_path_bytes
    }

    pub const fn opened_device(&self) -> LcePassiveOpenedDevice {
        self.body.opened_device
    }

    pub const fn requested_terminal_profile(&self) -> LceRuntimeTerminalProfile {
        self.body.requested_terminal_profile
    }

    pub const fn requested_duration_seconds(&self) -> u16 {
        self.body.requested_duration_seconds
    }

    pub const fn normalized_open_policy(&self) -> LcePassiveOpenPolicy {
        self.body.normalized_open_policy
    }

    pub const fn normalized_termios_before(&self) -> &LceNormalizedTermios {
        &self.body.normalized_termios_before
    }

    pub const fn normalized_termios_after(&self) -> &LceNormalizedTermios {
        &self.body.normalized_termios_after
    }

    pub const fn exclusive_mode_status(&self) -> LcePassiveExclusiveModeStatus {
        self.body.exclusive_mode_status
    }

    pub const fn line_discipline_before(&self) -> i32 {
        self.body.line_discipline_before
    }

    pub const fn line_discipline_after(&self) -> i32 {
        self.body.line_discipline_after
    }

    pub const fn pre_capture_input_queue(&self) -> LcePassiveInputQueueObservation {
        self.body.pre_capture_input_queue
    }

    pub fn entries(&self) -> &[LcePassiveRxEntry] {
        &self.body.entries
    }

    pub const fn ended_at_micros(&self) -> u64 {
        self.body.ended_at_micros
    }

    pub const fn end_reason(&self) -> LcePassiveCaptureEndReason {
        self.body.end_reason
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>, LcePassiveCaptureError> {
        let bytes = serde_json::to_vec(&LcePassiveCaptureEnvelope {
            capture: self.body.clone(),
            sha256: self.sha256.clone(),
        })
        .map_err(|error| LcePassiveCaptureError::Serialization(error.to_string()))?;
        if bytes.len() > MAX_SERIALIZED_PASSIVE_CAPTURE_BYTES {
            return Err(LcePassiveCaptureError::SerializedCaptureTooLarge { bytes: bytes.len() });
        }
        Ok(bytes)
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, LcePassiveCaptureError> {
        if bytes.len() > MAX_SERIALIZED_PASSIVE_CAPTURE_BYTES {
            return Err(LcePassiveCaptureError::SerializedCaptureTooLarge { bytes: bytes.len() });
        }
        let version = serde_json::from_slice::<LcePassiveCaptureVersionProbe>(bytes)
            .map_err(|error| LcePassiveCaptureError::Serialization(error.to_string()))?
            .capture
            .format_version;
        if version != LCE_PASSIVE_CAPTURE_FORMAT_VERSION {
            return Err(LcePassiveCaptureError::UnsupportedVersion { version });
        }
        let envelope = serde_json::from_slice::<LcePassiveCaptureEnvelope>(bytes)
            .map_err(|error| LcePassiveCaptureError::Serialization(error.to_string()))?;
        validate_body(&envelope.capture)?;
        let expected = body_sha256(&envelope.capture)?;
        if envelope.sha256 != expected {
            return Err(LcePassiveCaptureError::IntegrityMismatch);
        }
        Ok(Self {
            body: envelope.capture,
            sha256: envelope.sha256,
        })
    }

    /// Replays exactly the tty-delivered RX entries through a fresh R1 decoder.
    pub fn replay_r1(&self) -> Vec<LceTranscriptEvent> {
        let mut decoder = LceTranscriptDecoder::new();
        let mut events = Vec::new();
        for entry in &self.body.entries {
            for chunk in entry.bytes.chunks(MAX_INPUT_CHUNK_BYTES) {
                events.extend(decoder.push(chunk));
            }
        }
        events.extend(decoder.finish());
        events
    }

    /// Replays RX only through the existing R2.3 terminal-context classifier.
    /// It never calls `issue_query` and therefore cannot produce terminal TX.
    pub fn replay_r2_context(&self) -> Result<Vec<LceTerminalSessionEvent>, LceTerminalUseError> {
        let mut session = LceTerminalSession::new(
            ObservationSessionId::new(self.session_id()),
            LceInterfaceState::Unknown,
            self.requested_terminal_profile(),
            Duration::from_secs(1),
            self.origin(),
        )?;
        let mut events = session.connect(Duration::ZERO)?;
        events.extend(session.transport_ready(Duration::ZERO)?);
        for entry in &self.body.entries {
            if matches!(session.state(), LceTerminalSessionState::FailedClosed(_)) {
                break;
            }
            events.extend(session.receive(
                Duration::from_micros(entry.read_observed_at_micros),
                &entry.bytes,
            )?);
        }
        Ok(events)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LcePassiveCaptureError {
    SessionIdEmpty,
    SessionIdTooLarge { bytes: usize },
    InvalidRequestedDevicePath,
    DevicePathTooLarge { bytes: usize },
    InvalidDuration { seconds: u16 },
    InvalidOpenPolicy,
    InvalidExclusiveStatus,
    InvalidCaptureTermios,
    UnsupportedLineDiscipline { observed: i32 },
    InconsistentDurationElapsed { expected: u64, actual: u64 },
    NonMonotonicTime { previous: u64, next: u64 },
    TimeBeyondRequestedDuration { micros: u64 },
    TooManyEntries,
    EntryTooLarge { bytes: usize },
    RawCaptureTooLarge { bytes: usize },
    SerializedCaptureTooLarge { bytes: usize },
    UnsupportedVersion { version: u16 },
    InvalidProfile,
    InvalidCapture(&'static str),
    IntegrityMismatch,
    Serialization(String),
}

impl fmt::Display for LcePassiveCaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionIdEmpty => f.write_str("passive capture session ID is empty"),
            Self::SessionIdTooLarge { bytes } => write!(
                f,
                "passive capture session ID has {bytes} bytes; maximum is {MAX_PASSIVE_CAPTURE_SESSION_ID_BYTES}"
            ),
            Self::InvalidRequestedDevicePath => f.write_str(
                "passive capture requested device path must be nonempty, absolute, and contain no NUL",
            ),
            Self::DevicePathTooLarge { bytes } => write!(
                f,
                "passive capture requested device path has {bytes} bytes; maximum is {MAX_PASSIVE_DEVICE_PATH_BYTES}"
            ),
            Self::InvalidDuration { seconds } => write!(
                f,
                "passive capture duration is {seconds} seconds; expected 1..={MAX_PASSIVE_CAPTURE_DURATION_SECONDS}"
            ),
            Self::InvalidOpenPolicy => {
                f.write_str("passive capture open policy is not Linux software RX-only")
            }
            Self::InvalidExclusiveStatus => f.write_str(
                "passive capture exclusive-mode status contradicts the observed open policy",
            ),
            Self::InvalidCaptureTermios => f.write_str(
                "passive capture observed termios does not match the explicit local capture policy",
            ),
            Self::UnsupportedLineDiscipline { observed } => write!(
                f,
                "passive capture observed unsupported tty line discipline {observed}; expected Linux N_TTY (0)"
            ),
            Self::InconsistentDurationElapsed { expected, actual } => write!(
                f,
                "DurationElapsed ended at {actual}us; expected the requested duration boundary {expected}us"
            ),
            Self::NonMonotonicTime { previous, next } => write!(
                f,
                "passive capture time moved backwards from {previous}us to {next}us"
            ),
            Self::TimeBeyondRequestedDuration { micros } => write!(
                f,
                "passive capture timestamp {micros}us exceeds the requested duration"
            ),
            Self::TooManyEntries => write!(
                f,
                "passive capture exceeds {MAX_PASSIVE_CAPTURE_ENTRIES} entries"
            ),
            Self::EntryTooLarge { bytes } => write!(
                f,
                "passive capture entry has {bytes} bytes; maximum is {MAX_PASSIVE_CAPTURE_ENTRY_BYTES}"
            ),
            Self::RawCaptureTooLarge { bytes } => write!(
                f,
                "passive capture has {bytes} raw bytes; maximum is {MAX_PASSIVE_CAPTURE_RAW_BYTES}"
            ),
            Self::SerializedCaptureTooLarge { bytes } => write!(
                f,
                "serialized passive capture has {bytes} bytes; maximum is {MAX_SERIALIZED_PASSIVE_CAPTURE_BYTES}"
            ),
            Self::UnsupportedVersion { version } => {
                write!(f, "unsupported passive capture version {version}")
            }
            Self::InvalidProfile => {
                f.write_str("passive capture has an unsupported terminal profile")
            }
            Self::InvalidCapture(reason) => write!(f, "invalid passive capture: {reason}"),
            Self::IntegrityMismatch => f.write_str("passive capture SHA-256 mismatch"),
            Self::Serialization(error) => {
                write!(f, "passive capture serialization failed: {error}")
            }
        }
    }
}

impl Error for LcePassiveCaptureError {}

/// A bounded RX evidence recorder. This type has no fd, transport, command,
/// write, modem-line, or query API.
///
/// ```compile_fail
/// # use liftassistant_lce::LcePassiveCaptureRecorder;
/// # let mut recorder: LcePassiveCaptureRecorder = todo!();
/// recorder.write_all(b"STA\r");
/// ```
///
/// ```compile_fail
/// # use liftassistant_lce::LcePassiveCaptureRecorder;
/// # let mut recorder: LcePassiveCaptureRecorder = todo!();
/// recorder.write(b"STA\r");
/// ```
///
/// ```compile_fail
/// # use liftassistant_lce::LcePassiveCaptureRecorder;
/// # let mut recorder: LcePassiveCaptureRecorder = todo!();
/// recorder.send_raw(b"STA\r");
/// ```
///
/// ```compile_fail
/// # use liftassistant_lce::LcePassiveCaptureRecorder;
/// # let mut recorder: LcePassiveCaptureRecorder = todo!();
/// recorder.issue_query("STA");
/// ```
///
/// ```compile_fail
/// # use liftassistant_lce::LcePassiveCaptureRecorder;
/// # let mut recorder: LcePassiveCaptureRecorder = todo!();
/// recorder.command(String::from("STA"));
/// ```
///
/// ```compile_fail
/// # use liftassistant_lce::LcePassiveCaptureRecorder;
/// # let mut recorder: LcePassiveCaptureRecorder = todo!();
/// recorder.transmit(b"STA\r");
/// ```
///
/// ```compile_fail
/// # use liftassistant_lce::LcePassiveCaptureRecorder;
/// # let mut recorder: LcePassiveCaptureRecorder = todo!();
/// recorder.set_rts(true);
/// ```
///
/// ```compile_fail
/// # use liftassistant_lce::LcePassiveCaptureRecorder;
/// # let mut recorder: LcePassiveCaptureRecorder = todo!();
/// recorder.set_dtr(true);
/// ```
#[derive(Debug)]
pub struct LcePassiveCaptureRecorder {
    body: LcePassiveCaptureBody,
    last_read_observed_at_micros: u64,
    raw_bytes: usize,
}

impl LcePassiveCaptureRecorder {
    pub fn new(metadata: LcePassiveCaptureMetadata) -> Result<Self, LcePassiveCaptureError> {
        let body = LcePassiveCaptureBody {
            format_version: LCE_PASSIVE_CAPTURE_FORMAT_VERSION,
            session_id: metadata.session_id,
            origin: metadata.origin,
            acquisition_layer: LcePassiveAcquisitionLayer::LinuxTtyDeliveredRx,
            requested_device_path_bytes: metadata.requested_device_path_bytes,
            opened_device: metadata.opened_device,
            requested_terminal_profile: metadata.requested_terminal_profile,
            requested_duration_seconds: metadata.requested_duration_seconds,
            normalized_open_policy: metadata.normalized_open_policy,
            normalized_termios_before: metadata.normalized_termios_before,
            normalized_termios_after: metadata.normalized_termios_after,
            exclusive_mode_status: metadata.exclusive_mode_status,
            line_discipline_before: metadata.line_discipline_before,
            line_discipline_after: metadata.line_discipline_after,
            pre_capture_input_queue: metadata.pre_capture_input_queue,
            entries: Vec::new(),
            ended_at_micros: 0,
            end_reason: LcePassiveCaptureEndReason::DurationElapsed,
        };
        validate_metadata(&body)?;
        Ok(Self {
            body,
            last_read_observed_at_micros: 0,
            raw_bytes: 0,
        })
    }

    pub fn record_tty_delivered_rx(
        &mut self,
        at: Duration,
        bytes: &[u8],
    ) -> Result<(), LcePassiveCaptureError> {
        if bytes.is_empty() {
            return Err(LcePassiveCaptureError::InvalidCapture(
                "empty read result must end acquisition as EOF",
            ));
        }
        if bytes.len() > MAX_PASSIVE_CAPTURE_ENTRY_BYTES {
            return Err(LcePassiveCaptureError::EntryTooLarge { bytes: bytes.len() });
        }
        if self.body.entries.len() >= MAX_PASSIVE_CAPTURE_ENTRIES {
            return Err(LcePassiveCaptureError::TooManyEntries);
        }
        let total = self
            .raw_bytes
            .checked_add(bytes.len())
            .ok_or(LcePassiveCaptureError::RawCaptureTooLarge { bytes: usize::MAX })?;
        if total > MAX_PASSIVE_CAPTURE_RAW_BYTES {
            return Err(LcePassiveCaptureError::RawCaptureTooLarge { bytes: total });
        }
        let read_observed_at_micros = duration_micros(at)?;
        validate_offset(
            self.last_read_observed_at_micros,
            read_observed_at_micros,
            self.body.requested_duration_seconds,
        )?;
        self.body.entries.push(LcePassiveRxEntry {
            read_observed_at_micros,
            kind: LcePassiveRxEntryKind::TtyDeliveredRx,
            bytes: bytes.to_vec(),
        });
        self.last_read_observed_at_micros = read_observed_at_micros;
        self.raw_bytes = total;
        Ok(())
    }

    pub fn finalize(
        mut self,
        at: Duration,
        reason: LcePassiveCaptureEndReason,
    ) -> Result<FinalizedLcePassiveCapture, LcePassiveCaptureError> {
        let ended_at_micros = duration_micros(at)?;
        validate_offset(
            self.last_read_observed_at_micros,
            ended_at_micros,
            self.body.requested_duration_seconds,
        )?;
        self.body.ended_at_micros = ended_at_micros;
        self.body.end_reason = reason;
        validate_body(&self.body)?;
        let sha256 = body_sha256(&self.body)?;
        Ok(FinalizedLcePassiveCapture {
            body: self.body,
            sha256,
        })
    }
}

fn validate_metadata(body: &LcePassiveCaptureBody) -> Result<(), LcePassiveCaptureError> {
    if body.session_id.is_empty() {
        return Err(LcePassiveCaptureError::SessionIdEmpty);
    }
    if body.session_id.len() > MAX_PASSIVE_CAPTURE_SESSION_ID_BYTES {
        return Err(LcePassiveCaptureError::SessionIdTooLarge {
            bytes: body.session_id.len(),
        });
    }
    if body.requested_device_path_bytes.len() > MAX_PASSIVE_DEVICE_PATH_BYTES {
        return Err(LcePassiveCaptureError::DevicePathTooLarge {
            bytes: body.requested_device_path_bytes.len(),
        });
    }
    if body.requested_device_path_bytes.first() != Some(&b'/')
        || body.requested_device_path_bytes.contains(&0)
    {
        return Err(LcePassiveCaptureError::InvalidRequestedDevicePath);
    }
    if body.requested_duration_seconds == 0
        || body.requested_duration_seconds > MAX_PASSIVE_CAPTURE_DURATION_SECONDS
    {
        return Err(LcePassiveCaptureError::InvalidDuration {
            seconds: body.requested_duration_seconds,
        });
    }
    if body.requested_terminal_profile != LceRuntimeTerminalProfile::historical_9600_8n1() {
        return Err(LcePassiveCaptureError::InvalidProfile);
    }
    let policy = body.normalized_open_policy;
    if policy.access_mode != LcePassiveAccessMode::ReadOnly
        || !policy.no_controlling_tty
        || !policy.nonblocking
        || !policy.close_on_exec
        || !policy.exclusive_requested
    {
        return Err(LcePassiveCaptureError::InvalidOpenPolicy);
    }
    let status_observed = matches!(
        body.exclusive_mode_status,
        LcePassiveExclusiveModeStatus::ObservedEnabled
    );
    if policy.exclusive_observed != status_observed {
        return Err(LcePassiveCaptureError::InvalidExclusiveStatus);
    }
    if !body.normalized_termios_after.is_capture_policy() {
        return Err(LcePassiveCaptureError::InvalidCaptureTermios);
    }
    for observed in [body.line_discipline_before, body.line_discipline_after] {
        if observed != LINUX_N_TTY_LINE_DISCIPLINE {
            return Err(LcePassiveCaptureError::UnsupportedLineDiscipline { observed });
        }
    }
    Ok(())
}

fn validate_body(body: &LcePassiveCaptureBody) -> Result<(), LcePassiveCaptureError> {
    if body.format_version != LCE_PASSIVE_CAPTURE_FORMAT_VERSION {
        return Err(LcePassiveCaptureError::UnsupportedVersion {
            version: body.format_version,
        });
    }
    if body.acquisition_layer != LcePassiveAcquisitionLayer::LinuxTtyDeliveredRx {
        return Err(LcePassiveCaptureError::InvalidCapture(
            "unsupported acquisition layer",
        ));
    }
    validate_metadata(body)?;
    if body.entries.len() > MAX_PASSIVE_CAPTURE_ENTRIES {
        return Err(LcePassiveCaptureError::TooManyEntries);
    }
    let mut previous = 0;
    let mut total = 0usize;
    for entry in &body.entries {
        validate_offset(
            previous,
            entry.read_observed_at_micros,
            body.requested_duration_seconds,
        )?;
        if entry.read_observed_at_micros > body.ended_at_micros {
            return Err(LcePassiveCaptureError::InvalidCapture(
                "entry timestamp exceeds capture end",
            ));
        }
        if entry.kind != LcePassiveRxEntryKind::TtyDeliveredRx {
            return Err(LcePassiveCaptureError::InvalidCapture(
                "unsupported passive entry kind",
            ));
        }
        if entry.bytes.is_empty() {
            return Err(LcePassiveCaptureError::InvalidCapture(
                "passive RX entry is empty",
            ));
        }
        if entry.bytes.len() > MAX_PASSIVE_CAPTURE_ENTRY_BYTES {
            return Err(LcePassiveCaptureError::EntryTooLarge {
                bytes: entry.bytes.len(),
            });
        }
        total = total
            .checked_add(entry.bytes.len())
            .ok_or(LcePassiveCaptureError::RawCaptureTooLarge { bytes: usize::MAX })?;
        previous = entry.read_observed_at_micros;
    }
    if total > MAX_PASSIVE_CAPTURE_RAW_BYTES {
        return Err(LcePassiveCaptureError::RawCaptureTooLarge { bytes: total });
    }
    validate_offset(
        previous,
        body.ended_at_micros,
        body.requested_duration_seconds,
    )?;
    if body.end_reason == LcePassiveCaptureEndReason::DurationElapsed {
        let expected = u64::from(body.requested_duration_seconds) * 1_000_000;
        if body.ended_at_micros != expected {
            return Err(LcePassiveCaptureError::InconsistentDurationElapsed {
                expected,
                actual: body.ended_at_micros,
            });
        }
    }
    Ok(())
}

fn validate_offset(
    previous: u64,
    next: u64,
    requested_duration_seconds: u16,
) -> Result<(), LcePassiveCaptureError> {
    if next < previous {
        return Err(LcePassiveCaptureError::NonMonotonicTime { previous, next });
    }
    if next > u64::from(requested_duration_seconds) * 1_000_000 {
        return Err(LcePassiveCaptureError::TimeBeyondRequestedDuration { micros: next });
    }
    Ok(())
}

fn duration_micros(value: Duration) -> Result<u64, LcePassiveCaptureError> {
    u64::try_from(value.as_micros()).map_err(|_| {
        LcePassiveCaptureError::InvalidCapture("relative time exceeds u64 microseconds")
    })
}

fn body_sha256(body: &LcePassiveCaptureBody) -> Result<String, LcePassiveCaptureError> {
    let bytes = serde_json::to_vec(body)
        .map_err(|error| LcePassiveCaptureError::Serialization(error.to_string()))?;
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut text, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture_termios() -> LceNormalizedTermios {
        LceNormalizedTermios {
            ignbrk: false,
            brkint: false,
            ignpar: false,
            parmrk: false,
            inpck: false,
            ixon: false,
            ixoff: false,
            ixany: false,
            icrnl: false,
            inlcr: false,
            igncr: false,
            istrip: false,
            iuclc: false,
            icanon: false,
            echo: false,
            echoe: false,
            echonl: false,
            isig: false,
            iexten: false,
            opost: false,
            character_size_bits: 8,
            cs8: true,
            parenb: false,
            parodd: false,
            cstopb: false,
            cread: true,
            clocal: true,
            crtscts: false,
            hupcl: false,
            input_speed: 9600,
            output_speed: 9600,
            vmin: 0,
            vtime: 0,
        }
    }

    fn recorder() -> LcePassiveCaptureRecorder {
        let after = capture_termios();
        LcePassiveCaptureRecorder::new(LcePassiveCaptureMetadata {
            session_id: "LIFTASSISTANT_SYNTHETIC/passive-A".to_owned(),
            origin: LceCaptureOrigin::LiftAssistantSynthetic,
            requested_device_path_bytes: b"/dev/pts/synthetic-A".to_vec(),
            opened_device: LcePassiveOpenedDevice::character_device(136, 7),
            requested_terminal_profile: LceRuntimeTerminalProfile::historical_9600_8n1(),
            requested_duration_seconds: 1,
            normalized_open_policy: LcePassiveOpenPolicy::linux_rx_only(true),
            normalized_termios_before: after.clone(),
            normalized_termios_after: after,
            exclusive_mode_status: LcePassiveExclusiveModeStatus::ObservedEnabled,
            line_discipline_before: 0,
            line_discipline_after: 0,
            pre_capture_input_queue: LcePassiveInputQueueObservation::Observed { bytes: 0 },
        })
        .unwrap()
    }

    #[test]
    fn raw_artifact_round_trips_and_replays_context_without_tx() {
        let mut recorder = recorder();
        recorder
            .record_tty_delivered_rx(Duration::from_micros(10), b"banner\r\n?\r\n")
            .unwrap();
        recorder
            .record_tty_delivered_rx(Duration::from_micros(20), b"VTEST1\r\n")
            .unwrap();
        recorder
            .record_tty_delivered_rx(Duration::from_micros(30), b"\x00\xff\r\n")
            .unwrap();
        let capture = recorder
            .finalize(
                Duration::from_micros(40),
                LcePassiveCaptureEndReason::EndOfFile,
            )
            .unwrap();
        let bytes = capture.to_json_bytes().unwrap();
        let decoded = FinalizedLcePassiveCapture::from_json_bytes(&bytes).unwrap();
        assert_eq!(capture, decoded);
        assert_eq!(capture.sha256().len(), 64);
        let events = decoded.replay_r2_context().unwrap();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LceTerminalSessionEvent::MainLevelBoundary))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LceTerminalSessionEvent::VTestBoundary))
        );
    }

    #[test]
    fn artifact_enforces_duration_and_chunk_bounds() {
        let mut recorder = recorder();
        assert!(matches!(
            recorder.record_tty_delivered_rx(
                Duration::ZERO,
                &vec![0; MAX_PASSIVE_CAPTURE_ENTRY_BYTES + 1]
            ),
            Err(LcePassiveCaptureError::EntryTooLarge { .. })
        ));
        assert!(matches!(
            recorder.record_tty_delivered_rx(Duration::from_secs(2), b"late"),
            Err(LcePassiveCaptureError::TimeBeyondRequestedDuration { .. })
        ));
    }

    #[test]
    fn artifact_enforces_aggregate_and_entry_count_bounds() {
        let mut aggregate = recorder();
        for index in 0..(MAX_PASSIVE_CAPTURE_RAW_BYTES / MAX_PASSIVE_CAPTURE_ENTRY_BYTES) {
            aggregate
                .record_tty_delivered_rx(
                    Duration::from_micros(index as u64),
                    &vec![0; MAX_PASSIVE_CAPTURE_ENTRY_BYTES],
                )
                .unwrap();
        }
        assert!(matches!(
            aggregate.record_tty_delivered_rx(Duration::from_micros(100), b"overflow"),
            Err(LcePassiveCaptureError::RawCaptureTooLarge { .. })
        ));
        let aggregate = aggregate
            .finalize(
                Duration::from_micros(101),
                LcePassiveCaptureEndReason::AggregateByteLimit,
            )
            .unwrap();
        assert!(aggregate.to_json_bytes().unwrap().len() <= MAX_SERIALIZED_PASSIVE_CAPTURE_BYTES);

        let mut entries = recorder();
        for index in 0..MAX_PASSIVE_CAPTURE_ENTRIES {
            entries
                .record_tty_delivered_rx(Duration::from_micros(index as u64), b"x")
                .unwrap();
        }
        assert_eq!(
            entries.record_tty_delivered_rx(Duration::from_micros(5000), b"x"),
            Err(LcePassiveCaptureError::TooManyEntries)
        );
    }

    #[test]
    fn artifact_checksum_detects_valid_json_mutation() {
        let mut recorder = recorder();
        recorder
            .record_tty_delivered_rx(Duration::from_micros(10), b"original")
            .unwrap();
        let capture = recorder
            .finalize(
                Duration::from_micros(20),
                LcePassiveCaptureEndReason::EndOfFile,
            )
            .unwrap();
        let text = String::from_utf8(capture.to_json_bytes().unwrap()).unwrap();
        let tampered = text.replace("passive-A", "passive-B");
        assert_eq!(
            FinalizedLcePassiveCapture::from_json_bytes(tampered.as_bytes()),
            Err(LcePassiveCaptureError::IntegrityMismatch)
        );
    }

    #[test]
    fn passive_v1_is_rejected_without_compatibility_alias() {
        assert_eq!(
            FinalizedLcePassiveCapture::from_json_bytes(
                br#"{"capture":{"format_version":1},"sha256":"legacy-v1"}"#
            ),
            Err(LcePassiveCaptureError::UnsupportedVersion { version: 1 })
        );
    }

    #[test]
    fn source_and_line_discipline_metadata_fail_closed() {
        let mut invalid_path = recorder();
        invalid_path.body.requested_device_path_bytes = b"relative/tty".to_vec();
        invalid_path.body.end_reason = LcePassiveCaptureEndReason::EndOfFile;
        let invalid_path_envelope = LcePassiveCaptureEnvelope {
            sha256: body_sha256(&invalid_path.body).unwrap(),
            capture: invalid_path.body,
        };
        assert_eq!(
            FinalizedLcePassiveCapture::from_json_bytes(
                &serde_json::to_vec(&invalid_path_envelope).unwrap()
            ),
            Err(LcePassiveCaptureError::InvalidRequestedDevicePath)
        );

        let mut unsupported_line_discipline = recorder();
        unsupported_line_discipline.body.line_discipline_after = 1;
        unsupported_line_discipline.body.end_reason = LcePassiveCaptureEndReason::EndOfFile;
        let unsupported_line_discipline_envelope = LcePassiveCaptureEnvelope {
            sha256: body_sha256(&unsupported_line_discipline.body).unwrap(),
            capture: unsupported_line_discipline.body,
        };
        assert_eq!(
            FinalizedLcePassiveCapture::from_json_bytes(
                &serde_json::to_vec(&unsupported_line_discipline_envelope).unwrap()
            ),
            Err(LcePassiveCaptureError::UnsupportedLineDiscipline { observed: 1 })
        );
    }

    #[test]
    fn duration_elapsed_requires_the_exact_requested_boundary() {
        assert_eq!(
            recorder().finalize(
                Duration::from_micros(999_999),
                LcePassiveCaptureEndReason::DurationElapsed,
            ),
            Err(LcePassiveCaptureError::InconsistentDurationElapsed {
                expected: 1_000_000,
                actual: 999_999,
            })
        );
        assert!(
            recorder()
                .finalize(
                    Duration::from_secs(1),
                    LcePassiveCaptureEndReason::DurationElapsed,
                )
                .is_ok()
        );
    }
}
