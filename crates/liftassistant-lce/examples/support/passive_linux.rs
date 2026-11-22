#![cfg_attr(test, allow(dead_code))]

use std::{
    error::Error,
    ffi::OsStr,
    fmt,
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
    time::{Duration, Instant},
};

use liftassistant_lce::{
    FinalizedLcePassiveCapture, LINUX_N_TTY_LINE_DISCIPLINE, LceCaptureOrigin,
    LceNormalizedTermios, LcePassiveCaptureEndReason, LcePassiveCaptureError,
    LcePassiveCaptureMetadata, LcePassiveCaptureRecorder, LcePassiveExclusiveModeStatus,
    LcePassiveInputQueueObservation, LcePassiveOpenPolicy, LcePassiveOpenedDevice,
    LceRuntimeTerminalProfile, MAX_PASSIVE_CAPTURE_ENTRY_BYTES, MAX_PASSIVE_DEVICE_PATH_BYTES,
};
use rustix::{
    event::{PollFd, PollFlags, poll},
    fs::{FileType, Mode, OFlags, fcntl_getfd, fcntl_getfl, fstat, major, minor, open},
    io::{self, Errno, FdFlags},
    rand::{GetRandomFlags, getrandom},
    termios::{
        ControlModes, InputModes, LocalModes, OptionalActions, OutputModes, SpecialCodeIndex,
        Termios, ioctl_tiocexcl, speed, tcgetattr, tcsetattr,
    },
};

#[cfg(test)]
use std::os::fd::{AsFd, BorrowedFd};

const POLL_SLICE_MILLIS: u64 = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassiveLinuxRequest<'a> {
    pub device: &'a Path,
    pub session_id: &'a str,
    pub origin: LceCaptureOrigin,
    pub duration_seconds: u16,
}

#[derive(Debug)]
pub enum PassiveLinuxError {
    InvalidDevicePath,
    KernelRandom(io::Errno),
    Open(io::Errno),
    ExclusiveRequest(io::Errno),
    ExclusiveNotObserved,
    DescriptorPolicy(io::Errno),
    DescriptorNotReadOnly,
    DescriptorFlagsNotApplied,
    DeviceMetadata(io::Errno),
    NotCharacterDevice,
    LineDisciplineRead(std::io::Error),
    UnsupportedLineDiscipline { observed: i32 },
    TermiosRead(io::Errno),
    TermiosConfigure(io::Errno),
    TermiosApply(io::Errno),
    Capture(LcePassiveCaptureError),
}

impl fmt::Display for PassiveLinuxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDevicePath => f.write_str(
                "explicit tty device path must be absolute, nonempty, and contain no NUL",
            ),
            Self::KernelRandom(error) => write!(f, "Linux getrandom failed: {error}"),
            Self::Open(error) => write!(f, "read-only tty open failed: {error}"),
            Self::ExclusiveRequest(error) => write!(f, "TIOCEXCL failed: {error}"),
            Self::ExclusiveNotObserved => {
                f.write_str("TIOCEXCL succeeded but TIOCGEXCL reported exclusive mode disabled")
            }
            Self::DescriptorPolicy(error) => write!(f, "F_GETFL failed: {error}"),
            Self::DescriptorNotReadOnly => f.write_str("opened tty descriptor is not O_RDONLY"),
            Self::DescriptorFlagsNotApplied => {
                f.write_str("opened tty descriptor is missing O_NONBLOCK or FD_CLOEXEC")
            }
            Self::DeviceMetadata(error) => write!(f, "fstat on opened tty failed: {error}"),
            Self::NotCharacterDevice => {
                f.write_str("opened tty path did not resolve to a character device")
            }
            Self::LineDisciplineRead(error) => {
                write!(f, "TIOCGETD failed for opened tty: {error}")
            }
            Self::UnsupportedLineDiscipline { observed } => write!(
                f,
                "opened tty uses unsupported line discipline {observed}; expected Linux N_TTY (0)"
            ),
            Self::TermiosRead(error) => write!(f, "tcgetattr failed: {error}"),
            Self::TermiosConfigure(error) => {
                write!(f, "9600/8N1 termios configuration failed: {error}")
            }
            Self::TermiosApply(error) => write!(f, "tcsetattr failed: {error}"),
            Self::Capture(error) => write!(f, "passive capture failed: {error}"),
        }
    }
}

