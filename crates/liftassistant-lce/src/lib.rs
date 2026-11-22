#![forbid(unsafe_code)]

//! Experimental parser and conservative canonical mapping for recorded KONE LCE
//! diagnostic-terminal tables.
//!
//! The normal library/product surface contains no real serial backend, device enumeration,
//! unlock logic, controller-control path, or production hardware integration. A Linux-only
//! experimental example performs bounded software-RX-only tty acquisition; it is not linked
//! into the production Host, Core, or Engine. The terminal session can generate only the typed
//! `STA`, `CMD`, and `MDE` observation queries for synthetic PTY validation. The crate also
//! accepts original synthetic recorded fixtures and bounded offline transcript bytes for the
//! same native families.

mod capture;
mod passive_capture;
mod terminal;
mod transcript;

pub use capture::{
    FinalizedLceEvidenceCapture, LCE_CAPTURE_FORMAT_VERSION, LceCaptureDirection,
    LceCaptureEndReason, LceCaptureEntry, LceCaptureError, LceCaptureMarker, LceCaptureOrigin,
    LceCapturePayload,
};
pub use passive_capture::{
    FinalizedLcePassiveCapture, LCE_PASSIVE_CAPTURE_FORMAT_VERSION, LINUX_N_TTY_LINE_DISCIPLINE,
    LceNormalizedTermios, LcePassiveAccessMode, LcePassiveAcquisitionLayer,
    LcePassiveCaptureEndReason, LcePassiveCaptureError, LcePassiveCaptureMetadata,
    LcePassiveCaptureOutcome, LcePassiveCaptureRecorder, LcePassiveExclusiveModeStatus,
    LcePassiveInputQueueObservation, LcePassiveOpenPolicy, LcePassiveOpenedDevice,
    LcePassiveOpenedDeviceKind, LcePassiveRxEntry, LcePassiveRxEntryKind,
    MAX_PASSIVE_CAPTURE_DURATION_SECONDS, MAX_PASSIVE_CAPTURE_ENTRIES,
    MAX_PASSIVE_CAPTURE_ENTRY_BYTES, MAX_PASSIVE_CAPTURE_RAW_BYTES,
    MAX_PASSIVE_CAPTURE_SESSION_ID_BYTES, MAX_PASSIVE_DEVICE_PATH_BYTES,
    MAX_SERIALIZED_PASSIVE_CAPTURE_BYTES,
};
pub use terminal::{
    LceDataBits, LceDisconnectReason, LceInterfaceState, LceParity, LceReadOnlyQuery,
    LceRuntimeTerminalProfile, LceStopBits, LceTerminalFailure, LceTerminalFailureKind,
    LceTerminalSession, LceTerminalSessionEvent, LceTerminalSessionState, LceTerminalTx,
    LceTerminalUseError,
};

pub use transcript::{
    DecodedLceTranscriptFrame, LceTranscriptDecoder, LceTranscriptError, LceTranscriptEvent,
    LceTranscriptFamily,
};

use std::{collections::BTreeMap, error::Error, fmt};

use liftassistant_model::{
    CarId, CarObservationUpdate, Direction, DoorEvidence, DoorState, Floor, MotionEvidence,
    MotionState, ObservationProvenance, OperatingMode, OperatingModeEvidence, PositionEvidence,
    ReportedDestination,
};

const MAX_FRAME_BYTES: usize = 8 * 1024;
const MAX_LINE_BYTES: usize = 512;
const MAX_LINES: usize = 32;
const MAX_TOKEN_BYTES: usize = 64;
const MAX_GROUP_ROWS: usize = 8;

const STA_FIELDS: [&str; 15] = [
    "ADV_POS", "SPD", "DIR", "SRV", "FLT_ID", "DOOR", "LVL_NO", "AT_DZN", "TERM_LIM", "SPDA_VID",
    "SPDB_VID", "LOAD", "CORR_DRV", "CC_PREFF", "AT_LEVEL",
];
const CMD_FIELDS: [&str; 7] = [
    "DEST",
    "PLN_DIR",
    "DOOR_SIDE",
    "OPEN_WDTH",
    "OPEN_TIME",
    "DEL",
    "CLS_MDE",
];
const MDE_FIELDS: [&str; 7] = [
    "LIFT_MODE",
    "SIGNL_GROUP",
    "REC_LDG",
    "REC_CAR",
    "FAULT_ID",
    "DOOR_STATE",
    "ADV_POS",
];
const GRP_FIELDS: [&str; 4] = ["LIFT", "SM", "DE", "FL"];

