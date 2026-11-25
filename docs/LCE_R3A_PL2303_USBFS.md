# LCE R3A PL2303GT raw-usb adapter validation

Status: **OFF-ELEVATOR EXPERIMENTAL / NOT A FIELD AUTHORIZATION**

This slice exists because the current Android host exposes the CableCreation
Prolific adapter through `/dev/bus/usb`, while its kernel does not provide a
PL2303 tty driver. It is an alternative acquisition backend for adapter/platform
validation, not a replacement for the existing Linux tty R3A contract.

## Exact supported adapter

The first implementation is intentionally narrow:

- USB VID:PID `067b:23c3`
- observed `bcdDevice=0305`
- Prolific PL2303GT / HXN-family behavior
- interface `0`
- bulk IN endpoint `0x83`
- historical runtime profile `9600/8N1`

It does not perform VID/PID discovery, serial-port discovery, baud scanning,
fallback probing, terminal navigation, unlock, or controller commands.

## Assurance boundary

The required acknowledgement is:

~~~text
--assurance pl2303-serial-payload-rx-only
--context off-elevator-adapter-validation
--profile historical-9600-8n1
~~~

The precise claim is:

~~~text
USBFS_FD_ACCESS=READ_WRITE_REQUIRED_BY_USBFS
APPLICATION_PROTOCOL_TX_REQUESTED=false
SERIAL_PAYLOAD_TX_REQUESTED=false
USB_CONTROL_OUT_ADAPTER_CONFIGURATION=9600_8N1_ONLY_IF_NEEDED
USB_BULK_OUT_ENDPOINT_USED=false
MODEM_CONTROL_REQUESTED=false
BREAK_REQUESTED=false
DEVICE_RESET_REQUESTED=false
ENDPOINT_RESET_REQUESTED=false
FLOW_CONTROL_WRITE_REQUESTED=false
PHYSICALLY_PASSIVE=false/unproven
REAL_LCE_COMPATIBLE=unproven
REAL_CONTROLLER_USE_AUTHORIZED=false
~~~

`/dev/bus/usb/*/*` must be opened read/write because usbfs userspace-driver
operations require write access. That does **not** mean the experiment sends a
serial payload. The code may issue one class control-OUT request to configure
the adapter's UART line coding to 9600/8N1 if readback shows another setting.
It does not issue PL2303 `SET_CONTROL_LINE_STATE`, `SEND_BREAK`, device reset,
endpoint reset, flow-control writes, bulk OUT, terminal commands, or elevator
protocol traffic.

This is therefore **serial-payload-RX-only**, not USB-transfer-RX-only and not
physical passivity.

## Why usbfs

Linux documents `/dev/bus/usb/BBB/DDD` as the userspace-driver interface:
userspace opens the device node read/write, validates descriptors, claims an
interface with `USBDEVFS_CLAIMINTERFACE`, and uses `USBDEVFS_CONTROL` and
`USBDEVFS_BULK` for synchronous transfers.

The implementation mirrors the UAPI structures from
`include/uapi/linux/usbdevice_fs.h` and asserts the 64-bit Linux ioctl numbers
used by both x86_64 CI and the current aarch64 Android/DroidSpaces target.

Primary references:

- Linux USB userspace API:
  https://www.kernel.org/doc/html/latest/driver-api/usb/usb.html
- Linux UAPI `usbdevice_fs.h`:
  https://github.com/torvalds/linux/blob/master/include/uapi/linux/usbdevice_fs.h
- Linux PL2303 driver:
  https://github.com/torvalds/linux/blob/master/drivers/usb/serial/pl2303.c
- usb-serial-for-android Prolific implementation:
  https://github.com/mik3y/usb-serial-for-android/blob/master/usbSerialForAndroid/src/main/java/com/hoho/android/usbserial/driver/ProlificSerialDriver.java

The Linux and Android implementations independently support the HXN-family
request layout used here: class line coding, vendor request `0x81` for HXN
register reads, and endpoints `0x02`/`0x83`. LiftAssistant deliberately does
not open/use the `0x02` bulk-OUT path in this experiment.

## Plan-only validation

Plan mode performs no hardware open:

~~~bash
cargo run -p liftassistant-lce \
  --example lce_pl2303_usb_rx_capture --locked -- \
  --plan \
  --assurance pl2303-serial-payload-rx-only \
  --context off-elevator-adapter-validation \
  --profile historical-9600-8n1 \
  --usb-node /dev/bus/usb/001/002 \
  --output /tmp/lce-pl2303-usb.json \
  --duration-seconds 10
~~~

The raw USB address can change after unplug/replug. Always re-check `lsusb`
before an execution run.

## Off-elevator execution

Execution is intended first for the loose adapter only, with **no CPU561 or
elevator connected**:

~~~bash
cargo run -p liftassistant-lce \
  --example lce_pl2303_usb_rx_capture --locked -- \
  --assurance pl2303-serial-payload-rx-only \
  --context off-elevator-adapter-validation \
  --profile historical-9600-8n1 \
  --usb-node /dev/bus/usb/001/002 \
  --output /tmp/lce-pl2303-usb.json \
  --duration-seconds 10
~~~

The first expected gate is interface ownership. If another Android/container
process owns interface 0, the program fails closed at `USBDEVFS_CLAIMINTERFACE`.
It does not detach a kernel driver, reset the USB device, or force-release an
unknown claimant.

If the interface can be claimed, the program:

1. reads the device descriptor and requires `067b:23c3 / bcdDevice 0305`;
2. reads current PL2303 line coding;
3. sets 9600/8N1 only if needed, then reads it back;
4. reads HXN flow-control register `0x0a` and refuses to modify it if it is not
   already the `NONE` state under the Linux driver's `0x1c` mask;
5. performs bounded `USBDEVFS_BULK` reads only from endpoint `0x83`;
6. writes a create-new JSON evidence artifact with a SHA-256 envelope.

A zero-byte duration-complete result remains valid adapter/platform evidence;
it says nothing about whether CPU561/XDB1 emits unsolicited traffic.

## Field boundary

This code does not authorize connecting to the passenger elevator. A future
CPU561/XDB1 session remains HUMAN-gated. Before that boundary, the adapter must
first pass off-elevator ownership, line-coding, flow-control, and receive-path
validation, and the physical RS-232 pin/cable assumptions must be reviewed
separately.

The existing tty R3A path remains unchanged.