impl Error for PassiveLinuxError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::KernelRandom(error)
            | Self::Open(error)
            | Self::ExclusiveRequest(error)
            | Self::DescriptorPolicy(error)
            | Self::DeviceMetadata(error)
            | Self::TermiosRead(error)
            | Self::TermiosConfigure(error)
            | Self::TermiosApply(error) => Some(error),
            Self::Capture(error) => Some(error),
            Self::LineDisciplineRead(error) => Some(error),
            Self::InvalidDevicePath
            | Self::ExclusiveNotObserved
            | Self::DescriptorNotReadOnly
            | Self::DescriptorFlagsNotApplied
            | Self::NotCharacterDevice
            | Self::UnsupportedLineDiscipline { .. } => None,
        }
    }
}

impl From<LcePassiveCaptureError> for PassiveLinuxError {
    fn from(value: LcePassiveCaptureError) -> Self {
        Self::Capture(value)
    }
}

pub fn fresh_session_id() -> Result<String, PassiveLinuxError> {
    let mut random = [0_u8; 16];
    let mut filled = 0;
    while filled < random.len() {
        let count = getrandom(&mut random[filled..], GetRandomFlags::empty())
            .map_err(PassiveLinuxError::KernelRandom)?;
        if count == 0 {
            return Err(PassiveLinuxError::KernelRandom(Errno::IO));
        }
        filled += count;
    }
    let mut session_id = String::from("r3a-linux-");
    for byte in random {
        use std::fmt::Write as _;
        write!(&mut session_id, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(session_id)
}

pub fn acquire(
    request: PassiveLinuxRequest<'_>,
) -> Result<FinalizedLcePassiveCapture, PassiveLinuxError> {
    PassiveLinuxTty::open(request.device)?.capture(request)
}

#[derive(Debug)]
pub struct PassiveLinuxTty {
    fd: OwnedFd,
    requested_device_path_bytes: Vec<u8>,
    opened_device: LcePassiveOpenedDevice,
    before: LceNormalizedTermios,
    after: LceNormalizedTermios,
    exclusive_status: LcePassiveExclusiveModeStatus,
    line_discipline_before: i32,
    line_discipline_after: i32,
    pre_capture_input_queue: LcePassiveInputQueueObservation,
}

impl PassiveLinuxTty {
    pub fn open(path: &Path) -> Result<Self, PassiveLinuxError> {
        validate_path(path.as_os_str())?;
        let flags = OFlags::RDONLY | OFlags::NOCTTY | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let fd = open(path, flags, Mode::empty()).map_err(PassiveLinuxError::Open)?;

        // This is deliberately the first operation after open. TIOCEXCL only
        // fences later ordinary open attempts; it cannot prove there was no
        // prior opener.
        ioctl_tiocexcl(&fd).map_err(PassiveLinuxError::ExclusiveRequest)?;
        let exclusive_status = match query_exclusive(&fd) {
            Ok(true) => LcePassiveExclusiveModeStatus::ObservedEnabled,
            Ok(false) => return Err(PassiveLinuxError::ExclusiveNotObserved),
            Err(error) => LcePassiveExclusiveModeStatus::RequestedUnverified {
                raw_os_error: error.raw_os_error(),
            },
        };

        let device_stat = fstat(&fd).map_err(PassiveLinuxError::DeviceMetadata)?;
        if FileType::from_raw_mode(device_stat.st_mode) != FileType::CharacterDevice {
            return Err(PassiveLinuxError::NotCharacterDevice);
        }
        let opened_device = LcePassiveOpenedDevice::character_device(
            major(device_stat.st_rdev),
            minor(device_stat.st_rdev),
        );

        let observed_flags = fcntl_getfl(&fd).map_err(PassiveLinuxError::DescriptorPolicy)?;
        if observed_flags & OFlags::ACCMODE != OFlags::RDONLY {
            return Err(PassiveLinuxError::DescriptorNotReadOnly);
        }
        let descriptor_flags = fcntl_getfd(&fd).map_err(PassiveLinuxError::DescriptorPolicy)?;
        if !observed_flags.contains(OFlags::NONBLOCK)
            || !descriptor_flags.contains(FdFlags::CLOEXEC)
        {
            return Err(PassiveLinuxError::DescriptorFlagsNotApplied);
        }

        let line_discipline_before =
            query_line_discipline(&fd).map_err(PassiveLinuxError::LineDisciplineRead)?;
        require_n_tty(line_discipline_before)?;

        // Bytes can reach N_TTY between open and this policy change. Inherited
        // ECHO/IXOFF/other flags may already have transformed, consumed, or
        // generated output for them without an application `write(2)` call.
        let mut settings = tcgetattr(&fd).map_err(PassiveLinuxError::TermiosRead)?;
        let before = normalize_termios(&settings);
        apply_capture_policy(&mut settings)?;
        tcsetattr(&fd, OptionalActions::Now, &settings).map_err(PassiveLinuxError::TermiosApply)?;
        let observed = tcgetattr(&fd).map_err(PassiveLinuxError::TermiosRead)?;
        let after = normalize_termios(&observed);
        let line_discipline_after =
            query_line_discipline(&fd).map_err(PassiveLinuxError::LineDisciplineRead)?;
        require_n_tty(line_discipline_after)?;
        let pre_capture_input_queue = observe_input_queue(&fd);

        Ok(Self {
            fd,
            requested_device_path_bytes: path.as_os_str().as_bytes().to_vec(),
            opened_device,
            before,
            after,
            exclusive_status,
            line_discipline_before,
            line_discipline_after,
            pre_capture_input_queue,
        })
    }

    pub fn capture(
        self,
        request: PassiveLinuxRequest<'_>,
    ) -> Result<FinalizedLcePassiveCapture, PassiveLinuxError> {
        let exclusive_observed = matches!(
            self.exclusive_status,
            LcePassiveExclusiveModeStatus::ObservedEnabled
        );
        let mut recorder = LcePassiveCaptureRecorder::new(LcePassiveCaptureMetadata {
            session_id: request.session_id.to_owned(),
            origin: request.origin,
            requested_device_path_bytes: self.requested_device_path_bytes.clone(),
            opened_device: self.opened_device,
            requested_terminal_profile: LceRuntimeTerminalProfile::historical_9600_8n1(),
            requested_duration_seconds: request.duration_seconds,
            normalized_open_policy: LcePassiveOpenPolicy::linux_rx_only(exclusive_observed),
            normalized_termios_before: self.before.clone(),
            normalized_termios_after: self.after.clone(),
            exclusive_mode_status: self.exclusive_status,
            line_discipline_before: self.line_discipline_before,
            line_discipline_after: self.line_discipline_after,
            pre_capture_input_queue: self.pre_capture_input_queue,
        })?;
        let requested_duration = Duration::from_secs(u64::from(request.duration_seconds));
        let started = Instant::now();
        let mut buffer = vec![0_u8; MAX_PASSIVE_CAPTURE_ENTRY_BYTES];

        loop {
            let elapsed = started.elapsed();
            if elapsed >= requested_duration {
                return recorder
                    .finalize(
                        requested_duration,
                        LcePassiveCaptureEndReason::DurationElapsed,
                    )
                    .map_err(Into::into);
            }

            let remaining = requested_duration - elapsed;
            let timeout = remaining.min(Duration::from_millis(POLL_SLICE_MILLIS));
            let timeout_millis = i32::try_from(timeout.as_millis().max(1)).unwrap_or(i32::MAX);
            let mut poll_fds = [PollFd::new(
                &self.fd,
                PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
            )];
            if let Err(error) = poll(&mut poll_fds, timeout_millis) {
                let at = started.elapsed().min(requested_duration);
                return recorder
                    .finalize(
                        at,
                        LcePassiveCaptureEndReason::ReadError {
                            raw_os_error: Some(error.raw_os_error()),
                        },
                    )
                    .map_err(Into::into);
            }

            if started.elapsed() >= requested_duration {
                return recorder
                    .finalize(
                        requested_duration,
                        LcePassiveCaptureEndReason::DurationElapsed,
                    )
                    .map_err(Into::into);
            }
            let events = poll_fds[0].revents();
            if events.is_empty() {
                continue;
            }

            let at = started.elapsed().min(requested_duration);
            match rustix::io::read(&self.fd, &mut buffer) {
                Ok(0) => {
                    let reason = if events.contains(PollFlags::HUP) {
                        LcePassiveCaptureEndReason::PeerHangup
                    } else {
                        LcePassiveCaptureEndReason::EndOfFile
                    };
                    return recorder.finalize(at, reason).map_err(Into::into);
                }
                Ok(count) => {
                    if let Err(error) = recorder.record_tty_delivered_rx(at, &buffer[..count]) {
                        if let Some(reason) = capture_limit_reason(&error) {
                            return recorder.finalize(at, reason).map_err(Into::into);
                        }
                        return Err(error.into());
                    }
                }
                Err(Errno::AGAIN) if events.contains(PollFlags::HUP) => {
                    return recorder
                        .finalize(at, LcePassiveCaptureEndReason::PeerHangup)
                        .map_err(Into::into);
                }
                Err(Errno::AGAIN) | Err(Errno::INTR) => {}
                Err(error) => {
                    return recorder
                        .finalize(
                            at,
                            LcePassiveCaptureEndReason::ReadError {
                                raw_os_error: Some(error.raw_os_error()),
                            },
                        )
                        .map_err(Into::into);
                }
            }
        }
    }

    #[cfg(test)]
    pub fn borrowed_fd_for_test(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    #[cfg(test)]
    pub fn before_for_test(&self) -> &LceNormalizedTermios {
        &self.before
    }

    #[cfg(test)]
    pub fn after_for_test(&self) -> &LceNormalizedTermios {
        &self.after
    }

    #[cfg(test)]
    pub fn exclusive_status_for_test(&self) -> LcePassiveExclusiveModeStatus {
        self.exclusive_status
    }

    #[cfg(test)]
    pub fn opened_device_for_test(&self) -> LcePassiveOpenedDevice {
        self.opened_device
    }

    #[cfg(test)]
    pub fn line_disciplines_for_test(&self) -> (i32, i32) {
        (self.line_discipline_before, self.line_discipline_after)
    }

    #[cfg(test)]
    pub fn pre_capture_input_queue_for_test(&self) -> LcePassiveInputQueueObservation {
        self.pre_capture_input_queue
    }
}

fn validate_path(path: &OsStr) -> Result<(), PassiveLinuxError> {
    let bytes = path.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_PASSIVE_DEVICE_PATH_BYTES
        || bytes.contains(&0)
        || !Path::new(path).is_absolute()
    {
        return Err(PassiveLinuxError::InvalidDevicePath);
    }
    Ok(())
}

fn query_exclusive(fd: &OwnedFd) -> Result<bool, std::io::Error> {
    let mut enabled = 0_i32;
    // SAFETY: TIOCGEXCL writes one `int` to the valid pointer and does not
    // consume or retain the borrowed tty descriptor.
    let result = unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCGEXCL, &mut enabled) };
    if result == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(enabled != 0)
    }
}

