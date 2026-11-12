# LiftAssistant Agent Guide

This is the authoritative operating guide for AI coding agents.

## Mission

LiftAssistant is an open, vendor-neutral operational layer for elevators. It should unify existing standards and controller ecosystems behind stable public semantics.

Rust is the reference implementation language. The LiftAssistant specification must remain language-independent.

## Default mode: SELF-ITERATION

Unless a task crosses a HUMAN-REVIEW boundary:

1. Read README.md, docs/PROJECT_MEMORY.md, docs/STATUS.md, docs/ARCHITECTURE.md, and this file.
2. Inspect code/tests before changing architecture.
3. Choose the smallest vertical slice that advances the current milestone.
4. Implement it.
5. Run formatting, linting, and tests.
6. Review the diff for architecture violations and unnecessary complexity.
7. Fix issues found during self-review.
8. Update docs/STATUS.md when project state materially changes.
9. Update docs/PROJECT_MEMORY.md only for durable decisions.
10. Continue within the requested task without repeatedly asking for approval.

Prefer working software and tests over speculative framework construction.

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

The simulator is the first runtime backend, not a disposable demo. Same configuration, seed, and inputs should be replayable.

### Existing standards are prior art

Map/reuse CiA 417, BACnet elevator objects, ETSI SAREF4LIFT, T/GSET 001-2025, Open-RMF/RFA, THOR/NeXt, and other relevant work.

### Fail safely at the integration boundary

Loss of LiftAssistant must not be designed to make a real elevator unsafe.

## Dependency discipline

Keep the workspace small. Prefer std where practical. Avoid early in-process third-party plugin ABIs. Keep unsafe code forbidden in initial core crates.

## Before completion

Run when available:

~~~bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
~~~

## Memory discipline

docs/PROJECT_MEMORY.md stores durable decisions.
docs/STATUS.md stores current progress/blockers/next work.
Do not copy conversations into either file.
