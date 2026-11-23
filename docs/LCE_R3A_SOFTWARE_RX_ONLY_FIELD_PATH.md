# LCE-R3A software RX-only field assurance contract

Status: `DESIGN_CONTRACT_ESTABLISHED — REAL_FIELD_CAPTURE_NOT_AUTHORIZED`

Reviewed: 2026-11-23

## Purpose

This contract defines the lower-assurance path for a first real observational
CPU561/XDB1 session after the R3A.0.2 evidence freeze.

It does **not** claim physical passivity. A normal reviewed RS-232 adapter may
retain an electrically capable transmit path, idle line-driver state,
modem-control behavior, and open/close or driver side effects. The accepted
assurance statement for this path is:

```text
SOFTWARE_RX_ONLY=true
APPLICATION_PROTOCOL_TX_REQUESTED=false
KERNEL_DRIVER_TX_FREE=false/unproven
PHYSICALLY_PASSIVE=false/unproven
REAL_LCE_COMPATIBLE=unproven
```

The purpose of this path is narrow: observe what the existing controller
interface makes available without LiftAssistant intentionally transmitting
protocol data, changing controller settings, unlocking an interface, or
navigating an interactive terminal.

The higher-assurance
[R3A.0.3 physical RX-only contract](LCE_R3A_PHYSICAL_RX_ONLY_DESIGN.md) remains
valid and unchanged in strength. R3A.0.4 isolated bench validation is required
before any future `PHYSICALLY_PASSIVE` claim, but it is not a prerequisite for
a separately HUMAN-approved session under this software-only contract.

## Application invariant

The acquisition application must retain the reviewed R3A.0/R3A.0.1 boundary:

- one HUMAN-supplied absolute tty path;
- `O_RDONLY | O_NOCTTY | O_NONBLOCK | O_CLOEXEC`;
- requested exclusive mode and recorded evidence when observable;
- no serial `write(2)` surface;
- no break-generation surface;
- no DTR/RTS/modem-output surface;
- no device enumeration, reconnect or automatic port discovery;
- no active baud scan or probe;
- no LCEUIO emulation, unlock or counter handling;
- no terminal navigation, keepalive or observation query;
- no active fallback after silence, malformed bytes, disconnect or error;
- offline-only interpretation after bounded raw capture.

The parser, R1/R2.3 logic, Core, Engine, Host, API, CLI, Flutter client and
plugins receive no LCE transport authority from this contract.

## Residual risk accepted by this assurance level

This path explicitly does not hide the difference between application intent
and electrical behavior.

A HUMAN approving an actual field session must accept that:

1. a conventional RS-232 adapter can remain physically full-duplex even when
   LiftAssistant opens the tty read-only;
2. its TX line can hold a normal RS-232 idle state;
3. USB/UART drivers may manipulate adapter state while enumerating, opening,
   closing, rebooting or faulting;
4. RTS/DTR or other control outputs may exist at the adapter even though the
   LiftAssistant application never exposes controls for them;
5. inherited tty/line-discipline state can exist during the open-to-policy
   configuration window;
6. another process can violate the no-active-traffic assumption if it opens or
   probes the same adapter;
7. therefore this path cannot establish `WIRE_PASSIVE`,
   `KERNEL_TX_FREE`, `ZERO_INFLUENCE` or `PHYSICALLY_PASSIVE`.

These are residual assurance limits, not evidence that a normal RS-232
connection will modify controller settings. The project simply does not claim
that software discipline proves electrical one-way behavior.

## Hardware and system preconditions

This contract does not provide a pin recipe or approve an improvised cable.

Before an actual R3A.1 session, a HUMAN must review and record:

1. **Connector/cable identity.** The controller-facing connection is supported
   by CPU561/XDB1-specific documentation, a known vendor-compatible cable, or
   qualified inspection. The current high-confidence per-pin inference alone
   is not enough to invent an ad-hoc field cable.
2. **Electrical class.** The host adapter is a real RS-232 interface appropriate
   to the documented port, not a TTL-UART-level adapter.
