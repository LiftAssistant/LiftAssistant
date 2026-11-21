# Architecture

## M6A virtual intent boundary

The controlled laboratory composes high-level `ServeFloor` and `ParkAt` with deterministic authority and a virtual lower controller. A hall call retains its `CallId` across dispatch decisions. Each distinct lower operation gets an `IntentId` from one deterministic issuer in the virtual orchestration lifetime. The caller-held request retains its original operation and call binding, so changed payloads still conflict after history eviction. Per-car admission watermarks prevent old IDs from executing again and let cars advance independently. Active calls and unresolved intents have separate registries from bounded terminal history. A retry of an evicted unchanged request returns a retired identity without resending.

An explicit grant authorizes one internal Principal for a controlled car. The grant identity is independent of controller identity and vendor holder/session. Write readiness checks that grant, configured controller binding and availability, current Operational evidence, explicit operation and floor coverage, and incompatible outstanding work. Read-only readiness supplies no write permission. Stale grants, conflict and unknown delivery fail closed. Engine and Core act on explicit inputs and use no wall clock.

Controller receipt is correlated by configured controller and operation identity. Only canonical Operational observations can satisfy an intent. The virtual predicate needs new position and motion evidence of a stopped car at the exact target after progress. For a car already at the target, a new closed-to-open door observation can establish that the queued operation had a physical effect. A linked hall call requires its own open-door evidence at the target. Reported destination stays separate from desired intent. Cancellation or expiry withdraws desired work and keeps conflicts fenced while execution is unresolved. The virtual checkpoint keeps ordering and unresolved work, fences old grants, and revokes stale observations. After restore, explicit receipt lookup can recover the controller outcome, and new Operational evidence can settle the physical outcome even while lookup is unavailable. The checkpoint is in memory. The externally reachable interfaces remain read-only.

Engine records conservative command state before entering a sink that may have
external side effects. Controlled submission reserves `Receipt::Indeterminate`;
controlled withdrawal reserves Requested. A sink interruption can therefore
leave uncertainty, but it cannot expose a false unsent state, permit duplicate
submission/withdrawal or claim that execution was prevented.

M6B.1 validates that boundary against the independently running official Open-RMF Hotel simulation. A simulation-only executable under `liftassistant-rmf/examples` supplies the narrow `/lift_requests` writer because it shares the concrete RMF message transport with the existing mapper; it is guarded by a fixed gateway-free test-network check and is not linked into the production Host, HTTP API, CLI or clients. Engine still allocates the Principal, grant and Intent. The example translates an Intent into an adapter-local RMF holder, treats holder observation as receipt evidence only, and completes through canonical Operational observations. Subscriber or writer recreation does not allocate another security identity, grant or Intent and does not resend an unresolved operation. The normal RMF adapter and long-running Host remain subscription-only.

## M6C external dispatch ownership boundary

Dispatch ownership is independent from `Intent`. LiftAssistant-owned calls keep
the existing path from `HallCall` through deterministic logical-car selection to
the car-specific `ServeFloor` intent. An external-controller-owned call instead
uses a separate internal sink carrying only its `CallId`, `HallCall` and
configured dispatch scope. Submission records boundary-local delivery state but
leaves the call `Pending`; it creates no placeholder car or `Intent`.

Each external dispatch scope binds one configured `ControllerId`, explicit
opaque-native-assignment-to-`CarId` mappings, and at most one current
Principal-held dispatch grant. This grant is separate from existing per-car
authority, is revocable/fenceable, and is required before submission. Controller
or vendor authentication does not create it. Assignment evidence from the
configured controller advances only `Pending -> Assigned(CarId)` after explicit
mapping. Unknown mappings, wrong controller/scope, conflicting reassignment and
terminal/retired calls return typed non-mutating results; identical assignment is
idempotent.

M6C.1b extends the [internal dispatch contract](M6C_DISPATCH_OWNERSHIP.md).
Explicit retry is limited to proven non-submission. A current dispatch grant
authorizes commands, and correlated evidence for already-sent work remains
eligible after revocation. Receipts and withdrawal outcomes are tracked
independently of service. Correlated serving evidence starts service for the
already assigned car. Only fresh Operational arrival and open-door evidence
can complete it through the existing call registry. Submission reserves
external ownership and uncertainty before sink entry, and withdrawal reserves
Requested before sink entry. An interruption cannot reopen either command for
automatic or accidental resend.

