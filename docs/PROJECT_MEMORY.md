# Project memory

Last consolidated: 2026-11-17

This file contains durable project facts and decisions only.

Current milestone/progress belongs in `docs/STATUS.md`. Agent workflow belongs in `AGENTS.md`. Architecture details belong in `docs/ARCHITECTURE.md`.

## Identity

- Project name: LiftAssistant.
- Earlier working names: OpenLift, LibreLift.
- OpenLift was rejected as a formal name due to existing software/elevator-industry uses.
- LiftAssistant reflects a neutral operational/integration layer, analogous in spirit to Home Assistant unifying fragmented smart-home ecosystems.

## Mission

LiftAssistant should make elevator systems behave more like an open computing platform:

- normalize fragmented controller/protocol ecosystems;
- expose common semantic state and capabilities;
- express high-level intents/policies independently of vendors;
- support simulation and real hardware through the same conceptual runtime boundary;
- integrate existing standards instead of replacing them.

It is not primarily a new elevator brand and not merely a monitoring dashboard.

## Durable architecture decisions

1. Public semantics are language-independent.
2. Rust is the primary reference implementation language.
3. Monorepo first; split only for genuinely independent lifecycles.
4. Core is functional/deterministic and vendor-neutral.
5. LiftAssistant says WHAT; validated runtime/safety systems decide HOW.
6. Low-level brake/drive/torque/safety-bypass primitives do not belong in Core.
7. Simulator is a first-class reference runtime backend and must remain deterministic/replayable.
8. TUI is a developer observable/composition client, not the architecture.
9. No early real passenger-elevator control.
10. Existing standards are inputs: CiA 417, BACnet, SAREF4LIFT, T/GSET 001-2025, Open-RMF/RFA, THOR/NeXt, and other relevant work.
11. Adoption can be layered: observe, diagnostics, policy/group semantics, adapters, later native runtime where appropriate.
12. Avoid premature application/service/plugin layers; extract them only when demonstrated by real callers.
13. Production architecture should keep semantic logic, user interfaces, external adapters, and the validated/certified execution domain separable.
14. Production debugging/diagnostics, incident evidence, and deterministic replay are first-class platform concerns rather than optional logging features.
15. Complete operational state should not be weakened merely to fit incomplete external sources; external uncertainty should be represented before operational-state resolution.
16. Production deployment must remain safe when LiftAssistant or one of its user-facing components is unavailable; the lower execution/safety domain remains independently responsible for elevator safety.

## Product philosophy

- Capabilities rather than OEM SKU gates should determine features.
- Hide industrial complexity from ordinary users unless requested.
- Developer tooling may expose raw events/protocol details.
- A future Home Assistant integration is a useful validation target, not an early milestone requirement.
- Prefer a working vertical slice before adding generalized infrastructure.
- Review relevant established standards/mature implementations before inventing durable public-facing semantics.

## Validated M1 baseline

M1 Interactive Simulator was functionally implemented and human-accepted in November 2026.

Validated capabilities include:

~~~text
HallCall
  -> deterministic dispatch
  -> high-level ServeFloor intent
  -> semantic runtime boundary
  -> deterministic simulator movement
  -> arrival / door-open state
  -> observable developer TUI
~~~

The developer TUI also validates fixed-step pause/single-step behavior, deterministic replay, availability fallback, no-eligible-car behavior, and bounded event tracing.

The key limitation intentionally left at M1 close was that calls were one-shot and could replace a car's current target; M2 replaced that behavior with persistent call semantics.

## Validated M2 call semantics

M2 uses deterministic sequential `CallId` values and a Core-owned internal lifecycle: `Pending -> Assigned(car) -> Serving(car) -> Completed`. Active duplicate floor+direction calls share identity; a later request after completion receives a new identity.

Pending calls are considered in identity order. Only Normal, capable, semantically idle cars are eligible, so a later call cannot replace an Assigned/Serving trip. Runtime intent acceptance starts service; matching arrival plus door-open feedback completes service and releases the car for immediate pending-work reevaluation. Core retains arrival evidence across event batches rather than relying on simulator-specific event grouping.

Core dispatch results include deterministic per-car explanations: observed mode, semantic/runtime occupancy, serve-floor capability availability, distance, rejection reasons, and whether selection used sole eligibility, nearest distance, or the car-ID tie-break. These are orchestration explanations and must not be conflated with standardized equipment faults/alarms.

The current building configuration is intentionally narrow: `BuildingTopology` is a canonical ordered nonempty set of served floors used by Core call validation and by composition/replay. It supports non-contiguous floors but does not yet claim public floor naming, door/service mapping, spatial modeling, or protocol index semantics.

