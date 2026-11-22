# LCE-R3A software RX-only acquisition

LCE-R3A.0 adds a Linux-only, bounded raw tty acquisition foundation before any
real KONE field connection is authorized. Its strongest claim is:

```text
SOFTWARE_RX_ONLY
LINUX_TTY_DELIVERED_RX
```

It does **not** claim `PHYSICALLY_PASSIVE`, `WIRE_PASSIVE`, `ZERO_INFLUENCE`,
`WIRE_CONFIRMED`, live LCE compatibility, or controller validation. Opening or
closing a tty can activate driver/device behavior, including modem-control
line handling. A full-duplex cable also remains physically connected even if
the application never calls `write(2)`.

Two assurance levels are now kept separate:

- **SOFTWARE_RX_ONLY field observation** may use a normal reviewed RS-232
  physical interface while the LiftAssistant application exposes no explicit
  data-TX or modem-output operation. It must be reported as
  `PHYSICALLY_PASSIVE=false/unproven` and requires a separate HUMAN field gate.
- **PHYSICAL_RX_ONLY hardened acquisition** additionally removes the intentional
  controller-receive drive path and requires the separate R3A.0.3/R3A.0.4
  design-and-bench path.

The second level is stronger, but it is no longer a mandatory prerequisite for
the first controlled software-RX-only observation.

## Ownership and evidence flow

The experimental Linux example is intentionally separate from the R2
interpreted terminal artifact:

```text
explicit Linux tty path
  -> O_RDONLY acquisition
  -> LCE passive capture v2
  -> exact chunks returned by read(2)
  -> offline replay
       -> R1 transcript decoder
       -> R2.3 terminal-context classifier
```

The acquisition layer does not infer floor, door, motion, mode, fault, prompt
meaning, or controller identity. R1/R2.3 interpretation is offline and does
not call `issue_query`. The existing R2 active session, fixed `STA`/`CMD`/`MDE`
bytes, authority model, Core, and Engine are unchanged.

## Artifact v2

`FinalizedLcePassiveCapture` is internal/experimental and independent of the
R2 capture v4 format. There is no real passive-v1 field corpus, so v2 is the
only accepted passive format and v1 is rejected as `UnsupportedVersion`; there
is no migration or compatibility alias. V2 records:

- opaque acquisition session ID and existing
  `LIFTASSISTANT_SYNTHETIC`/`FIELD_EVIDENCE_CANDIDATE` origin;
- the exact bounded Linux pathname bytes supplied by the operator, without
  lossy UTF-8 conversion or pre-open canonicalization;
- character-device major/minor from `fstat(2)` on the already-open descriptor;
- `LINUX_TTY_DELIVERED_RX` acquisition layer;
- requested historical 9600/8N1 terminal profile;
- requested duration and normalized open policy;
- normalized termios before and after applying the local capture policy;
- requested/observed exclusive-mode status;
- `TIOCGETD` line-discipline observations before and after policy application;
- a best-effort `TIOCINQ` input-queue observation immediately before capture;
- `read_observed_at_micros`, the relative monotonic time when the application
  recorded each nonempty Linux `read(2)` result, and the exact result bytes;
- bounded termination reason and SHA-256.

The bounds are session ID 128 bytes, requested device pathname 4096 bytes,
4096 entries, 64 KiB per read chunk, 1 MiB aggregate RX, 5 MiB serialized
artifact, and 1–300 seconds. `DurationElapsed` is valid only at the exact
requested duration boundary. EOF, peer hangup, read or poll error, duration,
or evidence limit ends the current acquisition. There is no reopen, reconnect,
port discovery, or session reuse. SIGINT/crash-durable finalization is not
implemented; operators should allow the bounded duration to finish normally.