/// A bounded, parsed record from one documented LCE read family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordedLceFrame {
    Sta(LceStaRecord),
    Cmd(LceCmdRecord),
    Mde(LceMdeRecord),
    Grp(LceGroupRecord),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LceStaRecord {
    pub native_lift: String,
    pub advance_position: String,
    pub speed: String,
    pub direction: String,
    pub service: String,
    pub fault_id: String,
    pub door: String,
    pub level_number: String,
    pub at_door_zone: String,
    pub terminal_limit: String,
    pub speed_plate_a: String,
    pub speed_plate_b: String,
    pub load: String,
    pub correction_drive: String,
    pub car_call_preference: String,
    pub at_level: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LceCmdRecord {
    pub native_lift: String,
    pub destination: String,
    pub planned_direction: String,
    pub door_side: String,
    pub opening_width: String,
    pub open_time: String,
    pub delay: String,
    pub closing_mode: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LceMdeRecord {
    pub native_lift: String,
    pub lift_mode: String,
    pub single_group: String,
    pub record_landings: String,
    pub record_car_calls: String,
    pub fault_id: String,
    pub door_state: String,
    pub advance_position: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LceGroupRecord {
    pub native_group: String,
    pub traffic: String,
    pub rows: Vec<LceGroupRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LceGroupRow {
    pub native_lift: String,
    pub serving_mode: String,
    pub destination: String,
    pub current_floor: String,
}

/// Source-local context retained beside a canonical update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LceSourceContext {
    Sta(LceStaRecord),
    Cmd(LceCmdRecord),
    Mde(LceMdeRecord),
    Group {
        native_group: String,
        traffic: String,
        row: LceGroupRow,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappedLceUpdate {
    pub update: CarObservationUpdate,
    pub source_context: LceSourceContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LceParseError {
    FrameTooLarge {
        bytes: usize,
    },
    TooManyLines {
        lines: usize,
    },
    LineTooLarge {
        line: usize,
        bytes: usize,
    },
    NonAscii {
        line: usize,
    },
    TokenTooLarge {
        line: usize,
    },
    Empty,
    UnsupportedFamily(String),
    InvalidHeader {
        family: &'static str,
    },
    InvalidColumns {
        family: &'static str,
    },
    InvalidValueCount {
        family: &'static str,
        expected: usize,
        actual: usize,
    },
    TooManyGroupRows {
        rows: usize,
    },
}

impl fmt::Display for LceParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameTooLarge { bytes } => {
                write!(
                    f,
                    "recorded LCE frame has {bytes} bytes; maximum is {MAX_FRAME_BYTES}"
                )
            }
            Self::TooManyLines { lines } => {
                write!(
                    f,
                    "recorded LCE frame has {lines} lines; maximum is {MAX_LINES}"
                )
            }
            Self::LineTooLarge { line, bytes } => write!(
                f,
                "recorded LCE line {line} has {bytes} bytes; maximum is {MAX_LINE_BYTES}"
            ),
            Self::NonAscii { line } => {
                write!(f, "recorded LCE line {line} is not ASCII")
            }
            Self::TokenTooLarge { line } => write!(
                f,
                "recorded LCE line {line} contains a token longer than {MAX_TOKEN_BYTES} bytes"
            ),
            Self::Empty => f.write_str("recorded LCE frame is empty"),
            Self::UnsupportedFamily(family) => {
                write!(f, "unsupported recorded LCE family {family:?}")
            }
            Self::InvalidHeader { family } => {
                write!(f, "invalid {family} recorded LCE header")
            }
            Self::InvalidColumns { family } => {
                write!(f, "invalid {family} recorded LCE columns")
            }
            Self::InvalidValueCount {
                family,
                expected,
                actual,
            } => write!(
                f,
                "invalid {family} recorded LCE value count: expected {expected}, got {actual}"
            ),
            Self::TooManyGroupRows { rows } => write!(
                f,
                "recorded LCE GRP frame has {rows} rows; maximum is {MAX_GROUP_ROWS}"
            ),
        }
    }
}

impl Error for LceParseError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LceMappingError {
    InvalidNumericToken { field: &'static str, value: String },
    UnknownFloorToken(String),
}

impl fmt::Display for LceMappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNumericToken { field, value } => {
                write!(
                    f,
                    "LCE field {field} contains invalid numeric token {value:?}"
                )
            }
            Self::UnknownFloorToken(value) => {
                write!(f, "LCE floor token {value:?} has no configured mapping")
            }
        }
    }
}

impl Error for LceMappingError {}

/// Parses one original synthetic fixture frame.
///
/// The fixture format mirrors the documented LCE row/column families but is not a
/// byte-for-byte controller transcript or a terminal-emulation protocol.
pub fn parse_recorded_frame(input: &str) -> Result<RecordedLceFrame, LceParseError> {
    if input.len() > MAX_FRAME_BYTES {
        return Err(LceParseError::FrameTooLarge { bytes: input.len() });
    }
    let lines = input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Err(LceParseError::Empty);
    }
    if lines.len() > MAX_LINES {
        return Err(LceParseError::TooManyLines { lines: lines.len() });
    }
    for (index, line) in lines.iter().enumerate() {
        if line.len() > MAX_LINE_BYTES {
            return Err(LceParseError::LineTooLarge {
                line: index + 1,
                bytes: line.len(),
            });
        }
        if !line.is_ascii() {
            return Err(LceParseError::NonAscii { line: index + 1 });
        }
        if line
            .split_ascii_whitespace()
            .any(|token| token.len() > MAX_TOKEN_BYTES)
        {
            return Err(LceParseError::TokenTooLarge { line: index + 1 });
        }
    }

    let family = lines[0].split_ascii_whitespace().next().unwrap_or_default();
    match family {
        "STA" => parse_sta(&lines).map(RecordedLceFrame::Sta),
        "CMD" => parse_cmd(&lines).map(RecordedLceFrame::Cmd),
        "MDE" => parse_mde(&lines).map(RecordedLceFrame::Mde),
        "GRP" => parse_group(&lines).map(RecordedLceFrame::Grp),
        other => Err(LceParseError::UnsupportedFamily(other.to_owned())),
    }
}