3. **Adapter identity.** The exact adapter and host tty identity are known before
   capture; LiftAssistant does not discover or select a device automatically.
4. **Service isolation.** ModemManager, serial-getty, gpsd, brltty and other
   relevant processes are not already owning or probing the tty. Unknown
   inspection results remain Unknown.
5. **Reviewed capture policy.** The R3A.0/R3A.0.1 open/termios policy and public
   no-write/no-modem-output API boundary remain intact.
6. **Residual-risk acceptance.** The approving HUMAN explicitly accepts
   `PHYSICALLY_PASSIVE=false/unproven` for this session.
7. **No active recovery.** Silence or failure never causes an automatic query,
   baud scan, unlock, keepalive, reconnect or terminal command.

The project must not automatically stop services, kill processes, change udev
rules, reconfigure the host, or alter controller configuration to satisfy these
preconditions.

## Hardened runner acknowledgement and planning

The Linux runner requires the exact command-line acknowledgements
`--assurance software-rx-only` and `--profile historical-9600-8n1` for both
planning and execution. No physical-passivity value, generic serial profile,
baud scan, fallback, or hardware discovery is available. `--plan` validates
the normal arguments and prints the proposed path, output, duration, profile,
and assurance without opening hardware, creating an artifact, issuing an
ioctl, or inspecting services.

Before real acquisition the runner states that the LiftAssistant application
requested no controller protocol TX while
`KERNEL_DRIVER_TX_FREE=false/unproven` and
`PHYSICALLY_PASSIVE=false/unproven`. It reserves the output path without
overwrite and then enters the existing reviewed backend. Ordinary errors
before complete artifact write and `sync_all` remove the reservation. This is
evidence hygiene, not crash-atomic persistence: SIGKILL, abort, kernel crash,
or power loss may leave an empty or partial file, which must fail or pass the
normal artifact parser/integrity checks on its own merits.

These guardrails and the final bounded software correctness pass establish
`PRE_FIELD_SOFTWARE_CORRECTNESS_COMPLETE`; they do not authorize an R3A.1
session, approve any adapter/cable/tty, prove CPU561 compatibility, or upgrade
the physical assurance level.

## First-session evidence semantics

A future R3A.1 session under this contract is observation only.

Allowed outcomes include:

- bounded received bytes;
- banner or prompt text;
- unknown or uninterpretable bytes;
- `PassiveSilent`;
- early disconnect/HUP/error;
- setup refusal because a precondition is not satisfied.

No outcome authorizes a follow-up transmit action.

In particular:

```text
PASSIVE_SILENT
!= permission to send one test byte
!= permission to scan baud rates actively
!= proof of wrong wiring
!= proof of a locked port
!= proof of request/response-only behavior
```

Any protocol interpretation happens from the finalized artifact offline.

## Relationship to the hardened path

The two assurance levels answer different questions:

| Path | What it can establish | What it cannot establish |
| --- | --- | --- |
| Software RX-only | LiftAssistant intentionally requested no protocol TX and exposes no application write/modem-output surface | Electrical one-way behavior, zero adapter/driver influence, physical passivity |
| Physical RX-only hardened path | After R3A.0.4 evidence, a reviewed candidate may support a physical one-way claim within its tested envelope | Protocol compatibility or useful unsolicited data by itself |

Failure or silence on the software path does not automatically escalate to the
physical path or to active diagnostics. The hardened path may be pursued
independently for stronger assurance.

## HUMAN gate

This document changes the architecture decision about **what may be considered**
for first field observation; it does not authorize the actual observation.

`LCE-R3A.1 — FIRST REAL SOFTWARE-RX-ONLY FIELD CAPTURE` remains
`HUMAN_PENDING`.

A future approval must identify the actual adapter/cable, host tty, field
conditions and accepted assurance level. Without that explicit approval:

- no real `/dev/tty*` field open;
- no CPU561/XDB1 attachment;
- no LCEUIO;
- no active terminal interaction;
- no controller query or write.

Gate result:

`SOFTWARE_RX_ONLY FIELD CONTRACT ESTABLISHED — REAL FIELD CAPTURE NOT AUTHORIZED.`
