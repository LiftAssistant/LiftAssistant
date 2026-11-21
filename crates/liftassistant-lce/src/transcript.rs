use std::fmt;

use crate::{LceParseError, RecordedLceFrame, parse_recorded_frame};

const MAX_TRANSCRIPT_LINE_BYTES: usize = 512;
const MAX_PENDING_FRAME_BYTES: usize = 1024;
const MAX_PENDING_FRAME_LINES: usize = 16;
pub(crate) const MAX_INPUT_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LceTranscriptFamily {
    Sta,
    Cmd,
    Mde,
    Grp,
    Cal,
    Node,
    Spb,
    Rst,
}

impl LceTranscriptFamily {
    const fn name(self) -> &'static str {
        match self {
            Self::Sta => "STA",
            Self::Cmd => "CMD",
            Self::Mde => "MDE",
            Self::Grp => "GRP",
            Self::Cal => "CAL",
            Self::Node => "N",
            Self::Spb => "SPB",
            Self::Rst => "RST",
        }
    }
}

impl fmt::Display for LceTranscriptFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LceTranscriptError {
    ChunkTooLarge {
        bytes: usize,
    },
    LineTooLarge {
        raw_prefix: Vec<u8>,
    },
    InvalidByte {
        byte: u8,
        raw_prefix: Vec<u8>,
    },
    UnsupportedLineEnding {
        raw_prefix: Vec<u8>,
    },
    PendingFrameTooLarge {
        family: LceTranscriptFamily,
        raw_prefix: Vec<u8>,
    },
    PendingFrameTooManyLines {
        family: LceTranscriptFamily,
        lines: usize,
        raw_prefix: Vec<u8>,
    },
    IncompleteFrame {
        family: LceTranscriptFamily,
        lines: usize,
        raw: Vec<u8>,
    },
    UnsupportedFamily {
        family: LceTranscriptFamily,
        raw: Vec<u8>,
    },
    MalformedFrame {
        family: LceTranscriptFamily,
        raw: Vec<u8>,
        source: LceParseError,
    },
}

impl fmt::Display for LceTranscriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChunkTooLarge { bytes } => write!(
                f,
                "LCE transcript chunk has {bytes} bytes; maximum is {MAX_INPUT_CHUNK_BYTES}"
            ),
            Self::LineTooLarge { .. } => write!(
                f,
                "LCE transcript line exceeds {MAX_TRANSCRIPT_LINE_BYTES} bytes"
            ),
            Self::InvalidByte { byte, .. } => {
                write!(f, "LCE transcript line contains invalid byte 0x{byte:02X}")
            }
            Self::UnsupportedLineEnding { .. } => {
                f.write_str("LCE transcript contains a bare carriage return")
            }
            Self::PendingFrameTooLarge { family, .. } => write!(
                f,
                "pending LCE {family} transcript frame exceeds {MAX_PENDING_FRAME_BYTES} bytes"
            ),
            Self::PendingFrameTooManyLines { family, lines, .. } => write!(
                f,
                "pending LCE {family} transcript frame reached {lines} lines; maximum is {MAX_PENDING_FRAME_LINES}"
            ),
            Self::IncompleteFrame { family, lines, .. } => write!(
                f,
                "incomplete LCE {family} transcript frame ended after {lines} lines"
            ),
            Self::UnsupportedFamily { family, .. } => {
                write!(f, "LCE {family} raw transcript extraction is unsupported")
            }
            Self::MalformedFrame { family, source, .. } => {
                write!(f, "malformed LCE {family} transcript frame: {source}")
            }
        }
    }
}

impl std::error::Error for LceTranscriptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MalformedFrame { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedLceTranscriptFrame {
    /// The exact bounded bytes for the recognized table, including its observed
    /// LF/CRLF endings. This is adapter-local evidence, not canonical state.
    pub raw: Vec<u8>,
    pub native: RecordedLceFrame,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LceTranscriptEvent {
    NativeFrame(Box<DecodedLceTranscriptFrame>),
    /// One bounded line that was not recognized as a supported table or an
    /// explicitly unsupported documented family. It carries no fault meaning.
    UnknownBounded(Vec<u8>),
    Rejected(LceTranscriptError),
}

#[derive(Debug)]
struct PendingFrame {
    family: LceTranscriptFamily,
    lines: Vec<String>,
    raw: Vec<u8>,
    physical_lines: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingFramePushError {
    TooLarge,
    TooManyLines,
}

impl PendingFrame {
    fn new(
        family: LceTranscriptFamily,
        header: String,
        raw_line: &[u8],
        ending: LineEnding,
    ) -> Self {
        let mut raw = Vec::with_capacity(raw_line.len() + ending.bytes().len());
        raw.extend_from_slice(raw_line);
        raw.extend_from_slice(ending.bytes());
        Self {
            family,
            lines: vec![header],
            raw,
            physical_lines: 1,
        }
    }

