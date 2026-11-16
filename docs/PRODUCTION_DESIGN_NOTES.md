# Production design notes

Status: **non-normative design notes**.

These notes preserve production-oriented ideas that have been reviewed as useful directions but are not all implemented, frozen, or part of a public LiftAssistant specification.

Current implementation truth remains in `docs/STATUS.md`. Architecture that is already accepted belongs in `docs/ARCHITECTURE.md`. Durable project decisions belong in `docs/PROJECT_MEMORY.md`.

## Production goals

LiftAssistant is expected to eventually operate in real production environments while remaining above an independently safe elevator execution domain.

Production design should therefore optimize for:

- separable Core/Engine, clients, adapters, and execution/safety domains;
- high-level semantic intents rather than raw actuator control;
- graceful loss of LiftAssistant without making the elevator unsafe;
- strong observability and fast field debugging;
- deterministic incident replay where available observations permit it;
- read-only adoption before write/control adoption;
- reversible internal semantics before public-schema stabilization.

## Observation model direction

Do not weaken complete operational `CarState` into a bag of optional fields merely to accommodate incomplete external sources.

Prefer a separate canonical observation layer:

~~~text
protocol/vendor data
        |
        v
partial canonical observation
quality / provenance / freshness
        |
        v
state resolver
        |
        v
complete operational state
        |
        v
semantic engine
~~~

Useful concepts to preserve for later design:

- explicit known / unknown / unavailable / uncertain / stale distinctions;
- source identity and sequence;
- source timestamp and receive timestamp when the source can support them;
- evidence/provenance rather than invented confidence scores;
- per-signal freshness/deadline/liveliness/lifespan semantics;
- raw evidence retention where practical so historical mappings can be re-evaluated.

The exact Rust types and any future public schema are not frozen.

## Runtime boundary direction

M4 implementation evidence separated the prototype responsibilities into:

~~~text
ObservationSource
IntentSink
Simulation/Replay Driver
~~~

Read-only adapters do not implement a write method merely to reject it. Simulator/replay time advancement does not define the semantics of a live production adapter. The internal Rust contracts remain reversible; no external protocol is frozen.

## Engine and client separation

Target direction:

~~~text
TUI / WebUI / HA / BMS / API
            |
            v
      semantic API
            |
            v
          Engine
  state / calls / dispatch
  resolver / reconciliation
            |
            v
 observation / intent boundary
            |
            v
         adapters
            |
            v
validated/certified controller
~~~

TUI and WebUI should become replaceable clients. A UI failure must not imply Engine failure.

An Engine crate/service should only be extracted after the common orchestration boundary is demonstrated by real callers; do not create a generic framework merely because this diagram exists.

## Reconciliation direction

Production control should not assume that a successful command submission proves the desired physical result.

Preserve the distinction between:

- desired semantic operation;
- accepted/submitted intent;
- reported observations;
- resolved operational state;
- completed/superseded/failed operation.

Restart recovery should resynchronize with current observations before new write authority is restored.

The exact authority/lease/idempotency model remains HUMAN-REVIEW territory.

## Diagnostics as a first-class capability

Fast debugging is a primary production requirement, not an optional logging feature.

Candidate diagnostics plane:

~~~text
observations ------------------------------+
       |                                   |
       v                                   v
state resolver / engine              diagnostics plane
                                           |
                         +-----------------+----------------+
                         |                 |                |
                    flight recorder   anomaly detection   alerts/issues
                         |
                    incident capsule
                         |
                    deterministic replay
~~~

Preserve these ideas:

- bounded flight recorder around recent observations/events/intents/decisions;
- incident capsule that freezes evidence before and after an anomaly/crash;
- stable machine-readable diagnostic codes rather than free-text-only logs;
- incident grouping/correlation;
- deterministic rule/invariant detectors first, trend detection second, ML only as an additional detector;
- anomaly reports should include evidence and remain distinct from confirmed root cause;
- actionable maintenance issues should be distinct from low-level observations and informational anomalies;
- subsystem aggregation so operators see which area is degraded rather than a flat warning flood;
- production release builds remain diagnosable through build IDs, symbols/coredumps, traces and replay.

## Developer and shadow debugging

Do not rely on a debug build as the only way to diagnose production failures.

Candidate operating roles:

