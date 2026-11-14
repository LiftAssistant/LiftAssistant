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

The simulator is the first active runtime backend. M4 adds a second, recorded/read-only backend for deterministic playback of native semantic observations. Future real-world runtimes should implement the same semantic boundary.

The TUI currently performs the small amount of application composition needed to connect core and runtime. Do not extract an application/service layer until another frontend or composition target creates real duplication.

## Functional core

Core logic should evolve toward:

~~~text
Semantic State + Semantic Event
        ->
New Semantic State + High-level Effects/Intents
~~~

The implementation should grow into this shape from demonstrated behavior rather than through a repository-wide rewrite.

M1 established deterministic one-shot dispatch. M2.1 through M2.5 added persistent call identity/lifecycle, multiple pending calls, runtime-feedback-driven state, explainable dispatch decisions, and explicit served-floor topology. The M2 semantic vertical path is implemented and machine-reviewed. Its model remains internal and reversible; human semantic/terminology review is non-blocking for independent internal work but required before any normative public schema freeze.

M3 added developer-only executable scenarios, complete trace/final-state inspection, expected-outcome verification, and a one-shot simulator intent-rejection control. These remain composition/simulator tools rather than Core semantics, equipment diagnostics, a public scenario format, or a certification/conformance system.

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
- `liftassistant-sim`: deterministic active simulator plus native-semantic recorded/read-only research backend.
- `liftassistant-tui`: developer UI and current composition root.

These five packages remain sufficient for the current work. New packages require demonstrated ownership/lifecycle pressure, not speculative layering.

## Determinism boundary

The simulator advances only when given an explicit elapsed simulation duration.

A UI/event loop may use wall-clock time to decide when to request the next step, but:

- wall-clock time must not enter Core decisions;
- identical initial semantic/physical state + identical explicit inputs + identical step durations must produce identical semantic results;
- replay must operate without sleeping in real time.

M2 call lifecycle and assignment, M3 executable scenarios, and any recorded runtime must preserve the same determinism property.

## Semantic design method

Before introducing durable domain semantics or reusable external boundaries:

1. inspect current implemented behavior;
2. review the relevant established standard/project concepts listed in `docs/PRIOR_ART.md`;
3. reuse compatible terminology where it improves interoperability;
4. keep deliberate divergences explicit;
5. implement the smallest reversible internal model first;
6. require HUMAN-REVIEW before freezing normative public semantics.

Existing standards are inputs and constraints, not a requirement to copy any one architecture wholesale.

## Current call semantics

Core owns deterministic call identity and the lifecycle:

~~~text
Pending
  -> Assigned(car)
  -> Serving(car)
  -> Completed
~~~

Multiple calls coexist without a complex group-control algorithm. Pending calls are considered in `CallId` order and cannot overwrite an Assigned/Serving car target.

Runtime intent acceptance advances Assigned to Serving. Arrival evidence followed by an open door completes the matching call and immediately reevaluates pending work. Arrival evidence is Core-owned so feedback may span runtime event batches. The developer UI observes this state; it does not own the lifecycle.

Dispatch returns a structured deterministic decision. It evaluates every car's mode, semantic/runtime occupancy, serve-floor capability, and distance, records rejection reasons, and identifies whether selection used sole eligibility, nearest distance, or the car-ID tie-break. These are orchestration explanations, not equipment fault/alarm semantics and not a policy language.

`BuildingTopology` is the current minimal configuration input: a canonical ordered set of served floors. Core derives hall-call membership and terminal-direction validity from it, and replay receives the same topology explicitly. It is not a universal building model; floor names, door/service mappings, spatial relationships, and protocol indexes remain outside the model until demonstrated by adapters or integrations.

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
