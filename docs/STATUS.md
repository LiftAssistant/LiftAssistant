# Project status

Updated: 2026-11-12

## Current milestone

**M1 — Interactive Simulator**

M0 bootstrap is complete. Stop polishing bootstrap infrastructure unless it blocks M1.

## Current baseline

Working today:
- canonical model types for cars, floors, motion, doors, modes, capabilities, calls, and intents;
- pure nearest-eligible-car dispatch;
- runtime trait and semantic runtime events;
- deterministic fixed-duration simulator tick;
- simulated movement, arrival, and door-open state;
- out-of-service filtering in dispatch;
- interactive TUI with valid hall calls, fixed stepping, car details, and bounded event trace;
- minimal CI as a compile/lint/regression gate.

The interactive loop and deterministic replay are implemented; availability fallback is next READY.

## M1.1 — Interactive loop

**Completed.**

- Two-car/eight-floor building and exact car detail views.
- Valid floor/direction selection (1F UP only, 8F DOWN only).
- Calls flow through existing dispatch and runtime submission.
- Running/paused state, explicit 500 ms single step, reset, quit.
- Last decision plus a bounded 100-entry trace of decisions/runtime events/errors.
- Alternate screen/raw mode restored on normal exit and ordinary error returns.

Validation: formatting, Clippy, six workspace tests, and a PTY smoke test of calls,
pause/single-step, arrival/open door, reset, boundary rejection, and terminal restoration.

UI timing only requests fixed steps; wall-clock elapsed never enters simulator physics.

HUMAN_PENDING: optional TUI usability/visual feedback; does not block replay or fallback.

## M1.2 — Deterministic replay

**Completed.**

- Developer-only in-memory commands represent hall calls and individual fixed steps.
- Live input and replay share the same deterministic application execution path.
- P runs the recorded session twice from fresh reference simulators; compares complete
  semantic outputs and final states, checks against the live state, and pauses the result.
- Recording is bounded at 2,048 commands; incomplete recordings cannot be replayed.
- Reset starts a fresh recording. No file format or public schema was needed.

Validation: nine workspace tests (including nonempty semantic replay, a changed-step
negative check, replay continuation, and recording overflow) and PTY replay smoke passed.

## M1.3 — Availability fallback

**READY — next implementation slice.**

Make the existing out-of-service behavior visible end to end:
- mark the normally preferred car out of service using simulator/developer controls;
- create the hall call;
- show that dispatch selects another eligible car;
- expose the decision and resulting event trace in the TUI.

## M1 acceptance criteria

M1 is done when:
- `cargo run -p liftassistant-tui` opens an interactive simulator;
- hall calls can be created from the UI;
- dispatch -> intent -> movement -> arrival -> door state is visible in one flow;
- pause/single-step is deterministic;
- replaying identical inputs produces identical semantic events;
- the out-of-service fallback works visibly;
- no real elevator protocol or low-level actuator control has been introduced.

## After M1

**M2 — Semantic core**
- multiple pending calls;
- call lifecycle rather than one-shot dispatch;
- decision reasons/diagnostics;
- refine observed state/capability semantics against standards research;
- extract shared orchestration only if multiple frontends actually need it.

**M3 — Adapter laboratory**
- virtual/recorded/read-only adapters first;
- map established standards;
- define an external adapter boundary from demonstrated needs.

## Explicit non-goals right now

- real passenger-elevator control;
- cloud services;
- production web/mobile UI;
- database infrastructure;
- plugin marketplace or plugin ABI;
- microservices;
- custom CANopen stack;
- broad CI matrices;
- coverage targets;
- fuzzing/benchmark programs;
- certification/conformance claims;
- license selection without human review.
