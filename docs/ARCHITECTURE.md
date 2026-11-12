# Architecture

## v0.1 composition

~~~text
Developer TUI (composition root)
        |
        | hall call / developer controls
        v
LiftAssistant Core
        |
        | high-level Intent
        v
Runtime API
        |
        v
Simulator
        |
        | semantic runtime events + observed state
        +------------------------------+
                                       |
                                       v
                                  Developer TUI
~~~

The simulator is the first runtime backend. Future real-world runtimes should implement the same semantic boundary.

The TUI currently performs the small amount of application composition needed to connect core and runtime. Do not extract an application/service layer until another frontend or composition target creates real duplication.

## Functional core

Core logic behaves conceptually like:

~~~text
State + Event -> New State + Effects
~~~

The current bootstrap is smaller than that full model: dispatch is already a pure deterministic decision function, while runtime state progression lives in the simulator. Expand the functional-core model only as M1/M2 behavior requires it.

Core must not own networking, field buses, wall-clock time, persistence, UI, or vendor SDKs.

## Core/runtime boundary

Core defines WHAT: serve a floor, park a car, register a call.

The runtime/controller defines HOW.

Low-level motion and safety primitives stay outside core.

A runtime may expose observed state, capabilities, semantic events, and acceptance/rejection of high-level intents. It must not force vendor-specific protocol objects into core.

## Current packages

- `liftassistant-model`: canonical domain types with no vendor/runtime dependencies.
- `liftassistant-core`: pure deterministic decisions and, later, policy/state transitions.
- `liftassistant-runtime`: semantic execution contract and runtime events.
- `liftassistant-sim`: deterministic reference backend.
- `liftassistant-tui`: developer UI and current composition root.

Five packages are enough for M1. New packages require demonstrated ownership/lifecycle pressure, not speculative layering.

## Determinism boundary

The simulator advances only when given an explicit elapsed simulation duration.

A UI/event loop may use wall-clock time to decide when to request the next step, but:
- wall-clock time must not enter core decisions;
- identical initial state plus identical explicit inputs and step durations must produce identical semantic results;
- replay should operate without sleeping in real time.

## Future backends

Possible future backends include:
- virtual/SocketCAN CiA 417;
- T/GSET 001-2025 gateway;
- documented vendor APIs;
- THOR/other controller adapters;
- LiftAssistant-native reference runtime.

These are roadmap candidates, not M1 work.

External adapters should eventually use a versioned language-neutral protocol rather than an in-process Rust ABI, but that protocol should be designed from demonstrated adapter needs.

## Safety boundary

~~~text
LiftAssistant policy / group / intent
                |
        semantic runtime boundary
                |
========================================
 validated / certified execution domain
========================================
                |
       motion / door / safety logic
                |
             hardware
~~~

LiftAssistant must never rely on its own availability for a real elevator to remain safe.
