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
the application never calls `write(2)`. Physical RX-only construction is a
separate HUMAN hardware gate.

## Ownership and evidence flow

The experimental Linux example is intentionally separate from the R2
interpreted terminal artifact:

```text
explicit Linux tty path
  -> O_RDONLY acquisition
  -> LCE passive capture v1
  -> exact chunks returned by read(2)
  -> offline replay
       -> R1 transcript decoder
       -> R2.3 terminal-context classifier
```

The acquisition layer does not infer floor, door, motion, mode, fault, prompt
meaning, or controller identity. R1/R2.3 interpretation is offline and does
not call `issue_query`. The existing R2 active session, fixed `STA`/`CMD`/`MDE`
bytes, authority model, Core, and Engine are unchanged.

## Artifact v1

`FinalizedLcePassiveCapture` is internal/experimental and independent of the
R2 capture v4 format. It records:

- opaque acquisition session ID and existing
  `LIFTASSISTANT_SYNTHETIC`/`FIELD_EVIDENCE_CANDIDATE` origin;
- `LINUX_TTY_DELIVERED_RX` acquisition layer;
- requested historical 9600/8N1 terminal profile;
- requested duration and normalized open policy;
- normalized termios before and after applying the local capture policy;
- requested/observed exclusive-mode status;
- relative monotonic microseconds and exact nonempty byte chunks returned by
  Linux `read(2)`;
- bounded termination reason and SHA-256.

The bounds are session ID 128 bytes, 4096 entries, 64 KiB per read chunk,
1 MiB aggregate RX, 5 MiB serialized artifact, and 1–300 seconds. EOF, peer
hangup, read or poll error, duration, or evidence limit ends the current
acquisition. There is no reopen, reconnect, port discovery, or session reuse.
SIGINT finalization is not implemented; the required duration is the normal
bounded stop mechanism.

SHA-256 provides mutation detection only. It is not source authentication, a
signature, or chain-of-custody proof. A zero-entry artifact means only that
Linux delivered no tty RX bytes during the configured window.

## Linux open and termios policy

The example requires one HUMAN-supplied absolute device path. It neither
enumerates nor scans devices. Its open flags are:

```text
O_RDONLY | O_NOCTTY | O_NONBLOCK | O_CLOEXEC
```

It requests `TIOCEXCL` immediately, then uses `TIOCGEXCL` when supported. The
artifact distinguishes observed exclusive mode from a requested-but-
unverifiable result. `TIOCEXCL` fences later ordinary opens; it does not prove
that no process already had the tty open. The PTY proof checks `F_GETFL &
O_ACCMODE == O_RDONLY`, `O_NONBLOCK`, `FD_CLOEXEC`, a later `EBUSY` open (except
the documented privileged bypass), and `EBADF` from a synthetic-only write
attempt on the read-only slave descriptor.

The code reads and records termios before configuration, applies the historical
9600/8N1 profile, and reads back the observed result. The following are
explicit **LiftAssistant local capture policy**, not KONE protocol facts:

- disable `IXON`, `IXOFF`, `IXANY`, CR/LF translation, `ISTRIP`, canonical
  input, echo, signal generation, extended input processing, output post-
  processing, RTS/CTS flow control, and `HUPCL`;
- enable `CS8`, `CREAD`, and `CLOCAL`; disable parity and two-stop-bit mode;
- set input/output speeds to 9600 and `VMIN=0`, `VTIME=0`.

The acquisition code never invokes `tcflush`, `TIOCMSET`, `TIOCMBIS`,
`TIOCMBIC`, RTS/DTR setters, or break generation. It still cannot prevent a
Linux tty/USB serial driver from changing RTS/DTR or device state as an
open/close side effect.

The Linux-only example builds with:

```bash
cargo build -p liftassistant-lce --example lce_passive_serial_capture --locked
```

Its runtime arguments are exactly `--device <absolute-path> --output
<new-file> --duration-seconds <1..300>`. The output is create-new. Running it
against a real `/dev/tty*` was not authorized or performed in R3A.0.

## FIELD_PRECHECK for a future R3A.1 session

Before a real field open, a HUMAN must separately confirm all of the following:

1. the controller-side connector and signal direction from verified hardware
   documentation or direct qualified inspection;
2. a physical RX-only path with no capture-side TX/RTS/DTR/controller-RX
   connection;
3. the exact target device path and adapter identity without discovery or baud
   scanning;
4. that ModemManager, serial-getty, gpsd, brltty, and other processes are not
   owning or probing the target tty;
5. that the software open/termios policy remains the reviewed R3A.0 policy.

Read-only process/service inspection (`lsof`, `fuser`, process listings,
service state, and current opener inspection) may support the precheck. A
permission-limited or unavailable check is `Unknown`, never a guarantee that
no opener exists. LiftAssistant must not stop services, kill processes, change
udev rules, or modify system configuration automatically. ModemManager's own
documentation shows that eligible serial ports can be opened and sent AT
probe commands; service isolation is therefore a first-class field condition.

No CPU561/XDB1/LCEUIO pinout is guessed here. If the physical signal direction
or service isolation is not positively established, R3A.1 must not open the
device.

## Future HUMAN gate

`LCE-R3A.1 — FIRST REAL SOFTWARE-RX-ONLY FIELD CAPTURE` requires separate
HUMAN approval after the physical and system prechecks above. Its first session
is observation only: no `STA`, `CMD`, `MDE`, V-test navigation, RETURN, Escape,
Ctrl-C serial byte, unlock, or other TX. Silence, banner text, `?`, `VTEST1`,
unknown bytes, and disconnect are all valid evidence outcomes.
