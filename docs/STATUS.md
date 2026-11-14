# Project status

Updated: 2026-11-14

## Current milestone

**M2 — Semantic Core**

M1 Interactive Simulator is closed and human-accepted.

## Validated baseline

M1 established and validated:

- canonical model types for cars, floors, motion, doors, modes, capabilities, calls, and intents;
- pure deterministic nearest-eligible-car dispatch;
- runtime trait and semantic runtime events;
- deterministic fixed-duration simulator stepping;
- simulated movement, arrival, and door-open state;
- interactive two-car/eight-floor TUI;
- valid hall-call boundary handling;
- pause/resume/single-step/reset/quit controls;
- bounded decision/runtime trace;
- deterministic in-memory replay;
- availability fallback with OutOfService cars;
- no-eligible-car behavior;
- terminal restoration on normal/error exit paths.

Automated validation at M1 close:
- `cargo fmt --all -- --check`: passed;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed;
- `cargo test --workspace --locked`: 12 passed;
- Rust 1.85 all-target workspace check with committed lockfile: passed;
- PTY smoke checks passed.

Human validation on 2026-11-14:
- TUI launched and rendered correctly in Konsole;
- tested key-driven functions behaved as expected;
- M1 developer TUI accepted as the first usable version.

No remaining M1 HUMAN_PENDING item.

## Current limitation driving M2

Current call handling is still one-shot:

~~~text
HallCall
  -> immediate dispatch
  -> ServeFloor
  -> runtime target
~~~

A later call can replace an already accepted target. There is no persistent call identity/lifecycle or queue of pending calls.

This is the primary M2 problem.

## M2.1 — Call identity and lifecycle

**READY — next implementation slice.**

Introduce the smallest durable call model needed to represent active hall calls.

Target lifecycle:

~~~text
Pending
  -> Assigned(car)
  -> Serving(car)
  -> Completed
~~~

Requirements:
- calls have stable identity;
- duplicate active floor+direction calls are not multiplied unnecessarily;
- lifecycle state is observable;
- keep semantics deterministic;
- do not introduce a generic workflow/state-machine framework.

## M2.2 — Multiple pending calls

After M2.1 works:

- multiple hall calls may coexist;
- one active trip must not be overwritten by a later call;
- idle eligible cars may take pending work;
- if no eligible idle car exists, the call stays pending;
- after service completion, pending work is reevaluated deterministically;
- keep dispatch policy intentionally simple before optimizing group control.

## M2.3 — Runtime feedback drives lifecycle

Use semantic runtime feedback to advance call state rather than treating runtime events as display-only logs.

At minimum:
- accepted assignment becomes serving at an appropriate deterministic point;
- arrival/service completion marks the relevant call completed;
- replay verifies call lifecycle as well as physical car state.

## M2.4 — Decision reasons / diagnostics

Evolve opaque `Option<Intent>`-style outcomes toward structured, explainable decisions without building a policy DSL.

Useful diagnostics include:
- selected car;
- eligibility/rejection reason;
- mode/capability constraints;
- deterministic distance/tie-break information;
- no-eligible-car reason.

## M2.5 — Capability/configuration refinement

Only after the lifecycle path is working:
- reduce TUI-only hard-coded semantic assumptions where there is a demonstrated need;
- refine building topology/capability/configuration inputs;
- compare terminology and concepts with existing standards before freezing public semantics.

Any safety/authority-model change remains HUMAN-REVIEW.

## Explicit non-goals in M2

- real passenger-elevator control;
- real CAN/CANopen/BACnet controller writes;
- complex group-control optimization;
- destination-control optimization;
- large policy/service-profile DSL;
- production web/mobile UI;
- Home Assistant integration;
- database/cloud infrastructure;
- plugin marketplace or plugin ABI;
- broad CI/coverage/fuzzing programs;
- certification/conformance claims;
- license selection without human review.

## Repository governance status

- `AGENTS.md` is the single authoritative agent guide.
- `agent.md` is a generic compatibility/navigation pointer only.
- Project state is carried by repository documentation, not by vendor-specific agent files.
- Git commits should use the intended owner identity `Axiaobo7788`.