These call semantics remain an internal, reversible model informed by CiA 417, BACnet, SAREF4LIFT, and Open-RMF prior art. They are not a frozen normative public schema.

## Validated M4 read-only boundary

The first non-simulator backend is a deterministic, native-semantic recorded feed. Canonical `CarObservationUpdate` values enter Engine through one ingestion boundary; Engine owns incremental evidence and the resolved read-side state used by dispatch and clients. `IntentSink` remains the separate high-level LiftAssistant intent boundary, while `SimulationDriver` owns explicit-time simulator/replay progression. The active simulator emits observations and implements intent plus driving behavior. The recorded feed exposes observations and replay driving only, so it has no write interface to reject or accidentally expose. Its position/door `ObservationCoverage` stays in the source layer and is not an execution capability. It defines no file format or live transport.

The first external mapping experiment uses recorded Open-RMF `LiftState` fields. External partial state enters a separate internal `CarObservation` with explicit source identity, source-session identity, deterministic sequence, and semantic evidence. Moving-with-floor-reference-only position, undirected door movement, declared unknowns, and source-specific modes remain honest evidence instead of mapping failures or guessed operational fields. Source floor labels, modes, native `lift_name`, and Open-RMF control `session_id` remain adapter context; the Open-RMF session value is not the LiftAssistant source lifetime token.

Operational `CarState` remains complete without requiring fabricated precision. Its `CarLocation` distinguishes floor-level association from a genuine continuous `Position`; the deterministic resolver preserves either precision and promotes an observation only when location, motion, directed door state, and LiftAssistant operating mode are all sufficiently established. This observation model and mapping remain internal and reversible. No normative schema, generic telemetry framework, live middleware dependency, protocol write, or hardware path is approved or frozen.

Incremental canonical input uses `CarObservationUpdate`: an omitted outer optional field leaves current evidence unchanged, while an included `Unknown` evidence value explicitly replaces prior knowledge. The deterministic in-memory `ObservationAssembler` keeps evidence isolated by configured source session and logical car, retains the last supplying provenance per field, and rejects non-increasing source-global sequence within a session without treating sequence as wall-clock freshness. Source sessions are opaque identity tokens activated explicitly by Engine; switching sessions invalidates that source's prior evidence/state, allows sequence restart, and prevents delayed retired-session data from restoring old state. It adds no persistence, confidence scoring, smoothing, freshness, heartbeat, or generic telemetry framework.

The second independent mapping experiment uses selected decoded properties from the official BACnet Lift object. It demonstrated that canonical evidence also needs explicit unknown position and unknown aggregate door state. BACnet universal floor numbers require configured mapping and remain floor references; its car-door status is an array and disagreeing doors cannot be collapsed into one car state. BACnet object `Out_Of_Service` means the object is decoupled from the represented lift, so all current operational evidence becomes unknown rather than mapping that flag to LiftAssistant `OperatingMode::OutOfService`. Raw object properties remain adapter context. This is a typed in-memory projection, not a BACnet stack, live device, file/packet schema, write path, or conformance claim.

`CarId` is LiftAssistant logical car identity, while `ObservationSourceId` identifies a configured feed. Source-native identities require explicit adapter mapping to `CarId`; equal names do not create identity. Engine requires explicit `(source, car)` bindings with Operational or Auxiliary role, permits one source to bind multiple cars and one car to have multiple sources, and rejects a second Operational authority for the same logical car.

The shared internal `liftassistant-engine` owns topology, call lifecycle, dispatch and pending scheduling, execution capabilities, explicit observation bindings/sessions, incremental observation assembly, unique operational state by logical `CarId`, isolated per-source evidence/state, high-level intent submission/result handling, runtime-feedback transitions, and semantic events. Only the configured Operational source can update or invalidate dispatch/readiness truth. Auxiliary sources remain inspectable and diagnostic but cannot overwrite or fill authority state, enter dispatch, or fail over automatically. Core consumes only `ExecutionCapabilities`; observation coverage and observation authority do not grant write authority or an `IntentSink`. Dispatch, TUI, headless, and scenario read paths use this Engine-owned operational view rather than querying adapter-owned truth. Core remains deterministic and I/O-free. The developer TUI retains presentation, simulator-only controls, explicit driving, display trace, and replay input recording; both interactive and non-interactive scenario paths use the same Engine orchestration.

Engine also owns a bounded in-memory diagnostic flight recorder over canonical observations and machine-readable Engine events. Observation entries retain source, source session, logical car, sequence, and binding role. Its first deterministic detectors cover non-increasing per-source-session observation sequence and runtime intent acceptance without a matching generated intent. A trigger freezes a bounded before/after incident window that can be replayed through the same detector. These anomalies are invariant evidence, not equipment faults, alarms, root causes, or maintenance advice. The implementation deliberately defines no durable storage, external telemetry, public code registry, cross-source value-conflict alarm, or broad diagnostic taxonomy.

