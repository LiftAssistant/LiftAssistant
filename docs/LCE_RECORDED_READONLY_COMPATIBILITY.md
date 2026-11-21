# LCE-R0 recorded read-only compatibility probe

Status: `RECORDED_READONLY_PARTIAL`

This is an experimental legacy-LCE compatibility research result. It is not a
KONE-supported integration, a serial implementation, a controller tool, a
conformance statement, or authorization to connect to any elevator. The code
accepts only original synthetic recorded fixtures and emits existing canonical
observation updates. It contains no terminal command generation, unlock logic,
serial transport, write path, hardware access, or real-controller fixture.

## Source hierarchy and evidence boundary

Sources were reviewed on 2026-11-20. No source document, screenshot, binary,
firmware, or proprietary software is stored in this repository.

1. **KONE-origin interface evidence.** *LCEUIO Dongle User Manual*, EDMS
   884634en.A.2, first issue 2006, sections 1--3, identifies LCECPU40,
   LCECPU561 and LOPCB-side serial access, places LCETERM downstream of the
   LCEUIO device, and documents the counter-related lock boundary. The reviewed
   copy is a [public mirror of the KONE-origin document](https://v6-file.globalso.com/file_manage/1579/20240913/kone-service-tool.pdf).
   It does not document the runtime terminal table grammar.
2. **Current independent interoperability evidence.** ELFIN's
   [KONE LCE CPU40 installation manual, version 20250115](https://elfin.de/wp-content/uploads/2025/01/InstManual-KONE-LCE-CPU40-20250115.pdf),
   pages 5--12, shows an eleon S1/C1 monitoring gateway using the controller's
   diagnostic path and X1-side RS232 connection. EDNL's
   [Sense monitoring-module manual, version 1.15](https://www.ednl.nl/wp-content/uploads/2025/07/Handleiding-EDNL-Sense-monitoringsmodule-v1.15.pdf),
   connection table and shared-port appendix, names Kone LCE, XL17 and RS232.
   ELFIN's [MQTT Consumer API 4.0.0](https://api.eleon.elfin.de/) exposes
   controller connection, current floor, moving, per-door state, status and
   controller-specific error data. These sources corroborate real monitoring
   value and a read-side data path. They do not publish LCE terminal wire
   grammar and cannot be used to reverse-map ELFIN's normalized fields.
3. **Historical terminal semantics.** The publicly readable but weakly
   provenance-bound [*LCE Terminal Using Instructions*](https://www.scribd.com/document/379489546/LCE-Terminal-using-instructions-with-logo-KONE-pdf),
   sections describing N test and V-test `STA`, `CMD`, `CAL`, `GRP`, and `MDE`,
   documents the field meanings used by this recorded probe. The same material
   also describes memory modification, parameter changes, call generation and
   other active functions. Its read tables therefore supply no write authority.
4. **Community corroboration only.** A
   [VatorTrader discussion](https://www.vatortrader.com/forums/ubbthreads.php?Board=1&main=6770&type=thread&ubb=printthread)
   reports use of a 9600/8-bit RS232 terminal path on LCE, and an
   [Ascensoristas discussion](https://www.ascensoristas.net/viewtopic.php?style=117&t=6973)
   links the historical instructions. These anecdotes do not establish field
   semantics, safety, firmware coverage, or compatibility.

## Focused evidence matrix

| Family / field | What the reviewed material establishes | LiftAssistant treatment | Evidence limit |
| --- | --- | --- | --- |
| `STA LVL NO` | Floor the car is at or above | `FloorReference` only when stopped, in a door zone, and at a documented leveling state; otherwise `Unknown` | Never an exact continuous position by itself |
| `STA ADV POS` | Floor where the car may still stop | Adapter-local context only | Never current physical position or desired target |
| `STA SPD` + `DIR` | Stopped/slow/full and stopped/down/up codes | Canonical stopped/up/down only when the pair is consistent; contradictions become `Unknown` | Speed magnitude is not promoted |
| `STA DOOR` | Locked/opening/open/closing/closed-free plus undefined and time-encoded values | Exact simple codes map to canonical door evidence; undefined, combined and unknown values stay `Unknown` | No attempt to decode undocumented composites |
| `STA SRV`, `FLT ID`, door-zone, at-level, load, correction drive | Operational flag and diagnostic/status context | Retained as source context; door-zone/at-level can jointly qualify floor evidence | Fault bits never diagnose root cause or force `OutOfService` |
| `CMD DEST` | Controller-reported destination floor | Configured floor token becomes `ReportedDestination::Known`; documented/ambiguous sentinel becomes `Unknown` | Never `Intent`, desired state, assignment, or completion |
| `CMD PLN DIR`, door/open fields | Planned direction and destination-door timing/context | Adapter-local context only | Not current motion or canonical door state |
| `CAL` | Active up/down/car calls plus a car-position column | Research/context only; not parsed into the canonical path | Active controller calls are not LiftAssistant `HallCall`s or call ownership |
| `GRP SM/DE/FL` | Per-group-lift serving mode, destination and current floor, with special sentinels | Only explicitly bound native lift rows emit updates; floor is a floor reference and destination is reported evidence | Group/lift numbers never create `CarId`; serving mode stays local |
| `MDE LIFT MODE` | Normal, service, synchronization, fire, emergency, attendant, parking, test, out-of-service, priority and other source modes | Only exact normal, fire and out-of-service equivalences map; documented other modes are `SourceSpecific`; unknown values are `Unknown` | Service/parking/priority/test are not guessed into canonical modes |
| `MDE DOOR STATE`, fault, advance position | Simple door state plus encoded/fault values and context | Exact simple door codes map; other values and all fault/advance fields remain unknown/local | Fault ID never supplies equipment diagnosis or mode |
| `N` node number/Neuron ID/name/state/floor/side | Diagnostic node table | Source-local inspection context only | Node identity never becomes `CarId`, floor binding, or authority |

## Implemented recorded vertical

At R0, `liftassistant-lce` was a model-only normal dependency crate and parsed
bounded ASCII representations of the documented row/column families `STA`, `CMD`,
`MDE`, and `GRP`. The checked-in fixture text is original and synthetic; it is
not copied controller output. `CAL` and `N` remain research-only because they
do not add honest canonical car state for this slice.

The parser rejects oversized frames, excessive lines/group rows, oversized
tokens, non-ASCII input, wrong columns, missing values, truncation, and
unsupported families. Numeric enum values that are syntactically valid but not
semantically established are preserved as source context and map to explicit
unknown evidence where applicable. It never panics on rejected fixture input.

Mapping requires caller-supplied logical `CarId`, native-floor mapping and, for
`GRP`, explicit native-lift-to-`CarId` binding. A caller-owned
`ObservationProvenance` supplies source, session and source-global sequence.
The crate does not derive any of them from LCE lift, group, node, CPU, Neuron,
board, floor, or label values.

The Engine regression proves the actual path:

```text
synthetic recorded STA/CMD/MDE table
  -> CarObservationUpdate
  -> explicit Engine binding/session
  -> Engine assembler/resolver
  -> one operational CarState
```

`STA` and `CMD` remain incomplete until `MDE` contributes a supported mode.
Omitted fields preserve prior evidence; explicit unknown mode/door evidence
removes the operational state. A fresh session clears prior state, a delayed
old-session update is rejected, and an unbound native-looking car does not
create a logical car. No auxiliary source is used to fill fields.

## Why the result is partial

The reviewed public evidence supports a useful recorded/read-only semantic
subset without changing LiftAssistant's canonical model. It does not establish
one production parser across LCE software generations, exact raw framing and
echo behavior, full table layout variants, repeated-sample semantics,
freshness, controller lifecycle, encoded door/fault variants, or passive
transport access. No real LCE capture, bench controller, serial endpoint, OEM
tool, or hardware was used. Current commercial products corroborate that useful
monitoring data exists, but their normalized APIs do not prove how each value
is obtained from this historical terminal interface.

Consequently the terminal result is `RECORDED_READONLY_PARTIAL`, not
`RECORDED_READONLY_VIABLE` for production and not `RECORDED_READONLY_BLOCKED`.

## Smallest future evidence gaps before a hardware/serial proposal

These are planning gaps only; no hardware work is authorized or implemented.

- a separately HUMAN-approved, isolated non-passenger bench with positively
  identified LCE controller and software versions;
- qualified HUMAN confirmation of the exact diagnostic interface, electrical
  isolation and safe read-only operating procedure for that bench;
- original, privately retained captures covering idle, movement, landing,
  doors, supported modes, ambiguity, restart and malformed/interrupted output;
- evidence of raw framing, line endings, echo, table cadence and version/layout
  differences sufficient to replace the synthetic projection with a bounded
  capture parser;
- an explicit proof that any future acquisition path cannot issue terminal,
  call, memory, parameter, unlock, firmware or control writes.

M7.1b and its HIL/bench-controller HUMAN gate are unchanged. LCE-R0 grants no
serial, bench, controller, passenger-equipment, unlock, write or production
authority.
