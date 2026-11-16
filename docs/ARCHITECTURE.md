# Architecture

## Current composition

~~~text
Developer TUI / Scenario runner
              |
              | hall calls / runtime feedback / observations
              v
            Engine ----------> LiftAssistant Core
              |
              | high-level intents
              v
          IntentSink -----------------> Simulator
                                         ^      |
                 explicit duration       |      | runtime events
          SimulationDriver --------------+------+
                                         |
                                  ObservationSource
~~~

The simulator is the first active runtime backend and implements observation, high-level-intent, and deterministic-driving contracts. M4 adds a second, recorded/read-only source that implements observation and replay driving but has no intent-submission contract. Live sources and intent backends should implement only the responsibilities they actually provide; explicit `advance(Duration)` is not a universal live-adapter requirement.

The shared Engine owns topology, call lifecycle, dispatch, pending scheduling, high-level intent submission/result handling, runtime-feedback processing, and canonical observation resolution. The developer TUI composition retains only UI state, simulator-specific controls, explicit time driving, bounded display trace, and replay input recording. Interactive and non-interactive scenario callers use the same Engine path.

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

M4 read-only experiments demonstrate that the operational `CarState` is intentionally complete: it requires exact position, directed door state, and LiftAssistant operating mode. External sources that report only a floor label, undirected door movement, or source-specific control modes enter a separate canonical `CarObservation` and must not fabricate those fields. A deterministic resolver is the only promotion path from this partial evidence to complete operational state; source-specific context stays outside Core. The observation model remains internal and reversible rather than a frozen public schema.

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

`ObservationSource` exposes resolved operational state and capabilities. `IntentSink` accepts only high-level LiftAssistant intents. `SimulationDriver` produces semantic runtime events from explicit simulated/replay duration. A component implements only the contracts it actually supports. Vendor protocol objects must not leak into Core semantics.

## Current packages

- `liftassistant-model`: canonical domain types.
- `liftassistant-core`: deterministic vendor-neutral semantic decisions/state transitions.
- `liftassistant-engine`: shared deterministic semantic orchestration and observation resolution.
- `liftassistant-headless`: internal non-interactive read-only host/lifecycle proof.
- `liftassistant-runtime`: semantic execution contract and runtime events.
- `liftassistant-sim`: deterministic active simulator, native-semantic recorded/read-only backend, and recorded external-state mapping experiments.
- `liftassistant-tui`: developer simulator composition, scenario runner, and UI.

These seven packages reflect demonstrated ownership boundaries. The headless package
exists because Engine now has a real second composition independent of the TUI; it
does not establish a general service framework. Further packages still require real
lifecycle/reuse pressure rather than speculative layering.

## Determinism boundary

The simulator advances only when given an explicit elapsed simulation duration.

A UI/event loop may use wall-clock time to decide when to request the next step, but:

- wall-clock time must not enter Core decisions;
- identical initial semantic/physical state + identical explicit inputs + identical step durations must produce identical semantic results;
- replay must operate without sleeping in real time.

M2 call lifecycle and assignment, M3 executable scenarios, and any recorded runtime must preserve the same determinism property.

The Engine keeps a bounded in-memory flight recorder of canonical observations and
machine-readable semantic events. Its deliberately small deterministic detector set
can freeze bounded incident evidence before and after a sequence invariant violation,
then replay that evidence through the same detector. Diagnostic anomalies remain
distinct from equipment faults, alarms, root causes, and maintenance actions. No
durable store, telemetry service, or stable public diagnostic schema is implied.

The headless proof keeps lifecycle and readiness separate: a live process begins
`Starting/NotReady`, incomplete evidence remains `Synchronizing/NotReady`, sufficient
evidence permits `ReadyReadOnly/ReadOnly`, and a source-sequence invariant violation
produces `Degraded/NotReady`. The last resolved state may remain inspectable after
degradation, but it is not current-readiness evidence. These internal states do not
grant write authority and are not a frozen public lifecycle model.

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


## Target production direction

This section describes the reviewed production direction, not the current implementation topology.

~~~text
TUI / WebUI / HA / BMS / API
            |
            v
      semantic client/API boundary
            |
            v
          Engine
 calls / dispatch / resolver
            |
      +-----+------+
      |            |
      v            v
observations    high-level intents
      |            |
      v            v
   adapters    authority/write gate
      |            |
      +------v-----+
             |
             v
 validated/certified execution domain
             |
   motion / door / safety / hardware
~~~

Production design should preserve these separations:

- UI/client failure must not imply semantic Engine failure.
- Adapter failure should be isolatable from Engine and from unrelated adapters.
- Partial external observations should be normalized before they become complete operational state.
- High-level intents must remain separate from low-level safety/actuator control.
- Loss/restart of LiftAssistant must not make the elevator unsafe.
- Production debugging and diagnostics should consume the same canonical event/observation flow rather than rely on ad-hoc logging.

M4 replaced the all-in-one prototype `ElevatorRuntime` with separate internal observation, high-level-intent, and simulation/replay-driving contracts. These boundaries remain reversible and are not frozen as an external protocol.

Detailed non-normative production ideas are preserved in `docs/PRODUCTION_DESIGN_NOTES.md`.
