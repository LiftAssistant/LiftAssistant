# Project memory

Last consolidated: 2026-11-12

Durable decisions only.

## Identity
- Project name: LiftAssistant.
- Earlier working names: OpenLift, LibreLift.
- OpenLift was rejected as a formal name due to existing software/elevator-industry uses.
- LiftAssistant reflects a neutral control/integration layer, analogous in spirit to Home Assistant unifying fragmented smart-home ecosystems.

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
4. Core is functional and vendor-neutral.
5. LiftAssistant says WHAT; validated runtime/safety systems decide HOW.
6. Simulator is the first runtime backend and should become deterministic/replayable.
7. TUI is the first observable client, not the architecture.
8. No early real passenger-elevator control.
9. Existing standards are inputs: CiA 417, BACnet, SAREF4LIFT, T/GSET 001-2025, Open-RMF/RFA, THOR/NeXt, etc.
10. Adoption can be layered: observe, diagnostics, policy, group control, native runtime.

## Product philosophy
- Capabilities rather than OEM SKU gates should determine features.
- Hide industrial complexity from ordinary users unless requested.
- Developer tooling may expose raw events/protocol details.
- A future Home Assistant integration is a useful validation target.

## First milestone
~~~text
2 cars / 8 floors
hall call
-> dispatcher
-> high-level intent
-> simulator
-> observed movement/state
-> developer TUI
~~~

Second scenario: one car out of service; dispatch chooses another eligible car.

## License
No license selected. Human-review decision.
