# Roadmap

The roadmap is ordered by demonstrated need. Do not implement later milestones merely because they are listed here.

## M0 — Bootstrap — complete

- repository principles and agent workflow;
- Rust workspace;
- model/core/runtime/sim/TUI boundaries;
- minimal CI.

## M1 — Interactive Simulator — complete and human-accepted

- 2 cars / 8 floors;
- valid hall calls;
- deterministic nearest-eligible-car dispatch;
- `ServeFloor` intent;
- deterministic simulated movement/arrival/door state;
- interactive developer TUI;
- bounded event/decision trace;
- fixed-step pause/single-step;
- deterministic in-memory replay;
- availability fallback and no-eligible-car behavior.

M1 acceptance was confirmed by automated checks and hands-on TUI use.

## M2 — Semantic Core — complete; human semantic review pending (non-blocking)

Goal: evolve from one-shot dispatch into persistent, observable elevator-call semantics.

For new M2 semantics, use targeted prior-art research from `docs/PRIOR_ART.md` before finalizing terminology or reusable boundaries. Research should inform the next vertical slice, not replace implementation.

### M2.1 — Call identity and lifecycle
- stable call identity;
- Pending -> Assigned -> Serving -> Completed;
- duplicate active call handling;
- lifecycle visible to developer tooling.

### M2.2 — Multiple pending calls
- calls coexist without overwriting an active trip;
- deterministic assignment to eligible/idle cars;
- pending work waits when no car is available;
- reevaluate work after service completion.

### M2.3 — Runtime feedback integration
- semantic runtime events drive call lifecycle;
- replay validates semantic lifecycle and physical state.

### M2.4 — Decision explanations
- structured selected/rejected reasons;
- capability/mode/distance/tie-break evidence;
- no large policy DSL.

### M2.5 — Capability/configuration refinement
- move demonstrated semantic assumptions out of UI-only code where appropriate;
- refine topology/capability/configuration vocabulary against existing standards;
- HUMAN-REVIEW before changing safety/authority semantics or freezing normative public schemas.

## M3 — Research and conformance laboratory — complete

- reusable scenario representation only when real use requires it;
- trace inspection;
- controlled fault injection for simulator/bench work;
- standards mappings;
- backend semantic conformance checks;
- virtual/read-only adapter experiments.

Do not turn M3 into a certification claim.

Generic multi-backend conformance machinery is deferred until a second backend exists. Protocol mappings are deferred until a concrete bridge supplies evidence.

## M4 — Production boundary and real-world bridges — complete

Goal: make the prototype architecture honest about incomplete real-world observations, separable runtime responsibilities, shared orchestration, diagnostics, and headless operation before introducing real control.

Ordered direction:

1. canonical partial observation proof while keeping operational state complete — implemented;
2. separate observation-source, high-level intent-sink, and simulator/replay driving responsibilities — implemented;
3. extract a shared Engine boundary once duplicated orchestration demonstrates the need — implemented;
4. prove a minimal diagnostics/flight-recorder/replay substrate — implemented;
5. prove a headless read-only host/lifecycle — implemented;
6. validate the observation model with at least one additional independent source/protocol family — implemented with recorded BACnet evidence;
7. harden observation semantics for reported-vs-desired state, location precision, incremental updates, and Engine-owned operational state — implemented;
8. obtain live read-only evidence or a supplied sanitized real capture/test endpoint — acquired with the official Open-RMF Hotel simulation;
9. expand recorded/virtual/live-read-only adapters only when they add new semantic or deployment evidence — deferred into M5;
10. controlled non-passenger HIL/bench observation work — deferred beyond the M5 read-only scope;
11. only later consider any high-level write path under explicit HUMAN-REVIEW — deferred to M6 or later.

Candidate bridge work after the boundary is sound:

- Open-RMF live/read-only integration;
- BACnet elevator/building observation mapping;
- virtual CAN / SocketCAN;
- CiA 417 mapping experiments;
- T/GSET 001-2025 gateway research;
- documented controller APIs;
- sanitized recorded controller/protocol fixtures.

M4 exit evidence includes two independent recorded source families, a headless read-only path, hardened observation semantics, explicit observation authority/session ownership, and official Open-RMF live read-only lifecycle evidence. M4.10 human review was accepted and the live idle destination evidence corrected the remaining overly broad dispatch Busy heuristic.

Any passenger-elevator control, safety-authority change, normative public-schema freeze, or first real write path requires HUMAN-REVIEW.

## M5 — Production Read-only Platform — complete

Goal: make LiftAssistant genuinely useful on real sites without requiring control authority.

M4 semantic hardening and official live evidence satisfy the entry gate. The ordered M5 direction is:

1. **M5.1 — Live Open-RMF Observation Adapter**;
2. **M5.2 — Source Lifecycle / Freshness / Health**;
3. **M5.3 — Production Read-only Host**;
4. **M5.4 — Production Diagnostics**;
5. **M5.5 — Incident Capsule / Replay**;
6. **M5.6 — Sanitization / Community Fixtures**;
7. **M5.7 — Adapter Validation Profiles**;
8. **M5.8 — Second Production Source Family**;
9. **M5.9 — Experimental Read-only API**;
10. **M5.10 — TUI/CLI Client Separation**;
11. **M5.11 — LiftAssistant Client / WebUI** — conditional;
12. **M5.12 — HA/BMS observation integration** — conditional;
13. **M5.13 — Failure / Soak Matrix**;
14. **M5.14 — M5 Completion Audit**.

