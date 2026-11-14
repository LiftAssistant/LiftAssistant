# Project memory

Last consolidated: 2026-11-14

Durable decisions and validated long-lived facts only. Current task status belongs in `docs/STATUS.md`.

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

## Agent workflow decisions

- `AGENTS.md` at repository root is the single authoritative agent workflow file.
- Default mode is SELF-ITERATION.
- HUMAN-REVIEW is required for safety/authority, normative public spec freezes, license, foundational infrastructure/ABI changes, major rewrites, release/security policy, and certification/conformance claims.
- A HUMAN_PENDING item blocks only dependent work; independent READY work continues.
- Green CI is a gate, not a milestone.
- Tests primarily protect implemented behavior and concrete regressions.
- `agent.md` is a generic compatibility/navigation pointer only and must not fork the authoritative rules.
- Project context should remain tool-agnostic and live in repository documentation rather than vendor-specific agent files.
- Intended GitHub commit identity is the project owner's primary account `Axiaobo7788`, not alternate account `xiaobo-kipfel`.

## Documentation roles

- `docs/PROJECT_MEMORY.md`: durable decisions and validated long-lived facts.
- `docs/STATUS.md`: current milestone, current progress, READY/HUMAN_PENDING/blockers.
- `docs/ROADMAP.md`: milestone sequence and future scope.
- `docs/ARCHITECTURE.md`: architecture and safety boundaries.
- `README.md`: human-facing overview and runnable entry points.

## Product philosophy

- Capabilities rather than OEM SKU gates should determine features.
- Hide industrial complexity from ordinary users unless requested.
- Developer tooling may expose raw events/protocol details.
- A future Home Assistant integration is a useful validation target, not an early milestone requirement.
- Prefer a working vertical slice before adding generalized infrastructure.

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

The key limitation intentionally left for M2 is that calls are still one-shot and can replace a car's current target; persistent call lifecycle/multiple pending calls are not yet modeled.

## License

No license selected. This remains a HUMAN-REVIEW decision.
