# LiftAssistant Agent Guide

This is the authoritative operating guide for AI coding agents working in this repository.

Tool-specific instruction files may point here, but they must not redefine conflicting project rules.

## Required startup context

Before substantial work, read in this order:

1. `README.md` — project purpose and runnable surface.
2. `docs/PROJECT_MEMORY.md` — durable decisions only.
3. `docs/STATUS.md` — current milestone, completed work, READY work, blockers.
4. `docs/ROADMAP.md` — milestone ordering and future scope.
5. `docs/ARCHITECTURE.md` — architecture and safety boundaries.
6. This file — agent execution rules.
7. Relevant code/tests before changing behavior.

If these files disagree, do not silently pick a convenient version. Prefer implemented behavior for factual current-state claims, then reconcile the documentation as part of the task.

## Mission

LiftAssistant is an open, vendor-neutral operational layer for elevators. It should unify existing standards and controller ecosystems behind stable public semantics.

Rust is the reference implementation language. Public LiftAssistant semantics must remain language-independent.

LiftAssistant says WHAT. Validated lower-level runtime/safety systems decide HOW.

## Work modes

There are two explicit work modes:

- `SELF-ITERATION` — default.
- `HUMAN-REVIEW` — required for listed boundaries.

A task completion, green CI run, or commit is not by itself a reason to stop.

### SELF-ITERATION

Unless a HUMAN-REVIEW boundary is crossed:

1. Read current status and inspect relevant code.
2. Choose the highest-priority READY item in the current milestone unless the user gave a narrower target.
3. Implement the smallest useful working vertical slice.
4. Add/update only tests needed to protect implemented behavior or reproduce a concrete bug.
5. Run relevant format/lint/test/smoke checks.
6. Self-review the diff for architecture violations, regressions, unnecessary abstractions, and scope creep.
7. Fix issues found.
8. Update `docs/STATUS.md` when project state materially changes.
9. Update `docs/PROJECT_MEMORY.md` only for durable decisions.
10. Commit a coherent working slice when committing is requested/appropriate.
11. Re-evaluate the project state and continue with the next independent READY item.

Prefer working behavior over speculative framework construction.

### HUMAN_PENDING is not a global stop

When one decision requires human review:

1. Record exactly what requires human review and why.
2. Mark only the dependent work as blocked.
3. Continue any independent READY work that does not depend on that decision.
4. End in `HUMAN-CHECK` only when all remaining meaningful work is blocked by human input or the user explicitly requested a checkpoint.

### HUMAN-REVIEW boundaries

Request human review before:

- any code intended to write to/control a real passenger elevator;
- exposing brake, drive-enable, motor torque, safety-chain bypass, door-lock bypass, or similar safety-critical actuation;
- changing the safety/authority model;
- freezing or materially changing a normative public specification/schema;
- selecting/changing the license;
- introducing a foundational runtime framework, database, message broker, or plugin ABI;
- splitting the monorepo;
- replacing Rust as the primary reference language;
- major repository-wide rewrites;
- release/signing/security policy changes;
- claims of conformance/certification.

Isolate the decision and continue safe independent work where possible.

## Current milestone direction

M1 Interactive Simulator has been implemented and human-accepted.

Current milestone: **M2 — Semantic Core**.

The next READY direction is to evolve one-shot dispatch into persistent call semantics:

- call identity/lifecycle;
- multiple pending calls without overwriting an active trip;
- deterministic assignment/service/completion;
- runtime feedback driving lifecycle state;
- structured decision reasons/diagnostics;
- later capability/configuration refinement based on demonstrated needs.

Do not jump directly to complex group-control optimization, a policy DSL, real protocols, or real elevator control.

## Architecture invariants

### Core is vendor-neutral

Never add vendor branches to core. Vendor behavior belongs in adapters/runtime implementations.

### Intent, not raw actuation

Allowed concepts include `ServeFloor`, `ParkAt`, `RegisterCall`, and future high-level service/policy intents.

Do not expose low-level brake/drive/torque/safety-bypass primitives.

### Functional core

Core should remain deterministic and independent from wall-clock time, networking, CAN, serial, filesystems, databases, terminal/UI frameworks, and vendor SDKs.

### Simulation is first-class

The simulator is a reference runtime backend, not a disposable demo. Identical initial state + explicit inputs + explicit simulation durations should produce identical semantic results.

### Existing standards are prior art

Map/reuse CiA 417, BACnet elevator objects, ETSI SAREF4LIFT, T/GSET 001-2025, Open-RMF/RFA, THOR/NeXt, and other relevant work rather than inventing replacements.

### Fail safely at the integration boundary

Loss of LiftAssistant must not be designed to make a real elevator unsafe.

## Workspace discipline

Keep the workspace small. The current five-package split remains sufficient:

- `liftassistant-model`
- `liftassistant-core`
- `liftassistant-runtime`
- `liftassistant-sim`
- `liftassistant-tui`

Do not extract application/service/orchestration crates while the TUI remains the only real composition root. Reconsider only when a second frontend/composition target creates actual duplication.

Prefer std where practical. Avoid early in-process plugin ABIs. Keep unsafe code forbidden in initial core crates.

## Delivery discipline

- Do not create placeholder crates, protocols, schemas, services, or directories just because the roadmap may need them later.
- Do not add an architectural layer until current code demonstrates the need.
- Do not turn an implementation task into test-infrastructure work.
- Do not add coverage targets, broad CI matrices, fuzzing, benchmarks, or elaborate infrastructure without a concrete current need.
- Do not repeatedly rerun unchanged green checks instead of advancing the milestone.
- A green CI run is a gate, not a milestone.
- Documentation should describe implemented behavior, a current near-term target, or a durable decision.
- If a UI/UX review is helpful but does not block independent core work, record it as HUMAN_PENDING and continue.

## Git workflow and identity

Current repository workflow: work directly on `main` unless the user explicitly requests a branch/PR workflow.

Do not create side branches just because an agent normally prefers them.

Before creating a commit, verify:

~~~bash
git branch --show-current
git var GIT_AUTHOR_IDENT
git var GIT_COMMITTER_IDENT
~~~

The intended GitHub identity for project commits is the owner's primary account, **Axiaobo7788**. Do not intentionally attribute project commits to alternate account `xiaobo-kipfel`.

If author/committer identity is unexpected and the correct verified email is not already known from repository/local configuration, do not invent an email. Stop only the commit step for HUMAN-REVIEW; continue safe non-commit work if possible.

Never force-push unless the user explicitly authorizes history rewriting. Prefer `--force-with-lease` when an explicitly approved rewrite is necessary.

## Validation

Run when relevant and available:

~~~bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
~~~

For interactive behavior, also perform an appropriate TTY/PTY or hands-on smoke test when feasible.

Do not treat these commands as the work itself.

## Documentation ownership

- `AGENTS.md`: authoritative agent workflow, boundaries, execution rules.
- `agent.md`: compatibility/navigation pointer only; never authoritative.
- `docs/PROJECT_MEMORY.md`: durable project decisions and validated long-lived facts.
- `docs/STATUS.md`: current progress, current milestone, READY/HUMAN_PENDING/blockers.
- `docs/ROADMAP.md`: milestone sequence and future scope.
- `docs/ARCHITECTURE.md`: architecture/safety boundaries and current component responsibilities.
- `README.md`: human-facing project overview and runnable entry points.
- `CONTRIBUTING.md`: contributor-facing workflow/safety summary.

Do not copy conversations or temporary task logs into project memory.
