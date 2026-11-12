# LiftAssistant Agent Guide

This is the authoritative operating guide for AI coding agents.

## Mission

LiftAssistant is an open, vendor-neutral operational layer for elevators. It should unify existing standards and controller ecosystems behind stable public semantics.

Rust is the reference implementation language. The LiftAssistant specification must remain language-independent.

## Default mode: SELF-ITERATION

Unless a task crosses a HUMAN-REVIEW boundary:

1. Read README.md, docs/PROJECT_MEMORY.md, docs/STATUS.md, docs/ARCHITECTURE.md, and this file.
2. Inspect code/tests before changing architecture.
3. Take the first unfinished item in the current milestone unless the user gives a narrower target.
4. Implement the smallest working vertical slice that advances it.
5. Add or update only the tests needed to lock the implemented behavior.
6. Run formatting, linting, and tests.
7. Review the diff for architecture violations and unnecessary complexity.
8. Fix issues found during self-review.
9. Update docs/STATUS.md when project state materially changes.
10. Update docs/PROJECT_MEMORY.md only for durable decisions.
11. Continue to the next directly dependent milestone item when the requested task still has scope.

Prefer working behavior over speculative framework construction.

## Delivery discipline

The project is intentionally implementation-first during early milestones.

- Do not create placeholder crates, protocols, schemas, services, or directories just because the roadmap may need them later.
- Do not add a new architectural layer until current code demonstrates why it is needed.
- Do not turn an implementation task into a test-infrastructure task.
- Tests should primarily protect behavior that exists or reproduce a concrete bug.
- Do not add coverage targets, broad platform matrices, fuzzing, benchmarks, or elaborate CI during M1 unless a concrete defect requires them.
- Do not repeatedly rerun unchanged green checks instead of advancing the milestone.
- A green CI run is a gate, not a milestone.
- Documentation should describe implemented behavior, a current near-term target, or a durable decision; avoid speculative design catalogs.

When choosing between a clean abstraction with no caller and a small implementation that completes the current vertical slice, prefer the vertical slice unless doing so would violate an architecture invariant.

## HUMAN-REVIEW mode

Request human review before:

- any code intended to write to/control a real passenger elevator;
- exposing brake, drive-enable, motor torque, safety-chain bypass, door-lock bypass, or similar safety-critical actuation;
- changing a normative public spec/schema;
- changing the safety/authority model;
- selecting/changing the license;
- introducing a foundational runtime framework, database, message broker, or plugin ABI;
- splitting the monorepo;
- replacing Rust as primary reference language;
- major repository-wide rewrites;
- release/signing/security policy changes;
- claims of conformance/certification.

Isolate the decision and continue safe independent work where possible.

## Architecture invariants

### Core is vendor-neutral

Never add vendor branches to core. Vendor behavior belongs in adapters/runtime implementations.

### Intent, not raw actuation

Allowed concepts include ServeFloor, ParkAt, RegisterCall, ApplyServiceProfile.

Do not expose low-level brake/drive/torque/safety-bypass primitives.

### Functional core

Core should be deterministic and independent from wall-clock time, networking, CAN, serial, filesystems, databases, UI, and vendor SDKs.

### Simulation is first-class

The simulator is the first runtime backend, not a disposable demo. Same configuration and explicit inputs should be replayable.

### Existing standards are prior art

Map/reuse CiA 417, BACnet elevator objects, ETSI SAREF4LIFT, T/GSET 001-2025, Open-RMF/RFA, THOR/NeXt, and other relevant work.

### Fail safely at the integration boundary

Loss of LiftAssistant must not be designed to make a real elevator unsafe.

## Workspace discipline

Keep the workspace small. The current five-package split is sufficient for M1:
- model;
- core;
- runtime contract;
- simulator;
- developer TUI.

Do not extract an application/service/orchestration crate while the TUI is the only composition root. Reconsider that boundary when a second frontend or composition target appears.

Prefer std where practical. Avoid early in-process third-party plugin ABIs. Keep unsafe code forbidden in initial core crates.

## Before completion

Run when available:

~~~bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
~~~

Do not treat these commands as the work itself. If they are green and acceptance criteria are met, advance.

## Memory discipline

docs/PROJECT_MEMORY.md stores durable decisions.
docs/STATUS.md stores current progress, ordered work, and blockers.
Do not copy conversations into either file.
