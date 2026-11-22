# LCE-R3A.0.3 — Physical RX-only design contract

Status: `DESIGN_CONTRACT_ESTABLISHED — BENCH_VALIDATION_NOT_PERFORMED`

Reviewed: 2026-11-23

## Scope and safety invariant

This is a design contract, not a schematic, cable recipe, approved hardware
design or field procedure. It deliberately gives no instruction for connecting
specific pins to a live controller.

The required invariant for any future candidate is:

> The LiftAssistant acquisition system has no intentional electrical capability
> to transmit data or control signals into the controller-side receive path.

Software RX-only is necessary but insufficient. The physical boundary must
remain safe against host UART/USB initialization, modem-control outputs,
background probing, accidental software writes and the reviewed single-fault
conditions. No candidate is `PHYSICALLY_PASSIVE` until a later isolated bench
gate produces reviewable evidence.

## Required layer separation

```text
physical acquisition boundary
  -> OS/backend capture
  -> bounded raw capture artifact
  -> offline decoding and analysis
  -> optional higher LiftAssistant semantic projection
```

The physical boundary owns electrical directionality and isolation. The Linux
backend owns only an explicit tty path and `LINUX_TTY_DELIVERED_RX`. The raw
artifact preserves evidence before interpretation. R1/R2.3 analysis remains
offline. Core and Engine receive no LCE transport capability.

No parser, UI, API, Host, CLI, Flutter client, Engine, adapter or plugin may
obtain a serial transmit, modem-output, unlock, navigation or active-probe
surface through this design.

## Candidate-design requirements

A future candidate submitted to bench review must demonstrate all of the
following without relying on application intent alone:

1. **No controller-receive path.** No host or adapter transmit output has an
   intentional conductive, capacitive, powered or software-selectable path into
   the controller-side receive signal.
2. **No modem-control path.** DTR, RTS and other adapter control outputs cannot
   reach controller data, control, reset, unlock or power-related conductors.
3. **Receive-only function.** The acquisition-side receive function remains
   capable of observing the controller-origin signal without adding a reverse
   command path.
4. **Power-path control.** Host/USB/adapter power cannot backfeed the controller
   connector or create an unreviewed reference/power path.
5. **Ground-potential treatment.** The design must explicitly analyze expected
   common-mode/ground differences rather than assuming signal ground is always
   harmless.
6. **Fault containment.** Defined adapter, component and cable faults must fail
   without creating an intentional controller-input drive path.
7. **Orientation and identity control.** Connector orientation, board/interface
   identity and signal-direction assumptions must be reviewable and resistant
   to an accidental reversed or wrong-connector installation.
8. **Software regression containment.** A future code regression, generic
   serial library, OS probe or direct syscall must not be able to defeat the
   physical one-way property.
9. **No active fallback.** Silence, malformed bytes or setup failure must never
   trigger a query, keepalive, baud probe, unlock attempt or modem-line change.
10. **Inspectable construction.** The exact candidate revision, schematic,
    component identity, connector mapping and assembly must be identifiable for
    review; undocumented substitutions invalidate prior evidence.

These are requirements for a later candidate. They do not approve a particular
component, topology or implementation.

## Threat model and required future evidence

| Threat | Required design mitigation | Evidence required at the later isolated bench gate |
| --- | --- | --- |
| Host UART/USB adapter transmits during enumeration, open, close, reboot or fault | Physical boundary blocks every host-origin data path toward controller receive | Instrumented observation across power-up/down, enumeration, process start/stop and host reboot states |
| DTR/RTS or other modem-control outputs toggle automatically | Those outputs are absent from the controller-facing path and cannot be repurposed by software | Continuity/isolation inspection plus instrumented control-line toggle attempts on the isolated bench |
| OS service or background process probes a tty | Device/process isolation plus a physical boundary that remains one-way even if probing occurs | Service inventory and a deliberate synthetic probe attempt proving no controller-facing signal crosses the candidate |
| LiftAssistant or a future dependency accidentally calls `write(2)` | `O_RDONLY`/API compile-fail controls remain, while hardware blocks transmission independently | Existing software regressions plus isolated bench fault injection; neither alone is sufficient |
| Ground-potential difference or unexpected power path | Reviewed isolation/reference strategy with defined ratings and no USB/host backfeed | Electrical review and bounded bench measurements under the candidate's stated operating envelope |
| Adapter/component fault | Defined single-fault analysis does not create a controller-input drive path | Reviewable fault analysis and safe bench injections selected by a qualified HUMAN reviewer |
| Cable fault, wrong orientation or incorrect connector | Keying, labeling, mapping review and fail-safe construction prevent a plausible mistake from becoming a transmit path | Independent visual/continuity inspection against the frozen candidate revision before energization |
| CPU561/XDB1 pin-direction inference is wrong | No field use until direction is directly verified or safely resolved in an isolated non-passenger fixture | Source review or isolated qualified measurement; the evidence-freeze inference is insufficient |
| Silence is misdiagnosed and triggers active behavior | `PassiveSilent` remains a terminal, inconclusive observation outcome | Synthetic silent-artifact regression and review showing no query/fallback path exists |

## R3A.0.4 isolated bench-validation contract

R3A.0.4 is a future stage and is **not authorized by this document**. If a
HUMAN later authorizes it, the bench must be isolated from passenger equipment
and real elevator controllers. The validation package must identify:

- candidate schematic/assembly revision and independently checked orientation;
- source/sink fixtures and instruments used, including relevant calibration or
  self-test evidence;
- continuity/isolation results for every possible host-origin data, control and
  power path considered by the threat model;
- observations during power transitions, adapter enumeration, tty open/close,
  host reboot and deliberate host-side TX/modem-control attempts;
- normal receive evidence and `PASSIVE_SILENT` behavior;
- fault injections selected by a qualified reviewer and their limits;
- all deviations, ambiguous readings and negative results.

Passing ordinary RX, compiling the Rust code, seeing no bytes, or observing one
quiet oscilloscope trace is not sufficient. Bench evidence must show that the
candidate's one-way property survives the required lifecycle and fault cases.

## Stop conditions

Stop for HUMAN review if the candidate would require:

- any controller-side receive, DTR, RTS, unlock, reset or power conductor;
- active controller queries, keepalives, baud probes or terminal navigation;
- an assumption that the current pin-direction inference is confirmed;
- live passenger equipment to resolve a design uncertainty;
- software permission, process discipline or `O_RDONLY` as the only TX barrier;
- a claim of electrical safety, certification or field readiness without an
  independent qualified review.

## Gate result

`LCE-R3A.0.3 DESIGN CONTRACT ESTABLISHED — BENCH VALIDATION NOT PERFORMED.`

The next possible stage is separately authorized R3A.0.4 isolated bench
validation. R3A.1 remains behind a later HUMAN gate even if bench validation
eventually passes.
