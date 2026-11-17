# Architecture

## Current composition

~~~text
Simulator / recorded feed ---- canonical observation updates ----> Engine
Live RMF subscriber / mapper -- canonical observation updates ---->   |
                                                                     |
Developer TUI / scenarios / headless <--- Engine-owned state --------+
                                                                     |
                                                                     +--> LiftAssistant Core
                                                                     |
                                                                     +--> IntentSink --> Simulator

SimulationDriver -- explicit duration --> semantic runtime events --> Engine
~~~

The simulator is the first active runtime backend and emits canonical observation updates while implementing high-level-intent and deterministic-driving contracts. M4 adds deterministic recorded/read-only feeds and replay driving without an intent-submission contract. The M5 Open-RMF adapter adds a live subscription and source-owned mapping/session/sequence boundary without an intent-submission contract. Live feeds and intent backends implement only the responsibilities they actually provide; explicit `advance(Duration)` is not a universal live-adapter requirement.

The shared Engine owns topology, call lifecycle, dispatch, pending scheduling, execution capabilities, explicit observation bindings/sessions, incremental observation assembly, resolved operational state/evidence, high-level intent submission/result handling, and runtime-feedback processing. Dispatch and read-side clients consume this Engine-owned view rather than querying an adapter-owned state store. The developer TUI composition retains only UI state, simulator-specific controls, explicit time driving, bounded display trace, and replay input recording. Interactive, headless, and non-interactive scenario callers use the same Engine ingestion/read path.

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

M4 read-only experiments demonstrate that operational `CarState` is intentionally complete while retaining truthful location precision: `CarLocation::Floor` records only floor association, whereas `CarLocation::Continuous` contains a genuine continuous coordinate. It also requires known motion, directed door state, and a LiftAssistant operating mode. External sources with unknown location, unknown/undirected/aggregate door state, or source-specific control modes enter canonical observation evidence and must not fabricate those fields. `CarObservationUpdate` distinguishes an omitted field from explicit unknown evidence; the deterministic in-memory `ObservationAssembler` merges fields only within one `(source, session, logical car)` stream, retains per-field provenance, and enforces one source-global ordered sequence within each explicitly activated session without treating sequence as freshness. A deterministic resolver is the only promotion path from assembled evidence to complete operational state; source-specific context stays outside Core. The Open-RMF experiment demonstrates floor references, undirected door movement, and source-specific modes. The BACnet experiment demonstrates floor-level operational state plus explicit unknown evidence for a decoupled object and for per-door arrays that cannot safely become one car door state. The observation model remains internal and reversible rather than a frozen public schema.

`CarId` is LiftAssistant logical car identity. `ObservationSourceId` identifies a configured feed, and source-native identifiers such as Open-RMF `lift_name` remain adapter context that must map explicitly to `CarId`. Engine accepts observations only for an explicit `(source, car)` binding and explicitly activated opaque source session. One source may bind several cars and one car may have several sources, but each logical car has at most one Operational observation authority. Auxiliary sources retain isolated evidence and resolved source state for inspection/diagnostics; they do not enter `car_states()`, Core dispatch, readiness truth, field fusion, or automatic failover. Starting a new source session invalidates only that source's prior evidence/state, retires the old session token, and resets ordering without inferring restart from sequence values.

Observation authority and execution capability are independent. Core consumes `ExecutionCapabilities` for high-level `ServeFloor`/`ParkAt` decisions. Source-side observation coverage stays with the adapter when demonstrated, and neither Operational nor Auxiliary observation binding grants an `IntentSink` or write authority.

The live Open-RMF adapter maps every configured native `lift_name` explicitly to a logical `CarId`, creates its own `ObservationSessionId` for each ingestion lifetime, and assigns one source-global sequence across all mapped cars. Recorded and live RMF inputs share the same semantic mapper. `MODE_AGV` and `MODE_HUMAN` remain source-specific by default; a configured RMF source may explicitly project either to canonical Normal evidence, while raw current/available modes and RMF control-session context remain adapter-owned evidence. This narrow projection is not a global mode equivalence.