Proven prevention and physical completion release active external records into
bounded terminal history. Virtual restore keeps operation identity and fences
grants and stale observation evidence. This is an internal in-memory boundary.
Vendor mapping, durable recovery, destination calls, production adapters and
public write interfaces remain separate review work.

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

The ingestion-lifecycle owner supplies a fresh incarnation token for every new lifetime. Engine fences input by equality with the current token and retains one current token per configured source. The production Host obtains 128 random bits per incarnation from the system RNG. Observation ordering, arrival deadlines, Host revision and diagnostic sequence each have separate state.

Freshness policy stays outside Core and Engine. RMF applies the configured maximum arrival silence to both the feed and each bound car that has produced a payload. Before its first payload a car awaits data. A silent car can lose its evidence while a sibling stays current. The Host translates time into deterministic single-car or whole-source invalidation actions. Either scope preserves the active token and source-global ordering watermark. A newer same-session sample can rebuild evidence. Recreating the subscriber resets current transport discovery to subscriber-created and keeps stale payload state until actual data arrives.

`BuildingTopology` provides the building-level floor universe. Each RMF lift's reported `available_floors` may cover a subset. Explicit mapping and containment keep out-of-building floors invalid. Unconfigured RMF entities produce diagnostic evidence and are ignored. Mapping failures for a configured lift revoke only that binding. Transport failure can revoke the whole source. Per-lift floor context stays on the read side.

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

- `liftassistant-cli`: independent experimental HTTP/SSE read-only client process.
- `liftassistant-model`: canonical domain types.
- `liftassistant-core`: deterministic vendor-neutral semantic decisions/state transitions.
- `liftassistant-engine`: shared deterministic semantic orchestration, observation assembly/resolution, and operational state ownership.
- `liftassistant-headless`: internal non-interactive read-only host/lifecycle and live integration proofs.
- `liftassistant-lce`: bounded offline legacy-LCE recorded/transcript parsing and conservative mapping with no transport or command path.
- `liftassistant-rmf`: shared Open-RMF semantic mapping, explicit native/logical binding adapter, and Rust-native subscription-only transport.
- `liftassistant-runtime`: semantic execution contract and runtime events.
- `liftassistant-sim`: deterministic active simulator, native-semantic recorded/read-only backend, and recorded BACnet mapping experiment.
- `liftassistant-tui`: developer simulator composition, scenario runner, and UI.

These ten packages reflect demonstrated ownership boundaries. The headless package
exists because Engine has a real second composition independent of the TUI. The RMF
package exists because recorded and live paths now share protocol-specific mapping
while transport dependencies must remain outside model, Core, and Engine. The LCE
package remains model-only in normal dependencies: its transcript decoder is offline,
bounded, and separate from serial/terminal acquisition. None of these packages
establishes a general service or adapter framework.
Further packages still require real lifecycle/reuse pressure rather than speculative
layering.

The CLI is a real caller across the external process boundary, not a second composition of
Engine. It has a deliberately local `experimental/v0` response parser and only HTTP client,
JSON, and display concerns; it does not depend on model, Core, Engine, runtime, RMF, simulator,
or headless internals. Snapshot and continuously consumed SSE state both render host
lifecycle/readiness and revision, operational cars, source/session summaries, and recent
diagnostic identities. This proves separation without creating a shared client SDK, generated
schema, generic messaging layer, or commitment to a future frontend technology.

`client/` (`liftassistant_client`) is the first independent graphical client across that same
`experimental/v0` HTTP snapshot plus SSE boundary, targeting Android, Linux, and Web from one
shared Flutter codebase with platform-specific code limited to native `dart:io` versus browser
`fetch`/`ReadableStream` streaming. It links no LiftAssistant Rust crate, owns no elevator or
dispatch truth, replaces its read state from complete snapshots, and accepts Host-local revision
resets across Host restarts without durable cursors or event-log replay. Against normal
`liftassistant-headless` Hosts it remains strictly read-only; only when connected to an
explicitly opted-in loopback `sim-dev-server` does it expose the isolated development-only
Simulator `ServeFloor` workflow (`/api/dev-sim/v0/*`). Browser Same-Origin Policy on
unauthenticated HTTP means Web deployments currently use same-origin hosting or reverse proxying
rather than relaxing the Host's no-wildcard-CORS posture.

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