fn parse_sta(lines: &[&str]) -> Result<LceStaRecord, LceParseError> {
    require_three_lines(lines, "STA")?;
    let native_lift = parse_lift_header(lines[0], "STA", "STATUS_OF_LIFT")?;
    require_columns(lines[1], &STA_FIELDS, "STA")?;
    let values = values(lines[2], STA_FIELDS.len(), "STA")?;
    Ok(LceStaRecord {
        native_lift,
        advance_position: values[0].to_owned(),
        speed: values[1].to_owned(),
        direction: values[2].to_owned(),
        service: values[3].to_owned(),
        fault_id: values[4].to_owned(),
        door: values[5].to_owned(),
        level_number: values[6].to_owned(),
        at_door_zone: values[7].to_owned(),
        terminal_limit: values[8].to_owned(),
        speed_plate_a: values[9].to_owned(),
        speed_plate_b: values[10].to_owned(),
        load: values[11].to_owned(),
        correction_drive: values[12].to_owned(),
        car_call_preference: values[13].to_owned(),
        at_level: values[14].to_owned(),
    })
}

fn parse_cmd(lines: &[&str]) -> Result<LceCmdRecord, LceParseError> {
    require_three_lines(lines, "CMD")?;
    let native_lift = parse_lift_header(lines[0], "CMD", "COMMAND_OF_LIFT")?;
    require_columns(lines[1], &CMD_FIELDS, "CMD")?;
    let values = values(lines[2], CMD_FIELDS.len(), "CMD")?;
    Ok(LceCmdRecord {
        native_lift,
        destination: values[0].to_owned(),
        planned_direction: values[1].to_owned(),
        door_side: values[2].to_owned(),
        opening_width: values[3].to_owned(),
        open_time: values[4].to_owned(),
        delay: values[5].to_owned(),
        closing_mode: values[6].to_owned(),
    })
}

fn parse_mde(lines: &[&str]) -> Result<LceMdeRecord, LceParseError> {
    require_three_lines(lines, "MDE")?;
    let native_lift = parse_lift_header(lines[0], "MDE", "MODE_OF_LIFT")?;
    require_columns(lines[1], &MDE_FIELDS, "MDE")?;
    let values = values(lines[2], MDE_FIELDS.len(), "MDE")?;
    Ok(LceMdeRecord {
        native_lift,
        lift_mode: values[0].to_owned(),
        single_group: values[1].to_owned(),
        record_landings: values[2].to_owned(),
        record_car_calls: values[3].to_owned(),
        fault_id: values[4].to_owned(),
        door_state: values[5].to_owned(),
        advance_position: values[6].to_owned(),
    })
}

fn parse_group(lines: &[&str]) -> Result<LceGroupRecord, LceParseError> {
    if lines.len() < 3 {
        return Err(LceParseError::InvalidHeader { family: "GRP" });
    }
    let header = lines[0].split_ascii_whitespace().collect::<Vec<_>>();
    if header.len() != 5 || header[0] != "GRP" || header[1] != "GROUP" || header[3] != "TRAFFIC" {
        return Err(LceParseError::InvalidHeader { family: "GRP" });
    }
    require_columns(lines[1], &GRP_FIELDS, "GRP")?;
    let row_count = lines.len() - 2;
    if row_count > MAX_GROUP_ROWS {
        return Err(LceParseError::TooManyGroupRows { rows: row_count });
    }
    let rows = lines[2..]
        .iter()
        .map(|line| {
            let values = values(line, GRP_FIELDS.len(), "GRP")?;
            Ok(LceGroupRow {
                native_lift: values[0].to_owned(),
                serving_mode: values[1].to_owned(),
                destination: values[2].to_owned(),
                current_floor: values[3].to_owned(),
            })
        })
        .collect::<Result<Vec<_>, LceParseError>>()?;
    Ok(LceGroupRecord {
        native_group: header[2].to_owned(),
        traffic: header[4].to_owned(),
        rows,
    })
}

