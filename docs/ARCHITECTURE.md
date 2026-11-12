# Architecture

## v0.1

~~~text
Developer TUI
     |
     v
LiftAssistant Core
     |
   Intent
     |
     v
Runtime API
     |
     v
Simulator
     |
 Runtime events
     +----------> Core
~~~

The simulator is the first runtime backend. Future real-world runtimes should implement the same semantic boundary.

## Functional core

Core behaves like:

~~~text
State + Event -> New State + Effects
~~~

It should not own networking, field buses, wall-clock time, persistence, or UI.

## Core/runtime boundary

Core defines WHAT: serve a floor, park a car, register a call.
The runtime/controller defines HOW.

Low-level motion and safety primitives stay outside core.

## Initial modules

- liftassistant-model: pure domain types.
- liftassistant-core: deterministic orchestration/dispatch.
- liftassistant-runtime: semantic execution contract and runtime events.
- liftassistant-sim: deterministic reference backend.
- liftassistant-tui: first observable developer client.

## Future backends

- virtual/SocketCAN CiA 417;
- T/GSET 001-2025 gateway;
- documented vendor APIs;
- THOR/other controller adapters;
- LiftAssistant-native reference runtime.

External adapters should eventually use a versioned language-neutral protocol rather than an in-process Rust ABI.

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