The production read-only composition adds a separate bounded host diagnostic stream around
that Engine evidence. Private machine identities partition transport, source/session,
mapping, observation/resolution, and readiness events; applicable entries retain source,
session, logical car, binding role, and source-native entity context. Transport or mapping
failure can revoke current operational truth but does not become an elevator equipment
fault, inferred root cause, or maintenance action. This line-oriented operator/developer
view remains internal and experimental; it is not a public registry or persistence format.

An internal incident capsule may freeze a bounded host action window together with topology,
explicit observation bindings, source/session metadata, a deterministic trigger, and the
expected lifecycle/readiness outcome. Its replay harness constructs a fresh host/Engine and
reapplies only recorded explicit session, observation, and invalidation actions. A capsule
missing an explicit source-session action is rejected rather than inferring lifecycle from an
observation. The JSON representation is versioned, size-bounded, create-new, and experimental;
it is neither a public schema nor a telemetry database. Production capture is opt-in and the
current RMF host attempts at most the first configured payload-stale capsule. A failed
optional output attempt emits a warning and leaves the Host's observation lifecycle intact.
The action window supports explicit single-car revocation and checks that each source
incarnation begins at most once within the artifact.

Sanitization is a separate deterministic transformation from a private capsule to a new
create-only candidate. It applies stable pseudonyms to source/session/car identities and
relationship-preserving floor remapping, then replays the candidate and requires the same
deterministic trigger before writing. The private source is not mutated. Every candidate
embeds a mandatory manual-review marker: the transformation is not perfect anonymization,
security certification, publication approval, or an automatic upload path.

The M5 host/health/diagnostic/capsule boundary is not RMF-specific: the existing recorded
BACnet Lift mapper drives the same explicit binding/session, readiness, source invalidation,
typed diagnostics, fresh-host capsule replay, and deterministic sanitization path in a
headless regression. BACnet remains a test-only dependency of headless, so the normal host
composition does not acquire the simulator or an unproved BACnet transport. This proves a
second canonical source family can exercise the internal platform; it does not prove live
BACnet discovery, COV/liveliness/freshness, controller compatibility, or production operation.

The first external read-only client boundary belongs only to `liftassistant-headless`. An
API-owned `experimental/v0` JSON projection exposes current host lifecycle/readiness,
Engine-owned operational cars, source/session health and binding roles, recent bounded
diagnostics, and a host-local monotonic revision without serializing canonical Rust or
internal event types. HTTP provides a complete current snapshot; SSE distributes the same
complete current model whenever that revision changes. A bounded latest-value channel may
coalesce intermediate revisions for slow clients, and reconnect begins from a fresh complete
snapshot. SSE is therefore current-state distribution, not the Flight Recorder or Incident
Capsule forensic history, and it promises no durable cursor, exactly-once delivery, or event
backlog. HTTP connection state and keepalive have no role in source freshness or Engine
readiness. The server is disabled by default, requires an explicit bind address, has no write
route or wildcard CORS, and remains experimental and unauthenticated rather than a stable
public API or authorization boundary.

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

## M7 production intent gateway boundary

The first production-shaped write path is an Open-RMF-specific gateway process,
not a generic adapter framework and not a public control service:

~~~text
local OS peer identity
        -> configured PrincipalId -> fresh AuthorityGrant
        -> Engine IntentId / admission / reconciliation
        -> private RMF gateway transport + durable unresolved record
        -> /lift_requests

/lift_states
        -> existing RMF mapper -> canonical Operational observations
        -> Engine receipt correlation and physical reconciliation
~~~

Engine continues to own authority, intent and operation identity, receipt
meaning, canonical observations and physical completion. The gateway owns RMF
transport/session mechanics, configured logical/native mapping, backend-local
correlation, wall-clock supervision, reconnect/readiness and a bounded private
restart record. A durable unresolved record is committed before a potentially
side-effecting publish. Restart never creates a replacement operation and never
automatically resends; it restores the same operation for observation-driven
reconciliation or remains explicitly recovery-required.