    fn push_line(
        &mut self,
        line: &str,
        raw_line: &[u8],
        ending: LineEnding,
    ) -> Result<(), PendingFramePushError> {
        self.push_raw(raw_line, ending)?;
        self.lines.push(line.to_owned());
        Ok(())
    }

    fn push_raw_only(
        &mut self,
        raw_line: &[u8],
        ending: LineEnding,
    ) -> Result<(), PendingFramePushError> {
        self.push_raw(raw_line, ending)
    }

    fn push_raw(
        &mut self,
        raw_line: &[u8],
        ending: LineEnding,
    ) -> Result<(), PendingFramePushError> {
        if self.physical_lines == MAX_PENDING_FRAME_LINES {
            return Err(PendingFramePushError::TooManyLines);
        }
        let Some(total) = self
            .raw
            .len()
            .checked_add(raw_line.len())
            .and_then(|bytes| bytes.checked_add(ending.bytes().len()))
        else {
            return Err(PendingFramePushError::TooLarge);
        };
        if total > MAX_PENDING_FRAME_BYTES {
            return Err(PendingFramePushError::TooLarge);
        }
        self.raw.extend_from_slice(raw_line);
        self.raw.extend_from_slice(ending.bytes());
        self.physical_lines += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineEnding {
    Lf,
    CrLf,
    EndOfStream,
}

impl LineEnding {
    const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Lf => b"\n",
            Self::CrLf => b"\r\n",
            Self::EndOfStream => b"",
        }
    }
}

/// Incrementally extracts bounded `STA`, `CMD`, and `MDE` table projections from
/// offline LCE terminal transcript bytes.
///
/// Only LF and CRLF line endings are accepted. Unknown lines are emitted as bounded
/// adapter-local evidence and never interpreted as observations. Raw `GRP`, `CAL`,
/// `N`, `SPB`, and `RST` extraction remains unsupported because the reviewed evidence
/// does not establish a safe general block boundary for those variable or interactive
/// families. The accepted three-row projection is deliberately narrower than a claim
/// about every historical firmware's screen layout.
#[derive(Debug, Default)]
pub struct LceTranscriptDecoder {
    line: Vec<u8>,
    pending_cr: bool,
    discarding_line: bool,
    pending_frame: Option<PendingFrame>,
}

impl LceTranscriptDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn pending_family(&self) -> Option<LceTranscriptFamily> {
        self.pending_frame.as_ref().map(|frame| frame.family)
    }

    /// Accepts an arbitrary chunk and returns every complete frame or structural
    /// rejection produced by that chunk. A single call is bounded so the returned
    /// event collection is also bounded; larger offline buffers can be supplied in
    /// any sequence of smaller chunks.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<LceTranscriptEvent> {
        if chunk.len() > MAX_INPUT_CHUNK_BYTES {
            return vec![LceTranscriptEvent::Rejected(
                LceTranscriptError::ChunkTooLarge { bytes: chunk.len() },
            )];
        }
        let mut events = Vec::new();
        for &byte in chunk {
            if self.pending_cr {
                self.pending_cr = false;
                if byte == b'\n' {
                    self.finish_line(LineEnding::CrLf, &mut events);
                    continue;
                }
                let mut raw_prefix = self.line.clone();
                if raw_prefix.len() < MAX_TRANSCRIPT_LINE_BYTES {
                    raw_prefix.push(b'\r');
                }
                self.reject_current_line(
                    LceTranscriptError::UnsupportedLineEnding { raw_prefix },
                    &mut events,
                );
            }

            match byte {
                b'\n' => self.finish_line(LineEnding::Lf, &mut events),
                b'\r' => self.pending_cr = true,
                b'\t' | 0x20..=0x7e => self.push_byte(byte, &mut events),
                other => {
                    let mut raw_prefix = self.line.clone();
                    if raw_prefix.len() < MAX_TRANSCRIPT_LINE_BYTES {
                        raw_prefix.push(other);
                    }
                    self.reject_current_line(
                        LceTranscriptError::InvalidByte {
                            byte: other,
                            raw_prefix,
                        },
                        &mut events,
                    );
                }
            }
        }
        events
    }

