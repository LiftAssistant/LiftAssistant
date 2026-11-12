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
- bootstrap console program that runs the 8F-UP vertical slice;
- minimal CI as a compile/lint/regression gate.

The current console program proves the modules connect, but it is not yet the intended developer experience.

## M1.1 — Interactive loop

Next implementation work, in order:

1. Replace the hard-coded bootstrap run with an interactive TUI.
2. Render two cars across eight floors.
3. Show position, motion, door state, target, mode, and capabilities needed for the current decision.
4. Let the user create a valid hall call without editing source.
5. Show the dispatch decision and generated high-level intent.
6. Drive simulation through an explicit fixed step.
7. Add pause, single-step, resume, reset, and quit.
8. Keep a bounded visible decision/runtime-event trace.

UI timing may use wall-clock time to request simulation steps. Core and simulator results must depend only on explicit inputs and the supplied simulation duration.

## M1.2 — Deterministic replay

After M1.1 works:
- represent scenario input as explicit developer/simulator commands;
- replay the same command sequence against a fresh simulator;
- compare the resulting semantic event sequence;
- introduce a small scenario file format only when replay code needs one.

Do not build a generic conformance framework yet.

## M1.3 — Availability fallback

Then make the existing out-of-service behavior visible end to end:
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
