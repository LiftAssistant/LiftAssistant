# Contributing

LiftAssistant is in an early semantic-core and simulation phase.

Before substantial changes:

1. Read `AGENTS.md`.
2. Read `docs/PROJECT_MEMORY.md`, `docs/STATUS.md`, `docs/ROADMAP.md`, and `docs/ARCHITECTURE.md`.
3. Inspect the relevant code/tests before proposing architecture.
4. Keep Core deterministic, vendor-neutral, and independent from UI/protocol details.
5. Prefer the smallest working vertical slice over speculative framework construction.
6. Add tests that protect implemented behavior or concrete regressions.

## Current development direction

M1 Interactive Simulator is complete and human-accepted.

M2 focuses on persistent call semantics: call identity/lifecycle, multiple pending calls, runtime-feedback-driven state, and explainable decisions.

Do not use contribution work as a reason to introduce real passenger-elevator control, large policy DSLs, databases, microservices, plugin ABIs, or complex group-control optimization.

## Safety

Do not connect experimental LiftAssistant code to a passenger elevator or other safety-critical installation.

Early hardware work belongs in simulation, virtual buses, HIL, benches, or controlled non-passenger environments.

Changes to safety/authority semantics or passenger-elevator control require HUMAN-REVIEW.

## Agent-generated contributions

`AGENTS.md` is the authoritative agent workflow.

Any agent working in this repository should read the shared repository context rather than relying on tool-specific copies of project rules.

Before agent-created commits, verify the intended Git author/committer identity.

## Validation

When applicable:

~~~bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
~~~

## License status

No open-source license has been selected yet. License selection is a HUMAN-REVIEW decision.