    /// Completes a final unterminated line and reports any incomplete frame.
    /// The decoder is reset and can then be reused for another offline stream.
    pub fn finish(&mut self) -> Vec<LceTranscriptEvent> {
        let mut events = Vec::new();
        if self.pending_cr {
            self.pending_cr = false;
            let mut raw_prefix = self.line.clone();
            if raw_prefix.len() < MAX_TRANSCRIPT_LINE_BYTES {
                raw_prefix.push(b'\r');
            }
            self.reject_current_line(
                LceTranscriptError::UnsupportedLineEnding { raw_prefix },
                &mut events,
            );
        } else if !self.line.is_empty() || self.discarding_line {
            self.finish_line(LineEnding::EndOfStream, &mut events);
        }
        if let Some(pending) = self.pending_frame.take() {
            events.push(LceTranscriptEvent::Rejected(
                LceTranscriptError::IncompleteFrame {
                    family: pending.family,
                    lines: pending.lines.len(),
                    raw: pending.raw,
                },
            ));
        }
        self.line.clear();
        self.discarding_line = false;
        events
    }

    fn push_byte(&mut self, byte: u8, events: &mut Vec<LceTranscriptEvent>) {
        if self.discarding_line {
            return;
        }
        if self.line.len() == MAX_TRANSCRIPT_LINE_BYTES {
            self.reject_current_line(
                LceTranscriptError::LineTooLarge {
                    raw_prefix: self.line.clone(),
                },
                events,
            );
            return;
        }
        self.line.push(byte);
    }

    fn reject_current_line(
        &mut self,
        error: LceTranscriptError,
        events: &mut Vec<LceTranscriptEvent>,
    ) {
        if !self.discarding_line {
            events.push(LceTranscriptEvent::Rejected(error));
            if let Some(pending) = self.pending_frame.take() {
                events.push(LceTranscriptEvent::Rejected(
                    LceTranscriptError::IncompleteFrame {
                        family: pending.family,
                        lines: pending.lines.len(),
                        raw: pending.raw,
                    },
                ));
            }
        }
        self.line.clear();
        self.discarding_line = true;
    }