fn query_line_discipline(fd: &OwnedFd) -> Result<i32, std::io::Error> {
    let mut discipline = -1_i32;
    // SAFETY: TIOCGETD writes one `int` to the valid pointer and does not
    // consume or retain the borrowed tty descriptor.
    let result = unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCGETD, &mut discipline) };
    if result == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(discipline)
    }
}

fn require_n_tty(observed: i32) -> Result<(), PassiveLinuxError> {
    if observed == LINUX_N_TTY_LINE_DISCIPLINE {
        Ok(())
    } else {
        Err(PassiveLinuxError::UnsupportedLineDiscipline { observed })
    }
}

fn observe_input_queue(fd: &OwnedFd) -> LcePassiveInputQueueObservation {
    let mut bytes = 0_i32;
    // SAFETY: TIOCINQ writes one `int` to the valid pointer and does not
    // consume, flush, or retain the borrowed tty descriptor.
    let result = unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCINQ, &mut bytes) };
    if result == -1 || bytes < 0 {
        let raw_os_error = if result == -1 {
            std::io::Error::last_os_error().raw_os_error()
        } else {
            None
        };
        LcePassiveInputQueueObservation::Unavailable { raw_os_error }
    } else {
        LcePassiveInputQueueObservation::Observed {
            bytes: bytes as u32,
        }
    }
}

