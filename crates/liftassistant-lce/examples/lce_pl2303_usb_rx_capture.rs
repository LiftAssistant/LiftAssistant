#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("lce_pl2303_usb_rx_capture is available only on Linux");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    linux::main()
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        env,
        error::Error,
        fs::{File, OpenOptions, remove_file},
        io::{Read, Write},
        mem::size_of,
        os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
        path::{Path, PathBuf},
        process::ExitCode,
        time::{Duration, Instant},
    };

    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    const ASSURANCE: &str = "pl2303-serial-payload-rx-only";
    const CONTEXT: &str = "off-elevator-adapter-validation";
    const PROFILE: &str = "historical-9600-8n1";

    const VID: u16 = 0x067b;
    const PID: u16 = 0x23c3;
    const BCD_DEVICE: u16 = 0x0305;
    const INTERFACE: u32 = 0;
    const BULK_IN: u32 = 0x83;
    const LINE_9600_8N1: [u8; 7] = [0x80, 0x25, 0x00, 0x00, 0x00, 0x00, 0x08];

    const MAX_DURATION_SECONDS: u16 = 300;
    const MAX_ENTRIES: usize = 4096;
    const MAX_RAW_BYTES: usize = 1024 * 1024;
    const MAX_SERIALIZED_BYTES: usize = 5 * 1024 * 1024;

    const USBDEVFS_CONTROL: libc::c_ulong = ioc_readwrite::<UsbdevfsCtrlTransfer>(b'U', 0);
    const USBDEVFS_BULK: libc::c_ulong = ioc_readwrite::<UsbdevfsBulkTransfer>(b'U', 2);
    const USBDEVFS_CLAIMINTERFACE: libc::c_ulong = ioc_read::<libc::c_uint>(b'U', 15);
    const USBDEVFS_RELEASEINTERFACE: libc::c_ulong = ioc_read::<libc::c_uint>(b'U', 16);

    #[repr(C)]
    struct UsbdevfsCtrlTransfer {
        b_request_type: u8,
        b_request: u8,
        w_value: u16,
        w_index: u16,
        w_length: u16,
        timeout: u32,
        data: *mut libc::c_void,
    }

    #[repr(C)]
    struct UsbdevfsBulkTransfer {
        ep: u32,
        len: u32,
        timeout: u32,
        data: *mut libc::c_void,
    }

    const fn ioc(direction: u64, type_: u8, number: u8, size: usize) -> libc::c_ulong {
        const NR_SHIFT: u64 = 0;
        const TYPE_SHIFT: u64 = 8;
        const SIZE_SHIFT: u64 = 16;
        const DIR_SHIFT: u64 = 30;
        ((direction << DIR_SHIFT)
            | ((type_ as u64) << TYPE_SHIFT)
            | ((number as u64) << NR_SHIFT)
            | ((size as u64) << SIZE_SHIFT)) as libc::c_ulong
    }

    const fn ioc_read<T>(type_: u8, number: u8) -> libc::c_ulong {
        ioc(2, type_, number, size_of::<T>())
    }

    const fn ioc_readwrite<T>(type_: u8, number: u8) -> libc::c_ulong {
        ioc(3, type_, number, size_of::<T>())
    }

    #[derive(Debug)]
    struct Args {
        plan: bool,
        usb_node: PathBuf,
        output: PathBuf,
        duration_seconds: u16,
    }

    pub(super) fn main() -> ExitCode {
        match run(env::args().skip(1)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("LCE PL2303GT raw-USB RX capture failed: {error}");
                ExitCode::FAILURE
            }
        }
    }

    fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), Box<dyn Error>> {
        let args = parse_args(arguments)?;
        println!("{}", banner(&args, args.plan));

        if args.plan {
            return Ok(());
        }

        let mut output = ReservedOutput::create(&args.output)?;
        let capture = acquire(&args)?;
        let capture_bytes = serde_json::to_vec(&capture)?;
        let envelope = json!({
            "capture": capture,
            "sha256": sha256_hex(&capture_bytes),
        });
        let encoded = serde_json::to_vec(&envelope)?;
        if encoded.len() > MAX_SERIALIZED_BYTES {
            return Err(format!("serialized capture too large: {} bytes", encoded.len()).into());
        }

        output.file_mut().write_all(&encoded)?;
        output.file_mut().sync_all()?;
        output.commit();
        Ok(())
    }

    fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Args, Box<dyn Error>> {
        let mut plan = false;
        let mut assurance = None;
        let mut profile = None;
        let mut context = None;
        let mut usb_node = None;
        let mut output = None;
        let mut duration_seconds = None;
        let mut it = arguments.into_iter();

        while let Some(argument) = it.next() {
            if argument == "--plan" {
                if plan {
                    return Err("duplicate --plan".into());
                }
                plan = true;
                continue;
            }

            let slot = match argument.as_str() {
                "--assurance" => &mut assurance,
                "--profile" => &mut profile,
                "--context" => &mut context,
                "--usb-node" => &mut usb_node,
                "--output" => &mut output,
                "--duration-seconds" => &mut duration_seconds,
                _ => return Err(format!("unsupported argument {argument:?}").into()),
            };
            if slot.is_some() {
                return Err(format!("duplicate argument {argument:?}").into());
            }
            *slot = Some(
                it.next()
                    .ok_or_else(|| format!("missing value for {argument}"))?,
            );
        }

        if assurance.as_deref() != Some(ASSURANCE) {
            return Err(format!("--assurance must be {ASSURANCE}").into());
        }
        if profile.as_deref() != Some(PROFILE) {
            return Err(format!("--profile must be {PROFILE}").into());
        }
        if context.as_deref() != Some(CONTEXT) {
            return Err(format!("--context must be {CONTEXT}").into());
        }

        let usb_node = PathBuf::from(usb_node.ok_or("missing --usb-node")?);
        if !usb_node.is_absolute() || !usb_node.starts_with("/dev/bus/usb") {
            return Err("--usb-node must be an explicit absolute /dev/bus/usb path".into());
        }

        let output = PathBuf::from(output.ok_or("missing --output")?);
        let duration_seconds = duration_seconds
            .ok_or("missing --duration-seconds")?
            .parse::<u16>()?;
        if duration_seconds == 0 || duration_seconds > MAX_DURATION_SECONDS {
            return Err(format!("--duration-seconds must be in 1..={MAX_DURATION_SECONDS}").into());
        }

        Ok(Args {
            plan,
            usb_node,
            output,
            duration_seconds,
        })
    }

    fn banner(args: &Args, plan: bool) -> String {
        format!(
            "mode: {}\ncontext: {CONTEXT}\nusb_node: {}\noutput: {}\nduration: {}s\nprofile: {PROFILE}\nassurance: {ASSURANCE}\nUSBFS_FD_ACCESS=READ_WRITE_REQUIRED_BY_USBFS\nAPPLICATION_PROTOCOL_TX_REQUESTED=false\nSERIAL_PAYLOAD_TX_REQUESTED=false\nUSB_CONTROL_OUT_ADAPTER_CONFIGURATION=9600_8N1_ONLY_IF_NEEDED\nUSB_BULK_OUT_ENDPOINT_USED=false\nUSB_CONFIGURATION_TOPOLOGY_REQUIRED=interface0:0x81-int,0x02-bulk-out-present-unused,0x83-bulk-in\nMODEM_CONTROL_REQUESTED=false\nBREAK_REQUESTED=false\nDEVICE_RESET_REQUESTED=false\nENDPOINT_RESET_REQUESTED=false\nFLOW_CONTROL_WRITE_REQUESTED=false\nAUTO_DISCOVERY=false\nBAUD_SCAN=false\nUNLOCK_OR_TERMINAL_NAVIGATION=false\nPHYSICALLY_PASSIVE=false/unproven\nREAL_LCE_COMPATIBLE=unproven\nREAL_CONTROLLER_USE_AUTHORIZED=false\nreal_hardware_opened: {}",
            if plan { "PLAN_ONLY" } else { "EXECUTE" },
            args.usb_node.display(),
            args.output.display(),
            args.duration_seconds,
            !plan,
        )
    }

    fn acquire(args: &Args) -> Result<Value, Box<dyn Error>> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(&args.usb_node)?;

        let descriptor = read_device_descriptor(&mut file)?;
        let identity = (
            u16::from_le_bytes([descriptor[8], descriptor[9]]),
            u16::from_le_bytes([descriptor[10], descriptor[11]]),
            u16::from_le_bytes([descriptor[12], descriptor[13]]),
        );
        if identity != (VID, PID, BCD_DEVICE) {
            return Err(format!(
                "USB identity mismatch: {:04x}:{:04x} bcdDevice={:04x}; expected {VID:04x}:{PID:04x} bcdDevice={BCD_DEVICE:04x}",
                identity.0, identity.1, identity.2,
            )
            .into());
        }

        let configuration = read_single_configuration(&mut file, descriptor[17])?;
        let topology = validate_pl2303gt_topology(&configuration)?;

        let mut claim = INTERFACE;
        ioctl_ptr(
            file.as_raw_fd(),
            USBDEVFS_CLAIMINTERFACE,
            (&mut claim as *mut u32).cast(),
        )
        .map_err(|error| {
            format!(
                "cannot claim PL2303GT interface 0 ({error}); another process may own the interface"
            )
        })?;
        let _claim_guard = InterfaceClaim {
            fd: file.as_raw_fd(),
            interface: INTERFACE,
        };

        let line_before = control_in(&file, 0xa1, 0x21, 0, INTERFACE as u16, 7)?;
        let line_changed = line_before.as_slice() != LINE_9600_8N1;
        if line_changed {
            control_out(&file, 0x21, 0x20, 0, INTERFACE as u16, &LINE_9600_8N1)?;
        }

        let line_after = control_in(&file, 0xa1, 0x21, 0, INTERFACE as u16, 7)?;
        if line_after.as_slice() != LINE_9600_8N1 {
            return Err(format!("9600/8N1 readback mismatch: {line_after:02x?}").into());
        }

        let flow = control_in(&file, 0xc0, 0x81, 0x0a, 0, 1)?;
        if flow.len() != 1 || flow[0] & 0x1c != 0x1c {
            return Err(format!(
                "HXN flow-control register is not already NONE: {flow:02x?}; refusing to modify it"
            )
            .into());
        }

        let requested = Duration::from_secs(u64::from(args.duration_seconds));
        let started = Instant::now();
        let mut entries = Vec::<Value>::new();
        let mut raw_bytes = 0usize;
        let mut end_reason = "DURATION_ELAPSED".to_owned();
        let mut transfer_error: Option<String> = None;

        loop {
            let elapsed = started.elapsed();
            if elapsed >= requested {
                break;
            }

            let remaining = requested - elapsed;
            let timeout_ms = remaining
                .min(Duration::from_millis(250))
                .as_millis()
                .clamp(1, u128::from(u32::MAX)) as u32;
            let mut buffer = [0_u8; 64];

            match bulk_in(&file, BULK_IN, &mut buffer, timeout_ms) {
                Ok(read) => {
                    if read == 0 {
                        continue;
                    }
                    if entries.len() >= MAX_ENTRIES {
                        end_reason = "ENTRY_LIMIT".into();
                        break;
                    }
                    let next = raw_bytes
                        .checked_add(read)
                        .ok_or("RX byte count overflow")?;
                    if next > MAX_RAW_BYTES {
                        end_reason = "AGGREGATE_BYTE_LIMIT".into();
                        break;
                    }
                    raw_bytes = next;
                    entries.push(json!({
                        "observed_at_micros": micros(started.elapsed().min(requested)),
                        "bytes": buffer[..read].to_vec(),
                    }));
                }
                Err(error)
                    if matches!(
                        error.raw_os_error(),
                        Some(libc::ETIMEDOUT) | Some(libc::EINTR)
                    ) => {}
                Err(error) if error.raw_os_error() == Some(libc::ENODEV) => {
                    end_reason = "DEVICE_DISCONNECTED".into();
                    break;
                }
                Err(error) => {
                    end_reason = "TRANSFER_ERROR".into();
                    transfer_error = Some(error.to_string());
                    break;
                }
            }
        }

        let ended_at = if end_reason == "DURATION_ELAPSED" {
            requested
        } else {
            started.elapsed().min(requested)
        };

        Ok(json!({
            "format_version": 1,
            "purpose": "OFF_ELEVATOR_ADAPTER_PLATFORM_VALIDATION",
            "context_acknowledgement": CONTEXT,
            "session_id": fresh_session_id()?,
            "requested_usb_node": args.usb_node.to_string_lossy(),
            "requested_profile": PROFILE,
            "requested_duration_seconds": args.duration_seconds,
            "descriptor": {
                "vendor_id": identity.0,
                "product_id": identity.1,
                "usb_version": u16::from_le_bytes([descriptor[2], descriptor[3]]),
                "device_version": identity.2,
                "device_class": descriptor[4],
                "device_subclass": descriptor[5],
                "device_protocol": descriptor[6],
            },
            "configuration_topology": topology,
            "interface_number": INTERFACE,
            "bulk_in_endpoint": BULK_IN,
            "usbfs_fd_access": "READ_WRITE_REQUIRED_BY_USBFS",
            "application_protocol_tx_requested": false,
            "serial_payload_tx_requested": false,
            "usb_control_out_adapter_configuration_requested": line_changed,
            "usb_control_out_operations": if line_changed {
                vec!["SET_LINE_CODING_9600_8N1"]
            } else {
                Vec::<&str>::new()
            },
            "usb_bulk_out_endpoint_used": false,
            "modem_control_requested": false,
            "break_requested": false,
            "device_reset_requested": false,
            "endpoint_reset_requested": false,
            "flow_control_write_requested": false,
            "auto_discovery": false,
            "baud_scan": false,
            "unlock_or_terminal_navigation": false,
            "physical_passivity": "false/unproven",
            "real_lce_compatibility": "unproven",
            "real_controller_use_authorized": false,
            "line_coding_before": line_before,
            "line_coding_after": line_after,
            "hxn_flow_control_register_0a": flow[0],
            "hxn_flow_control_none_verified": true,
            "entries": entries,
            "raw_bytes": raw_bytes,
            "ended_at_micros": micros(ended_at),
            "end_reason": end_reason,
            "transfer_error": transfer_error,
        }))
    }

    fn read_device_descriptor(file: &mut File) -> Result<[u8; 18], Box<dyn Error>> {
        let mut descriptor = [0_u8; 18];
        file.read_exact(&mut descriptor)?;
        if descriptor[0] != 18 || descriptor[1] != 1 {
            return Err(format!(
                "unexpected USB device descriptor header: {:02x?}",
                &descriptor[..2]
            )
            .into());
        }
        Ok(descriptor)
    }

    fn read_single_configuration(
        file: &mut File,
        configuration_count: u8,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        if configuration_count != 1 {
            return Err(format!(
                "expected exactly one USB configuration, observed {configuration_count}"
            )
            .into());
        }

        let mut header = [0_u8; 9];
        file.read_exact(&mut header)?;
        if header[0] != 9 || header[1] != 2 {
            return Err(format!(
                "unexpected USB configuration descriptor header: {:02x?}",
                &header[..2]
            )
            .into());
        }

        let total_length = usize::from(u16::from_le_bytes([header[2], header[3]]));
        if !(9..=4096).contains(&total_length) {
            return Err(format!(
                "invalid USB configuration descriptor length {total_length}"
            )
            .into());
        }

        let mut bytes = vec![0_u8; total_length];
        bytes[..header.len()].copy_from_slice(&header);
        file.read_exact(&mut bytes[header.len()..])?;
        Ok(bytes)
    }

    fn validate_pl2303gt_topology(configuration: &[u8]) -> Result<Value, Box<dyn Error>> {
        if configuration.len() < 9
            || configuration[0] != 9
            || configuration[1] != 2
            || usize::from(u16::from_le_bytes([configuration[2], configuration[3]]))
                != configuration.len()
        {
            return Err("invalid USB configuration descriptor envelope".into());
        }
        if configuration[4] != 1 {
            return Err(format!(
                "expected one PL2303GT interface, observed {}",
                configuration[4]
            )
            .into());
        }

        let mut offset = 9usize;
        let mut target_interface = false;
        let mut interface_verified = false;
        let mut endpoints = Vec::<(u8, u8, u16, u8)>::new();

        while offset < configuration.len() {
            let length = usize::from(configuration[offset]);
            if length < 2 || offset + length > configuration.len() {
                return Err(format!(
                    "malformed USB descriptor at configuration offset {offset}"
                )
                .into());
            }

            match configuration[offset + 1] {
                4 => {
                    if length < 9 {
                        return Err("short USB interface descriptor".into());
                    }
                    let interface_number = configuration[offset + 2];
                    let alternate_setting = configuration[offset + 3];
                    target_interface = interface_number == 0 && alternate_setting == 0;
                    if target_interface {
                        let endpoint_count = configuration[offset + 4];
                        let class = configuration[offset + 5];
                        let subclass = configuration[offset + 6];
                        let protocol = configuration[offset + 7];
                        if endpoint_count != 3 || class != 0xff || subclass != 0 || protocol != 0 {
                            return Err(format!(
                                "unexpected PL2303GT interface 0 descriptor: endpoints={endpoint_count}, class={class:#04x}, subclass={subclass:#04x}, protocol={protocol:#04x}"
                            )
                            .into());
                        }
                        interface_verified = true;
                    }
                }
                5 if target_interface => {
                    if length < 7 {
                        return Err("short USB endpoint descriptor".into());
                    }
                    endpoints.push((
                        configuration[offset + 2],
                        configuration[offset + 3] & 0x03,
                        u16::from_le_bytes([
                            configuration[offset + 4],
                            configuration[offset + 5],
                        ]),
                        configuration[offset + 6],
                    ));
                }
                _ => {}
            }

            offset += length;
        }

        if !interface_verified {
            return Err("PL2303GT interface 0 alternate setting 0 was not found".into());
        }

        endpoints.sort_unstable();
        let expected = vec![
            (0x02, 0x02, 64, 0),
            (0x81, 0x03, 10, 1),
            (0x83, 0x02, 64, 0),
        ];
        if endpoints != expected {
            return Err(format!(
                "unexpected PL2303GT endpoint topology: {endpoints:02x?}"
            )
            .into());
        }

        Ok(json!({
            "verified": true,
            "interface": 0,
            "interface_class": 255,
            "interface_subclass": 0,
            "interface_protocol": 0,
            "endpoints": [
                {
                    "address": 0x81,
                    "transfer_type": "INTERRUPT_IN",
                    "max_packet_size": 10,
                    "interval": 1
                },
                {
                    "address": 0x02,
                    "transfer_type": "BULK_OUT_PRESENT_BUT_UNUSED",
                    "max_packet_size": 64,
                    "interval": 0
                },
                {
                    "address": 0x83,
                    "transfer_type": "BULK_IN_RX",
                    "max_packet_size": 64,
                    "interval": 0
                }
            ]
        }))
    }

    fn control_in(
        file: &File,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut bytes = vec![0_u8; usize::from(length)];
        let mut transfer = UsbdevfsCtrlTransfer {
            b_request_type: request_type,
            b_request: request,
            w_value: value,
            w_index: index,
            w_length: length,
            timeout: 1000,
            data: bytes.as_mut_ptr().cast(),
        };
        let transferred = ioctl_ptr(
            file.as_raw_fd(),
            USBDEVFS_CONTROL,
            (&mut transfer as *mut UsbdevfsCtrlTransfer).cast(),
        )?;
        let transferred = usize::try_from(transferred)?;
        if transferred > bytes.len() {
            return Err("usbfs control IN returned an impossible length".into());
        }
        bytes.truncate(transferred);
        Ok(bytes)
    }

    fn control_out(
        file: &File,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        bytes: &[u8],
    ) -> Result<(), Box<dyn Error>> {
        let length = u16::try_from(bytes.len())?;
        let mut transfer = UsbdevfsCtrlTransfer {
            b_request_type: request_type,
            b_request: request,
            w_value: value,
            w_index: index,
            w_length: length,
            timeout: 1000,
            data: bytes.as_ptr().cast_mut().cast(),
        };
        let transferred = ioctl_ptr(
            file.as_raw_fd(),
            USBDEVFS_CONTROL,
            (&mut transfer as *mut UsbdevfsCtrlTransfer).cast(),
        )?;
        if transferred != libc::c_int::from(length) {
            return Err(format!(
                "usbfs control OUT transferred {transferred} bytes; expected {length}"
            )
            .into());
        }
        Ok(())
    }

    fn bulk_in(
        file: &File,
        endpoint: u32,
        buffer: &mut [u8],
        timeout_ms: u32,
    ) -> std::io::Result<usize> {
        let mut transfer = UsbdevfsBulkTransfer {
            ep: endpoint,
            len: u32::try_from(buffer.len()).expect("small fixed bulk buffer"),
            timeout: timeout_ms,
            data: buffer.as_mut_ptr().cast(),
        };
        let transferred = ioctl_ptr(
            file.as_raw_fd(),
            USBDEVFS_BULK,
            (&mut transfer as *mut UsbdevfsBulkTransfer).cast(),
        )?;
        usize::try_from(transferred)
            .map_err(|_| std::io::Error::other("negative usbfs bulk length"))
    }

    fn ioctl_ptr(
        fd: libc::c_int,
        request: libc::c_ulong,
        argument: *mut libc::c_void,
    ) -> std::io::Result<libc::c_int> {
        // SAFETY: request numbers and argument layouts mirror linux/usbdevice_fs.h.
        let result = unsafe { libc::ioctl(fd, request, argument) };
        if result < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(result)
        }
    }

    fn fresh_session_id() -> Result<String, Box<dyn Error>> {
        let mut random = [0_u8; 16];
        let mut filled = 0;
        while filled < random.len() {
            let count = rustix::rand::getrandom(
                &mut random[filled..],
                rustix::rand::GetRandomFlags::empty(),
            )?;
            if count == 0 {
                return Err("Linux getrandom returned zero bytes".into());
            }
            filled += count;
        }

        Ok(format!(
            "r3a-pl2303-usb-{}",
            random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ))
    }

    fn micros(value: Duration) -> u64 {
        u64::try_from(value.as_micros()).unwrap_or(u64::MAX)
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    struct InterfaceClaim {
        fd: libc::c_int,
        interface: u32,
    }

    impl Drop for InterfaceClaim {
        fn drop(&mut self) {
            let mut interface = self.interface;
            // SAFETY: best-effort release of the interface claimed by this fd.
            unsafe {
                libc::ioctl(
                    self.fd,
                    USBDEVFS_RELEASEINTERFACE,
                    (&mut interface as *mut u32).cast::<libc::c_void>(),
                );
            }
        }
    }

    struct ReservedOutput {
        path: PathBuf,
        file: Option<File>,
        committed: bool,
    }

    impl ReservedOutput {
        fn create(path: &Path) -> std::io::Result<Self> {
            Ok(Self {
                path: path.to_owned(),
                file: Some(OpenOptions::new().write(true).create_new(true).open(path)?),
                committed: false,
            })
        }

        fn file_mut(&mut self) -> &mut File {
            self.file.as_mut().expect("reserved output exists")
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
            match remove_file(&self.path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    eprintln!(
                        "warning: failed to remove incomplete {}: {error}",
                        self.path.display()
                    );
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn valid_args() -> Vec<String> {
            [
                "--assurance",
                ASSURANCE,
                "--profile",
                PROFILE,
                "--context",
                CONTEXT,
                "--usb-node",
                "/dev/bus/usb/001/002",
                "--output",
                "/tmp/lce-pl2303-usb.json",
                "--duration-seconds",
                "1",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        }

        #[test]
        fn ioctl_numbers_match_linux_64_bit_usbfs_abi() {
            assert_eq!(size_of::<UsbdevfsCtrlTransfer>(), 24);
            assert_eq!(size_of::<UsbdevfsBulkTransfer>(), 24);
            assert_eq!(USBDEVFS_CONTROL, 0xc018_5500);
            assert_eq!(USBDEVFS_BULK, 0xc018_5502);
            assert_eq!(USBDEVFS_CLAIMINTERFACE, 0x8004_550f);
            assert_eq!(USBDEVFS_RELEASEINTERFACE, 0x8004_5510);
        }

        #[test]
        fn plan_exposes_the_exact_non_field_assurance_boundary() {
            let mut values = valid_args();
            values.push("--plan".into());
            let parsed = parse_args(values).unwrap();
            let text = banner(&parsed, true);

            for expected in [
                "SERIAL_PAYLOAD_TX_REQUESTED=false",
                "USB_BULK_OUT_ENDPOINT_USED=false",
                "MODEM_CONTROL_REQUESTED=false",
                "DEVICE_RESET_REQUESTED=false",
                "FLOW_CONTROL_WRITE_REQUESTED=false",
                "REAL_CONTROLLER_USE_AUTHORIZED=false",
                "real_hardware_opened: false",
            ] {
                assert!(text.contains(expected));
            }
        }

        #[test]
        fn topology_validation_accepts_the_observed_pl2303gt_layout() {
            let configuration = [
                9, 2, 39, 0, 1, 1, 0, 0xa0, 50,
                9, 4, 0, 0, 3, 0xff, 0, 0, 0,
                7, 5, 0x81, 3, 10, 0, 1,
                7, 5, 0x02, 2, 64, 0, 0,
                7, 5, 0x83, 2, 64, 0, 0,
            ];
            let topology = validate_pl2303gt_topology(&configuration).unwrap();
            assert_eq!(topology["verified"], true);
            assert_eq!(topology["endpoints"][2]["address"], 0x83);
        }

        #[test]
        fn topology_validation_fails_closed_on_endpoint_drift() {
            let mut configuration = [
                9, 2, 39, 0, 1, 1, 0, 0xa0, 50,
                9, 4, 0, 0, 3, 0xff, 0, 0, 0,
                7, 5, 0x81, 3, 10, 0, 1,
                7, 5, 0x02, 2, 64, 0, 0,
                7, 5, 0x83, 2, 64, 0, 0,
            ];
            configuration[32] = 0x84;
            assert!(validate_pl2303gt_topology(&configuration).is_err());
        }

        #[test]
        fn parser_rejects_non_usbfs_paths_and_duplicates() {
            let mut wrong_path = valid_args();
            let index = wrong_path
                .iter()
                .position(|value| value == "--usb-node")
                .unwrap()
                + 1;
            wrong_path[index] = "/dev/ttyUSB0".into();
            assert!(parse_args(wrong_path).is_err());

            let mut duplicate = valid_args();
            duplicate.extend(["--profile".into(), PROFILE.into()]);
            assert!(parse_args(duplicate).is_err());
        }
    }
}