fn require_three_lines(lines: &[&str], family: &'static str) -> Result<(), LceParseError> {
    if lines.len() == 3 {
        Ok(())
    } else {
        Err(LceParseError::InvalidHeader { family })
    }
}

fn parse_lift_header(
    line: &str,
    family: &'static str,
    title: &str,
) -> Result<String, LceParseError> {
    let parts = line.split_ascii_whitespace().collect::<Vec<_>>();
    if parts.len() == 3 && parts[0] == family && parts[1] == title {
        Ok(parts[2].to_owned())
    } else {
        Err(LceParseError::InvalidHeader { family })
    }
}

fn require_columns(
    line: &str,
    expected: &[&str],
    family: &'static str,
) -> Result<(), LceParseError> {
    if line.split_ascii_whitespace().eq(expected.iter().copied()) {
        Ok(())
    } else {
        Err(LceParseError::InvalidColumns { family })
    }
}

fn values<'a>(
    line: &'a str,
    expected: usize,
    family: &'static str,
) -> Result<Vec<&'a str>, LceParseError> {
    let values = line.split_ascii_whitespace().collect::<Vec<_>>();
    if values.len() == expected {
        Ok(values)
    } else {
        Err(LceParseError::InvalidValueCount {
            family,
            expected,
            actual: values.len(),
        })
    }
}

/// Maps one STA record to incremental canonical evidence.
///
/// `LVL_NO` becomes a floor reference only when stopped, in a door zone and at
/// an explicitly documented leveling state. `ADV_POS` remains source context.
pub fn map_sta_record(
    record: &LceStaRecord,
    car: CarId,
    floors: &BTreeMap<String, Floor>,
    provenance: ObservationProvenance,
) -> Result<MappedLceUpdate, LceMappingError> {
    let speed = numeric("SPD", &record.speed)?;
    let direction = numeric("DIR", &record.direction)?;
    let at_door_zone = numeric("AT_DZN", &record.at_door_zone)?;
    let motion = map_motion(speed, direction);
    let stopped = motion == MotionEvidence::Known(MotionState::Stopped);
    let position = if stopped && at_door_zone > 0 && is_documented_at_level(&record.at_level) {
        PositionEvidence::FloorReference(mapped_floor(&record.level_number, floors)?)
    } else {
        PositionEvidence::Unknown
    };
    let door = map_door(numeric("DOOR", &record.door)?);

    Ok(MappedLceUpdate {
        update: CarObservationUpdate {
            provenance,
            car,
            position: Some(position),
            motion: Some(motion),
            door: Some(door),
            mode: None,
            reported_destination: None,
        },
        source_context: LceSourceContext::Sta(record.clone()),
    })
}

/// Maps the documented CMD destination as reported observation, never as an Intent.
pub fn map_cmd_record(
    record: &LceCmdRecord,
    car: CarId,
    floors: &BTreeMap<String, Floor>,
    provenance: ObservationProvenance,
) -> Result<MappedLceUpdate, LceMappingError> {
    let destination = map_destination(&record.destination, floors)?;
    Ok(MappedLceUpdate {
        update: CarObservationUpdate {
            provenance,
            car,
            position: None,
            motion: None,
            door: None,
            mode: None,
            reported_destination: Some(destination),
        },
        source_context: LceSourceContext::Cmd(record.clone()),
    })
}

/// Maps only strongly equivalent MDE modes. Documented service, synchronization,
/// priority, parking and test modes remain source-specific.
pub fn map_mde_record(
    record: &LceMdeRecord,
    car: CarId,
    provenance: ObservationProvenance,
) -> Result<MappedLceUpdate, LceMappingError> {
    let raw_mode = numeric("LIFT_MODE", &record.lift_mode)?;
    let mode = match raw_mode {
        0 => OperatingModeEvidence::Known(OperatingMode::Normal),
        3 => OperatingModeEvidence::Known(OperatingMode::FireService),
        10 => OperatingModeEvidence::Known(OperatingMode::OutOfService),
        1 | 2 | 4 | 5 | 6 | 8 | 9 | 11 | 12 | 15 | 16 | 17 | 18 => {
            OperatingModeEvidence::SourceSpecific
        }
        _ => OperatingModeEvidence::Unknown,
    };
    let door = map_door(numeric("DOOR_STATE", &record.door_state)?);
    Ok(MappedLceUpdate {
        update: CarObservationUpdate {
            provenance,
            car,
            position: None,
            motion: None,
            door: Some(door),
            mode: Some(mode),
            reported_destination: None,
        },
        source_context: LceSourceContext::Mde(record.clone()),
    })
}