Official Open-RMF Hotel validation confirms that one source/publisher may carry periodic complete snapshots for several source-native lift identities, including unchanged idle state. It also exercises adapter/source restart, old-state invalidation, new-session sequence restart, delayed-old-session rejection, and real simulation motion/door transitions through Engine-owned state. Transport discovery and DDS graph presence remain diagnostic evidence, not proof that payloads are flowing and not the sole authority for session lifecycle. Source timestamps and Open-RMF control `session_id` remain payload context; neither is promoted to LiftAssistant source generation.

Freshness policy stays outside Core and is source-specific rather than derived from observation sequence or a universal timeout. The RMF adapter accepts an explicit maximum payload-silence setting with no default and arms it only after that subscription has observed payload, keeping discovery/reader startup separate from an active data plane becoming stale. A host-owned timer produces an explicit deterministic Engine invalidation action; Engine calls no wall clock. Invalidation removes current source evidence/resolved state and Operational truth while retaining the active session and its ordering watermark, so delayed or duplicate samples cannot restore stale state. A newer sample in that same adapter lifetime may rebuild evidence; a new adapter lifetime still requires an explicit new source session.

Runtime health is not one `healthy` flag. The current host keeps transport/discovery evidence, payload activity, adapter-session activity, per-car observation availability, and Engine readiness distinct. A stale Operational source revokes readiness without becoming an elevator equipment fault. A stale Auxiliary source remains visible but cannot revoke a healthy Operational authority, be promoted automatically, or supply missing fields. Last known observations remain bounded diagnostic evidence after current operational truth is revoked.

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

Canonical `CarObservationUpdate` values enter Engine through one ingestion boundary; there is no adapter state-query trait in the dispatch path. `IntentSink` accepts only high-level LiftAssistant intents. `SimulationDriver` produces semantic runtime events from explicit simulated/replay duration. A component implements only the contracts it actually supports. Vendor protocol objects must not leak into Core semantics.

## Current packages

- `liftassistant-model`: canonical domain types.
- `liftassistant-core`: deterministic vendor-neutral semantic decisions/state transitions.
- `liftassistant-engine`: shared deterministic semantic orchestration, observation assembly/resolution, and operational state ownership.
- `liftassistant-headless`: internal non-interactive read-only host/lifecycle and live integration proofs.
- `liftassistant-rmf`: shared Open-RMF semantic mapping, explicit native/logical binding adapter, and Rust-native subscription-only transport.
- `liftassistant-runtime`: semantic execution contract and runtime events.
- `liftassistant-sim`: deterministic active simulator, native-semantic recorded/read-only backend, and recorded BACnet mapping experiment.
- `liftassistant-tui`: developer simulator composition, scenario runner, and UI.

These eight packages reflect demonstrated ownership boundaries. The headless package
exists because Engine has a real second composition independent of the TUI. The RMF
package exists because recorded and live paths now share protocol-specific mapping
while transport dependencies must remain outside model, Core, and Engine. Neither
package establishes a general service or adapter framework.
Further packages still require real lifecycle/reuse pressure rather than speculative
layering.

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

The headless host keeps lifecycle and readiness separate: a live process begins
`Starting/NotReady`, incomplete Operational evidence remains `Synchronizing/NotReady`,
and sufficient Operational evidence permits `ReadyReadOnly/ReadOnly`. Restart or an
invariant violation from the Operational authority revokes readiness, while an
Auxiliary anomaly remains diagnostic evidence without revoking a healthy authority's
readiness. Its first long-running RMF composition accepts only explicit source identity,
ROS domain, floor mappings, native-to-logical bindings and roles, interaction-mode
projection, and source-specific freshness. It generates the ingestion-lifetime source
session internally, reconnects after stale invalidation without changing that session,
and handles SIGINT/SIGTERM cooperatively even while awaiting its first payload. These
internal states and CLI inputs do not grant write authority and are not a frozen public
lifecycle or configuration model.

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

Current conservative Busy evidence treats Engine-owned Assigned/Serving calls, motion, an unknown reported destination, or a known destination different from the car's current floor reference as occupied. A stopped car reporting its current floor as destination is not busy from that field alone; official Open-RMF Hotel idle snapshots demonstrate that such a destination can persist while the lift is stationary. This remains a small dispatch policy, not a generalized activity, occupancy, confidence, or service-phase model.

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
