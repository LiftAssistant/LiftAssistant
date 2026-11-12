# Project status

Updated: 2026-11-12

## Current milestone

**M1 — Interactive Simulator: functional acceptance complete**

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

M1.1, M1.2, and M1.3 are implemented. All scoped AUTO work is complete;
remaining UI adjustments depend on hands-on usability feedback.

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

HUMAN_PENDING: hands-on TUI usability/visual review. All independent M1 implementation work is complete.

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

**Completed.**

- A/B toggle each simulated car between Normal and OutOfService using SimRuntime::set_mode.
- Mode changes are shown in car detail/trace and included in replay commands.
- With A unavailable, 7F UP dispatches to B and completes arrival/open-door flow.
- Both unavailable: No eligible car, no submitted intent; restoring A recovers dispatch.
- Reset restores both cars to Normal and clears targets, decisions, trace, and recording.
- Existing accepted trips continue if availability changes; these controls affect new dispatch only.

## M1 validation and remaining review

- cargo fmt --all -- --check: passed.
- cargo clippy --workspace --all-targets --locked -- -D warnings: passed.
- cargo test --workspace --locked: 12 passed, no failures or skipped tests.
- Rust 1.85 all-target workspace check with the committed lockfile: passed.
- PTY run via cargo run -p liftassistant-tui: calls, fixed-step pause/resume,
  arrival/open door, reset, invalid call, replay, availability fallback, no-eligible
  recovery, and Q exit passed. Esc/Ctrl-C and a broken-output error restore terminal settings.
- All three M1 slices preserve the five-package architecture; no real elevator integration added.

No remaining READY implementation items in this M1 cycle. Human check: run the
TUI in a preferred terminal, assess visual hierarchy/key usability, and report
specific adjustments if desired. This does not block any unfinished replay or
fallback work; those have both completed. M2 remains the subsequent roadmap phase.

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