The internal `liftassistant-headless` binary is the first non-TUI composition. It depends on Engine/model/runtime and now composes the RMF read-only adapter, but not the TUI, simulator, or an `IntentSink` implementation. A recorded canonical-observation feed drives reversible internal `Starting`, `Synchronizing`, `ReadyReadOnly`, and `Degraded` states with separate `NotReady`/`ReadOnly` readiness. Insufficient Operational evidence cannot become ready, and an Operational session reset or sequence anomaly revokes readiness without turning the anomaly into a root-cause or equipment-fault claim. Readiness requires complete operational state for every configured Operational car. Auxiliary observations and anomalies remain visible but do not revoke healthy Operational readiness. These names and interfaces are not a stable public API or production-host claim.

Official Open-RMF Hotel live evidence closed the M4 external-input gate. One `/lift_states` publisher carried periodic complete snapshots for two lifts at about one update per lift per second; late subscribers and observer/source restarts recovered complete state. The test also established that DDS discovery does not prove payload delivery and that process death does not immediately remove graph evidence. LiftAssistant source sessions therefore remain explicitly controlled by adapter lifecycle; DDS graph state is diagnostic evidence, and RMF `lift_time` or control `session_id` must not become `ObservationSessionId`.

The same live idle snapshots repeatedly reported `current_floor == destination_floor` while motion was stopped. Core therefore does not treat a known reported destination equal to the current floor reference as Busy by itself. Motion, an unknown or different reported destination, and Engine-owned Assigned/Serving calls remain conservative Busy evidence. This is a bounded dispatch rule, not a new activity or occupancy domain model.

M5.1 implements a concrete `liftassistant-rmf` ownership boundary because recorded and live Open-RMF paths need one semantic mapper while ROS 2/RustDDS dependencies must remain outside the canonical model, Core, and Engine. Its live side is subscription-only: explicit native `lift_name` bindings produce logical `CarId` updates, an adapter-owned source session supplies one source-global/session-local sequence, and canonical updates enter Engine for assembly/resolution and operational ownership. The adapter exposes no `/lift_requests` publisher or `IntentSink`.

Open-RMF `MODE_AGV` and `MODE_HUMAN` remain source-specific evidence by default. A configured RMF source may explicitly and independently project either interaction mode to canonical Normal evidence; this never changes Fire, Offline, Unknown, or Emergency handling and never discards raw current/available modes or RMF control-session context. Official Hotel validation exercised two lifts through explicit mapping, ReadyReadOnly state, adapter/source restart, old-session invalidation/rejection, sequence restart, and real simulation motion/door transitions without giving LiftAssistant a write path.

Freshness is adapter/host-owned and source-specific; observation sequence remains ordering only and Core owns no wall clock. Engine exposes a deterministic explicit source-observation invalidation action for the active session. It removes current evidence, resolved source state, and any Operational truth contributed by that source while retaining the active session and sequence watermark. This prevents delayed or duplicate same-session data from restoring stale evidence and permits genuinely newer data in the same adapter lifetime to resynchronize.

The read-only host separates transport/discovery evidence, payload activity, source-session activity, per-car observation availability, and Engine readiness rather than collapsing them into one health flag. The RMF payload-silence policy has no default and starts only after that subscription has received payload, so reader discovery delay is not a false stale event. Official Hotel validation established Operational stale/readiness revocation and same-session recovery. Auxiliary staleness remains diagnostic/inspectable and cannot revoke a healthy Operational authority or trigger failover/fusion.

`liftassistant-headless` now has a long-running experimental RMF read-only composition in addition to its executable proofs. Its minimal CLI requires explicit ROS domain, source identity, source-specific freshness, floor labels, native-to-logical car bindings with Operational/Auxiliary roles, and any AGV/HUMAN-to-Normal projection. The host generates an opaque adapter-lifetime session using host process context rather than RMF payload fields, reports readiness transitions, reconnects after stale invalidation, and exits cooperatively on SIGINT/SIGTERM even before payload discovery. This is an internal deployment surface, not a stable public configuration/API, and it has no publisher or `IntentSink`.

Production diagnostics now pair the Engine's bounded canonical Flight Recorder with a separate bounded host stream. Host entries have monotonic internal sequences and machine-readable identities for Transport, Source/session, Mapping, Observation/resolution, and Readiness, plus source/session/car/role/native-entity context where applicable. They remain operational evidence rather than equipment-fault, root-cause, repair, or public-schema assertions. Auxiliary diagnostics remain visible without revoking healthy Operational readiness.

## License

No license selected. This remains a HUMAN-REVIEW decision.