This ordering is a future plan, not blanket implementation authorization. `docs/STATUS.md` promotes only the highest-priority evidence-backed slice to READY. M5.9 and later require separate authorization; WebUI, Flutter, and HA/BMS implementation are not part of the current backend run.

M5 is allowed to become deployable read-only software, but must not claim passenger-elevator control capability or certification.

A public semantic API should not be declared Stable until multiple independent adapters have exercised it and migration expectations are understood.

## HUMAN checkpoint — Authority and write model

Before a general write-capable architecture is introduced, explicitly review:

- who/principal can request control;
- intent identity and idempotency;
- expiry/cancellation/supersession;
- receipts and reconciliation;
- stale/insufficient observation handling;
- readiness and write-enable state;
- restart/reconnect behavior;
- concurrent requester conflicts;
- audit requirements;
- adapter/controller capability boundaries.

This checkpoint is not permission for low-level brake/drive/safety actuation.

## M6 — Controlled Intent Laboratory

Goal: validate high-level write semantics without using passenger production as the experiment.

M6A covers the software/virtual phase through the completion audit. M6B candidate
external validation requires another HUMAN architecture review.

Entry requires a new HUMAN architecture review. Before a general call or write workload,
separate active calls from bounded completed-call history, define intent identity and
receipts, idempotency, expiry, cancellation/supersession, write readiness/authority and
restart reconciliation. Where cars have different service sets, establish evidence-backed
per-car execution floor capabilities before dispatch or writes depend on them.

Progression should be evidence-driven:

~~~text
Simulator
-> virtual controller
-> HIL
-> bench controller
-> controlled non-passenger environment
~~~

Candidate work:

- explicit intent identity/receipt lifecycle;
- desired-versus-reported reconciliation;
- idempotent retry behavior;
- expiry/cancellation handling;
- write readiness/authority gate;
- deterministic restart/resynchronization tests;
- write-capable adapter validation profiles;
- shadow comparison between commanded and observed behavior;
- incident capture around rejected/failed/inconsistent intents.

Only high-level intents belong here. Raw brake, motor torque, safety-chain bypass, door-lock bypass, or comparable safety primitives remain outside LiftAssistant Core semantics.

## M7 — Production Intent Gateway

Goal: allow approved high-level LiftAssistant intents to reach validated lower controllers while retaining clear safety and authority boundaries.

Possible scope after M6 evidence:

- production adapter processes or equivalent isolation where justified;
- authenticated/authorized high-level intent path;
- least-privilege device/network access;
- controller capability negotiation;
- reconciliation after disconnect/restart;
- audit and correlation from request to observed completion;
- read-only fallback when write readiness is lost;
- maintenance/service integrations where allowed;
- selected building/HA/BMS control integrations through the same semantic authority path.

This milestone does not make LiftAssistant the safety controller. The lower validated/certified execution domain remains responsible for motion, doors, brakes, safety chains and hardware safety.

Passenger-production write deployment remains HUMAN-REVIEW and may require external regulatory/safety assessment.

## M8 — Production Hardening

Goal: make deployment, recovery, updates, compatibility and field debugging robust enough for sustained operation.

Read-only deployment debt includes external RMF string/array cardinality limits, CLI
snapshot/SSE payload bounds, Flutter reconnect jitter, HTTP connection/rate limits,
TLS/auth and explicit browser origin policy, and stable compatibility once the wire
contract is approved.

Introduce these only when real deployment pressure exists:

- process/component supervision and restart-rate control;
- readiness/degraded-state reporting;
- crash capsules, symbols/coredumps and deterministic replay workflow;
- configuration validation and migration;
- semantic/API compatibility and deprecation policy;
- adapter compatibility manifests;
- least-privilege sandboxing;
- signed/versioned releases;
- health-checked activation and rollback;
- idempotent update steps;
- stale/incompatible release protection;
- backup/restore where actual persisted state requires it;
- long-term observability/retention policy.

Do not build an appliance/OS/OTA platform earlier merely because M8 lists one.

## M9 — Passenger-production Pilot — gated

This is a future deployment milestone, not an automatic coding target.

Entry requires explicit HUMAN-REVIEW and whatever external safety/regulatory/site approval is applicable.

The intended boundary remains:

~~~text
LiftAssistant supervisory semantics / diagnostics / high-level intent
                         |
                         v
          validated/certified controller domain
                         |
                         v
             motion / door / safety hardware
~~~

Pilot acceptance should focus on controlled scope, fallback behavior, diagnosability, rollback, operational procedures and evidence—not on expanding LiftAssistant into a safety controller.

## Long-running constraints

- integrate existing standards instead of replacing them;
- LiftAssistant expresses WHAT, validated lower systems decide HOW;
- simulation/replay before real hardware;
- read-only evidence before write authority;
- working vertical slices before infrastructure expansion;
- current `STATUS.md` outranks future roadmap items for implementation priority;
- do not freeze public semantics based on one adapter;
- no premature microservices, plugin marketplace, database, IPC ABI, cloud platform or large policy DSL;
- research existing standards/projects before inventing durable semantics, but do not turn research into a milestone by itself.
