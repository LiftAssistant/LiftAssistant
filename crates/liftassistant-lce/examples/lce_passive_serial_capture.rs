#[cfg(target_os = "linux")]
#[path = "support/passive_linux.rs"]
mod passive_linux;

#[cfg(target_os = "linux")]
#[path = "support/passive_runner.rs"]
mod passive_runner;

#[cfg(target_os = "linux")]
use std::{env, process::ExitCode};

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    match passive_runner::run_with_arguments(env::args().skip(1)) {
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