SHA-256 provides mutation detection only. It is not source authentication, a
signature, or chain-of-custody proof. Major/minor identifies only the kernel
character-device object represented by the opened descriptor during this
acquisition; it is not permanent adapter, cable, CPU561, or controller
identity. A duration-complete zero-entry artifact derives the outcome
`PassiveSilent`; this means only that Linux delivered no tty RX bytes during
the configured window. An early EOF/HUP/error with no payload instead derives
`EndedWithoutPayload`. Neither outcome diagnoses wiring, baud, interface lock,
controller behavior or protocol shape. Setup failures and artifact-integrity
failures remain errors outside a finalized-capture outcome.

## Linux open and termios policy

The example requires one HUMAN-supplied absolute device path. It neither
enumerates nor scans devices. Its open flags are:

```text
O_RDONLY | O_NOCTTY | O_NONBLOCK | O_CLOEXEC
```

It requests `TIOCEXCL` immediately, then uses `TIOCGEXCL` when supported. It
uses `fstat(2)` on that descriptor, fails closed unless it is a character
device, and records its major/minor separately from the requested pathname. The
artifact distinguishes observed exclusive mode from a requested-but-
unverifiable result. `TIOCEXCL` fences later ordinary opens; it does not prove
that no process already had the tty open. The PTY proof checks `F_GETFL &
O_ACCMODE == O_RDONLY`, `O_NONBLOCK`, `FD_CLOEXEC`, a later `EBUSY` open (except
the documented privileged bypass), and `EBADF` from a synthetic-only write
attempt on the read-only slave descriptor.

The code uses `TIOCGETD` before and after policy application and supports only
Linux `N_TTY`; it never calls `TIOCSETD`. Any other line discipline fails
closed. It reads and records termios before configuration, applies the
historical 9600/8N1 profile, then reads back and validates every controlled
field. The following are
explicit **LiftAssistant local capture policy**, not KONE protocol facts:

- disable `IGNBRK`, `BRKINT`, `IGNPAR`, `PARMRK`, `INPCK`, `IXON`, `IXOFF`,
  `IXANY`, `IUCLC`, CR/LF translation, `ISTRIP`, canonical input, echo, signal
  generation, extended input processing, output post-processing, RTS/CTS flow
  control, and `HUPCL`;
- enable `CS8`, `CREAD`, and `CLOCAL`; disable parity and two-stop-bit mode;
- set input/output speeds to 9600 and `VMIN=0`, `VTIME=0`.

The acquisition code never invokes `tcflush`, `TIOCMSET`, `TIOCMBIS`,
`TIOCMBIC`, RTS/DTR setters, or break generation. It still cannot prevent a
Linux tty/USB serial driver from changing RTS/DTR or device state as an
open/close side effect.

## Configuration window and queued input

Linux sends incoming tty bytes through the active line discipline before
userspace `read(2)`. During `open -> TIOCEXCL -> tcgetattr -> tcsetattr`, the
descriptor can still inherit earlier `N_TTY`/termios state. Flags such as
`ECHO` or `IXOFF` can therefore transform or consume input, or cause
kernel/line-discipline output, even though LiftAssistant never calls
`write(2)`. `SOFTWARE_RX_ONLY` means only that the acquisition application
exposes no explicit tty data-TX or modem-output operation. It does not mean
kernel-stack-TX-free, driver-side-effect-free, or electrically passive.

Immediately after the post-policy `tcgetattr` and `TIOCGETD`, the example uses
`TIOCINQ` without flushing or draining. V2 records either `Observed { bytes }`
or `Unavailable { raw_os_error }`. A positive count means those bytes were
already queued and normal capture will preserve them. A zero count cannot
prove earlier bytes were not discarded, transformed, consumed as flow control,
or echoed under inherited state. Each entry is one userspace read chunk;
`read_observed_at_micros` is not a UART arrival, wire, controller-send, or
per-byte timestamp.

The example intentionally performs no automatic termios restoration on close.
Restoring inherited state while physically connected could itself re-enable
`ECHO`, `IXOFF`, `HUPCL`, or other behavior; any adapter restoration is a HUMAN
post-disconnect action. Driver-dependent `TIOCGICOUNT` line-error telemetry is
possible future hardening, not part of R3A.0.1 or an R3A.1 blocker.