    fn finish_line(&mut self, ending: LineEnding, events: &mut Vec<LceTranscriptEvent>) {
        if self.discarding_line {
            self.discarding_line = false;
            self.line.clear();
            return;
        }
        let bytes = std::mem::take(&mut self.line);
        let line = String::from_utf8(bytes.clone()).expect("accepted transcript bytes are ASCII");
        let line = line.trim();
        if line.is_empty() {
            if let Some(pending) = self.pending_frame.as_mut() {
                if let Err(error) = pending.push_raw_only(&bytes, ending) {
                    let family = pending.family;
                    let lines = pending.physical_lines + 1;
                    let raw_prefix = pending_raw_prefix(pending, &bytes, ending);
                    self.pending_frame = None;
                    events.push(LceTranscriptEvent::Rejected(match error {
                        PendingFramePushError::TooLarge => {
                            LceTranscriptError::PendingFrameTooLarge { family, raw_prefix }
                        }
                        PendingFramePushError::TooManyLines => {
                            LceTranscriptError::PendingFrameTooManyLines {
                                family,
                                lines,
                                raw_prefix,
                            }
                        }
                    }));
                }
            } else {
                events.push(LceTranscriptEvent::UnknownBounded(raw_line(&bytes, ending)));
            }
            return;
        }

        if let Some((family, header)) = supported_header(line) {
            if let Some(pending) = self.pending_frame.take() {
                events.push(LceTranscriptEvent::Rejected(
                    LceTranscriptError::IncompleteFrame {
                        family: pending.family,
                        lines: pending.lines.len(),
                        raw: pending.raw,
                    },
                ));
            }
            self.pending_frame = Some(PendingFrame::new(family, header, &bytes, ending));
            return;
        }

        if let Some(family) = unsupported_header(line) {
            if let Some(pending) = self.pending_frame.take() {
                events.push(LceTranscriptEvent::Rejected(
                    LceTranscriptError::IncompleteFrame {
                        family: pending.family,
                        lines: pending.lines.len(),
                        raw: pending.raw,
                    },
                ));
            }
            events.push(LceTranscriptEvent::Rejected(
                LceTranscriptError::UnsupportedFamily {
                    family,
                    raw: raw_line(&bytes, ending),
                },
            ));
            return;
        }

        let Some(pending) = self.pending_frame.as_mut() else {
            events.push(LceTranscriptEvent::UnknownBounded(raw_line(&bytes, ending)));
            return;
        };
        if let Err(error) = pending.push_line(line, &bytes, ending) {
            let family = pending.family;
            let lines = pending.physical_lines + 1;
            let raw_prefix = pending_raw_prefix(pending, &bytes, ending);
            self.pending_frame = None;
            events.push(LceTranscriptEvent::Rejected(match error {
                PendingFramePushError::TooLarge => {
                    LceTranscriptError::PendingFrameTooLarge { family, raw_prefix }
                }
                PendingFramePushError::TooManyLines => {
                    LceTranscriptError::PendingFrameTooManyLines {
                        family,
                        lines,
                        raw_prefix,
                    }
                }
            }));
            return;
        }
        if pending.lines.len() == 3 {
            let pending = self.pending_frame.take().expect("pending frame exists");
            let text = pending.lines.join("\n");
            match parse_recorded_frame(&text) {
                Ok(native) => events.push(LceTranscriptEvent::NativeFrame(Box::new(
                    DecodedLceTranscriptFrame {
                        raw: pending.raw,
                        native,
                    },
                ))),
                Err(source) => events.push(LceTranscriptEvent::Rejected(
                    LceTranscriptError::MalformedFrame {
                        family: pending.family,
                        raw: pending.raw,
                        source,
                    },
                )),
            }
        }
    }
}

fn pending_raw_prefix(pending: &PendingFrame, line: &[u8], ending: LineEnding) -> Vec<u8> {
    let mut raw_prefix = pending.raw.clone();
    let remaining = MAX_PENDING_FRAME_BYTES.saturating_sub(raw_prefix.len());
    let line_bytes = line.len().min(remaining);
    raw_prefix.extend_from_slice(&line[..line_bytes]);
    let remaining = MAX_PENDING_FRAME_BYTES.saturating_sub(raw_prefix.len());
    raw_prefix.extend_from_slice(&ending.bytes()[..ending.bytes().len().min(remaining)]);
    raw_prefix
}

fn raw_line(line: &[u8], ending: LineEnding) -> Vec<u8> {
    let mut raw = Vec::with_capacity(line.len() + ending.bytes().len());
    raw.extend_from_slice(line);
    raw.extend_from_slice(ending.bytes());
    raw
}

fn supported_header(line: &str) -> Option<(LceTranscriptFamily, String)> {
    let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
    let (family, normalized_title) = match tokens.first().copied() {
        Some("STA") => (LceTranscriptFamily::Sta, "STATUS_OF_LIFT"),
        Some("CMD") => (LceTranscriptFamily::Cmd, "COMMAND_OF_LIFT"),
        Some("MDE") => (LceTranscriptFamily::Mde, "MODE_OF_LIFT"),
        _ => return None,
    };
    let lift = match tokens.as_slice() {
        [_, title, lift] if title.eq_ignore_ascii_case(normalized_title) => *lift,
        [_, first, second, third, lift]
            if first
                .eq_ignore_ascii_case(normalized_title.split('_').next().unwrap_or_default())
                && second.eq_ignore_ascii_case("OF")
                && third.eq_ignore_ascii_case("LIFT") =>
        {
            *lift
        }
        _ => return None,
    };
    Some((
        family,
        format!("{} {normalized_title} {lift}", family.name()),
    ))
}

fn normalized_tokens(line: &str) -> String {
    line.split_ascii_whitespace()
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn group_header(line: &str) -> bool {
    let tokens = line.split_ascii_whitespace().collect::<Vec<_>>();
    matches!(tokens.as_slice(), ["GRP", "GROUP", _, "TRAFFIC", _])
        || normalized_tokens(line) == "GRP"
}

fn documented_unsupported_header(line: &str) -> Option<LceTranscriptFamily> {
    match normalized_tokens(line).as_str() {
        "CAL" | "CAL ACTIVE_CALLS" | "CAL ACTIVE CALLS" => Some(LceTranscriptFamily::Cal),
        "N" | "N NODE_TABLE" | "N NODE TABLE" | "N NODE TABLE LISTING" => {
            Some(LceTranscriptFamily::Node)
        }
        "SPB" => Some(LceTranscriptFamily::Spb),
        "RST" => Some(LceTranscriptFamily::Rst),
        _ => None,
    }
}

fn unsupported_header(line: &str) -> Option<LceTranscriptFamily> {
    if group_header(line) {
        Some(LceTranscriptFamily::Grp)
    } else {
        documented_unsupported_header(line)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use liftassistant_engine::{
        Engine, ObservationBindingRole, ObservationIngestError, ObservationIngestResult,
    };
    use liftassistant_model::{
        BuildingTopology, CarId, CarLocation, Floor, ObservationProvenance, ObservationSequence,
        ObservationSessionId, ObservationSourceId, OperatingMode, ReportedDestination,
    };

    use super::*;
    use crate::{LceSourceContext, map_cmd_record, map_mde_record, map_sta_record};

    const STA: &str = include_str!("../tests/fixtures/sta_stopped.txt");
    const CMD: &str = include_str!("../tests/fixtures/cmd_known_destination.txt");
    const MDE: &str = include_str!("../tests/fixtures/mde_normal.txt");

    fn decode(chunks: impl IntoIterator<Item = Vec<u8>>) -> Vec<LceTranscriptEvent> {
        let mut decoder = LceTranscriptDecoder::new();
        let mut events = Vec::new();
        for chunk in chunks {
            events.extend(decoder.push(&chunk));
        }
        events.extend(decoder.finish());
        events
    }

    fn frames(events: &[LceTranscriptEvent]) -> Vec<RecordedLceFrame> {
        events
            .iter()
            .filter_map(|event| match event {
                LceTranscriptEvent::NativeFrame(decoded) => Some(decoded.native.clone()),
                LceTranscriptEvent::UnknownBounded(_) | LceTranscriptEvent::Rejected(_) => None,
            })
            .collect()
    }

    fn transcript() -> String {
        format!("PRINTER MODE\r\nLCECPU/fixture\r\n?\r\nV VTEST1\r\nSTA\r\n{STA}{CMD}{MDE}?\r\n")
    }

    fn floors() -> BTreeMap<String, Floor> {
        [("01", 1), ("02", 2), ("03", 3), ("04", 4), ("05", 5)]
            .into_iter()
            .map(|(native, logical)| (native.to_owned(), Floor::new(logical)))
            .collect()
    }

    fn provenance(session: &str, sequence: u64) -> ObservationProvenance {
        ObservationProvenance {
            source: ObservationSourceId::new("offline/lce-transcript"),
            session: ObservationSessionId::new(session),
            sequence: ObservationSequence::new(sequence),
        }
    }

    #[test]
    fn chunk_boundaries_do_not_change_supported_frames() {
        let input = transcript().into_bytes();
        let whole = decode([input.clone()]);
        let bytewise = decode(input.iter().copied().map(|byte| vec![byte]));
        let two_byte = decode(input.chunks(2).map(<[u8]>::to_vec));
        let mut chunks = Vec::new();
        let mut offset = 0;
        let sizes = [1, 7, 2, 19, 3, 5, 31, 11];
        let mut index = 0;
        while offset < input.len() {
            let end = (offset + sizes[index % sizes.len()]).min(input.len());
            chunks.push(input[offset..end].to_vec());
            offset = end;
            index += 1;
        }
        let partitioned = decode(chunks);

        assert_eq!(whole, bytewise);
        assert_eq!(whole, two_byte);
        assert_eq!(whole, partitioned);
        assert_eq!(frames(&whole).len(), 3);
    }

    #[test]
    fn manual_title_spacing_and_lf_or_fragmented_crlf_are_supported() {
        let spaced = STA.replacen("STA STATUS_OF_LIFT", "STA STATUS OF LIFT", 1);
        let lf = decode([spaced.clone().into_bytes()]);
        let crlf = spaced.replace('\n', "\r\n").into_bytes();
        let fragmented = decode(crlf.iter().copied().map(|byte| vec![byte]));
        assert_eq!(frames(&lf), frames(&fragmented));
        assert_eq!(frames(&lf).len(), 1);

        let historical_title_case = MDE.replacen("MDE MODE_OF_LIFT", "MDE mode of lift", 1);
        assert_eq!(
            frames(&decode([historical_title_case.into_bytes()])).len(),
            1
        );
    }

    #[test]
    fn banner_prompt_echo_and_unknown_output_remain_noncanonical_raw_lines() {
        let prefix = b"PRINTER MODE\r\nLCECPU/fixture\r\n?\r\nV VTEST1\r\nSTA\r\n";
        let suffix = b"HLP fixture-only output\n?\n";
        let events = decode([prefix.as_slice(), STA.as_bytes(), suffix.as_slice()].map(Vec::from));

        assert_eq!(frames(&events).len(), 1);
        let unknown = events
            .iter()
            .filter_map(|event| match event {
                LceTranscriptEvent::UnknownBounded(raw) => Some(raw.as_slice()),
                LceTranscriptEvent::NativeFrame(_) | LceTranscriptEvent::Rejected(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            unknown,
            [
                b"PRINTER MODE\r\n".as_slice(),
                b"LCECPU/fixture\r\n".as_slice(),
                b"?\r\n".as_slice(),
                b"V VTEST1\r\n".as_slice(),
                b"STA\r\n".as_slice(),
                b"HLP fixture-only output\n".as_slice(),
                b"?\n".as_slice(),
            ]
        );
    }

    #[test]
    fn header_columns_and_values_may_each_cross_chunk_boundaries() {
        let bytes = STA.as_bytes();
        let first_newline = bytes.iter().position(|byte| *byte == b'\n').unwrap();
        let second_newline = bytes[first_newline + 1..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| offset + first_newline + 1)
            .unwrap();
        let cuts = [
            2,
            first_newline - 2,
            first_newline + 5,
            second_newline - 3,
            second_newline + 4,
            bytes.len() - 2,
        ];
        let mut offset = 0;
        let mut chunks = Vec::new();
        for cut in cuts {
            chunks.push(bytes[offset..cut].to_vec());
            offset = cut;
        }
        chunks.push(bytes[offset..].to_vec());

        let decoded = decode(chunks);
        assert_eq!(frames(&decoded), frames(&decode([bytes.to_vec()])));
    }

    #[test]
    fn incomplete_and_malformed_frames_are_explicit_and_recovery_is_local() {
        let incomplete = decode([STA
            .lines()
            .take(2)
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()]);
        assert!(matches!(
            incomplete.as_slice(),
            [LceTranscriptEvent::Rejected(
                LceTranscriptError::IncompleteFrame {
                    family: LceTranscriptFamily::Sta,
                    lines: 2,
                    ..
                }
            )]
        ));

        let malformed = STA.replacen("ADV_POS", "WRONG", 1);
        let recovered = decode([format!("{malformed}{CMD}").into_bytes()]);
        assert!(matches!(
            recovered.first(),
            Some(LceTranscriptEvent::Rejected(
                LceTranscriptError::MalformedFrame {
                    family: LceTranscriptFamily::Sta,
                    source: LceParseError::InvalidColumns { family: "STA" },
                    ..
                }
            ))
        ));
        let LceTranscriptEvent::Rejected(LceTranscriptError::MalformedFrame { raw, .. }) =
            &recovered[0]
        else {
            unreachable!()
        };
        assert_eq!(raw, malformed.as_bytes());
        assert!(matches!(
            recovered.get(1),
            Some(LceTranscriptEvent::NativeFrame(decoded))
                if matches!(decoded.native, RecordedLceFrame::Cmd(_))
        ));
    }

    #[test]
    fn bounds_and_invalid_bytes_recover_at_the_next_line() {
        let oversized_line = format!("{}\n{STA}", "X".repeat(MAX_TRANSCRIPT_LINE_BYTES + 1));
        let events = decode([oversized_line.into_bytes()]);
        assert!(matches!(
            events.first(),
            Some(LceTranscriptEvent::Rejected(
                LceTranscriptError::LineTooLarge { .. }
            ))
        ));
        assert!(matches!(
            events.get(1),
            Some(LceTranscriptEvent::NativeFrame(decoded))
                if matches!(decoded.native, RecordedLceFrame::Sta(_))
        ));

        let invalid = decode([b"noise\x00ignored\n".to_vec(), CMD.as_bytes().to_vec()]);
        assert!(matches!(
            invalid.first(),
            Some(LceTranscriptEvent::Rejected(
                LceTranscriptError::InvalidByte { byte: 0, .. }
            ))
        ));
        assert!(matches!(
            invalid.get(1),
            Some(LceTranscriptEvent::NativeFrame(decoded))
                if matches!(decoded.native, RecordedLceFrame::Cmd(_))
        ));

        let mut decoder = LceTranscriptDecoder::new();
        assert!(matches!(
            decoder
                .push(&vec![b'X'; MAX_INPUT_CHUNK_BYTES + 1])
                .as_slice(),
            [LceTranscriptEvent::Rejected(
                LceTranscriptError::ChunkTooLarge { .. }
            )]
        ));
        let recovered = decoder.push(STA.as_bytes());
        assert!(matches!(
            recovered.as_slice(),
            [LceTranscriptEvent::NativeFrame(decoded)]
                if matches!(decoded.native, RecordedLceFrame::Sta(_))
        ));
    }

    #[test]
    fn recognized_raw_evidence_keeps_observed_whitespace_and_line_endings() {
        let input = STA.replacen('\n', "\r\n", 1);
        let mut decoder = LceTranscriptDecoder::new();
        let events = decoder.push(input.as_bytes());
        let Some(LceTranscriptEvent::NativeFrame(decoded)) = events.first() else {
            panic!("STA should produce native evidence")
        };
        assert_eq!(decoded.raw, input.as_bytes());
    }

    #[test]
    fn pending_frame_bound_and_unsupported_families_do_not_fabricate_frames() {
        let large = format!(
            "STA STATUS_OF_LIFT 02\n{}\n{}\n",
            "A".repeat(MAX_TRANSCRIPT_LINE_BYTES),
            "B".repeat(MAX_TRANSCRIPT_LINE_BYTES)
        );
        let events = decode([large.into_bytes()]);
        assert!(matches!(
            events.as_slice(),
            [LceTranscriptEvent::Rejected(
                LceTranscriptError::PendingFrameTooLarge {
                    family: LceTranscriptFamily::Sta,
                    ..
                }
            )]
        ));

        let too_many_lines = format!(
            "STA STATUS_OF_LIFT 02\n{}",
            "\n".repeat(MAX_PENDING_FRAME_LINES)
        );
        let events = decode([too_many_lines.into_bytes()]);
        assert!(matches!(
            events.as_slice(),
            [LceTranscriptEvent::Rejected(
                LceTranscriptError::PendingFrameTooManyLines {
                    family: LceTranscriptFamily::Sta,
                    lines,
                    ..
                }
            )] if *lines == MAX_PENDING_FRAME_LINES + 1
        ));

        let unsupported = decode([
            b"GRP GROUP 7 TRAFFIC 0\nLIFT SM DE FL\n1 30 04 03\n".to_vec(),
            b"CAL ACTIVE_CALLS\nN Node table listing\nSPB\nRST\nfuture banner\n".to_vec(),
        ]);
        assert!(frames(&unsupported).is_empty());
        for family in [
            LceTranscriptFamily::Grp,
            LceTranscriptFamily::Cal,
            LceTranscriptFamily::Node,
            LceTranscriptFamily::Spb,
            LceTranscriptFamily::Rst,
        ] {
            assert!(unsupported.iter().any(|event| matches!(
                event,
                LceTranscriptEvent::Rejected(LceTranscriptError::UnsupportedFamily {
                    family: actual,
                    ..
                }) if *actual == family
            )));
        }
        assert!(unsupported.iter().any(|event| matches!(
            event,
            LceTranscriptEvent::UnknownBounded(raw) if raw == b"future banner\n"
        )));
    }

    #[test]
    fn bare_cr_is_rejected_without_treating_it_as_a_supported_line_ending() {
        let events = decode([b"banner\rSTA STATUS_OF_LIFT 02\n".to_vec()]);
        assert!(matches!(
            events.first(),
            Some(LceTranscriptEvent::Rejected(
                LceTranscriptError::UnsupportedLineEnding { .. }
            ))
        ));
        assert!(frames(&events).is_empty());
    }

    #[test]
    fn deterministic_untrusted_bytes_never_panic_or_accumulate_across_finish() {
        for seed in 0..2_048_u32 {
            let length = (seed as usize * 53) % 769;
            let mut state = seed.wrapping_add(7);
            let mut input = Vec::with_capacity(length);
            for _ in 0..length {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                input.push((state >> 16) as u8);
            }
            let mut decoder = LceTranscriptDecoder::new();
            for chunk in input.chunks(13) {
                let _ = decoder.push(chunk);
            }
            let _ = decoder.finish();
            assert!(decoder.line.is_empty());
            assert!(decoder.pending_frame.is_none());
        }
    }

    #[test]
    fn chunked_transcript_reuses_existing_mapping_and_engine_session_rules() {
        let rich_sta = STA.replace(
            "05 0 0 0 0 1 03 1 0 3 3 0 0 0 FF",
            "05 0 0 0 64 1 03 1 2 3 3 1 1 0 FF",
        );
        let input = format!("{rich_sta}{CMD}{MDE}").into_bytes();
        let events = decode(input.chunks(7).map(<[u8]>::to_vec));
        let Some(LceTranscriptEvent::NativeFrame(first)) = events.first() else {
            panic!("first event should be the decoded STA evidence")
        };
        assert_eq!(first.raw, rich_sta.as_bytes());
        let decoded = frames(&events);
        assert_eq!(decoded.len(), 3);

        let topology = BuildingTopology::new((1..=5).map(Floor::new)).expect("valid topology");
        let mut engine = Engine::new(topology);
        let source = ObservationSourceId::new("offline/lce-transcript");
        let session_a = ObservationSessionId::new("offline-session-A");
        let car = CarId::new("logical-A");
        engine
            .bind_observation(
                source.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("explicit binding should succeed");
        engine
            .begin_source_session(source.clone(), session_a.clone())
            .expect("explicit session should begin");

        for (index, frame) in decoded.iter().enumerate() {
            let provenance = provenance("offline-session-A", index as u64 + 1);
            let update = match frame {
                RecordedLceFrame::Sta(record) => {
                    let mapped = map_sta_record(record, car.clone(), &floors(), provenance)
                        .expect("configured STA should map");
                    let LceSourceContext::Sta(context) = &mapped.source_context else {
                        panic!("STA context should remain local")
                    };
                    assert_eq!(context.advance_position, "05");
                    assert_eq!(context.fault_id, "64");
                    assert_eq!(context.terminal_limit, "2");
                    assert_eq!(context.load, "1");
                    assert_eq!(context.correction_drive, "1");
                    assert_eq!(mapped.update.mode, None);
                    mapped.update
                }
                RecordedLceFrame::Cmd(record) => {
                    map_cmd_record(record, car.clone(), &floors(), provenance)
                        .expect("configured CMD should map")
                        .update
                }
                RecordedLceFrame::Mde(record) => {
                    map_mde_record(record, car.clone(), provenance)
                        .expect("configured MDE should map")
                        .update
                }
                RecordedLceFrame::Grp(_) => panic!("raw GRP extraction is unsupported"),
            };
            let result = engine.ingest_observation_update(update);
            if index < 2 {
                assert!(matches!(result, ObservationIngestResult::Incomplete(_)));
            } else {
                assert!(matches!(result, ObservationIngestResult::Resolution(_)));
            }
        }

        let state = engine
            .operational_car_state(&car)
            .expect("three existing semantic families should resolve the car");
        assert_eq!(state.location, CarLocation::Floor(Floor::new(3)));
        assert_eq!(state.mode, OperatingMode::Normal);
        assert_eq!(
            state.reported_destination,
            ReportedDestination::Known(Floor::new(4))
        );

        let unbound = CarId::new("native-02-is-not-a-logical-car");
        let RecordedLceFrame::Cmd(command) = &decoded[1] else {
            panic!("second frame should be CMD")
        };
        let unmapped_command = crate::LceCmdRecord {
            destination: "09".to_owned(),
            ..command.clone()
        };
        assert_eq!(
            map_cmd_record(
                &unmapped_command,
                car.clone(),
                &floors(),
                provenance("offline-session-A", 4),
            ),
            Err(crate::LceMappingError::UnknownFloorToken("09".to_owned()))
        );
        let unbound = map_cmd_record(
            command,
            unbound,
            &floors(),
            provenance("offline-session-A", 5),
        )
        .expect("adapter mapping takes caller-supplied identity");
        assert!(matches!(
            engine.ingest_observation_update(unbound.update),
            ObservationIngestResult::Rejected(ObservationIngestError::UnboundObservation { .. })
        ));

        engine
            .begin_source_session(
                source.clone(),
                ObservationSessionId::new("offline-session-B"),
            )
            .expect("new explicit session should invalidate old evidence");
        assert!(engine.operational_car_state(&car).is_none());
        let delayed = map_cmd_record(
            command,
            car.clone(),
            &floors(),
            provenance("offline-session-A", 6),
        )
        .expect("old-session command still maps locally");
        assert!(matches!(
            engine.ingest_observation_update(delayed.update),
            ObservationIngestResult::Rejected(ObservationIngestError::InactiveSourceSession { .. })
        ));

        let fresh_partial = map_cmd_record(
            command,
            car.clone(),
            &floors(),
            provenance("offline-session-B", 1),
        )
        .expect("new-session command should map locally");
        assert!(matches!(
            engine.ingest_observation_update(fresh_partial.update),
            ObservationIngestResult::Incomplete(_)
        ));
        assert!(
            engine.operational_car_state(&car).is_none(),
            "new-session partial evidence must not inherit old-session fields"
        );
    }
}