fn apply_capture_policy(settings: &mut Termios) -> Result<(), PassiveLinuxError> {
    settings.make_raw();
    settings.input_modes.remove(
        InputModes::IGNBRK
            | InputModes::BRKINT
            | InputModes::IGNPAR
            | InputModes::PARMRK
            | InputModes::INPCK
            | InputModes::IXON
            | InputModes::IXOFF
            | InputModes::IXANY
            | InputModes::ICRNL
            | InputModes::INLCR
            | InputModes::IGNCR
            | InputModes::ISTRIP
            | InputModes::IUCLC,
    );
    settings.local_modes.remove(
        LocalModes::ICANON
            | LocalModes::ECHO
            | LocalModes::ECHOE
            | LocalModes::ECHONL
            | LocalModes::ISIG
            | LocalModes::IEXTEN,
    );
    settings.output_modes.remove(OutputModes::OPOST);
    settings.control_modes.remove(
        ControlModes::CSIZE
            | ControlModes::PARENB
            | ControlModes::PARODD
            | ControlModes::CSTOPB
            | ControlModes::CRTSCTS
            | ControlModes::HUPCL,
    );
    settings
        .control_modes
        .insert(ControlModes::CS8 | ControlModes::CREAD | ControlModes::CLOCAL);
    settings.special_codes[SpecialCodeIndex::VMIN] = 0;
    settings.special_codes[SpecialCodeIndex::VTIME] = 0;
    settings
        .set_speed(speed::B9600)
        .map_err(PassiveLinuxError::TermiosConfigure)
}