The Linux-only example builds with:

```bash
cargo build -p liftassistant-lce --example lce_passive_serial_capture --locked
```

Its runtime arguments are exactly `--device <absolute-path> --output
<new-file> --duration-seconds <1..300>`. The output is create-new. Running it
against a real `/dev/tty*` was not authorized or performed in R3A.0.

## Pre-field assurance split

The [R3A.0.2 evidence freeze](LCE_R3A_XDB1_EVIDENCE.md) remains common to both
field-assurance paths. It separates direct KONE facts from CPU561 pin-direction
inference and unknown idle-output behavior.

The [software-RX-only field contract](LCE_R3A_SOFTWARE_RX_ONLY_FIELD_PATH.md)
defines the lower-assurance path for a first controlled observational session.
It explicitly accepts that a normal RS-232 adapter may retain an electrically
capable TX path and that tty/driver lifecycle side effects are not proven absent.

The [R3A.0.3 physical RX-only design contract](LCE_R3A_PHYSICAL_RX_ONLY_DESIGN.md)
and future R3A.0.4 isolated bench remain the stronger hardened path. Passing
that path is required before claiming `PHYSICALLY_PASSIVE`, but is not a
prerequisite for a separately HUMAN-approved software-RX-only R3A.1 session.

None of these documents by themselves authorizes a real controller connection.

## FIELD_PRECHECK for a future software-RX-only R3A.1 session

Before a real field open, a HUMAN must separately confirm all of the following:

1. the controller-side connector/cable identity is supported by reviewed
   CPU561/XDB1 evidence, vendor-compatible hardware, or qualified inspection;
   the current high-confidence per-pin inference alone must not be used to
   invent an ad-hoc field cable;
2. the adapter is a real RS-232 interface appropriate to the documented port,
   not a TTL-UART-level adapter, and its exact identity is recorded;
3. the operator explicitly accepts the residual assurance state
   `SOFTWARE_RX_ONLY=true`, `PHYSICALLY_PASSIVE=false/unproven`: the adapter
   may have a connected TX line, idle line-driver state, modem-control behavior,
   and driver/open/close side effects even though LiftAssistant exposes no TX;
4. the exact target device path is selected explicitly without discovery,
   active baud scanning or automatic fallback;
5. ModemManager, serial-getty, gpsd, brltty and other processes are not owning
   or probing the target tty;
6. the reviewed `O_RDONLY`, exclusive-mode, N_TTY and termios capture policy
   remains unchanged, with no write, break, modem-output, query or unlock API;
7. silence, malformed bytes, disconnect or setup failure terminate or preserve
   evidence without triggering any active probe.

Read-only process/service inspection (`lsof`, `fuser`, process listings,
service state, and current opener inspection) may support the precheck. A
permission-limited or unavailable check is `Unknown`, never a guarantee that
no opener exists. LiftAssistant must not stop services, kill processes, change
udev rules, or modify system configuration automatically. ModemManager's own
documentation shows that eligible serial ports can be opened and sent AT
probe commands; service isolation is therefore a first-class field condition.

No CPU561/XDB1/LCEUIO pinout is guessed here. If connector/cable identity,
adapter electrical class, or service isolation is not positively established,
R3A.1 must not open the device.

## Future HUMAN gate

`LCE-R3A.1 — FIRST REAL SOFTWARE-RX-ONLY FIELD CAPTURE` still requires a
separate explicit HUMAN approval for the actual session. That approval may
accept the software-RX-only assurance level without completing R3A.0.4, but the
session must be reported truthfully as non-physically-passive/unproven.

The first session is observation only: no `STA`, `CMD`, `MDE`, V-test
navigation, RETURN, Escape, Ctrl-C serial byte, unlock, keepalive, active baud
probe, or other intentional TX. Silence, banner text, `?`, `VTEST1`, unknown
bytes, and disconnect are all valid evidence outcomes.
