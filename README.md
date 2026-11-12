# LiftAssistant

**LiftAssistant is an open, vendor-neutral software platform for elevators.**

The long-term goal is to make elevator controllers, protocols, capabilities, policies, diagnostics, simulation, and building integrations available through one common operational model — without replacing standards that already solve part of the problem.

LiftAssistant is currently an early architecture and simulation project. **It is not approved for controlling passenger elevators or any safety-critical equipment.**

## v0.1 direction

- Rust reference implementation; language-independent public semantics
- canonical elevator domain model
- deterministic core logic
- runtime abstraction
- deterministic simulator
- developer TUI
- two cars / eight floors
- hall-call -> dispatch -> intent -> simulated movement -> observed state

The TUI is the first observable client, not the architecture.

## Architecture rules

1. Stable semantics, replaceable implementations.
2. Intent, not actuator control.
3. Core stays vendor-neutral.
4. Simulation first.
5. Integrate existing standards instead of inventing replacements.
6. Safety authority stays below LiftAssistant.
7. No real passenger-elevator control in early milestones.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/PROJECT_MEMORY.md](docs/PROJECT_MEMORY.md), [docs/STATUS.md](docs/STATUS.md), and [AGENTS.md](AGENTS.md).

## License

A license has **not** been selected yet. This is an explicit human-review decision.