fn normalize_termios(settings: &Termios) -> LceNormalizedTermios {
    let character_size = settings.control_modes & ControlModes::CSIZE;
    let character_size_bits = if character_size == ControlModes::CS5 {
        5
    } else if character_size == ControlModes::CS6 {
        6
    } else if character_size == ControlModes::CS7 {
        7
    } else if character_size == ControlModes::CS8 {
        8
    } else {
        0
    };
    LceNormalizedTermios {
        ignbrk: settings.input_modes.contains(InputModes::IGNBRK),
        brkint: settings.input_modes.contains(InputModes::BRKINT),
        ignpar: settings.input_modes.contains(InputModes::IGNPAR),
        parmrk: settings.input_modes.contains(InputModes::PARMRK),
        inpck: settings.input_modes.contains(InputModes::INPCK),
        ixon: settings.input_modes.contains(InputModes::IXON),
        ixoff: settings.input_modes.contains(InputModes::IXOFF),
        ixany: settings.input_modes.contains(InputModes::IXANY),
        icrnl: settings.input_modes.contains(InputModes::ICRNL),
        inlcr: settings.input_modes.contains(InputModes::INLCR),
        igncr: settings.input_modes.contains(InputModes::IGNCR),
        istrip: settings.input_modes.contains(InputModes::ISTRIP),
        iuclc: settings.input_modes.contains(InputModes::IUCLC),
        icanon: settings.local_modes.contains(LocalModes::ICANON),
        echo: settings.local_modes.contains(LocalModes::ECHO),
        echoe: settings.local_modes.contains(LocalModes::ECHOE),
        echonl: settings.local_modes.contains(LocalModes::ECHONL),
        isig: settings.local_modes.contains(LocalModes::ISIG),
        iexten: settings.local_modes.contains(LocalModes::IEXTEN),
        opost: settings.output_modes.contains(OutputModes::OPOST),
        character_size_bits,
        cs8: character_size == ControlModes::CS8,
        parenb: settings.control_modes.contains(ControlModes::PARENB),
        parodd: settings.control_modes.contains(ControlModes::PARODD),
        cstopb: settings.control_modes.contains(ControlModes::CSTOPB),
        cread: settings.control_modes.contains(ControlModes::CREAD),
        clocal: settings.control_modes.contains(ControlModes::CLOCAL),
        crtscts: settings.control_modes.contains(ControlModes::CRTSCTS),
        hupcl: settings.control_modes.contains(ControlModes::HUPCL),
        input_speed: settings.input_speed(),
        output_speed: settings.output_speed(),
        vmin: settings.special_codes[SpecialCodeIndex::VMIN],
        vtime: settings.special_codes[SpecialCodeIndex::VTIME],
    }
}

fn capture_limit_reason(error: &LcePassiveCaptureError) -> Option<LcePassiveCaptureEndReason> {
    match error {
        LcePassiveCaptureError::TooManyEntries => Some(LcePassiveCaptureEndReason::EntryLimit),
        LcePassiveCaptureError::EntryTooLarge { .. } => {
            Some(LcePassiveCaptureEndReason::ChunkLimit)
        }
        LcePassiveCaptureError::RawCaptureTooLarge { .. } => {
            Some(LcePassiveCaptureEndReason::AggregateByteLimit)
        }
        _ => None,
    }
}
