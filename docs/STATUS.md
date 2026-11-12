# Project status

Updated: 2026-11-12

## Current milestone
**M0 — Bootstrap**

## Completed
- [x] Project established as LiftAssistant.
- [x] Architecture direction consolidated.
- [x] Rust selected for reference implementation.
- [x] Language-independent spec principle adopted.
- [x] Monorepo-first decision adopted.
- [x] SELF-ITERATION and HUMAN-REVIEW modes defined.
- [x] Project memory, architecture, principles, prior-art index and roadmap created.
- [x] Initial five-crate workspace scaffold created.
- [x] CI definition created.

## In progress
- [ ] Verify initial Rust workspace in GitHub CI.
- [ ] Replace bootstrap console demo with first developer TUI.
- [ ] Make simulator scenario/state progression explicitly deterministic and replayable.

## Next vertical slice
~~~text
2 cars / 8 floors
8F UP hall call
-> dispatch
-> ServeFloor intent
-> simulator movement
-> arrival + door open
-> visible state/event trace
~~~

Then add an out-of-service scenario.

## Explicit non-goals right now
- real passenger-elevator control;
- cloud services;
- production web UI;
- database infrastructure;
- plugin marketplace;
- custom CANopen stack;
- replacing CiA 417/BACnet/SAREF4LIFT/GSET/RMF;
- premature microservices;
- license selection without human review.