Command transport and observation subscription have independent lifecycles.
Loss of the command channel degrades write readiness without stopping state
ingestion; stale Operational observations block admission and completion without
inventing an Auxiliary failover. The first capability is exactly one configured
car-specific `ServeFloor`; RMF session correlation is backend-local and is not a
Principal, grant, `IntentId`, or observation source session. ROS publish success
is not a receipt. Existing Headless HTTP/SSE, CLI and Flutter interfaces remain
read-only. This boundary is authorized only for software/simulation/HIL/bench
progression and grants no passenger or real-controller write authority.

The first concrete process uses a mode-`0600` Unix socket plus kernel peer UID
for local caller authentication. Configuration maps that UID to one
`PrincipalId`; Engine creates a fresh process-local grant. A separate explicit
UID/Principal mapping grants the narrow recovery permission. Recovery never
turns that operator into the ordinary command Principal. A local advisory lease
derived from the RMF domain and native lift prevents overlapping processes from
covering the same control domain through different sockets; it is not a
cross-host lock. Neither the socket, RMF participant, holder session nor durable
file is a reusable grant. The private file retains one unresolved or
resumption-required operation, the `IntentId` watermark and 32 recent recovery
audit records, is committed and directory-synchronized before publication or
recovery transitions, and fails closed on corruption, incompatible revision,
unsafe permissions or an unreadable state. This is bounded single-operation
recovery, not a database or exactly-once transport.

RMF subscriber generations are transport resources inside one adapter source
lifetime. A generation that discovers peers but receives no first payload is
recreated after an explicit source-specific interval without changing
`ObservationSessionId` or claiming stale payload before data has ever flowed.
Once data has flowed, silence revokes Operational truth; only a real payload can
restore it. This keeps transport discovery, current payload activity, source
incarnation and canonical readiness distinct.

M7.1b keeps uncertain crash recovery gateway-local. An authorized operator may
retire the one durable unresolved operation only as outcome unknown; that action
does not mark Engine completion, prove remote inactivity or restore write
readiness. It creates a durable `ResumptionRequired` boundary. A distinct
authorized resumption action must re-evaluate the current local control-domain
lease, configuration revision, fresh complete Operational evidence, command
transport, observable RMF holder/correlation and absence of active local work.
Any accepted residual uncertainty is recorded as uncertainty in a bounded audit
trail. Generic Engine call/receipt meanings and all public read-only interfaces
remain unchanged.

A restarted gateway may restore persisted receipt/progress into Engine only
after a fresh observation carries the exact unresolved backend holder. Persisted
progress plus a periodic target-floor snapshot is not causal evidence that the
old operation completed. If the holder has already cleared, the gateway remains
`RecoveryRequired` until the explicit operator path retires the local record as
outcome unknown. This exact-holder fence was required by the completed official
Hotel late-restart proof; it is gateway-local correlation policy and does not
change Engine receipt or reconciliation semantics.


## Development-only Simulator Operator Surface (`dev-sim/v0`)

The Operator Surface Foundation slice exercises the first end-to-end Flutter-to-Engine
`ServeFloor` workflow exclusively against the in-process `VirtualController(SimRuntime)`
simulator without altering production authority, recovery, or public read-only API contracts:

~~~text
Flutter client (backend-agnostic UI projection)
  -> GET  /api/experimental/v0/{snapshot,events}    (strictly read-only, unchanged)
  -> GET  /api/dev-sim/v0/capabilities              (opt-in loopback dev-sim only)
  -> POST /api/dev-sim/v0/serve-floor/preview       (advisory, side-effect-free)
  -> POST /api/dev-sim/v0/serve-floor/submit        (client submission_key -> Engine admission)
  -> GET  /api/dev-sim/v0/submissions/{key}         (read-only reconnect projection, no resend)
        |
        v
sim-dev-server (liftassistant-headless example; dev-dependency only)
        |
        v
Existing Engine controlled-intent path (PrincipalId, AuthorityGrant, IntentId, receipt, reconciliation)
        |
        v
VirtualController(SimRuntime) -> canonical Operational observations -> Engine reconciliation
~~~

