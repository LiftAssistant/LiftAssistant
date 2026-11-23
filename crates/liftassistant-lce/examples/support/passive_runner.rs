use std::{
    error::Error,
    fs::{File, OpenOptions, remove_file},
    io::Write,
    path::{Path, PathBuf},
};

use liftassistant_lce::{
    LceCaptureOrigin, MAX_PASSIVE_CAPTURE_DURATION_SECONDS, MAX_SERIALIZED_PASSIVE_CAPTURE_BYTES,
};

use crate::passive_linux::{PassiveLinuxRequest, acquire, fresh_session_id};

const SOFTWARE_RX_ONLY_ASSURANCE: &str = "software-rx-only";
const HISTORICAL_9600_8N1_PROFILE: &str = "historical-9600-8n1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunnerMode {
    Execute,
    PlanOnly,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Arguments {
    mode: RunnerMode,
    device: PathBuf,
    output: PathBuf,
    duration_seconds: u16,
}

pub(crate) fn run_with_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<(), Box<dyn Error>> {
    let arguments = parse_arguments(arguments)?;
    if arguments.mode == RunnerMode::PlanOnly {
        println!("{}", plan_summary(&arguments));
        return Ok(());
    }

    eprintln!("{}", execution_banner(&arguments));
    let mut output = ReservedOutput::create(&arguments.output)?;
    let session_id = fresh_session_id()?;
    let capture = acquire(PassiveLinuxRequest {
        device: &arguments.device,
        session_id: &session_id,
        origin: LceCaptureOrigin::FieldEvidenceCandidate,
        duration_seconds: arguments.duration_seconds,
    })?;
    let bytes = capture.to_json_bytes()?;
    if bytes.len() > MAX_SERIALIZED_PASSIVE_CAPTURE_BYTES {
        return Err("serialized artifact exceeds the passive-capture bound".into());
    }
    output.file_mut().write_all(&bytes)?;
    output.file_mut().sync_all()?;
    output.commit();
    eprintln!(
        "captured {} TTY_DELIVERED_RX entries ({} bytes), outcome={:?}, end={:?}, sha256={}",
        capture.entries().len(),
        capture.total_rx_bytes(),
        capture.outcome(),
        capture.end_reason(),
        capture.sha256()
    );
    Ok(())
}

pub(crate) fn parse_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<Arguments, Box<dyn Error>> {
    let mut mode = RunnerMode::Execute;
    let mut assurance = None;
    let mut profile = None;
    let mut device = None;
    let mut output = None;
    let mut duration_seconds = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--plan" => {
                if mode == RunnerMode::PlanOnly {
                    return Err(format!("duplicate argument {argument:?}").into());
                }
                mode = RunnerMode::PlanOnly;
            }
            "--assurance" => {
                assurance = Some(singleton_value(
                    &mut arguments,
                    &argument,
                    assurance.is_some(),
                )?);
            }
            "--profile" => {
                profile = Some(singleton_value(
                    &mut arguments,
                    &argument,
                    profile.is_some(),
                )?);
            }
            "--device" => {
                device = Some(PathBuf::from(singleton_value(
                    &mut arguments,
                    &argument,
                    device.is_some(),
                )?));
            }
            "--output" => {
                output = Some(PathBuf::from(singleton_value(
                    &mut arguments,
                    &argument,
                    output.is_some(),
                )?));
            }
            "--duration-seconds" => {
                let value = singleton_value(&mut arguments, &argument, duration_seconds.is_some())?;
                duration_seconds = Some(value.parse::<u16>()?);
            }
            _ => return Err(format!("unsupported argument {argument:?}").into()),
        }
    }

    match assurance.as_deref() {
        Some(SOFTWARE_RX_ONLY_ASSURANCE) => {}
        Some(value) => return Err(format!("unsupported --assurance value {value:?}").into()),
        None => return Err("missing --assurance software-rx-only".into()),
    }
    match profile.as_deref() {
        Some(HISTORICAL_9600_8N1_PROFILE) => {}
        Some(value) => return Err(format!("unsupported --profile value {value:?}").into()),
        None => return Err("missing --profile historical-9600-8n1".into()),
    }

    let duration_seconds = duration_seconds.ok_or("missing --duration-seconds")?;
    if duration_seconds == 0 || duration_seconds > MAX_PASSIVE_CAPTURE_DURATION_SECONDS {
        return Err(format!(
            "--duration-seconds must be in 1..={MAX_PASSIVE_CAPTURE_DURATION_SECONDS}"
        )
        .into());
    }
    let device = device.ok_or("missing --device")?;
    if !device.is_absolute() {
        return Err("--device must be an explicit absolute path".into());
    }

    Ok(Arguments {
        mode,
        device,
        output: output.ok_or("missing --output")?,
        duration_seconds,
    })
}

fn singleton_value(
    arguments: &mut impl Iterator<Item = String>,
    argument: &str,
    already_seen: bool,
) -> Result<String, Box<dyn Error>> {
    if already_seen {
        return Err(format!("duplicate argument {argument:?}").into());
    }
    arguments
        .next()
        .ok_or_else(|| format!("missing value for {argument}").into())
}

pub(crate) fn plan_summary(arguments: &Arguments) -> String {
    format!(
        "mode: PLAN_ONLY\ndevice: {}\noutput: {}\nduration: {}s\nprofile: \
         {HISTORICAL_9600_8N1_PROFILE}\nassurance: SOFTWARE_RX_ONLY\nSOFTWARE_RX_ONLY=true\n\
         APPLICATION_PROTOCOL_TX_REQUESTED=false\nKERNEL_DRIVER_TX_FREE=false/unproven\n\
         PHYSICALLY_PASSIVE=false/unproven\nREAL_LCE_COMPATIBLE=unproven\n\
         real_hardware_opened: false",
        arguments.device.display(),
        arguments.output.display(),
        arguments.duration_seconds
    )
}

fn execution_banner(arguments: &Arguments) -> String {
    format!(
        "LCE software-RX-only capture\ndevice: {}\noutput: {}\nduration: {}s\nprofile: \
         {HISTORICAL_9600_8N1_PROFILE}\nSOFTWARE_RX_ONLY=true\n\
         APPLICATION_PROTOCOL_TX_REQUESTED=false\nKERNEL_DRIVER_TX_FREE=false/unproven\n\
         PHYSICALLY_PASSIVE=false/unproven\nREAL_LCE_COMPATIBLE=unproven",
        arguments.device.display(),
        arguments.output.display(),
        arguments.duration_seconds
    )
}

struct ReservedOutput {
    path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl ReservedOutput {
    fn create(path: &Path) -> std::io::Result<Self> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Self {
            path: path.to_owned(),
            file: Some(file),
            committed: false,
        })
    }

    fn file_mut(&mut self) -> &mut File {
        self.file
            .as_mut()
            .expect("reserved output file exists until commit")
    }

    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for ReservedOutput {
    fn drop(&mut self) {
        if self.committed {
            return;
        }

        drop(self.file.take());
        if let Err(error) = remove_file(&self.path) {
            eprintln!(
                "warning: failed to remove incomplete reserved output {}: {error}",
                self.path.display()
            );
        }
    }
}
