# Architecture

## Current composition

~~~text
Developer TUI (current composition root)
        |
        | hall calls / developer controls
        v
LiftAssistant Core
        |
        | high-level intents / semantic decisions
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

Core logic should evolve toward:

~~~text
Semantic State + Semantic Event
        ->
New Semantic State + High-level Effects/Intents
~~~

The implementation should grow into this shape from demonstrated behavior rather than through a repository-wide rewrite.

M1 established deterministic one-shot dispatch. M2 evolves Core toward persistent call identity/lifecycle, multiple pending calls, runtime-feedback-driven state, and explainable decisions.

Core must not own networking, field buses, wall-clock time, persistence, terminal/UI frameworks, or vendor SDKs.

## Core/runtime boundary

Core defines WHAT, for example:

- register/track a call;
- choose/assign eligible service;
- serve a floor;
- park a car;
- apply high-level policy when such semantics are later justified.

The runtime/controller defines HOW.

Low-level motion and safety primitives remain outside Core.

A runtime may expose observed state, capabilities, semantic events, and acceptance/rejection of high-level intents. Vendor protocol objects must not leak into Core semantics.

## Current packages

- `liftassistant-model`: canonical domain types.
- `liftassistant-core`: deterministic vendor-neutral semantic decisions/state transitions.
- `liftassistant-runtime`: semantic execution contract and runtime events.
- `liftassistant-sim`: deterministic reference backend.
- `liftassistant-tui`: developer UI and current composition root.

These five packages remain sufficient for M2. New packages require demonstrated ownership/lifecycle pressure, not speculative layering.

## Determinism boundary

The simulator advances only when given an explicit elapsed simulation duration.

A UI/event loop may use wall-clock time to decide when to request the next step, but:

- wall-clock time must not enter Core decisions;
- identical initial semantic/physical state + identical explicit inputs + identical step durations must produce identical semantic results;
- replay must operate without sleeping in real time.

M2 call lifecycle and assignment must preserve the same determinism property.

## Call semantics direction

Current M1 behavior is one-shot and may replace a car target.

M2 should introduce the smallest persistent call model needed for:

~~~text
Pending
  -> Assigned(car)
  -> Serving(car)
  -> Completed
~~~

Multiple calls should coexist without requiring an early complex group-control algorithm.

Runtime feedback should eventually advance lifecycle state; developer UI should observe that state rather than own it.

## Future backends

Possible later backends include:

- virtual/SocketCAN CiA 417;
- T/GSET 001-2025 gateways;
- documented vendor APIs;
- THOR/other controller adapters;
- LiftAssistant-native reference runtime.

These are roadmap candidates, not M2 implementation work.

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

Changing the safety/authority model is a HUMAN-REVIEW boundary.
