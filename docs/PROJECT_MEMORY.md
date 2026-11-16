# Project memory

Last consolidated: 2026-11-14

This file contains durable project facts and decisions only.

Current milestone/progress belongs in `docs/STATUS.md`. Agent workflow belongs in `AGENTS.md`. Architecture details belong in `docs/ARCHITECTURE.md`.

## Identity

- Project name: LiftAssistant.
- Earlier working names: OpenLift, LibreLift.
- OpenLift was rejected as a formal name due to existing software/elevator-industry uses.
- LiftAssistant reflects a neutral operational/integration layer, analogous in spirit to Home Assistant unifying fragmented smart-home ecosystems.

## Mission

LiftAssistant should make elevator systems behave more like an open computing platform:

- normalize fragmented controller/protocol ecosystems;
- expose common semantic state and capabilities;
- express high-level intents/policies independently of vendors;
- support simulation and real hardware through the same conceptual runtime boundary;
- integrate existing standards instead of replacing them.

It is not primarily a new elevator brand and not merely a monitoring dashboard.

## Durable architecture decisions

1. Public semantics are language-independent.
2. Rust is the primary reference implementation language.
3. Monorepo first; split only for genuinely independent lifecycles.
4. Core is functional/deterministic and vendor-neutral.
5. LiftAssistant says WHAT; validated runtime/safety systems decide HOW.
6. Low-level brake/drive/torque/safety-bypass primitives do not belong in Core.
7. Simulator is a first-class reference runtime backend and must remain deterministic/replayable.
8. TUI is a developer observable/composition client, not the architecture.
9. No early real passenger-elevator control.
10. Existing standards are inputs: CiA 417, BACnet, SAREF4LIFT, T/GSET 001-2025, Open-RMF/RFA, THOR/NeXt, and other relevant work.
11. Adoption can be layered: observe, diagnostics, policy/group semantics, adapters, later native runtime where appropriate.
12. Avoid premature application/service/plugin layers; extract them only when demonstrated by real callers.

## Product philosophy

- Capabilities rather than OEM SKU gates should determine features.
- Hide industrial complexity from ordinary users unless requested.
- Developer tooling may expose raw events/protocol details.
- A future Home Assistant integration is a useful validation target, not an early milestone requirement.
- Prefer a working vertical slice before adding generalized infrastructure.
- Review relevant established standards/mature implementations before inventing durable public-facing semantics.

## Validated M1 baseline

M1 Interactive Simulator was functionally implemented and human-accepted in November 2026.

Validated capabilities include:

~~~text
HallCall
  -> deterministic dispatch
  -> high-level ServeFloor intent
  -> semantic runtime boundary
  -> deterministic simulator movement
  -> arrival / door-open state
  -> observable developer TUI
~~~

The developer TUI also validates fixed-step pause/single-step behavior, deterministic replay, availability fallback, no-eligible-car behavior, and bounded event tracing.

The key limitation intentionally left at M1 close was that calls were one-shot and could replace a car's current target; M2 replaced that behavior with persistent call semantics.

## Validated M2 call semantics

M2 uses deterministic sequential `CallId` values and a Core-owned internal lifecycle: `Pending -> Assigned(car) -> Serving(car) -> Completed`. Active duplicate floor+direction calls share identity; a later request after completion receives a new identity.

Pending calls are considered in identity order. Only Normal, capable, semantically idle cars are eligible, so a later call cannot replace an Assigned/Serving trip. Runtime intent acceptance starts service; matching arrival plus door-open feedback completes service and releases the car for immediate pending-work reevaluation. Core retains arrival evidence across event batches rather than relying on simulator-specific event grouping.

Core dispatch results include deterministic per-car explanations: observed mode, semantic/runtime occupancy, serve-floor capability availability, distance, rejection reasons, and whether selection used sole eligibility, nearest distance, or the car-ID tie-break. These are orchestration explanations and must not be conflated with standardized equipment faults/alarms.

The current building configuration is intentionally narrow: `BuildingTopology` is a canonical ordered nonempty set of served floors used by Core call validation and by composition/replay. It supports non-contiguous floors but does not yet claim public floor naming, door/service mapping, spatial modeling, or protocol index semantics.

These call semantics remain an internal, reversible model informed by CiA 417, BACnet, SAREF4LIFT, and Open-RMF prior art. They are not a frozen normative public schema.

## Validated M4 read-only boundary

The first non-simulator backend is a deterministic, native-semantic recorded runtime. It advertises observation capabilities only, rejects every submitted intent, and advances timed observation frames solely through explicit elapsed input. It defines no file format or live transport.

The first external mapping experiment uses recorded Open-RMF `LiftState` fields. Explicit floor-label configuration and stopped/open at-floor evidence can produce a complete internal `CarState` for Fire/Offline modes while retaining source session/mode context. Moving/unknown position, undirected door movement, closed-door position uncertainty, Human/AGV control modes, unsupported modes, and unknown/unadvertised floors remain structured errors rather than guessed values.

This evidence shows a boundary between complete operational state and partial external observations. No partial-observation model, normative mapping, live middleware dependency, protocol write, or hardware path has been approved or frozen.

## License

No license selected. This remains a HUMAN-REVIEW decision.