/// Maps one GRP row only when its native lift identifier has an explicit logical binding.
/// The caller assigns a distinct source-global sequence to each emitted canonical update.
pub fn map_group_row(
    group: &LceGroupRecord,
    row: &LceGroupRow,
    bindings: &BTreeMap<String, CarId>,
    floors: &BTreeMap<String, Floor>,
    provenance: ObservationProvenance,
) -> Result<Option<MappedLceUpdate>, LceMappingError> {
    let Some(car) = bindings.get(&row.native_lift).cloned() else {
        return Ok(None);
    };
    let position = if is_ambiguous_floor(&row.current_floor) {
        PositionEvidence::Unknown
    } else {
        PositionEvidence::FloorReference(mapped_floor(&row.current_floor, floors)?)
    };
    let destination = map_destination(&row.destination, floors)?;
    Ok(Some(MappedLceUpdate {
        update: CarObservationUpdate {
            provenance,
            car,
            position: Some(position),
            motion: None,
            door: None,
            mode: None,
            reported_destination: Some(destination),
        },
        source_context: LceSourceContext::Group {
            native_group: group.native_group.clone(),
            traffic: group.traffic.clone(),
            row: row.clone(),
        },
    }))
}

fn map_motion(speed: u16, direction: u16) -> MotionEvidence {
    match (speed, direction) {
        (0, 0) => MotionEvidence::Known(MotionState::Stopped),
        (1 | 2, 1) => MotionEvidence::Known(MotionState::Moving(Direction::Down)),
        (1 | 2, 2) => MotionEvidence::Known(MotionState::Moving(Direction::Up)),
        _ => MotionEvidence::Unknown,
    }
}

fn map_door(value: u16) -> DoorEvidence {
    match value {
        1 | 5 => DoorEvidence::Known(DoorState::Closed),
        2 => DoorEvidence::Known(DoorState::Opening),
        3 => DoorEvidence::Known(DoorState::Open),
        4 => DoorEvidence::Known(DoorState::Closing),
        _ => DoorEvidence::Unknown,
    }
}

fn map_destination(
    value: &str,
    floors: &BTreeMap<String, Floor>,
) -> Result<ReportedDestination, LceMappingError> {
    if is_ambiguous_floor(value) {
        Ok(ReportedDestination::Unknown)
    } else {
        mapped_floor(value, floors).map(ReportedDestination::Known)
    }
}

fn mapped_floor(value: &str, floors: &BTreeMap<String, Floor>) -> Result<Floor, LceMappingError> {
    floors
        .get(value)
        .copied()
        .ok_or_else(|| LceMappingError::UnknownFloorToken(value.to_owned()))
}

fn is_ambiguous_floor(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "FF" | "EE" | "C0" | "C1" | "C2" | "?" | "UNKNOWN"
    )
}

fn is_documented_at_level(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "0F" | "0FH" | "F0" | "F0H" | "FF" | "FFH" | "0FFH"
    )
}

