# Roadmap

## M0 — Bootstrap
Repository principles, memory/status, Rust workspace, model/core/runtime/sim/TUI boundaries, CI.

## M1 — Core + deterministic simulator + TUI
- 2 cars / 8 floors
- hall calls
- nearest eligible-car dispatch
- ServeFloor intent
- simulated movement/arrival/door state
- event trace
- developer TUI
- deterministic/replayable tests

Acceptance:
1. 8F up call is assigned and served.
2. With car A out of service, eligible car B is selected.

## M2 — LiftAssistant semantics
Capabilities, authority, normalized diagnostics, service profiles/policy, configuration, fault injection. Do not prematurely design a large policy DSL.

## M3 — Conformance/research tooling
Scenario format, replay, backend conformance suite, trace inspection, standards mappings.

## M4 — Real-world bridges
Controlled non-passenger work first: virtual CAN/SocketCAN, CiA 417, T/GSET 001-2025, HIL/bench, documented controller APIs.

Any passenger-elevator control requires HUMAN-REVIEW mode.

## M5 — User/building integrations
Stable API, web UI, Home Assistant, BACnet/building integration, installation/update/rollback story, later evaluate an appliance/OS distribution.
