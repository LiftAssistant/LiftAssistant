# LiftAssistant

**LiftAssistant is an open, vendor-neutral software platform for elevators.**

The long-term goal is to make elevator controllers, protocols, capabilities, policies, diagnostics, simulation, and building integrations available through one common operational model — without replacing standards that already solve part of the problem.

LiftAssistant is an early semantic-core and simulation project. **It is not approved for controlling passenger elevators or any safety-critical equipment.**

## Current target: M4 — Real-world bridges (recorded/read-only first)

M1 Interactive Simulator is implemented and human-accepted.

The current working vertical path is:

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
runtime boundary
        |
        v
fixed-step simulator
        |
        v
state + semantic event trace
        |
        v
developer TUI
~~~

M2 evolved this from one-shot dispatch into persistent call semantics: call identity/lifecycle, multiple pending calls, runtime-feedback-driven completion, explainable decisions, and explicit served-floor topology. That semantic vertical path is implemented and machine-reviewed.

M3 turned the deterministic simulator and replay path into a small research laboratory with executable scenarios, trace inspection, expected-outcome checks, and one-shot integration-failure injection. M4 starts with a native-semantic recorded/read-only runtime before any protocol or hardware writes. None of this implies certification or conformance claims.

The TUI is a composition/debugging client, not the architecture.

## Run the simulator

Requires Rust stable (minimum 1.85) and a terminal at least 70 columns by 24 rows.

~~~bash
cargo run -p liftassistant-tui
~~~

Run the built-in M2 lifecycle reference scenario without a TTY:

~~~bash
cargo run -p liftassistant-tui -- --scenario m2-call-lifecycle
~~~

The command executes the same developer composition path twice, rejects a deterministic or expected-outcome mismatch, and prints the semantic trace plus final call/car state. It is a research tool, not a conformance or certification claim.

Inspect the recorded/read-only runtime boundary:

~~~bash
cargo run -p liftassistant-tui -- --scenario recorded-read-only-observation
~~~

This scenario replays native semantic observations at explicit elapsed times and proves that the backend rejects every submitted intent without disturbing playback. It performs no protocol, network, device, or hardware I/O.

Controls:

- Up/Down select a floor; U/D create an UP/DOWN hall call.
- 1F permits UP only; 8F permits DOWN only.
- Space pauses/resumes; `.` advances exactly one 500 ms step while paused.
- A/B toggle the corresponding simulated car between Normal and OutOfService.
- P replays the recorded session twice from fresh simulators, verifies deterministic semantic output/live state, and leaves the result paused.
- R resets the simulator/UI.
- Q, Esc, or Ctrl-C quits.

The building shows nearest-floor positions; car details show exact positions, mode, target, door state, and serve-floor capability. The latest 100 trace entries show calls, dispatch, intents, runtime events, and errors.

Recording is capped at 2,048 commands. Pauses and floor selection do not affect physics and are not scenario commands.

Calls now have deterministic identity and persistent Core-owned state. Multiple calls can coexist: idle cars receive pending work deterministically, while excess work remains pending rather than replacing an active target. Runtime acceptance, arrival, and door-open feedback advance calls through serving/completion and release cars for deterministic pending-work reevaluation. Each dispatch reports per-car mode, busy/capability constraints, distance, rejection reasons, and the deterministic selection basis. An explicit served-floor topology drives call validation, TUI selection/rendering, reset, and replay. M2 semantic implementation and machine review are complete. Human review of the internal terminology remains welcome before any normative public schema is frozen, but it does not block reversible internal bridge work.

## Repository layout

~~~text
AGENTS.md                  authoritative AI-agent workflow
agent.md                   compatibility/navigation pointer

crates/
  liftassistant-model/     canonical domain types
  liftassistant-core/      deterministic semantic decisions
  liftassistant-runtime/   semantic runtime contract
  liftassistant-sim/       deterministic reference backend
  liftassistant-tui/       developer composition root / UI

docs/
  PROJECT_MEMORY.md        durable decisions / validated long-lived facts
  STATUS.md                current milestone / READY work / blockers
  ROADMAP.md               milestone sequence / future scope
  ARCHITECTURE.md          architecture and safety boundaries
  PRIOR_ART.md             research triggers / standards-project index

scenarios/                 simulator/replay assets as real needs emerge
spec/                      future language-independent public semantics
~~~

Do not add more crates merely to represent hypothetical future layers. Extract a shared application/service layer only when a second frontend or runtime composition actually needs it.

## Architecture rules

1. Stable semantics, replaceable implementations.
2. Intent, not actuator control.
3. Core stays vendor-neutral.
4. Simulation/replay first.
5. Integrate existing standards instead of inventing replacements.
6. Safety authority stays below LiftAssistant.
7. No real passenger-elevator control in early milestones.
8. Working vertical slices come before framework expansion.

See [AGENTS.md](AGENTS.md), [docs/PROJECT_MEMORY.md](docs/PROJECT_MEMORY.md), [docs/STATUS.md](docs/STATUS.md), [docs/ROADMAP.md](docs/ROADMAP.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and [docs/PRIOR_ART.md](docs/PRIOR_ART.md).

## License

A license has **not** been selected yet. This is an explicit HUMAN-REVIEW decision.