Key boundary invariants:

- **Isolated Simulator-only control surface:** `sim-dev-server` is a development-only example
  target in `liftassistant-headless` that reuses `liftassistant-sim` strictly via
  `[dev-dependencies]` (keeping `liftassistant-headless`'s normal dependency tree free of
  `liftassistant-sim`). Control routes under `/api/dev-sim/v0/*` are disabled by default, require
  explicit `--enable-dev-sim-control`, refuse non-loopback bind addresses, enforce 4 KiB request
  bounds with `deny_unknown_fields`, and cannot reach RMF, LCE, HIL, or real hardware.
  `/api/experimental/v0/snapshot` and `/api/experimental/v0/events` remain strictly read-only GET.
- **Single truth owner:** Engine continues to own controlled-intent admission, `PrincipalId` /
  `AuthorityGrant`, `IntentId`, controller receipts, canonical Operational observations, and
  physical completion/reconciliation. `sim-dev-server` mints only a process-local development
  Principal and per-car Engine grant for the Simulator fixture; this is not a production identity
  or multi-user authorization design.
- **Submission correlation (`submission_key`) is not a canonical identity:** Before any possible
  side effect, the Flutter client generates a 128-bit random `submission_key` that stays solely in
  the `dev-sim/v0` transport/client correlation layer and never enters `liftassistant-model`,
  `liftassistant-core`, or `liftassistant-engine`. Mapping is
  `submission_key -> admitted submission -> Engine IntentId`. Same key plus identical immutable
  `(car, floor)` is idempotent and never allocates a second `IntentId`; same key plus different
  `(car, floor)` fails closed with HTTP 409 conflict; pre-side-effect rejections remain terminal
  rejected for that key; a new `submission_key` with the same `(car, floor)` is a fresh user
  submission evaluated by live Engine admission. The in-memory submission registry is bounded
  (128 records) and preserves active Engine operations during eviction.
- **Reconnect and process-restart honesty without automatic resend:** If a submission response is
  lost after dispatch, Flutter transitions the operation to first-class `Unknown` and never
  automatically resends. Within the same `control_instance_id` lifetime, reconnect performs
  read-only `GET /api/dev-sim/v0/submissions/{submission_key}` to project the latest Engine
  `IntentRecord` facts (`accepted`, `in_progress`, `completed`, `diverged`, `rejected`,
  `not_found`, or `knowledge_unavailable`). Across a `sim-dev-server` restart, the fresh 128-bit
  ephemeral `control_instance_id` causes queries carrying `expected_instance_id` to report
  `knowledge_unavailable` (`control_process_restarted`) rather than claiming the request never
  happened, and no automatic resend occurs.
- **Distinct capability dimensions and floor legality:** Read-side capability and advisory preview
  projections preserve `support`, `control_path_availability` (with explicit reason),
  `authorization` (honestly reporting `dev_process_local_grant` rather than fabricating
  multi-user `authorized = true`), and per-request `eligible` / Engine `WriteBlocker`. Preview is
  strictly advisory and side-effect-free; submit always re-evaluates real Engine admission.
  Target floor choices come exclusively from Engine `ExecutionCapabilities` (never from full
  `BuildingTopology`, observation floor lists, or UI guesses). Configured-but-unavailable cars
  without valid Operational truth cannot preview or submit, and Auxiliary bindings never restore
  command eligibility.
- **Scoped command surface (`State + ServeFloor` only):** Flutter remains backend-agnostic with
  no backend-specific UI branches and no second operation lifecycle state machine. Mode control
  (`SetMode`, Normal/OOS/FireService/Inspection/Independent/parking) and persistent
  Setting/Parameter configuration are out of scope — persistent settings have a different
  lifecycle from one-shot operations and are intentionally omitted.
- **Real-backend exclusions:** Neither `sim-dev-server` nor Flutter connects to the M7 RMF
  gateway, exposes RMF over HTTP, alters M7 durability/recovery, writes to LCE (which remains
  `RECORDED_READONLY_PARTIAL`), or accesses any real controller, CAN/BACnet/OPC bus, HIL rig, or
  passenger elevator. This boundary is experimental and development-only, not a frozen public
  schema or production control API.

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