fn numeric(field: &'static str, value: &str) -> Result<u16, LceMappingError> {
    let upper = value.to_ascii_uppercase();
    let result = if let Some(hex) = upper.strip_suffix('H') {
        u16::from_str_radix(hex, 16)
    } else {
        upper.parse::<u16>()
    };
    result.map_err(|_| LceMappingError::InvalidNumericToken {
        field,
        value: value.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_engine::{
        Engine, ObservationBindingRole, ObservationIngestError, ObservationIngestResult,
    };
    use liftassistant_model::{
        BuildingTopology, CarLocation, ObservationSequence, ObservationSessionId,
        ObservationSourceId,
    };

    const STA_STOPPED: &str = include_str!("../tests/fixtures/sta_stopped.txt");
    const CMD_KNOWN: &str = include_str!("../tests/fixtures/cmd_known_destination.txt");
    const MDE_NORMAL: &str = include_str!("../tests/fixtures/mde_normal.txt");

    fn floors() -> BTreeMap<String, Floor> {
        [("01", 1), ("02", 2), ("03", 3), ("04", 4), ("05", 5)]
            .into_iter()
            .map(|(native, logical)| (native.to_owned(), Floor::new(logical)))
            .collect()
    }

    fn provenance(sequence: u64) -> ObservationProvenance {
        ObservationProvenance {
            source: ObservationSourceId::new("fixture/lce-terminal"),
            session: ObservationSessionId::new("lce-fixture-session-A"),
            sequence: ObservationSequence::new(sequence),
        }
    }

    fn sta(input: &str) -> LceStaRecord {
        let RecordedLceFrame::Sta(record) =
            parse_recorded_frame(input).expect("STA fixture should parse")
        else {
            panic!("fixture did not produce STA")
        };
        record
    }

    fn cmd(input: &str) -> LceCmdRecord {
        let RecordedLceFrame::Cmd(record) =
            parse_recorded_frame(input).expect("CMD fixture should parse")
        else {
            panic!("fixture did not produce CMD")
        };
        record
    }

    fn mde(input: &str) -> LceMdeRecord {
        let RecordedLceFrame::Mde(record) =
            parse_recorded_frame(input).expect("MDE fixture should parse")
        else {
            panic!("fixture did not produce MDE")
        };
        record
    }

    #[test]
    fn stationary_level_requires_joint_leveling_evidence() {
        let record = sta(STA_STOPPED);
        let mapped = map_sta_record(&record, CarId::new("logical-A"), &floors(), provenance(1))
            .expect("configured floor should map");
        assert_eq!(
            mapped.update.position,
            Some(PositionEvidence::FloorReference(Floor::new(3)))
        );
        assert_eq!(
            mapped.update.motion,
            Some(MotionEvidence::Known(MotionState::Stopped))
        );
        assert_eq!(
            mapped.update.door,
            Some(DoorEvidence::Known(DoorState::Closed))
        );

        let no_level = STA_STOPPED.replace("0 3 3 0 0 0 FF", "0 3 3 0 0 0 0");
        let no_level = map_sta_record(
            &sta(&no_level),
            CarId::new("logical-A"),
            &floors(),
            provenance(2),
        )
        .expect("lack of leveling evidence is representable");
        assert_eq!(no_level.update.position, Some(PositionEvidence::Unknown));
        assert_eq!(
            no_level.source_context,
            LceSourceContext::Sta(sta(&no_level_source()))
        );
    }

    fn no_level_source() -> String {
        STA_STOPPED.replace("0 3 3 0 0 0 FF", "0 3 3 0 0 0 0")
    }

    #[test]
    fn motion_requires_consistent_speed_and_direction() {
        let moving = STA_STOPPED.replace("05 0 0", "05 2 2");
        let mapped = map_sta_record(
            &sta(&moving),
            CarId::new("logical-A"),
            &floors(),
            provenance(1),
        )
        .expect("moving record should map");
        assert_eq!(
            mapped.update.motion,
            Some(MotionEvidence::Known(MotionState::Moving(Direction::Up)))
        );
        assert_eq!(mapped.update.position, Some(PositionEvidence::Unknown));

        let contradictory = STA_STOPPED.replace("05 0 0", "05 0 2");
        let mapped = map_sta_record(
            &sta(&contradictory),
            CarId::new("logical-A"),
            &floors(),
            provenance(2),
        )
        .expect("contradictory numeric states remain representable");
        assert_eq!(mapped.update.motion, Some(MotionEvidence::Unknown));
    }

    #[test]
    fn advance_position_fault_and_compound_door_do_not_invent_canonical_state() {
        let ambiguous = STA_STOPPED
            .replace("05 0 0", "04 0 0")
            .replace("0 1 03", "64 2563 03")
            .replace("0 3 3 0 0 0 FF", "0 3 3 0 0 0 0");
        let mapped = map_sta_record(
            &sta(&ambiguous),
            CarId::new("logical-A"),
            &floors(),
            provenance(1),
        )
        .expect("documented numeric context should map conservatively");
        assert_eq!(mapped.update.position, Some(PositionEvidence::Unknown));
        assert_eq!(mapped.update.door, Some(DoorEvidence::Unknown));
        assert_eq!(mapped.update.mode, None);
        let LceSourceContext::Sta(context) = mapped.source_context else {
            panic!("STA context should remain available")
        };
        assert_eq!(context.advance_position, "04");
        assert_eq!(context.fault_id, "64");
    }

    #[test]
    fn mde_maps_only_strong_mode_equivalents() {
        let cases = [
            ("0", OperatingModeEvidence::Known(OperatingMode::Normal)),
            (
                "3",
                OperatingModeEvidence::Known(OperatingMode::FireService),
            ),
            (
                "10",
                OperatingModeEvidence::Known(OperatingMode::OutOfService),
            ),
            ("1", OperatingModeEvidence::SourceSpecific),
            ("8", OperatingModeEvidence::SourceSpecific),
            ("99", OperatingModeEvidence::Unknown),
        ];
        for (raw, expected) in cases {
            let fixture = MDE_NORMAL.replacen("0 1 1 1", &format!("{raw} 1 1 1"), 1);
            let mapped = map_mde_record(&mde(&fixture), CarId::new("logical-A"), provenance(1))
                .expect("numeric MDE mode should remain representable");
            assert_eq!(mapped.update.mode, Some(expected));
        }
    }

    #[test]
    fn cmd_destination_is_reported_evidence_with_explicit_floor_mapping() {
        let known = map_cmd_record(
            &cmd(CMD_KNOWN),
            CarId::new("logical-A"),
            &floors(),
            provenance(1),
        )
        .expect("configured destination should map");
        assert_eq!(
            known.update.reported_destination,
            Some(ReportedDestination::Known(Floor::new(4)))
        );

        let unknown = CMD_KNOWN.replace("04 2 0", "FF 2 0");
        let unknown = map_cmd_record(
            &cmd(&unknown),
            CarId::new("logical-A"),
            &floors(),
            provenance(2),
        )
        .expect("documented sentinel should remain unknown");
        assert_eq!(
            unknown.update.reported_destination,
            Some(ReportedDestination::Unknown)
        );

        let unmapped = CMD_KNOWN.replace("04 2 0", "09 2 0");
        assert_eq!(
            map_cmd_record(
                &cmd(&unmapped),
                CarId::new("logical-A"),
                &floors(),
                provenance(3),
            ),
            Err(LceMappingError::UnknownFloorToken("09".into()))
        );
    }

    #[test]
    fn group_rows_require_explicit_native_to_logical_bindings() {
        let fixture = include_str!("../tests/fixtures/group_two_lifts.txt");
        let RecordedLceFrame::Grp(group) =
            parse_recorded_frame(fixture).expect("GRP fixture should parse")
        else {
            panic!("fixture did not produce GRP")
        };
        let bindings = [("1".to_owned(), CarId::new("logical-A"))]
            .into_iter()
            .collect();
        let mapped = map_group_row(&group, &group.rows[0], &bindings, &floors(), provenance(1))
            .expect("configured row should map")
            .expect("configured row should emit an update");
        assert_eq!(mapped.update.car, CarId::new("logical-A"));
        assert_eq!(
            mapped.update.position,
            Some(PositionEvidence::FloorReference(Floor::new(3)))
        );
        assert_eq!(
            map_group_row(&group, &group.rows[1], &bindings, &floors(), provenance(2),)
                .expect("unconfigured row is not a parse failure"),
            None
        );
    }

    #[test]
    fn group_rows_keep_one_source_global_sequence_across_cars() {
        let fixture = include_str!("../tests/fixtures/group_two_lifts.txt");
        let RecordedLceFrame::Grp(group) =
            parse_recorded_frame(fixture).expect("GRP fixture should parse")
        else {
            panic!("fixture did not produce GRP")
        };
        let source = provenance(1).source;
        let session = provenance(1).session;
        let car_a = CarId::new("logical-A");
        let car_b = CarId::new("logical-B");
        let bindings = [
            ("1".to_owned(), car_a.clone()),
            ("2".to_owned(), car_b.clone()),
        ]
        .into_iter()
        .collect();
        let topology = BuildingTopology::new((1..=5).map(Floor::new)).expect("valid topology");
        let mut engine = Engine::new(topology);
        for car in [&car_a, &car_b] {
            engine
                .bind_observation(
                    source.clone(),
                    car.clone(),
                    ObservationBindingRole::Operational,
                )
                .expect("explicit group-row binding should succeed");
        }
        engine
            .begin_source_session(source, session)
            .expect("explicit group session should begin");

        for (sequence, row) in [1_u64, 2].into_iter().zip(&group.rows) {
            let mapped = map_group_row(&group, row, &bindings, &floors(), provenance(sequence))
                .expect("configured group row should map")
                .expect("configured group row should emit");
            assert!(matches!(
                engine.ingest_observation_update(mapped.update),
                ObservationIngestResult::Incomplete(_)
            ));
        }
        let newer_a = map_group_row(&group, &group.rows[0], &bindings, &floors(), provenance(3))
            .expect("newer A row should map")
            .expect("configured A row should emit");
        assert!(matches!(
            engine.ingest_observation_update(newer_a.update),
            ObservationIngestResult::Incomplete(_)
        ));
    }

    #[test]
    fn parser_is_bounded_and_fails_truncation_without_fabricating_fields() {
        let truncated = "STA STATUS_OF_LIFT 02\nADV_POS SPD DIR SRV FLT_ID DOOR LVL_NO AT_DZN TERM_LIM SPDA_VID SPDB_VID LOAD CORR_DRV CC_PREFF AT_LEVEL\n05 0";
        assert_eq!(
            parse_recorded_frame(truncated),
            Err(LceParseError::InvalidValueCount {
                family: "STA",
                expected: 15,
                actual: 2,
            })
        );
        assert!(matches!(
            parse_recorded_frame(&"X".repeat(MAX_FRAME_BYTES + 1)),
            Err(LceParseError::FrameTooLarge { .. })
        ));
        assert!(matches!(
            parse_recorded_frame("CAL ACTIVE_CALLS\nPOS FLOOR 01\nCALL 1"),
            Err(LceParseError::UnsupportedFamily(_))
        ));
        assert!(matches!(
            parse_recorded_frame("N NODE_TABLE\nNODE NAME STATE FLOOR SIDE"),
            Err(LceParseError::UnsupportedFamily(_))
        ));
    }

    #[test]
    fn parser_rejects_deterministic_ascii_noise_without_panicking() {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 _\n\r\t?";
        for seed in 0..2_048_u32 {
            let length = (seed as usize * 37) % 257;
            let mut state = seed.wrapping_add(1);
            let mut fixture = String::with_capacity(length);
            for _ in 0..length {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                fixture.push(ALPHABET[state as usize % ALPHABET.len()] as char);
            }
            let _ = parse_recorded_frame(&fixture);
        }
    }

    #[test]
    fn recorded_families_assemble_through_engine_without_field_fusion() {
        let topology = BuildingTopology::new((1..=5).map(Floor::new)).expect("valid topology");
        let mut engine = Engine::new(topology);
        let source = provenance(1).source;
        let session = provenance(1).session;
        let car = CarId::new("logical-A");
        engine
            .bind_observation(
                source.clone(),
                car.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("explicit binding should succeed");
        engine
            .begin_source_session(source.clone(), session.clone())
            .expect("explicit session should begin");

        let sta = map_sta_record(&sta(STA_STOPPED), car.clone(), &floors(), provenance(1))
            .expect("STA should map");
        assert!(matches!(
            engine.ingest_observation_update(sta.update),
            ObservationIngestResult::Incomplete(_)
        ));
        assert!(engine.car_states().is_empty());

        let mapped_cmd = map_cmd_record(&cmd(CMD_KNOWN), car.clone(), &floors(), provenance(2))
            .expect("CMD should map");
        assert!(matches!(
            engine.ingest_observation_update(mapped_cmd.update),
            ObservationIngestResult::Incomplete(_)
        ));

        let mapped_mde =
            map_mde_record(&mde(MDE_NORMAL), car.clone(), provenance(3)).expect("MDE should map");
        assert!(matches!(
            engine.ingest_observation_update(mapped_mde.update),
            ObservationIngestResult::Resolution(_)
        ));
        assert_eq!(engine.car_states().len(), 1);
        let state = engine
            .operational_car_state(&car)
            .expect("state should resolve");
        assert_eq!(state.location, CarLocation::Floor(Floor::new(3)));
        assert_eq!(
            state.reported_destination,
            ReportedDestination::Known(Floor::new(4))
        );

        let unknown_mde = MDE_NORMAL
            .replacen("0 1 1 1", "99 1 1 1", 1)
            .replace("0 1 05", "0 6 05");
        let update = map_mde_record(&mde(&unknown_mde), car.clone(), provenance(4))
            .expect("unknown evidence should map");
        engine.ingest_observation_update(update.update);
        assert!(engine.operational_car_state(&car).is_none());

        let restored = map_mde_record(&mde(MDE_NORMAL), car.clone(), provenance(5))
            .expect("known MDE should map");
        engine.ingest_observation_update(restored.update);
        let state = engine
            .operational_car_state(&car)
            .expect("omitted fields should be preserved during recovery");
        assert_eq!(
            state.reported_destination,
            ReportedDestination::Known(Floor::new(4))
        );

        let unbound = map_cmd_record(
            &cmd(CMD_KNOWN),
            CarId::new("native-02-is-not-a-car-id"),
            &floors(),
            ObservationProvenance {
                source: source.clone(),
                session: session.clone(),
                sequence: ObservationSequence::new(6),
            },
        )
        .expect("mapping itself uses explicit caller identity");
        assert!(matches!(
            engine.ingest_observation_update(unbound.update),
            ObservationIngestResult::Rejected(ObservationIngestError::UnboundObservation { .. })
        ));

        engine
            .begin_source_session(
                source.clone(),
                ObservationSessionId::new("lce-fixture-session-B"),
            )
            .expect("fresh session should begin");
        assert!(engine.operational_car_state(&car).is_none());
        let delayed = map_cmd_record(
            &cmd(CMD_KNOWN),
            car,
            &floors(),
            ObservationProvenance {
                source,
                session,
                sequence: ObservationSequence::new(7),
            },
        )
        .expect("delayed fixture still maps locally");
        assert!(matches!(
            engine.ingest_observation_update(delayed.update),
            ObservationIngestResult::Rejected(ObservationIngestError::InactiveSourceSession { .. })
        ));
    }
}