- **Production**: optimized, observable, flight recorder enabled.
- **Diagnostic mode**: same production binary, deeper read-only introspection; safety semantics unchanged.
- **Shadow engine**: consumes mirrored live observations but has no write authority; can be paused/attached/replayed safely.
- **Engineering build**: simulator/HIL/bench use with extra assertions, fault injection and debugger-friendly settings.

A maintenance debug session against a primary engine, if ever supported, should close write authority first and require resynchronization before writes are re-enabled.

## Lifecycle and readiness

Process existence is not the same as operational readiness.

Useful lifecycle concepts to evaluate:

~~~text
Starting
-> Synchronizing
-> ReadyReadOnly
-> ReadyOperational

with Degraded / Failed as separate conditions
~~~

Do not freeze these exact state names yet.

Adapters and Engine should expose enough health/freshness information for a supervisor to distinguish alive, ready, degraded and stale.

## Adapter quality and isolation

Future adapters should expose an explicit validation profile, not just a boolean "supported":

- observation fields covered and their quality;
- timestamps/freshness behavior;
- supported high-level intents;
- recorded fixtures/replay coverage;
- virtual/bench/production-observation evidence;
- known ambiguities and unsupported semantics.

Production adapters should eventually be isolatable from Engine and from one another. Process separation, least privilege, device/network access restrictions and authenticated local IPC are candidate mechanisms. Do not freeze an IPC/plugin ABI before real adapter pressure exists.

## Semantic/API maturity

Do not declare new public semantics stable merely because an internal implementation works.

Useful maturity model to consider:

~~~text
Internal
-> Experimental
-> Preview
-> Stable
~~~

Stable semantics should have evidence from multiple real integrations/adapters and a migration/deprecation story.

Compatibility may eventually require translators or a temporary dual-emission window rather than breaking all older adapters at once.

## Update and recovery direction

Production deployment should eventually support:

- signed/versioned artifacts;
- health-checked activation;
- rollback on failed startup/compatibility checks;
- idempotent update steps;
- protection against accidentally accepting stale/incompatible releases;
- separation of update failure from elevator safety.

Do not build an OTA framework in M4 merely because this requirement exists.

## Community debugging and fixtures

A strong open ecosystem can reduce hardware dependence by sharing sanitized evidence.

Candidate repository/ecosystem concept:

~~~text
recorded fixtures
raw/sanitized protocol evidence
expected canonical observations
expected diagnostics
replay scenarios
~~~

Useful outputs:

- full private incident capsule for site use;
- sanitized shareable diagnostic bundle for community issues;
- recorded fixture corpus for adapter development without physical hardware.

Raw evidence should be retained for a bounded period where practical so improved mappings can re-analyze old incidents.

## Deferred decisions

These notes intentionally do **not** decide:

- a public observation schema;
- exact observation quality enum/type hierarchy;
- exact persistence/database format;
- authority/lease model;
- adapter IPC protocol or plugin ABI;
- production daemon topology;
- exact diagnostic code registry;
- update framework/provider;
- cloud architecture;
- passenger-elevator write/control approval;
- any safety-certification claim.

Promote a candidate from this file into architecture/memory only after implementation or explicit human review justifies it.


## Long-range phase boundaries

The long-range roadmap intentionally separates four kinds of evidence:

1. **M4 — semantic/production boundary evidence**: can real incomplete observations be represented, resolved, replayed and hosted without the TUI?
2. **M5 — operational read-only evidence**: can real sites obtain useful monitoring, diagnostics, anomaly evidence and field-debug workflows without control authority?
3. **M6/M7 — controlled-intent evidence**: can high-level requests be reconciled safely through virtual/HIL/bench/non-passenger environments and then approved production gateways?
4. **M8/M9 — sustained production evidence**: can deployment, recovery, update, compatibility, fallback and operational procedures withstand real field use?

This phase separation is deliberate. A feature from a later phase should be pulled earlier only when a current working slice produces concrete pressure for it.

A useful decision rule:

~~~text
Can this be proven with read-only evidence?
  -> do that first.

Does this require changing a public semantic contract?
  -> gather evidence from more than one independent adapter first.

Does this introduce write authority?
  -> HUMAN-REVIEW before generalizing it.

Does this affect the validated/certified safety execution domain?
  -> outside normal SELF-ITERATION.
~~~

The roadmap may advance quickly when evidence is strong, but later milestones are not a backlog to implement mechanically.
