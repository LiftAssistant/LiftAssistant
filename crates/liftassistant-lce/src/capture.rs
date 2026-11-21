use std::{error::Error, fmt, time::Duration};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    LceDisconnectReason, LceInterfaceState, LceReadOnlyQuery, LceRuntimeTerminalProfile,
    LceTerminalFailureKind, LceTranscriptDecoder, LceTranscriptEvent,
    transcript::MAX_INPUT_CHUNK_BYTES,
};

pub const LCE_CAPTURE_FORMAT_VERSION: u16 = 2;

const MAX_CAPTURE_SESSION_ID_BYTES: usize = 128;
const MAX_CAPTURE_ENTRIES: usize = 4096;
const MAX_CAPTURE_ENTRY_BYTES: usize = 64 * 1024;
const MAX_CAPTURE_RAW_BYTES: usize = 1024 * 1024;
const MAX_SERIALIZED_CAPTURE_BYTES: usize = 5 * 1024 * 1024;

/// Provenance classification for a capture. R2 tests use only the synthetic PTY
/// value; a future field acquisition remains an unvalidated evidence candidate
/// until separately reviewed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LceCaptureOrigin {
    LiftAssistantSynthetic,
    FieldEvidenceCandidate,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LceCaptureDirection {
    Tx,
    Rx,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LceCaptureMarker {
    SessionStarted,
    TransportReady,
    KnownBoundary,
    QueryIssued { query: LceReadOnlyQuery },
    EchoObserved { query: LceReadOnlyQuery },
    NativeFrame { query: LceReadOnlyQuery },
    UnknownText,
    TranscriptRejected,
    FailedClosed { reason: LceTerminalFailureKind },
    Disconnected { reason: LceDisconnectReason },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LceCapturePayload {
    Bytes {
        direction: LceCaptureDirection,
        bytes: Vec<u8>,
    },
    Marker {
        marker: LceCaptureMarker,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LceCaptureEntry {
    offset_micros: u64,
    query: Option<LceReadOnlyQuery>,
    payload: LceCapturePayload,
}

impl LceCaptureEntry {
    pub const fn offset_micros(&self) -> u64 {
        self.offset_micros
    }

    pub const fn query(&self) -> Option<LceReadOnlyQuery> {
        self.query
    }

    pub const fn payload(&self) -> &LceCapturePayload {
        &self.payload
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LceCaptureEndReason {
    Completed,
    Disconnected { reason: LceDisconnectReason },
    FailedClosed { reason: LceTerminalFailureKind },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct LceEvidenceCaptureBody {
    format_version: u16,
    session_id: String,
    origin: LceCaptureOrigin,
    initial_interface_state: LceInterfaceState,
    profile: LceRuntimeTerminalProfile,
    entries: Vec<LceCaptureEntry>,
    ended_at_micros: u64,
    end_reason: LceCaptureEndReason,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct LceEvidenceCaptureEnvelope {
    capture: LceEvidenceCaptureBody,
    sha256: String,
}

/// A bounded, finalized, internal evidence artifact. Its JSON representation is
/// versioned but experimental; it is not a stable/public protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizedLceEvidenceCapture {
    body: LceEvidenceCaptureBody,
    sha256: String,
}

impl FinalizedLceEvidenceCapture {
    pub fn session_id(&self) -> &str {
        &self.body.session_id
    }

    pub const fn origin(&self) -> LceCaptureOrigin {
        self.body.origin
    }

    /// Interface knowledge supplied by the caller when the session was
    /// constructed. Later availability evidence remains in capture markers.
    pub const fn initial_interface_state(&self) -> LceInterfaceState {
        self.body.initial_interface_state
    }

    pub const fn profile(&self) -> LceRuntimeTerminalProfile {
        self.body.profile
    }

    pub fn entries(&self) -> &[LceCaptureEntry] {
        &self.body.entries
    }

    pub const fn end_reason(&self) -> LceCaptureEndReason {
        self.body.end_reason
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>, LceCaptureError> {
        let bytes = serde_json::to_vec_pretty(&LceEvidenceCaptureEnvelope {
            capture: self.body.clone(),
            sha256: self.sha256.clone(),
        })
        .map_err(|error| LceCaptureError::Serialization(error.to_string()))?;
        if bytes.len() > MAX_SERIALIZED_CAPTURE_BYTES {
            return Err(LceCaptureError::SerializedCaptureTooLarge { bytes: bytes.len() });
        }
        Ok(bytes)
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, LceCaptureError> {
        if bytes.len() > MAX_SERIALIZED_CAPTURE_BYTES {
            return Err(LceCaptureError::SerializedCaptureTooLarge { bytes: bytes.len() });
        }
        let envelope = serde_json::from_slice::<LceEvidenceCaptureEnvelope>(bytes)
            .map_err(|error| LceCaptureError::Serialization(error.to_string()))?;
        validate_body(&envelope.capture)?;
        let expected = body_sha256(&envelope.capture)?;
        if envelope.sha256 != expected {
            return Err(LceCaptureError::IntegrityMismatch);
        }
        Ok(Self {
            body: envelope.capture,
            sha256: envelope.sha256,
        })
    }

    /// Replays only received bytes through a fresh R1 decoder. TX and marker
    /// entries remain evidence but never become decoder input.
    pub fn replay_r1(&self) -> Vec<LceTranscriptEvent> {
        let mut decoder = LceTranscriptDecoder::new();
        let mut events = Vec::new();
        for entry in &self.body.entries {
            let LceCapturePayload::Bytes {
                direction: LceCaptureDirection::Rx,
                bytes,
            } = &entry.payload
            else {
                continue;
            };
            for chunk in bytes.chunks(MAX_INPUT_CHUNK_BYTES) {
                events.extend(decoder.push(chunk));
            }
        }
        events.extend(decoder.finish());
        events
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LceCaptureError {
    SessionIdTooLarge { bytes: usize },
    NonMonotonicTime { previous: u64, next: u64 },
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

impl fmt::Display for LceCaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionIdTooLarge { bytes } => write!(
                f,
                "LCE evidence session ID has {bytes} bytes; maximum is {MAX_CAPTURE_SESSION_ID_BYTES}"
            ),
            Self::NonMonotonicTime { previous, next } => write!(
                f,
                "LCE evidence time moved backwards from {previous}us to {next}us"
            ),
            Self::TooManyEntries => write!(
                f,
                "LCE evidence capture exceeds {MAX_CAPTURE_ENTRIES} entries"
            ),
            Self::EntryTooLarge { bytes } => write!(
                f,
                "LCE evidence entry has {bytes} raw bytes; maximum is {MAX_CAPTURE_ENTRY_BYTES}"
            ),
            Self::RawCaptureTooLarge { bytes } => write!(
                f,
                "LCE evidence capture has {bytes} raw bytes; maximum is {MAX_CAPTURE_RAW_BYTES}"
            ),
            Self::SerializedCaptureTooLarge { bytes } => write!(
                f,
                "serialized LCE evidence capture has {bytes} bytes; maximum is {MAX_SERIALIZED_CAPTURE_BYTES}"
            ),
            Self::UnsupportedVersion { version } => {
                write!(f, "unsupported LCE evidence capture version {version}")
            }
            Self::InvalidProfile => f.write_str("LCE evidence capture has an unsupported profile"),
            Self::InvalidCapture(reason) => write!(f, "invalid LCE evidence capture: {reason}"),
            Self::IntegrityMismatch => f.write_str("LCE evidence capture SHA-256 mismatch"),
            Self::Serialization(error) => write!(f, "LCE evidence serialization failed: {error}"),
        }
    }
}

impl Error for LceCaptureError {}

#[derive(Debug)]
pub(crate) struct LceEvidenceRecorder {
    body: LceEvidenceCaptureBody,
    last_offset_micros: u64,
    raw_bytes: usize,
}

impl LceEvidenceRecorder {
    pub(crate) fn new(
        session_id: &str,
        origin: LceCaptureOrigin,
        initial_interface_state: LceInterfaceState,
        profile: LceRuntimeTerminalProfile,
    ) -> Result<Self, LceCaptureError> {
        if session_id.len() > MAX_CAPTURE_SESSION_ID_BYTES {
            return Err(LceCaptureError::SessionIdTooLarge {
                bytes: session_id.len(),
            });
        }
        Ok(Self {
            body: LceEvidenceCaptureBody {
                format_version: LCE_CAPTURE_FORMAT_VERSION,
                session_id: session_id.to_owned(),
                origin,
                initial_interface_state,
                profile,
                entries: Vec::new(),
                ended_at_micros: 0,
                end_reason: LceCaptureEndReason::Completed,
            },
            last_offset_micros: 0,
            raw_bytes: 0,
        })
    }

    pub(crate) fn record_bytes(
        &mut self,
        at: Duration,
        query: Option<LceReadOnlyQuery>,
        direction: LceCaptureDirection,
        bytes: &[u8],
    ) -> Result<(), LceCaptureError> {
        if bytes.len() > MAX_CAPTURE_ENTRY_BYTES {
            return Err(LceCaptureError::EntryTooLarge { bytes: bytes.len() });
        }
        let total = self
            .raw_bytes
            .checked_add(bytes.len())
            .ok_or(LceCaptureError::RawCaptureTooLarge { bytes: usize::MAX })?;
        if total > MAX_CAPTURE_RAW_BYTES {
            return Err(LceCaptureError::RawCaptureTooLarge { bytes: total });
        }
        self.push_entry(
            at,
            query,
            LceCapturePayload::Bytes {
                direction,
                bytes: bytes.to_vec(),
            },
        )?;
        self.raw_bytes = total;
        Ok(())
    }

    pub(crate) fn record_marker(
        &mut self,
        at: Duration,
        query: Option<LceReadOnlyQuery>,
        marker: LceCaptureMarker,
    ) -> Result<(), LceCaptureError> {
        self.push_entry(at, query, LceCapturePayload::Marker { marker })
    }

    fn push_entry(
        &mut self,
        at: Duration,
        query: Option<LceReadOnlyQuery>,
        payload: LceCapturePayload,
    ) -> Result<(), LceCaptureError> {
        let offset_micros = duration_micros(at)?;
        if offset_micros < self.last_offset_micros {
            return Err(LceCaptureError::NonMonotonicTime {
                previous: self.last_offset_micros,
                next: offset_micros,
            });
        }
        if self.body.entries.len() == MAX_CAPTURE_ENTRIES {
            return Err(LceCaptureError::TooManyEntries);
        }
        self.body.entries.push(LceCaptureEntry {
            offset_micros,
            query,
            payload,
        });
        self.last_offset_micros = offset_micros;
        Ok(())
    }

    pub(crate) fn finalize(
        mut self,
        at: Duration,
        reason: LceCaptureEndReason,
    ) -> Result<FinalizedLceEvidenceCapture, LceCaptureError> {
        let ended_at_micros = duration_micros(at)?;
        if ended_at_micros < self.last_offset_micros {
            return Err(LceCaptureError::NonMonotonicTime {
                previous: self.last_offset_micros,
                next: ended_at_micros,
            });
        }
        self.body.ended_at_micros = ended_at_micros;
        self.body.end_reason = reason;
        validate_body(&self.body)?;
        let sha256 = body_sha256(&self.body)?;
        Ok(FinalizedLceEvidenceCapture {
            body: self.body,
            sha256,
        })
    }
}

fn duration_micros(value: Duration) -> Result<u64, LceCaptureError> {
    u64::try_from(value.as_micros())
        .map_err(|_| LceCaptureError::InvalidCapture("relative time exceeds u64 microseconds"))
}

fn body_sha256(body: &LceEvidenceCaptureBody) -> Result<String, LceCaptureError> {
    let bytes = serde_json::to_vec(body)
        .map_err(|error| LceCaptureError::Serialization(error.to_string()))?;
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut text, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(text)
}

fn validate_body(body: &LceEvidenceCaptureBody) -> Result<(), LceCaptureError> {
    if body.format_version != LCE_CAPTURE_FORMAT_VERSION {
        return Err(LceCaptureError::UnsupportedVersion {
            version: body.format_version,
        });
    }
    if body.session_id.len() > MAX_CAPTURE_SESSION_ID_BYTES {
        return Err(LceCaptureError::SessionIdTooLarge {
            bytes: body.session_id.len(),
        });
    }
    if body.profile != LceRuntimeTerminalProfile::historical_9600_8n1() {
        return Err(LceCaptureError::InvalidProfile);
    }
    if body.entries.len() > MAX_CAPTURE_ENTRIES {
        return Err(LceCaptureError::TooManyEntries);
    }
    let mut previous = 0;
    let mut total = 0usize;
    let mut known_boundary_seen = false;
    for entry in &body.entries {
        if entry.offset_micros < previous || entry.offset_micros > body.ended_at_micros {
            return Err(LceCaptureError::InvalidCapture(
                "entry timestamps are not monotonic and bounded by capture end",
            ));
        }
        previous = entry.offset_micros;
        if let LceCapturePayload::Marker { marker } = &entry.payload {
            match marker {
                LceCaptureMarker::KnownBoundary => {
                    if body.initial_interface_state == LceInterfaceState::LockedOrUnavailable {
                        return Err(LceCaptureError::InvalidCapture(
                            "a locked/unavailable initial interface cannot observe a known boundary",
                        ));
                    }
                    known_boundary_seen = true;
                }
                LceCaptureMarker::QueryIssued { .. } if !known_boundary_seen => {
                    return Err(LceCaptureError::InvalidCapture(
                        "query issued before a known boundary",
                    ));
                }
                _ => {}
            }
        }
        if let LceCapturePayload::Bytes { direction, bytes } = &entry.payload {
            if bytes.len() > MAX_CAPTURE_ENTRY_BYTES {
                return Err(LceCaptureError::EntryTooLarge { bytes: bytes.len() });
            }
            total = total
                .checked_add(bytes.len())
                .ok_or(LceCaptureError::RawCaptureTooLarge { bytes: usize::MAX })?;
            if matches!(direction, LceCaptureDirection::Tx) && entry.query.is_none() {
                return Err(LceCaptureError::InvalidCapture(
                    "TX bytes are not associated with a typed query",
                ));
            }
            if matches!(direction, LceCaptureDirection::Tx) {
                if !known_boundary_seen {
                    return Err(LceCaptureError::InvalidCapture(
                        "TX bytes precede a known boundary",
                    ));
                }
                let query = entry.query.expect("TX query presence checked above");
                if bytes.as_slice() != query.command() {
                    return Err(LceCaptureError::InvalidCapture(
                        "TX bytes do not match the associated typed query",
                    ));
                }
            }
        }
    }
    if total > MAX_CAPTURE_RAW_BYTES {
        return Err(LceCaptureError::RawCaptureTooLarge { bytes: total });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        LceCaptureDirection, LceCaptureEndReason, LceCaptureMarker, LceCaptureOrigin,
        LceInterfaceState, LceReadOnlyQuery, LceRuntimeTerminalProfile,
        capture::{LceCaptureError, LceEvidenceRecorder},
    };

    #[test]
    fn finalized_capture_round_trips_and_detects_tampering() {
        let mut recorder = LceEvidenceRecorder::new(
            "fixture-session",
            LceCaptureOrigin::LiftAssistantSynthetic,
            LceInterfaceState::Unknown,
            LceRuntimeTerminalProfile::historical_9600_8n1(),
        )
        .unwrap();
        recorder
            .record_marker(Duration::ZERO, None, LceCaptureMarker::SessionStarted)
            .unwrap();
        recorder
            .record_marker(
                Duration::from_micros(1),
                None,
                LceCaptureMarker::TransportReady,
            )
            .unwrap();
        recorder
            .record_marker(
                Duration::from_micros(2),
                None,
                LceCaptureMarker::KnownBoundary,
            )
            .unwrap();
        recorder
            .record_bytes(
                Duration::from_millis(1),
                Some(LceReadOnlyQuery::Status),
                LceCaptureDirection::Tx,
                b"STA\r",
            )
            .unwrap();
        recorder
            .record_marker(
                Duration::from_millis(1),
                Some(LceReadOnlyQuery::Status),
                LceCaptureMarker::QueryIssued {
                    query: LceReadOnlyQuery::Status,
                },
            )
            .unwrap();
        recorder
            .record_bytes(
                Duration::from_millis(2),
                Some(LceReadOnlyQuery::Status),
                LceCaptureDirection::Rx,
                b"STA\r\n",
            )
            .unwrap();
        let capture = recorder
            .finalize(Duration::from_millis(3), LceCaptureEndReason::Completed)
            .unwrap();
        assert_eq!(capture.sha256().len(), 64);
        assert_eq!(capture.origin(), LceCaptureOrigin::LiftAssistantSynthetic);
        assert_eq!(
            capture.initial_interface_state(),
            LceInterfaceState::Unknown
        );
        assert_eq!(
            capture.profile(),
            LceRuntimeTerminalProfile::historical_9600_8n1()
        );
        assert_eq!(capture.end_reason(), LceCaptureEndReason::Completed);
        let encoded = capture.to_json_bytes().unwrap();
        let encoded_text = std::str::from_utf8(&encoded).unwrap();
        assert!(encoded_text.contains("\"format_version\": 2"));
        assert!(encoded_text.contains("\"initial_interface_state\": \"UNKNOWN\""));
        assert!(!encoded_text.contains("\"interface_state\":"));
        let decoded = super::FinalizedLceEvidenceCapture::from_json_bytes(&encoded).unwrap();
        assert_eq!(capture, decoded);

        let mut wrong_version = serde_json::from_slice::<serde_json::Value>(&encoded).unwrap();
        wrong_version["capture"]["format_version"] = serde_json::json!(1);
        let wrong_version = serde_json::to_vec(&wrong_version).unwrap();
        assert!(matches!(
            super::FinalizedLceEvidenceCapture::from_json_bytes(&wrong_version),
            Err(LceCaptureError::UnsupportedVersion { version: 1 })
        ));

        let mut tampered = encoded;
        let marker = b"\"sha256\": \"";
        let position = tampered
            .windows(marker.len())
            .position(|window| window == marker)
            .expect("capture envelope contains a checksum")
            + marker.len();
        tampered[position] = if tampered[position] == b'0' {
            b'1'
        } else {
            b'0'
        };
        assert!(matches!(
            super::FinalizedLceEvidenceCapture::from_json_bytes(&tampered),
            Err(LceCaptureError::Serialization(_)) | Err(LceCaptureError::IntegrityMismatch)
        ));
    }

    #[test]
    fn capture_validation_rejects_non_allowlisted_tx_bytes() {
        let mut recorder = LceEvidenceRecorder::new(
            "fixture-session",
            LceCaptureOrigin::LiftAssistantSynthetic,
            LceInterfaceState::Available,
            LceRuntimeTerminalProfile::historical_9600_8n1(),
        )
        .unwrap();
        recorder
            .record_marker(Duration::ZERO, None, LceCaptureMarker::KnownBoundary)
            .unwrap();
        recorder
            .record_bytes(
                Duration::ZERO,
                Some(LceReadOnlyQuery::Status),
                LceCaptureDirection::Tx,
                b"CAL\r",
            )
            .unwrap();
        assert!(matches!(
            recorder.finalize(Duration::ZERO, LceCaptureEndReason::Completed),
            Err(LceCaptureError::InvalidCapture(
                "TX bytes do not match the associated typed query"
            ))
        ));
    }

    #[test]
    fn capture_validation_requires_a_prior_known_boundary_for_queries_and_tx() {
        fn encoded_artifact(
            initial_interface_state: LceInterfaceState,
            entries: Vec<super::LceCaptureEntry>,
        ) -> Vec<u8> {
            let body = super::LceEvidenceCaptureBody {
                format_version: super::LCE_CAPTURE_FORMAT_VERSION,
                session_id: "fixture-session".to_owned(),
                origin: LceCaptureOrigin::LiftAssistantSynthetic,
                initial_interface_state,
                profile: LceRuntimeTerminalProfile::historical_9600_8n1(),
                entries,
                ended_at_micros: 4,
                end_reason: LceCaptureEndReason::Completed,
            };
            let sha256 = super::body_sha256(&body).unwrap();
            serde_json::to_vec(&super::LceEvidenceCaptureEnvelope {
                capture: body,
                sha256,
            })
            .unwrap()
        }

        fn marker(
            offset_micros: u64,
            query: Option<LceReadOnlyQuery>,
            marker: LceCaptureMarker,
        ) -> super::LceCaptureEntry {
            super::LceCaptureEntry {
                offset_micros,
                query,
                payload: super::LceCapturePayload::Marker { marker },
            }
        }

        fn tx(offset_micros: u64) -> super::LceCaptureEntry {
            super::LceCaptureEntry {
                offset_micros,
                query: Some(LceReadOnlyQuery::Status),
                payload: super::LceCapturePayload::Bytes {
                    direction: LceCaptureDirection::Tx,
                    bytes: b"STA\r".to_vec(),
                },
            }
        }

        let prefix = vec![
            marker(0, None, LceCaptureMarker::SessionStarted),
            marker(1, None, LceCaptureMarker::TransportReady),
        ];
        let mut invalid_query = prefix.clone();
        invalid_query.push(marker(
            2,
            Some(LceReadOnlyQuery::Status),
            LceCaptureMarker::QueryIssued {
                query: LceReadOnlyQuery::Status,
            },
        ));
        invalid_query.push(tx(3));
        assert!(matches!(
            super::FinalizedLceEvidenceCapture::from_json_bytes(&encoded_artifact(
                LceInterfaceState::Unknown,
                invalid_query,
            )),
            Err(LceCaptureError::InvalidCapture(
                "query issued before a known boundary"
            ))
        ));

        let mut invalid_tx = prefix.clone();
        invalid_tx.push(tx(2));
        assert!(matches!(
            super::FinalizedLceEvidenceCapture::from_json_bytes(&encoded_artifact(
                LceInterfaceState::Unknown,
                invalid_tx,
            )),
            Err(LceCaptureError::InvalidCapture(
                "TX bytes precede a known boundary"
            ))
        ));

        let mut valid = prefix;
        valid.push(marker(2, None, LceCaptureMarker::KnownBoundary));
        valid.push(marker(
            3,
            Some(LceReadOnlyQuery::Status),
            LceCaptureMarker::QueryIssued {
                query: LceReadOnlyQuery::Status,
            },
        ));
        valid.push(tx(4));
        assert!(
            super::FinalizedLceEvidenceCapture::from_json_bytes(&encoded_artifact(
                LceInterfaceState::Unknown,
                valid,
            ))
            .is_ok()
        );

        let mut impossible_locked = vec![
            marker(0, None, LceCaptureMarker::SessionStarted),
            marker(1, None, LceCaptureMarker::TransportReady),
        ];
        impossible_locked.push(marker(2, None, LceCaptureMarker::KnownBoundary));
        assert!(matches!(
            super::FinalizedLceEvidenceCapture::from_json_bytes(&encoded_artifact(
                LceInterfaceState::LockedOrUnavailable,
                impossible_locked,
            )),
            Err(LceCaptureError::InvalidCapture(
                "a locked/unavailable initial interface cannot observe a known boundary"
            ))
        ));
    }
}
