# LiftAssistant Agent Guide

This file defines **how an agent works in this repository**.

It is not project memory, not the current project status, not the roadmap, and not the architecture specification. Do not duplicate those sources here.

## Required project context

Before substantial work, read:

1. `README.md` — human-facing project overview and runnable surface.
2. `docs/PROJECT_MEMORY.md` — durable project facts and decisions.
3. `docs/STATUS.md` — current milestone, completed work, READY work, HUMAN_PENDING items, blockers.
4. `docs/ROADMAP.md` — milestone sequence and future scope.
5. `docs/ARCHITECTURE.md` — architecture, component responsibilities, and safety boundaries.
6. `docs/PRIOR_ART.md` — research triggers, source hierarchy, and established references.
7. Relevant code/tests before changing behavior.

Source-of-truth rule:

- never infer the current milestone from this file;
- never store validated feature history here;
- never mirror the project mission/architecture here;
- when project state changes, update the owning document rather than this file.

If repository documents disagree, inspect the implementation to establish current factual behavior, then reconcile the owning documents.

## Work modes

There are two explicit work modes:

- `SELF-ITERATION` — default.
- `HUMAN-REVIEW` — required at defined boundaries.

A completed task, commit, test run, or green CI result is not by itself a reason to stop.

### SELF-ITERATION

Unless a HUMAN-REVIEW boundary is crossed:

1. Read the current state from the repository sources above.
2. Choose the highest-priority READY item in `docs/STATUS.md`, unless the user gave a narrower target.
3. Implement the smallest useful working vertical slice.
4. Add/update only tests needed to protect implemented behavior or reproduce a concrete bug.
5. Run relevant format/lint/test/smoke checks.
6. Self-review the diff for regressions, architecture violations, unnecessary abstractions, and scope creep.
7. Fix issues found.
8. Update the owning documentation when project state or a durable decision materially changes.
9. Commit a coherent working slice when committing is requested/appropriate.
10. Re-read `docs/STATUS.md` and continue with the next independent READY item.

Prefer working behavior over speculative framework construction.

### Prior-art gate

Before introducing or materially changing domain semantics, capability models, normalized diagnostics, protocol mappings, external/public interfaces, or reusable adapter/runtime semantics:

1. Read `docs/PRIOR_ART.md`.
2. Review the most relevant official standard/project sources.
3. Identify reusable terminology/concepts and any deliberate LiftAssistant divergence.
4. Keep research scoped to the next vertical slice.
5. Implement and validate rather than turning research into an end in itself.

Clear bug fixes, semantic-preserving refactors, formatting, and already-approved implementation work do not require a fresh research pass.

If research approaches a normative public-schema freeze or changes safety/authority semantics, enter HUMAN-REVIEW.

### HUMAN_PENDING is not a global stop

When one decision requires human review:

1. Record exactly what requires review and why in `docs/STATUS.md`.
2. Mark only dependent work as blocked.
3. Continue independent READY work.
4. End in `HUMAN-CHECK` only when all remaining meaningful work is blocked by human input, or the user explicitly requested a checkpoint.

### HUMAN-REVIEW boundaries

Request human review before:

- code intended to write to/control a real passenger elevator;
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

## Delivery discipline

- Respect `docs/ARCHITECTURE.md`; do not duplicate its component or boundary definitions here.
- Do not create placeholder crates, protocols, schemas, services, or directories merely because they may be useful later.
- Do not add an architectural layer until current code demonstrates the need.
- Do not turn an implementation task into test-infrastructure work.
- Do not add coverage targets, broad CI matrices, fuzzing, benchmarks, or elaborate infrastructure without a concrete need.
- Do not repeatedly rerun unchanged green checks instead of advancing READY work.
- A green CI run is a gate, not a milestone.
- If a UI/UX or semantic review is useful but non-blocking, record it as HUMAN_PENDING and continue independent work.

## Git workflow and identity

Current repository workflow: work directly on `main` unless the user explicitly requests a branch/PR workflow.

Do not create side branches merely because an agent normally prefers them.

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

Do not treat validation commands as the work itself.

## Documentation ownership

- `AGENTS.md`: agent execution protocol only.
- `agent.md`: generic compatibility/navigation pointer only.
- `docs/PROJECT_MEMORY.md`: durable project facts and decisions.
- `docs/STATUS.md`: current project state, READY/HUMAN_PENDING/blockers.
- `docs/ROADMAP.md`: milestone sequence and future scope.
- `docs/ARCHITECTURE.md`: architecture and safety boundaries.
- `docs/PRIOR_ART.md`: research policy and curated prior-art index.
- `README.md`: human-facing project overview and runnable surface.
- `CONTRIBUTING.md`: contributor-facing workflow/safety summary.

Do not copy conversations, milestone history, architecture summaries, or temporary task logs into `AGENTS.md` or `agent.md`.
