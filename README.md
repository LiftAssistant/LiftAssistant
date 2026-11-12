# LiftAssistant

**LiftAssistant is an open, vendor-neutral software platform for elevators.**

The long-term goal is to make elevator controllers, protocols, capabilities, policies, diagnostics, simulation, and building integrations available through one common operational model — without replacing standards that already solve part of the problem.

LiftAssistant is currently an early architecture and simulation project. **It is not approved for controlling passenger elevators or any safety-critical equipment.**

## Current target: M1 — Interactive Simulator

The bootstrap is complete. The next target is one complete developer-facing simulation loop:

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
