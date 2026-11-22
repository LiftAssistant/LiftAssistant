#[cfg(target_os = "linux")]
#[path = "support/passive_linux.rs"]
mod passive_linux;

#[cfg(target_os = "linux")]
use std::{env, error::Error, fs::OpenOptions, io::Write, path::PathBuf, process::ExitCode};

#[cfg(target_os = "linux")]
use liftassistant_lce::{
    LceCaptureOrigin, MAX_PASSIVE_CAPTURE_DURATION_SECONDS, MAX_SERIALIZED_PASSIVE_CAPTURE_BYTES,
};
#[cfg(target_os = "linux")]
use passive_linux::{PassiveLinuxRequest, acquire, fresh_session_id};

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct Arguments {
    device: PathBuf,
    output: PathBuf,
    duration_seconds: u16,
}

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("LCE software-RX-only capture failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("lce_passive_serial_capture is available only on Linux");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn run() -> Result<(), Box<dyn Error>> {
    let arguments = parse_arguments(env::args().skip(1))?;
    let session_id = fresh_session_id()?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&arguments.output)?;
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
    output.write_all(&bytes)?;
    output.sync_all()?;
    eprintln!(
        "captured {} TTY_DELIVERED_RX entries ({} bytes), end={:?}, sha256={}",
        capture.entries().len(),
        capture
            .entries()
            .iter()
            .map(|entry| entry.bytes().len())
            .sum::<usize>(),
        capture.end_reason(),
        capture.sha256()
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn parse_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<Arguments, Box<dyn Error>> {
    let mut device = None;
    let mut output = None;
    let mut duration_seconds = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {argument}"))?;
        match argument.as_str() {
            "--device" => device = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--duration-seconds" => {
                duration_seconds = Some(value.parse::<u16>()?);
            }
            _ => return Err(format!("unsupported argument {argument:?}").into()),
        }
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
        device,
        output: output.ok_or("missing --output")?,
        duration_seconds,
    })
}
