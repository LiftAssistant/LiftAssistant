# LiftAssistant

**LiftAssistant is an open, vendor-neutral software platform for elevators.**

The long-term goal is to make elevator controllers, protocols, capabilities, policies, diagnostics, simulation, and building integrations available through one common operational model — without replacing standards that already solve part of the problem.

LiftAssistant is currently an early architecture and simulation project. **It is not approved for controlling passenger elevators or any safety-critical equipment.**

## Current target: M1 — Interactive Simulator

The interactive simulator connects the developer-facing simulation loop:

~~~text
interactive hall call
        |
        v
deterministic dispatch
        |
        v
high-level intent
        |
        v
simulator fixed-step execution
        |
        v
state + semantic event trace
        |
        v
developer TUI
~~~

M1 is deliberately narrow: two cars, eight floors, an interactive hall call, deterministic stepping/replay, and one out-of-service fallback scenario.

The TUI is a composition/debugging client, not the architecture.

## Run the simulator

Requires Rust stable (minimum 1.85) and a terminal at least 70 columns by 24 rows.

~~~bash
cargo run -p liftassistant-tui
~~~

- Up/Down select a floor; U/D create an UP/DOWN hall call.
- 1F permits UP only; 8F permits DOWN only. Start with the selected **7F UP**.
- Space pauses/resumes; `.` advances exactly one 500 ms step while paused.
- A/B toggle the corresponding simulated car between Normal and OutOfService.
- P replays the recorded session twice from fresh simulators, verifies identical
  semantic output and the live state, and leaves the result paused.
- R resets the two-car simulator and UI; Q, Esc, or Ctrl-C quits.

The building shows nearest-floor positions; car details show exact positions,
mode, target, door state, and serve-floor capability. The latest 100 trace entries
show calls, dispatch, intents, runtime events, and errors. Recording is capped at
2,048 commands (calls, mode changes, and fixed steps); if full, reset before replaying. Pauses and
floor selection do not affect physics and are not scenario commands. Dispatch is currently
one-shot: a new call can replace a car's target; pending-call lifecycle is M2 work.

To observe availability fallback: R, Space (pause), A (out of service), U
(selected 7F UP), then eight single steps with `.`. The trace shows
`Car A -> OutOfService`, `Dispatch -> B`, `ServeFloor(B, 7F)`, arrival and door open.
P verifies the same scenario by replay. Toggle B out of service too to see
`No eligible car`; toggle A back to Normal to recover dispatch.

Availability controls change future dispatch eligibility. They do not cancel an
already accepted trip in this simulator. They are developer controls, not a
real-elevator safety or emergency-stop interface.

## Repository layout

~~~text
crates/
  liftassistant-model/    canonical domain types
  liftassistant-core/     pure deterministic decisions
  liftassistant-runtime/  semantic runtime contract
  liftassistant-sim/      deterministic reference backend
  liftassistant-tui/      developer composition root / UI

docs/
  ARCHITECTURE.md         architecture boundaries
  PROJECT_MEMORY.md       durable project decisions
  STATUS.md               current milestone and ordered work

scenarios/                simulator/replay inputs as they become real
spec/                     future language-independent public semantics
~~~

Do not add more crates merely to represent hypothetical future layers. Extract a shared application/service layer only when a second frontend or runtime composition actually needs it.

## Architecture rules

1. Stable semantics, replaceable implementations.
2. Intent, not actuator control.
3. Core stays vendor-neutral.
4. Simulation first.
5. Integrate existing standards instead of inventing replacements.
6. Safety authority stays below LiftAssistant.
7. No real passenger-elevator control in early milestones.
8. Working vertical slices come before framework expansion.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/PROJECT_MEMORY.md](docs/PROJECT_MEMORY.md), [docs/STATUS.md](docs/STATUS.md), and [AGENTS.md](AGENTS.md).

## License

A license has **not** been selected yet. This is an explicit human-review decision.
