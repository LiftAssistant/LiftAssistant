# Project status

Updated: 2026-11-19

## Current milestone

**M5 — Production Read-only Platform**

M1 Interactive Simulator is closed and human-accepted.

M4 is COMPLETE. M4.10 HUMAN-CHECK was accepted on 2026-11-17 after all four slices and the live-evidence-exposed dispatch regression passed review.

M5.1 through M5.11 are implemented. HUMAN accepted the M5.11 first-version client boundary for backend progression on 2026-11-18. M5.12 HA/BMS is deferred and conditional. **M5.13 foundation correction and revalidation are in progress.** M5.14 follows the M5.13 gate. Second-family live validation remains `EXTERNAL_VALIDATION_PENDING` outside the M5 completion gate.

## Validated baseline

M1 established and validated:

- canonical model types for cars, floors, motion, doors, modes, capabilities, calls, and intents;
- pure deterministic nearest-eligible-car dispatch;
- runtime trait and semantic runtime events;
- deterministic fixed-duration simulator stepping;
- simulated movement, arrival, and door-open state;
- interactive two-car/eight-floor TUI;
- valid hall-call boundary handling;
- pause/resume/single-step/reset/quit controls;
- bounded decision/runtime trace;
- deterministic in-memory replay;
- availability fallback with OutOfService cars;
- no-eligible-car behavior;
- terminal restoration on normal/error exit paths.

Automated validation at M1 close:
- `cargo fmt --all -- --check`: passed;
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed;
- `cargo test --workspace --locked`: 12 passed;
- Rust 1.85 all-target workspace check with committed lockfile: passed;
- PTY smoke checks passed.

Human validation on 2026-11-14:
- TUI launched and rendered correctly in Konsole;
- tested key-driven functions behaved as expected;
- M1 developer TUI accepted as the first usable version.

No remaining M1 HUMAN_PENDING item.

## M2 completion summary

M1 call handling was one-shot:

~~~text
HallCall
  -> immediate dispatch
  -> ServeFloor
  -> runtime target
~~~

M2.1 through M2.5 now provide persistent call identity/lifecycle, multiple pending calls without target replacement, runtime-feedback-driven serving/completion/reassignment, structured dispatch explanations, and explicit served-floor topology. M2 implementation and machine review are complete.

## M2.1 — Call identity and lifecycle

**Implemented.**

Introduce the smallest durable call model needed to represent active hall calls.

Target lifecycle:

~~~text
Pending
  -> Assigned(car)
  -> Serving(car)
  -> Completed
~~~

Requirements:
- perform a targeted prior-art check using `docs/PRIOR_ART.md` before finalizing lifecycle terminology;
- calls have stable identity;
- duplicate active floor+direction calls are not multiplied unnecessarily;
- lifecycle state is observable;
- keep semantics deterministic;
- do not introduce a generic workflow/state-machine framework.

Implemented behavior:
- deterministic `CallId` allocation starts at `#1` and is replay-stable;
- Core owns a persistent `CallRegistry` with `Pending`, `Assigned(car)`, `Serving(car)`, and `Completed` states;
- duplicate active floor+direction calls reuse the existing identity; a still-pending duplicate may retry assignment, while an assigned/serving duplicate is not resubmitted;
- completed calls remain distinguishable and a later same floor+direction request receives a new identity;
- the developer TUI observes Core call records and semantic call-state events;
- runtime feedback now advances successfully submitted calls through `Assigned`, `Serving`, and `Completed` as described in M2.3.

Targeted prior-art findings and the deliberate internal lifecycle divergence are recorded in `docs/PRIOR_ART.md`.

## M2.2 — Multiple pending calls

**Implemented.**

- multiple hall calls may coexist;
- one active trip must not be overwritten by a later call;
- idle eligible cars may take pending work;
- if no eligible idle car exists, the call stays pending;
- after service completion, pending work is reevaluated deterministically;
- keep dispatch policy intentionally simple before optimizing group control.

Implemented behavior:
- Pending calls are considered in deterministic `CallId` order;
- dispatch only selects Normal, capable cars without an active runtime target or an Assigned/Serving Core call;
- successful assignment immediately makes a car busy for later calls in the same scheduling pass;
- multiple calls coexist and excess work stays Pending without replacing either active target;
- availability changes trigger pending-work reevaluation;
- dispatch/no-eligible/intent trace events identify the relevant `CallId`;
- deterministic replay covers the `7F UP -> A`, `5F DOWN -> B`, `2F UP -> Pending` scenario.
- completion feedback releases the assignment and invokes the pending scheduler through M2.3.

## M2.3 — Runtime feedback drives lifecycle

**Implemented.**

Use semantic runtime feedback to advance call state rather than treating runtime events as display-only logs.

At minimum:
- accepted assignment becomes serving at an appropriate deterministic point;
- arrival/service completion marks the relevant call completed;
- replay verifies call lifecycle as well as physical car state.

Implemented behavior:
- `IntentAccepted(ServeFloor)` advances the matching Core call from Assigned to Serving;
- matching `Arrived(car, floor)` records Core-owned arrival evidence, including when door feedback arrives in a later event batch;
- `DoorStateChanged(Open)` completes only an arrived Serving call for that car;
- each completion immediately reevaluates Pending calls while still excluding cars whose Core calls remain Assigned/Serving;
- the three-call scenario deterministically completes #1, assigns #3 to newly released A, and later completes #3;
- replay verifies identical call identities, lifecycle events, assignments, Core state, and physical simulator state.

## M2.4 — Decision reasons / diagnostics

**Implemented.**

Evolve opaque `Option<Intent>`-style outcomes toward structured, explainable decisions without building a policy DSL.

Useful diagnostics include:
- selected car;
- eligibility/rejection reason;
- mode/capability constraints;
- deterministic distance/tie-break information;
- no-eligible-car reason.

Implemented behavior:
- Core returns a structured `DispatchDecision` rather than an opaque `Option<Intent>`;
- every car is reported in deterministic ID order with mode, semantic/runtime busy status, serve-floor capability availability, and distance;
- rejection reasons distinguish non-Normal mode, Busy, unavailable capabilities, and unsupported ServeFloor;
- successful selection identifies `OnlyEligible`, `Nearest`, or deterministic `CarIdTieBreak` basis;
- semantically occupied cars remain visible as rejected candidates instead of being filtered before evaluation;
- TUI trace and replay expose the structured candidate/selection results with their `CallId`;
- these explanations are intentionally distinct from equipment fault/alarm diagnostics, as recorded in `docs/PRIOR_ART.md`.

## M2.5 — Capability/configuration refinement

**Implemented.**

Only after the lifecycle path is working:
- reduce TUI-only hard-coded semantic assumptions where there is a demonstrated need;
- refine building topology/capability/configuration inputs;
- compare terminology and concepts with existing standards before freezing public semantics.

Any safety/authority-model change remains HUMAN-REVIEW.

Implemented behavior:
- model owns a canonical, sorted, duplicate-free, nonempty `BuildingTopology` of served floors;
- Core validates floor membership and terminal UP/DOWN availability from topology rather than TUI constants;
- TUI selection/rendering iterates configured floors, including non-contiguous floor sets;
- invalid call events distinguish unserved floors from unavailable terminal direction;
- reset and replay preserve and compare the same topology alongside semantic and physical state;
- runtime-provided per-car execution capabilities remain the dispatch input; no speculative building/door/capability DSL was introduced;
- the deliberate boundary from BACnet floor indexes and SAREF4LIFT service/space graphs is recorded in `docs/PRIOR_ART.md`.

## M2 completion review

**AUTO review complete.**

- Core owns call identity, lifecycle, arrival evidence, deterministic scheduling, and decision explanations;
- the TUI remains the only composition root and does not own semantic state;
- runtime feedback remains correct across event-batch boundaries rather than relying on simulator grouping;
- replay compares semantic events, Core state, topology, and physical simulator state;
- M2 added no crate, dependency, framework, public schema, or vendor branch;
- targeted prior-art findings and deliberate vocabulary/model divergences are recorded in `docs/PRIOR_ART.md`;
- formatting, all-target linting, 25 workspace tests, and PTY reference flows passed at the checkpoint.

**HUMAN_PENDING (non-blocking):** review M2 semantic architecture and terminology before any normative public schema is frozen. The implemented model remains internal and reversible, so independent internal bridge work may continue.

## M3.1 — Executable reference scenario and trace inspection

**Implemented.**

Implemented behavior:
- `cargo run -p liftassistant-tui -- --scenario m2-call-lifecycle` runs without a TTY through the existing App/Core/Runtime/Simulator composition;
- the built-in scenario submits `7F UP`, `5F DOWN`, and `2F UP`, then advances 13 explicit fixed steps;
- two fresh executions must match in semantic events, Core call state, topology, recorded commands, step count, and physical simulator state;
- the scenario also checks its expected outcome: all three calls completed, A open at 2F, and B open at 5F;
- output includes the complete semantic trace and final call/car state;
- unknown scenario names fail before terminal initialization;
- the representation remains internal Rust code: no scenario file format, public schema, new crate, or conformance/certification claim was introduced.

Validation: formatting, all-target linting, 27 workspace tests, and the real non-interactive scenario/help/list commands passed.

## M3.2 — Controlled simulator fault injection

**Implemented.**

Implemented behavior:
- `SimRuntime::reject_next_intent` deterministically rejects exactly one subsequent intent without changing car state or queuing acceptance feedback;
- the control is recorded/replayed as a simulator-only developer command and is not exposed as a Core semantic, equipment fault, alarm, or fault DSL;
- `cargo run -p liftassistant-tui -- --scenario intent-rejection-recovery` arms rejection, submits `7F UP`, observes the runtime error while call `#1` remains Pending, then retries the same active call identity and completes service;
- two executions must match, and the scenario checks exactly one injected error, duplicate reuse of `#1`, final completion, and A open at 7F;
- targeted BACnet, SAREF4LIFT, and Open-RMF review confirmed that this integration-path control must remain distinct from reported equipment fault semantics.

Validation: formatting, 29 workspace tests, both real non-interactive scenarios, and final all-target linting passed. The first all-target clippy attempt found one `useless_format` in the new CLI error path; it was corrected, and the subsequent all-target clippy run passed.

## M3 review checkpoint

**AUTO review complete.**

Review result:
- the scenario command was introduced only after the lifecycle path had a real non-interactive caller, and its two scenarios share execution/reporting without defining a file schema or new framework;
- each scenario checks two-run determinism and its own expected semantic/physical result, rather than treating identical wrong runs as success;
- simulator rejection is one-shot, state-preserving, recorded/replayable, and kept outside Core/runtime protocol semantics and equipment-fault vocabulary;
- Core, runtime contract, package/dependency boundaries, and interactive TUI behavior remain unchanged;
- generic multi-backend conformance machinery is deferred until a second backend exists;
- protocol mappings are deferred until a concrete recorded/read-only bridge supplies evidence;
- there is no conformance/certification claim and no HUMAN-REVIEW boundary was crossed.

## M4.1 — Native-semantic recorded/read-only runtime

**Implemented.**

Implemented behavior:
- `RecordedRuntime` is a second `ElevatorRuntime` implementation that plays in-memory native `RuntimeEvent` frames according to explicit elapsed durations;
- partial ticks preserve remaining frame time, and one tick may deterministically release every frame whose delay is covered;
- position/arrival/door observations update the backend's canonical observed car state;
- construction rejects events for unknown cars rather than silently manufacturing state;
- source-side observation coverage advertises position/door evidence, while the recorded feed exposes no `IntentSink` and supplies no execution capability to Engine;
- every submitted intent is rejected without changing state or disturbing later playback;
- `cargo run -p liftassistant-tui -- --scenario recorded-read-only-observation` runs the recording twice, checks identical batches/states, verifies the no-write boundary, and shows that frames release at 500 ms and 1000 ms but not at 250 ms or 999 ms;
- the recording is internal Rust data: no file schema, protocol mapping, network transport, hardware access, or write path was added.

Validation: formatting, final all-target linting, 34 workspace tests, and the real recorded/read-only command passed. The first all-target clippy attempt found the new playback loop could use `while let`; it was corrected before the final workspace gate.

## M4.2 — Recorded external-state mapping experiment

**Implemented with Open-RMF `LiftState`.**

Implemented behavior:
- the internal recorded input mirrors the relevant official Open-RMF `LiftState` fields without adding ROS/message dependencies;
- every string floor label requires explicit configuration, and reported current/destination floors must also appear in `available_floors`;
- lift name, stopped/open at-floor position, Fire/Offline mode, destination, available floors, source modes, and session identity are mapped/preserved;
- `DOOR_MOVING` is rejected because LiftAssistant distinguishes Opening from Closing;
- a closed door did not confirm the exact at-floor position required by the then-current `CarState`, so that complete-state mapping was rejected;
- `MOTION_UP`/`MOTION_DOWN` were rejected because a string `current_floor` did not provide the then-current model's continuous position; `MOTION_UNKNOWN` was also rejected;
- Human/AGV control modes are rejected rather than conflated with LiftAssistant operating modes; unsupported/unknown/emergency modes remain explicit errors;
- mapped observations feed the existing recorded/read-only runtime, which still rejects every intent;
- `cargo run -p liftassistant-tui -- --scenario rmf-recorded-read-only` verifies two identical mappings/playbacks, retained session context, final observed state, no-write behavior, and visible ambiguity errors;
- no live ROS/network connection, request publication, hardware I/O, public protocol schema, or write path was introduced.

Self-review evidence preserved: the first moving-state implementation mapped `MOTION_UP` plus `current_floor=L3` to an exact `3.0F` position. That fabricated precision was removed; moving states failed mapping until M4.9b added an explicit floor-level operational location. The review then applied the same rule to stopped/closed states, which do not confirm an exact continuous at-floor coordinate. A subsequent focused build also exposed and removed one unused import.

Validation: formatting, all-target linting, 39 workspace tests, and the real Open-RMF recorded/read-only command passed.

## M4 review checkpoint

**AUTO review complete — M4 recorded/read-only foundation implemented; broader M4 remains open.**

Review result:
- the native recorded runtime is a genuine second backend with explicit-time determinism, state validation, observation-only capabilities, and unconditional intent rejection;
- the Open-RMF experiment is adapter/runtime-side and does not leak source concepts into Core;
- no dependency, crate, file schema, live transport, middleware connection, protocol write, hardware access, or conformance claim was added;
- external `LiftState` evidence proves the current complete `CarState` cannot honestly represent common partial observations such as moving-with-floor-label-only, undirected door movement, or source-specific control modes;
- continuing to reject incomplete state is safe but makes this mapping foundation-only, not a generally usable Open-RMF backend;
- adding a partial/uncertain observation type would change the foundational model/runtime contract and must not be smuggled in as an adapter-local convenience;
- another recorded mapper would largely duplicate the same limitation without resolving that contract decision.

**RESOLVED BY HUMAN REVIEW (2026-11-16):** keep operational `CarState` complete and introduce a separate internal/reversible canonical partial-observation layer before operational-state resolution. Do not freeze this as a public schema yet. The approved implementation direction is specified in M4.3 below.

Environment evidence for later virtual CAN work:
- Linux has `ip`, but no vcan interface is present;
- `candump` and `cansend` are not installed;
- creating a kernel interface or installing system packages was not performed as part of this repository-only read-only scope;
- no controller capture, bench device, or documented controller API was supplied.

Remaining M4 work is therefore classified as:
- **foundation complete:** native recorded/read-only runtime and conservative recorded Open-RMF mapping;
- **resolved:** canonical partial-observation/runtime-boundary direction approved for internal/reversible M4.3 work;
- **external-input pending:** virtual CAN environment or recorded controller/protocol evidence;
- **not started:** live read-only transport, CiA 417/T/GSET mapping, HIL/bench integration, and any writes.

## Explicit non-goals in completed M2/M3 and current M4 work

- real passenger-elevator control;
- real CAN/CANopen/BACnet controller writes;
- complex group-control optimization;
- destination-control optimization;
- large policy/service-profile DSL;
- production web/mobile UI;
- Home Assistant integration;
- database/cloud infrastructure;
- plugin marketplace or plugin ABI;
- broad CI/coverage/fuzzing programs;
- certification/conformance claims;
- license selection without human review.

## Repository governance status

- `AGENTS.md` is the single authoritative agent guide.
- `agent.md` is a generic compatibility/navigation pointer only.
- `docs/PRIOR_ART.md` defines when external research is required and which established references to check.
- Project state is carried by repository documentation, not by vendor-specific agent files.
- Git commits should use the intended owner identity `Axiaobo7788`.


## M4 next plan — reviewed 2026-11-16

The current recorded/Open-RMF experiments have supplied enough evidence to move beyond the original all-in-one runtime contract.

### M4.3 — Canonical partial observation proof

**Implemented.**

Implemented behavior:

- complete operational `CarState` remains unchanged;
- the internal canonical `CarObservation` carries configured source identity, deterministic source sequence, car identity, and explicit position/motion/door/operating-mode evidence;
- floor-reference-only position, unknown motion, undirected door movement, unknown mode, and source-specific mode are represented without fabricating an exact position, door direction, or LiftAssistant operating mode;
- the deterministic resolver returns either a complete `CarState` or machine-readable unresolved fields; it does no history inference, probability, or source-specific reasoning;
- this slice initially promoted a stopped/open floor reference to an exact at-floor position; M4.9b later corrected that overstatement by preserving it as `CarLocation::Floor`;
- the Open-RMF mapper now produces canonical observations and separately retains raw current/destination labels, available/current source modes, and session identity;
- declared Open-RMF `MOTION_UNKNOWN` and Human/AGV/Emergency modes are valid partial evidence, while unrecognized enum values and invalid/unconfigured floor labels remain mapping errors;
- the real `rmf-recorded-read-only` scenario runs the mapper/resolver flow twice and checks the expected incomplete `L3 + UP + DOOR_MOVING` result, the resolved `L4 + STOPPED + OPEN` result, source-specific AGV-mode non-resolution, provenance/sequence, and the existing no-write boundary;
- the model remains internal/reversible: no generic quality/confidence/timestamp system, storage format, ROS dependency, live transport, or public schema was added.

Focused prior-art review rechecked the official Open-RMF `LiftState` definition and OPC UA `DataValue`. The implementation preserves the demonstrated source semantics and provenance distinction without copying OPC UA quality/timestamp machinery that has no current caller.

Validation: formatting, all-target linting, 43 workspace tests, and the real Open-RMF recorded/read-only scenario passed.

### M4.4 — Separate observation, intent, and simulation-driving contracts

**Implemented.**

Implemented behavior:

- the all-in-one `ElevatorRuntime` trait was replaced by separate read, intent, and driving responsibilities; the then-current resolved-state `ObservationSource` trait was later retired by M4.9d in favor of canonical update ingestion plus Engine-owned state;
- `IntentSink` accepts only LiftAssistant high-level `Intent`; no low-level motion, drive, door-force, or safety primitive was introduced;
- `SimulationDriver::advance(Duration)` owns explicit-time simulator/replay progression and is not required by observation sources or intent sinks in general;
- `SimRuntime` is an active deterministic simulator that emits canonical observation updates and implements intent plus driving behavior;
- `RecordedRuntime` emits canonical observation updates and implements replay driving only; the previous always-error `submit_intent` method and its `IntentSink` capability are gone;
- TUI composition imports only the contracts it actually uses, and recorded/Open-RMF scenarios exercise observation/replay without attempting a write;
- existing simulator replay, runtime feedback, timing, partial-observation resolution, and scenario behavior remain deterministic.

No async runtime, database, message bus, network protocol, IPC, plugin ABI, or new package was added.

Validation: formatting, all-target linting, 43 workspace tests, and all four executable developer scenarios passed.

### M4.5 — Extract the demonstrated Engine boundary

**Implemented.**

Implemented behavior:

- a focused `liftassistant-engine` crate now owns building topology, call registry/lifecycle, dispatch, pending scheduling, high-level intent submission/result handling, runtime-feedback transitions, and machine-readable `EngineEvent` output;
- Engine exposes canonical observation resolution through the existing deterministic resolver rather than making the TUI or source adapter decide operational truth; M4.9d later added Engine-owned assembly and operational state;
- Core remains deterministic and I/O-free; Engine depends on Core/model/runtime but not simulator, terminal UI, networking, storage, or wall-clock time;
- the TUI `App` retains only simulator instance/control, selection/pause/step state, bounded display trace, and replay input recording;
- interactive execution and non-interactive simulator scenarios now use the same Engine orchestration; the recorded Open-RMF scenario uses the same Engine resolution boundary;
- simulator-specific mode/rejection controls remain in the developer composition and do not leak into Engine events;
- one direct Engine test protects intent generation, Assigned/Serving/Completed lifecycle, and runtime-feedback processing, while all existing replay/scenario tests protect migration parity.

The sixth crate was justified by actual reuse across interactive, non-interactive, resolver, and subsequently demonstrated headless callers; no service framework or generic application platform was introduced.

Validation: formatting, final all-target linting, 44 workspace tests with the updated lockfile, and all four executable developer scenarios passed.

### M4.6 — Diagnostics substrate

**Implemented.**

Implemented behavior:

- Engine owns a count-bounded in-memory `FlightRecorder` over canonical observations and all machine-readable `EngineEvent` values, including calls, dispatch candidates/selections, generated intents, structured intent rejection, state transitions, and runtime feedback;
- diagnostic entries receive deterministic local sequence numbers and the recorder retains at most its configured live capacity;
- two deliberately narrow invariant detectors flag non-increasing observation sequence per source and runtime intent acceptance without a matching generated Engine intent;
- a rejected generated intent is removed from detector state, so a later retry/acceptance does not create a false unsolicited-acceptance anomaly;
- an anomaly freezes up to eight preceding/trigger entries plus two following entries in one of at most eight in-memory incident snapshots; trigger evidence is retained even when the live recorder capacity is smaller than the incident window;
- a frozen incident reruns the same deterministic detector, and the executable `diagnostic-incident-replay` scenario proves bounded live retention, complete after-window capture, matching replay anomaly, and two-run determinism;
- anomalies remain invariant evidence rather than equipment-fault, alarm, root-cause, or maintenance claims.

This slice adds no database, file format, cloud telemetry, ML, public diagnostic-code registry, or broad taxonomy.

Validation: formatting, all-target linting, 46 workspace tests, and all five executable developer scenarios passed.

### M4.7 — Headless/read-only production host proof

**Implemented.**

Implemented behavior:

- a focused `liftassistant-headless` binary is the first non-TUI Engine composition and depends only on Engine/model/runtime, not the TUI, simulator, transport, or an `IntentSink` implementation;
- its internal recorded feed exposes only canonical observations and has no submission/write method;
- lifecycle and readiness are distinct: the process starts `Starting/NotReady`, partial evidence remains `Synchronizing/NotReady`, sufficient complete evidence reaches `ReadyReadOnly/ReadOnly`, and a duplicate source sequence produces `Degraded/NotReady`;
- the host preserves the last resolved state for inspection after degradation but does not treat it as current readiness evidence;
- Engine observation resolution and structured diagnostics run in the headless process, and the sequence anomaly is the machine-readable degradation trigger rather than an equipment-fault or root-cause assertion;
- new incidents are detected by their monotonic diagnostic-entry sequence rather than retained incident count, so readiness remains revoked even after the bounded eight-incident retention queue starts replacing old incidents;
- the executable proof runs twice and checks identical lifecycle/readiness, state, diagnostic, and degradation results.

This is an internal/reversible host proof. It adds no async/service framework, IPC/network API, durable state, supervision, freshness/reconnect semantics, passenger-elevator write, deployment claim, certification claim, or stable public API.

Validation: formatting, all-target linting, 52 workspace tests with the updated lockfile, all five TUI scenarios, and the real headless lifecycle scenario passed.

### M4.8 — Second independent observation-source validation

**Implemented with recorded BACnet Lift-object properties.**

Official evidence: ASHRAE Addendum 135-2012aq defines the BACnet Lift object and the selected property semantics used by this experiment. It establishes required `Car_Position`, `Car_Moving_Direction`, `Car_Door_Status`, and `Out_Of_Service`; optional `Car_Mode` and `Next_Stopping_Floor`; universal-floor-number position; per-door arrays; and object decoupling when `Out_Of_Service=true`.

Implemented behavior:

- a focused internal typed projection represents only the decoded Lift properties used by the mapping experiment; it is not a BACnet object, packet, codec, transport, or public schema;
- explicit configuration maps BACnet universal floor numbers to LiftAssistant floors and derives the experiment topology; unconfigured positions/targets and missing door entries are structured errors;
- `Car_Position` remains `FloorReference` rather than fabricated continuous/exact position, and standard moving direction maps to known/unknown motion evidence;
- per-door `Car_Door_Status` is retained in source context; identical safely mapped statuses may aggregate, while disagreeing doors, `UNKNOWN`, and `LIMITED_OPENED` produce unknown aggregate door evidence;
- optional clearly equivalent modes map to existing internal modes, while absent/unknown and standard but non-equivalent modes remain unknown/source-specific instead of being guessed;
- BACnet object `Out_Of_Service=true` makes position, motion, door, mode, and target operational evidence unknown because the standard says the object is decoupled from the represented lift; it is deliberately not mapped to LiftAssistant `OperatingMode::OutOfService`;
- canonical `PositionEvidence::Unknown` and `DoorEvidence::Unknown` plus machine-readable resolver reasons were added to express these demonstrated gaps while keeping operational `CarState` complete;
- the executable `bacnet-recorded-read-only` scenario runs twice and proves partial movement, sufficient stopped/open resolution, object decoupling, mixed multi-door handling, provenance, source context, and the existing no-write runtime boundary.

This second independent source exercises different ambiguity than Open-RMF without freezing public semantics. No live BACnet device, discovery, network, packet fixture, property read/write, conformance claim, hardware path, or passenger-elevator authority was added.

Validation: formatting, all-target linting, 59 workspace tests, all six TUI scenarios, and the headless lifecycle scenario passed.

### Live read-only environment proof

**ACQUIRED — official Open-RMF Hotel simulation, 2026-11-17.**

- the official ROS 2 Kilted demo published `rmf_lift_msgs/msg/LiftState` on `/lift_states` for `Lift1` and `Lift2` through one source/publisher;
- unchanged complete snapshots arrived periodically at about 1 Hz per lift and interleaved at about 2 Hz overall;
- late subscription and observer restart recovered complete snapshots, and explicit source restart recovered both lifts;
- transport discovery alone did not prove payload delivery, and source process death did not immediately remove the DDS graph endpoint;
- `lift_time` reset with the simulation/source lifetime while RMF `session_id` remained empty, so neither becomes LiftAssistant `ObservationSessionId`;
- the test remained simulation-only and read-only: no LiftAssistant write path, request publication, real elevator, host ROS installation, or privileged interface was introduced.

This evidence removes the previous external-input gate for a bounded Open-RMF live adapter proof. It does not establish a production freshness timeout, deployment profile, write authority, or second live production source.

## Preserved production ideas

Longer-term production/debugging ideas are preserved in `docs/PRODUCTION_DESIGN_NOTES.md`. They are constraints/candidates, not a mandate to implement them all during M4.

The highest-priority future themes include observation quality/freshness, reconciliation, shadow debugging, incident capsules/replay, stable diagnostic codes, adapter validation profiles, sandboxing, semantic maturity/versioning, safe update/rollback, sanitized diagnostic bundles, and a community recorded-fixture corpus.


## M4 semantic-boundary audit — 2026-11-16

A cross-project/code audit after M4.8 found that the current read-only prototype is healthy enough to continue, but live adapters would expose four semantic ambiguities that should be corrected before M5 or any write-capable work.

### Finding A — reported destination is not the same as desired intent (resolved by M4.9a)

The pre-M4.9a `CarObservation.target: Option<Floor>` and `CarState.target: Option<Floor>` could not distinguish:

- source explicitly reports no destination;
- the source does not provide/know a destination;
- simulator/Engine has a desired target;
- an external controller reports a next stop/destination.

Core also treated `car.target.is_some()` as one busy signal. That was safe for the simulator but was not a reliable production interpretation of optional external state.

Open-RMF itself separates reported `LiftState.destination_floor` from requested `LiftRequest.destination_floor`; BACnet `Next_Stopping_Floor` is optional. Desired/reported state must therefore remain distinct.

### Finding B — one whole-car observation cannot represent live partial updates cleanly (resolved by M4.9c)

The pre-M4.9c model exposed only whole-car `CarObservation` snapshots. A live protocol may update only one property/frame at a time, so it could not distinguish:

- this update omitted a field / did not change it;
- the source explicitly reports that field as unknown.

This distinction matters for BACnet COV, CiA 417 PDOs, and other incremental streams. A later live source must not erase valid evidence merely because one message did not contain every field.

### Finding C — floor reference is not continuous position (resolved by M4.9b)

The pre-M4.9b resolver promoted `FloorReference(floor) + Stopped + DoorOpen` into `Position::at_floor(floor)`, which encoded an exact continuous coordinate.

The reviewed standards do not justify that precision in general: Open-RMF reports a floor label; BACnet `Car_Position` reports a universal floor number; CiA 417 defines a distinct car-position unit for measured physical position. Stopped/open may establish landing/floor presence, but not an exact continuous encoder coordinate.

### Finding D — Engine did not own operational state (resolved by M4.9d)

Before M4.9d, `Engine::resolve_observation` recorded and resolved evidence but did not retain the resulting operational state. Headless/TUI callers retained or queried state outside Engine, and dispatch read `car_states()` from the backend through the then-current `ObservationSource` trait.

M4.9d replaces that split path with canonical update ingestion, Engine-owned assembly/evidence/resolved state and capabilities, and Engine-owned dispatch/read views. The misleading resolved-state trait is removed; `IntentSink` remains independent.

## M4.9 — Observation semantics hardening

**IMPLEMENTED — M4.9a–M4.9d complete; HUMAN-CHECK accepted by the explicit M4.10 authorization on 2026-11-16.**

Work through these sub-slices in order. Keep all changes internal/reversible and run the prior-art gate before changing the model.

### M4.9a — Separate reported destination from desired/control state

**IMPLEMENTED.**

- canonical observation/state now carry tri-state `ReportedDestination::{Known, None, Unknown}`;
- Engine desired destinations remain in call/intent state, while the simulator keeps its execution target privately and reports destination separately;
- Core considers only explicit reported `None` eligible as idle evidence; reported `Unknown` remains conservatively busy, and Engine call occupancy remains independent;
- undocumented empty Open-RMF destination, absent BACnet `Next_Stopping_Floor`, and decoupled BACnet state map conservatively to `Unknown`; explicit `None` is used only where the source/runtime actually establishes no reported destination;
- regression coverage preserves all three reported states and proves that an Engine desire for 7F survives unknown reported next-stop state.

Validation: formatting, warning-free workspace clippy, all 62 workspace tests, all six TUI scenarios, and the headless read-only lifecycle scenario pass. No new blocker was found; the live read-only external-input blocker remains unchanged.

Do not design the future authority/lease model here.

### M4.9b — Correct location precision semantics

**IMPLEMENTED.**

- operational `CarState` now carries `CarLocation::Floor` or `CarLocation::Continuous` explicitly;
- any canonical floor reference can resolve as floor-level location when the other operational fields are sufficient, without fabricating a `floor_position`;
- simulator/native continuous observations retain `Position`, while arrival-only recorded evidence remains floor-level;
- dispatch uses discrete floor coordinates for floor-level location and the genuine coordinate for continuous location, preserving deterministic nearest-car behavior without a spatial graph.

Validation: formatting, all 63 workspace tests, focused Open-RMF/BACnet/recorded scenario inspection, and the M4.9a regressions pass. No new blocker was found; the live read-only external-input blocker remains unchanged.

### M4.9c — Add incremental observation assembly

**IMPLEMENTED.**

- `CarObservationUpdate` uses outer optional fields for omitted/not-updated values while existing evidence variants retain explicit unknown/unavailable semantics;
- `ObservationAssembler` merges updates deterministically per source and car without cross-source evidence mixing;
- each assembled field retains the provenance of the update that last supplied it, while a snapshot records the latest accepted update provenance;
- non-increasing sequence is rejected without mutating current evidence; M4.10b later scoped this source-global ordering to each explicit source session, and sequence remains ordering evidence rather than freshness;
- no persistence, clock/freshness policy, probability, history smoothing, or telemetry framework was added.

Validation: formatting, warning-free workspace clippy, all 66 workspace tests, all six TUI scenarios, and the headless read-only lifecycle scenario pass. Focused regressions cover omitted-vs-explicit-unknown merge, per-field provenance, rejection without mutation, and source isolation. No new blocker was found; the live read-only external-input blocker remains unchanged.

### M4.9d — Make Engine own resolved operational state

**IMPLEMENTED.**

- canonical `CarObservationUpdate` values enter Engine through one ingestion boundary;
- Engine owns incremental per-source/car evidence, resolved operational state, and capabilities used by dispatch and read-side clients;
- incomplete or unresolved evidence removes the affected resolved state instead of leaving a stale operational snapshot;
- dispatch reads only Engine-owned state/capabilities; its backend generic requires `IntentSink` alone and cannot provide state;
- the old resolved-state `ObservationSource` trait is removed, while `IntentSink` stays separate;
- simulator and recorded runtimes emit canonical updates; TUI, scenarios, and headless lifecycle read Engine state;
- arrival and door lifecycle events no longer bypass canonical observations to mutate the recorded operational view;
- runtime-event ordering is preserved: dispatch cannot see an adapter state update that appears later in the same event batch;
- diagnostics record canonical incremental updates, and deterministic replay plus the no-write read-only host remain intact.

Validation: formatting, warning-free workspace clippy, all 71 workspace tests, all six TUI scenarios, and the headless read-only lifecycle scenario pass. Open-RMF and BACnet both pass through the corrected Engine ingestion path. Focused regressions cover omitted-vs-explicit-unknown merge, stale-state invalidation, reported-vs-desired destination, floor-level-vs-continuous position, per-field provenance, source isolation, recorded-feed cross-source rejection, construction-time recorded-sequence rejection, lifecycle-event non-mutation, non-increasing sequence rejection, and event-order-visible dispatch. No new blocker was found; the live read-only external-input blocker remains unchanged.

## M4.10 — Source / Entity Binding & Observation Authority

**IMPLEMENTED — M4.10a–M4.10d complete; HUMAN-CHECK accepted on 2026-11-17.**

Observation authority in this milestone selects the configured observation stream that may become Engine operational truth. It does not grant command/write authority, change `IntentSink`, or establish failover.

### M4.10a — Explicit logical-car/source binding

**IMPLEMENTED.**

- Engine requires an explicit `(ObservationSourceId, CarId)` binding with `Operational` or `Auxiliary` role before accepting an observation;
- one source may bind multiple logical cars and one car may bind multiple sources, while at most one source may have the Operational role for a logical `CarId`;
- duplicate bindings and a second Operational source fail explicitly rather than selecting by registration or update order;
- `CarId` remains LiftAssistant logical car identity and `ObservationSourceId` remains configured feed identity; equal text does not imply a relationship;
- the Open-RMF mapper receives logical `CarId` explicitly and preserves native `lift_name` in source context. The executable fixture deliberately maps `rmf-lift-east` to logical car `lift-A`, proving name equality is not identity resolution;
- Simulator, recorded runtime, Open-RMF, BACnet, diagnostics, and headless composition roots install their observation bindings explicitly;
- this slice establishes the binding boundary only. Operational/Auxiliary state isolation, source-session lifecycle, and capability ownership remain the following M4.10 slices.

Validation: three focused binding regressions, focused Open-RMF mapper tests, all 74 workspace tests, all six TUI scenarios, and the headless read-only lifecycle scenario pass. M4.10a adds no public binding schema, configuration service, database, broker, transport, write authority, or failover policy.

### M4.10b — Explicit source sessions

**IMPLEMENTED.**

- canonical observation provenance includes an opaque `ObservationSessionId` separate from source identity and message sequence; the token has identity semantics only and no numeric ordering, freshness, or wall-clock meaning;
- Engine requires `begin_source_session(source, session)` before accepting bound observations and rejects updates when no session is active or when their session is not the active one;
- explicitly switching a source session removes that source's assembled evidence and resolved states and resets its sequence expectation; the transition reports the previous session and invalidated logical cars deterministically;
- an activated session becomes retired when replaced. Delayed updates from it are rejected, and a later lifecycle call cannot reactivate the retired token;
- ordering is source-global and session-local: `car A seq=1`, `car B seq=2`, `car A seq=3` is valid within one source session, and a new explicitly activated session may restart at `seq=1`;
- diagnostic sequence tracking uses `(source, session)`, so an explicit new session does not create a false non-increasing anomaly; observation and anomaly records now retain the session identity;
- Simulator, recorded runtime, Open-RMF, BACnet, diagnostics, and headless fixtures start deterministic internal sessions explicitly. Open-RMF `LiftState.session_id` remains source context and is not reused as the LiftAssistant adapter/session lifetime token;
- no sequence reset is interpreted as a restart, and no heartbeat, freshness, deadline, liveliness, quality, or automatic failover semantics were added.

Validation: focused session-restart and source-global ordering regressions, all 76 workspace tests, all six TUI scenarios, and the headless read-only lifecycle scenario pass. The restart regression proves `session A seq=100 -> explicit session B -> seq=1`, old evidence/state invalidation, delayed `session A seq=101` rejection, retired-session non-reactivation, and continued same-session non-increasing rejection.

### M4.10c — Operational vs Auxiliary state ownership

**IMPLEMENTED.**

- Engine now maintains a unique operational state map keyed only by logical `CarId`, plus a separate per-source state/evidence view for inspection;
- `car_states()`, Core dispatch, and readiness-relevant headless state consume only the configured Operational binding. Multiple sources reporting one car cannot create duplicate operational entries;
- bound Auxiliary observations still assemble, resolve, and enter the Flight Recorder with source, session, sequence, logical car, and binding role context, while remaining excluded from operational state and dispatch;
- complete but contradictory Auxiliary state cannot change dispatch selection, overwrite authority state, or become an automatic backup;
- when Operational evidence becomes incomplete or unresolved, its logical car leaves the operational view even if a complete Auxiliary state exists. No per-field or whole-state fusion occurs;
- restarting an Auxiliary source clears only its own evidence/source state and preserves healthy authority state; restarting an Operational source invalidates that car's operational state;
- headless lifecycle follows the same boundary: Operational session reset revokes `ReadyReadOnly` until the new session supplies complete state, while an Auxiliary sequence anomaly remains visible in diagnostics/events without revoking healthy operational readiness;
- no automatic promotion, failover, disagreement alarm, timestamp comparison, equipment-fault inference, or write authority was introduced.

Validation: focused uniqueness/dispatch isolation and no-fusion regressions, focused headless Operational-reset and Auxiliary-anomaly regressions, all 80 workspace tests, all six TUI scenarios, and the headless read-only lifecycle scenario pass.

### M4.10d — Capability ownership cleanup

**IMPLEMENTED.**

- the former mixed `Capabilities` type is replaced by Core-facing `ExecutionCapabilities`, containing only the current high-level `serve_floor` and `park` execution abilities;
- Engine owns execution capability by logical `CarId`, and Core dispatch accepts only that execution-capability map;
- source observation coverage no longer masquerades as execution capability. RecordedRuntime keeps its demonstrated position/door `ObservationCoverage` locally in the source/adapter layer and does not write false execution capability into Engine;
- Simulator exposes execution capability explicitly for composition, while the TUI reads the Engine-owned execution view;
- an Auxiliary read-only source cannot remove or disable the logical car's configured execution capability, and observation authority still grants no `IntentSink` or write authority;
- Simulator checks `ServeFloor` and `ParkAt` against their independent execution capability rather than reusing the serve-floor flag;
- no observation-coverage negotiation, write-authority model, principal, lease, quality score, or adapter capability framework was introduced.

Validation: the dispatch capability regressions, per-intent Simulator capability regression, recorded source-boundary regression, and authority/dispatch isolation regression pass with the split types; all 81 workspace tests pass.

## M4.10 completion review

**PASS — HUMAN-CHECK accepted on 2026-11-17.**

- `CarId` remains LiftAssistant logical car identity; source-native identities remain adapter context and require explicit mapping;
- every accepted observation requires explicit source/car binding and an explicitly active opaque source session;
- each logical car has at most one Operational observation authority and exactly one operational state entry when that authority has complete resolvable evidence;
- Auxiliary evidence remains inspectable and diagnostic but cannot enter dispatch truth, fill authority gaps, trigger failover, or revoke healthy authority readiness merely because it has an anomaly;
- source restart invalidates that source's evidence/state, resets session-local ordering, and cannot be inferred from sequence values or reversed by delayed retired-session data;
- execution capability is independent from observation coverage and authority; `IntentSink` remains unchanged and no write ownership was added;
- no field fusion, automatic source failover, freshness/liveliness deadline, cross-source disagreement alarm, public binding schema, configuration service, transport, or M5 implementation was added.

Final validation at this checkpoint: formatting, warning-free workspace clippy, all 81 workspace tests, all six TUI scenarios, and the headless read-only lifecycle scenario pass after the durable architecture/memory documentation update.

Accepted slice results:

- M4.10a explicit logical-car/source binding: PASS;
- M4.10b explicit source sessions: PASS;
- M4.10c Operational/Auxiliary authority isolation: PASS;
- M4.10d execution-capability ownership: PASS.

## M4 closeout and M5 entry

**M4 COMPLETE; M5 READY.**

All M4 exit gates are now satisfied:

1. M4.9 observation semantic hardening is complete;
2. M4.10 binding, session, authority isolation, and capability ownership are complete and human-accepted;
3. recorded Open-RMF and BACnet mappings still pass through the corrected canonical Engine path;
4. official Open-RMF live read-only evidence exists for the next bounded live slice;
5. live idle evidence corrected Core's overly broad destination-based Busy policy.

The Busy policy now treats a stopped car reporting its current floor as destination as idle unless Engine owns an Assigned/Serving call. A different known destination, motion, an unknown destination, or Engine call occupancy remains conservatively busy. The official `LiftState`/`LiftRequest` separation and live Hotel idle snapshots support this narrow correction without introducing an activity/occupancy framework.

Closeout validation: formatting, warning-free all-target workspace clippy, all 84 workspace tests, all six TUI scenarios, and the headless read-only lifecycle scenario pass. The recorded Open-RMF and BACnet mappings, M4.10 regressions, and live-evidence Busy regressions are included in that gate.

## M5 execution queue

### M5.1a — Rust-native ROS 2 transport compatibility proof

**IMPLEMENTED.**

The bounded proof succeeded without changing the workspace or host toolchain:

- `ros2-client` 0.8.2 declares Rust 1.82 and resolved to RustDDS 0.11.8, which declares Rust 1.73, preserving the workspace Rust 1.85 MSRV;
- an adapter-local `serde` wire type matching the official `rmf_lift_msgs/msg/LiftState` field order decoded real Kilted payloads without the experimental message generator;
- a BestEffort, Volatile, KeepLast reader received complete `Lift1` and `Lift2` snapshots from the official Hotel `/lift_states` publisher through host-side RustDDS;
- a Reliable reader was discovered by the ROS graph but received no payload during the bounded window, so discovery was again insufficient evidence and the proven live reader profile is BestEffort;
- no system ROS installation, write publisher, generic adapter framework, sidecar protocol, host middleware change, or project-wide MSRV bump was introduced.

The proof used the official rootless Kilted image at digest `sha256:601a6bb7c04d559fb9ec654d9b7a41650918990e35712036d836f072a6430db5`. Temporary proof source and raw output remain outside the repository.

### M5.1b — RMF-owned live subscription and canonical ingestion

**IMPLEMENTED.**

The M5.1a payload proof reproduced the official Hotel source's steady-state `current_mode=MODE_AGV`. Open-RMF defines `MODE_AGV` and `MODE_HUMAN` as RMF interaction/control modes, while the same field may also report exceptional lift states such as fire or offline. They therefore remain source-specific evidence by default and are not globally redefined as LiftAssistant operating modes.

The approved resolution keeps operational `CarState` completeness and the existing Resolver unchanged. An RMF adapter may use an explicit, per-configured-source mapping policy to promote `MODE_AGV` and/or `MODE_HUMAN` to `OperatingMode::Normal` only when that source/site configuration deliberately asserts that the RMF interaction mode represents ordinary LiftAssistant service availability for that integration. This is an adapter-side projection, not a universal equivalence.

Required boundaries:

- the default remains conservative: without an explicit source-local projection, `MODE_AGV`/`MODE_HUMAN` remain `OperatingModeEvidence::SourceSpecific` and the operational state stays unresolved;
- the policy is opt-in per configured RMF source, not a global default and not inferred from `available_modes`, source names, payload history, or convenience;
- raw `current_mode`, `available_modes`, and RMF session context remain preserved as source context even when a configured projection supplies canonical `OperatingMode::Normal` evidence;
- `MODE_FIRE` continues to map to `FireService`, `MODE_OFFLINE` to `OutOfService`, and `MODE_UNKNOWN` remains unknown; `MODE_EMERGENCY` remains source-specific/unresolved unless separately reviewed;
- this approval does not authorize arbitrary RMF-mode mappings to `Independent`, `Inspection`, or other canonical modes;
- the official Hotel simulation may explicitly opt in to `MODE_AGV -> Normal` for its live validation profile, but that test configuration must not become a production default;
- do not weaken Resolver completeness, bypass canonical `CarObservationUpdate`, or add a generic mode-mapping framework.

M5.1b implementation should introduce only the smallest RMF-owned configuration/type required by this policy, share the existing RMF semantic mapper between recorded and live paths, keep ROS/RMF dependencies out of model/Core/Engine, use explicit native `lift_name` to logical `CarId` mapping and existing binding/session semantics, and prove the configured/default mode paths with focused regressions.

#### M5.1b-1 — Explicit RMF interaction-mode projection

**IMPLEMENTED.**

- the RMF-specific mapper owns a deliberately narrow `RmfInteractionModeProjection` with independent explicit opt-ins for AGV-as-Normal and HUMAN-as-Normal;
- the existing `map_rmf_lift_state` entry point remains conservative, while the explicit projection entry point shares the same floor, motion, door, destination, exceptional-mode, and source-context mapping logic;
- default AGV/HUMAN evidence remains source-specific and unresolved, while an otherwise complete explicitly projected observation resolves with `OperatingMode::Normal`;
- FIRE, OFFLINE, UNKNOWN, and EMERGENCY retain their approved mappings regardless of interaction projection;
- raw current mode and available source modes remain preserved, and applying a projection for one configured source does not change another source's conservative policy.

Focused RMF regressions and the complete workspace gate pass: formatting, warning-free all-target clippy, and all 86 workspace tests. The slice changes no canonical type, Resolver completeness, source/session/authority semantics, or write boundary.

#### M5.1b-2 — RMF-owned live subscription and Engine ingestion

**IMPLEMENTED.**

Implemented behavior and evidence:

- a concrete `liftassistant-rmf` package owns the shared recorded/live semantic mapper, explicit native `lift_name` bindings, adapter session/source-global sequence, and Rust-native `/lift_states` subscriber; ROS 2/RustDDS dependencies do not enter model, Core, or Engine ownership;
- `Lift1 -> A` and `Lift2 -> B` are explicit configuration in the Hotel proof; an unconfigured native name is rejected even when its text matches a logical `CarId`;
- the adapter's opaque `ObservationSessionId` is independent of RMF control `session_id` and `lift_time`, and restarting the adapter resets its source-global sequence only after an explicit Engine session transition;
- Headless readiness requires complete Engine-owned operational state for every configured Operational car rather than becoming ready after the first car resolves;
- the official Hotel source produced two operational cars and `ReadyReadOnly`; an explicit session transition invalidated both old states, a delayed old-session update was rejected, and the restarted source restored both cars under a new session starting at sequence 1;
- an independent simulation-only request moved `Lift1` to `L2`; the subscription observed moving and undirected door-moving evidence followed by stopped/open resolved state at `L2` while preserving raw RMF mode context;
- `liftassistant-rmf` contains no publisher, `LiftRequest`, or `IntentSink`, so observation authority grants no write authority.

The validated transport profile remains BestEffort, Volatile, KeepLast. The live proof used the official rootless Kilted Hotel image at the M5.1a digest, then removed only its named test container and image. The official simulation did not exit within Podman's ten-second SIGTERM window during restarts/cleanup and required SIGKILL; this is preserved as external lifecycle evidence, not reclassified as an elevator fault or a LiftAssistant adapter failure.

Validation: `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 91 workspace tests, all six TUI scenarios, the headless recorded lifecycle scenario, dependency/source pollution checks, and the official live two-lift/restart/movement proof pass.

### M5.2 — Source Lifecycle / Freshness / Health

**IMPLEMENTED.**

Implemented behavior and evidence:

- Engine now accepts an explicit invalidation action scoped to the currently active source/session. It deterministically removes assembled evidence, per-source resolved state, and Operational state contributed by that source while retaining the active session and sequence watermark;
- a stale event carrying an old session token is rejected and cannot revoke a newer active session; duplicate/non-increasing same-session data remains rejected after invalidation, while a genuinely newer complete update may rebuild state;
- invalidation is recorded in the bounded Flight Recorder with source/session/car context but is not an anomaly, equipment fault, root cause, or maintenance action;
- the internal host tracks transport/discovery evidence, payload activity, source-session activity, per-car observation availability, and Engine readiness separately. Discovery/subscriber creation without payload cannot make the host ready;
- the RMF adapter owns an explicit nonzero maximum-payload-silence policy with no default. The timer runs outside Core and only arms after that subscription has received payload, so initial DDS discovery delay cannot be misclassified as stale data-plane activity;
- Operational staleness changes `ReadyReadOnly -> Synchronizing/NotReady` and removes current operational truth. Auxiliary staleness remains visible without changing healthy Operational readiness, promoting authority, fusing fields, or failing over;
- official Hotel validation used an explicitly configured 3000 ms source policy: one continuous subscriber established two-car readiness, source silence revoked both operational states, and source recovery restored both cars under the same adapter session with source-global sequences continuing from 63/64.

Negative evidence retained: the first live attempt armed the timer on creation of a second reader and falsely declared stale during DDS discovery despite a healthy publisher. The implementation now distinguishes pre-payload Awaiting from post-payload Stale and the corrected continuous-subscription proof passes. A cold official-simulation start also had no `/lift_states` publisher during its initial asset-download window and an adjacent fleet adapter failed independently; container/process existence was not accepted as payload readiness.

Validation: `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 98 workspace tests, all six TUI scenarios, the headless recorded lifecycle scenario, missing-policy rejection, Core wall-clock isolation, RMF write-boundary checks, and the official live stale/recovery proof pass. The named Podman container and image were removed after validation; no global prune was used.

### M5.3 — Production Read-only Host

**IMPLEMENTED.**

Implemented behavior and evidence:

- `liftassistant-headless --run-rmf-read-only` is a real long-running composition using the existing RMF subscriber/mapper, canonical update path, Engine state, health/readiness supervision, and stale invalidation;
- its internal CLI requires an explicit ROS domain, source identity, nonzero source-specific silence setting, at least one configured floor, explicit `native lift_name = logical CarId : Operational/Auxiliary` bindings, and optional source-local AGV/HUMAN projections. Missing freshness, duplicate native identity, and an Auxiliary-only host are rejected before transport starts;
- the host creates an opaque ingestion-lifetime source session from host process context. RMF `session_id`, `lift_time`, and observation sequence never become that session token;
- the process reports `Starting/NotReady`, `Synchronizing/NotReady`, and `ReadyReadOnly/ReadOnly` changes. On payload stale or transport/mapping failure it revokes current source truth before reconnecting or exiting;
- after stale invalidation it reconnects under the same adapter lifetime/session, keeps source-global sequence ordering, and requires sufficient new canonical evidence before readiness returns;
- SIGINT/SIGTERM use cooperative flags. Shutdown supervision is independent of freshness and is checked both on a short host-control interval and after every payload, so no-publisher startup and high-rate payload flow cannot prevent graceful exit;
- the composition contains no RMF publisher, `LiftRequest`, `IntentSink`, plugin/config framework, database, daemon protocol, or stable public API.

Live official Hotel validation with explicit `Lift1 -> A` and `Lift2 -> B` Operational bindings reached `Starting -> Synchronizing -> ReadyReadOnly`, then handled SIGTERM with a successful graceful exit. A second run on a domain with no publisher also exited successfully from `Starting/NotReady` before any payload, proving shutdown is not dependent on discovery or data flow.

Validation: `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 100 workspace tests, all six TUI scenarios, the headless lifecycle scenario, invalid production-config rejection, dependency/write-boundary checks, the official live production composition, and the no-publisher shutdown path pass. The named Podman container and image were removed; no global prune was used.

### M5.4 — Production Diagnostics

**IMPLEMENTED.**

Implemented behavior and evidence:

- the headless composition now owns a fixed-capacity host diagnostic recorder with monotonic internal sequence numbers; eviction never reuses a sequence;
- private machine-readable identities and explicit layers distinguish Transport subscriber/discovery/failure evidence, Source session/stale evidence, Mapping rejection, Observation incomplete/unresolved/resolved/rejected evidence, and Engine readiness transitions;
- applicable entries preserve `ObservationSourceId`, `ObservationSessionId`, logical `CarId`, Operational/Auxiliary role, and source-native entity context. Auxiliary acceptance/rejection remains visible while a healthy Operational authority stays ready;
- the production RMF host emits the retained typed entries as line-oriented operator/developer diagnostics while the existing Engine Flight Recorder continues to retain canonical observations, Engine events, invalidations, and invariant anomalies;
- connect/receive errors, unconfigured or invalid RMF mapping, topology mismatch, payload stale, resolution loss, readiness change, and recovery are no longer collapsed into one health boolean or one generic fault. None is labelled an elevator fault, root cause, maintenance action, or public/stable diagnostic registry;
- runtime validation exposed that DDS port arithmetic panics for domain 233. Targeted official ROS 2 review confirmed the possible DDS-backed range is 0 through 232; both CLI parsing and the RMF transport boundary now reject larger values before RustDDS construction. The original panic is retained as negative evidence rather than erased by the corrected run;
- a no-publisher production run on domain 231 emitted distinct `readiness.changed`, `source.session-activated`, and `transport.subscriber-created` records, then exited cleanly on SIGINT. Focused tests exercise all five diagnostic layers, source/session/car/role/native context, bounded retention, Engine evidence coexistence, and Auxiliary isolation.

Validation: `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 103 workspace tests, all six TUI scenarios, the headless lifecycle scenario, no-publisher typed-diagnostic/SIGINT runtime, invalid-domain regression, canonical dependency/wall-clock isolation, and read-only write-boundary checks pass. No Podman resources were created for this slice, and the prior RMF validation resources remain absent.

### M5.5 — Incident Capsule / Deterministic Replay

**IMPLEMENTED.**

Implemented behavior and evidence:

- the existing headless host now records an independent fixed-capacity window of explicit observation bindings and ordered `begin source session`, canonical observation update, and explicit source-invalidation actions; when long-running ingestion evicts actions from that bounded window, pre-window active-session, assembled per-car evidence, and invalidation sequence-watermark state are emitted as explicit preamble actions within the capsule action limit rather than inferring lifecycle from sequence values or recording wall-clock delays;
- `liftassistant.incident-capsule/1` is a private serde/JSON wire layer owned only by headless. It converts current canonical values without adding serde/wire dependencies to model, Core, Engine, or RMF semantics;
- each capsule contains a producer/version, served-floor topology, explicit source/car/role bindings, bounded ordered actions, a typed deterministic trigger, and expected lifecycle/readiness. File input is capped at 1 MiB and internal collection counts are bounded;
- capsule validation rejects unknown versions, duplicate topology/binding data, conflicting operational authorities, unbound actions, invalid active-session invalidation, and any observation window that lacks a preceding explicit source-session action. Replay never guesses a restart or activates a session from payload sequence/timestamps;
- replay creates a fresh `ReadOnlyHost`/Engine, rebuilds bindings, explicitly performs recorded session transitions, replays observations and invalidations, then requires the typed trigger and final lifecycle/readiness to match;
- the executable `headless-incident-capsule-replay` proof covers initial readiness, payload-stale invalidation, same-session recovery, explicit session B reset, incomplete resolution, readiness recovery, and a final duplicate-sequence anomaly. Two captures produce identical JSON; a separate process reads the temporary file and reproduces `observation.sequence-not-increasing`; the file is then removed;
- production RMF configuration may opt into one create-new `--incident-capsule-out <path>`. The first payload-stale trigger freezes a replayable capsule; existing files are never overwritten and later stale cycles do not produce an unbounded archive;
- the format remains internal/versioned/experimental. There is no database, background uploader, public schema, coredump claim, root-cause inference, equipment-fault claim, or write path.

Validation: `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 106 workspace tests, all six TUI scenarios, the headless lifecycle scenario, the fresh-process capsule scenario, capsule round-trip/version/stale-capture regressions, dependency/wall-clock/write-boundary checks, temporary-artifact cleanup, and Podman-absence checks pass.

### M5.6 — Sanitization / Community Fixture Path

**IMPLEMENTED.**

Implemented behavior and evidence:

- `--sanitize-incident-capsule <private> <candidate>` reads but never mutates the private capsule and writes a distinct create-new candidate only after both private and candidate replay pass;
- the private `deterministic-pseudonymization/1` profile maps sorted source identities to stable `source-NNN`, preserves per-source session lifetimes through stable `session-NNN`, maps shared logical cars consistently to `car-NNN`, and remaps sorted topology floors to `1..N`;
- floor references and reported destinations use the same floor map. Exact continuous positions preserve their offset from the remapped nearest floor instead of retaining the original site floor coordinate;
- binding roles, source-to-car relationships, one source's session structure, source-global sequences, omitted-vs-unknown evidence, action ordering, invalidations, trigger kind, and expected lifecycle/readiness remain intact. Candidate replay must reproduce the same deterministic trigger;
- every candidate embeds `profile=deterministic-pseudonymization/1` and `manual_review_required=true`. Already-sanitized candidates are rejected from silent re-sanitization;
- the executable `headless-sanitized-capsule-replay` proof performs sanitization in a separate process, verifies that private identifiers are absent from the candidate, checks the source capsule remains byte-identical, replays the written candidate, reports the manual-review gate, and removes both temporary artifacts;
- no artifact is uploaded, published, added to a fixture corpus, or called perfectly/security-certified anonymized. The candidate remains internal/experimental until a human reviews it.

Validation: `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 107 workspace tests, all six TUI scenarios, all three non-live headless scenarios, dependency/wall-clock/write-boundary checks, private/candidate temporary-artifact cleanup, and Podman-absence checks pass.

### M5.7 — Adapter Validation Profiles

**IMPLEMENTED.**

Implemented behavior and evidence:

- the developer-only `adapter-validation-profiles` executable scenario runs the same fixed checklist against the concrete Open-RMF observation adapter and the existing recorded BACnet Lift mapper; it introduces no adapter trait, registry, public schema, plugin boundary, or canonical domain type;
- both profiles verify explicit native-to-logical identity, mandatory Engine binding, explicit source-session activation, source-global/session-local sequence, ambiguity as present canonical evidence rather than omission, restart invalidation and old-session rejection, unique Engine-owned operational state, and an observation-only path with no write action;
- the RMF profile proves one source maps `Lift1` and `Lift2` to distinct logical cars, rejects logical-CarId text as an unconfigured native lift, separates RMF `session_id` from the adapter SourceSession, preserves door-moving/empty-destination/source-specific-mode ambiguity, and resets its sequence only under an explicit new adapter session;
- the BACnet profile proves `Object_Name` remains source context while the caller supplies the logical CarId, rejects unconfigured universal floors, preserves multi-door disagreement as `DoorEvidence::Unknown`, and exercises the same Engine binding/session/restart path across two logical cars;
- evidence scope remains explicit in machine-executable output: RMF is backed by the official live Hotel proof plus recorded regression, while BACnet is recorded mapping only and has no supplied live production endpoint;
- two runs produce an identical report. The output says these are internal engineering checks and not certification; no family is called certified, conformant, safety-validated, or live when that evidence does not exist.

Validation: focused warning-free TUI clippy, all 22 TUI tests, deterministic execution of the new profile scenario, warning-free workspace all-target clippy, all 109 then-current workspace tests, all seven TUI scenarios, all three non-live headless scenarios, dependency/write-boundary checks, diff/whitespace review, and Podman-absence checks pass.

### M5.8 — Second Production Source Family

**COMPLETE — HUMAN ACCEPTED AT THE ARCHITECTURE-VALIDATION BOUNDARY.**

Implemented behavior and evidence:

- a headless test-only dependency reuses the existing recorded BACnet Lift mapper; the normal production dependency tree remains Engine/model/RMF/runtime plus host support and still contains neither `liftassistant-sim` nor a BACnet stack;
- real mapped BACnet canonical evidence now drives the protocol-neutral `ReadOnlyHost` from explicit binding/session through `ReadyReadOnly`, explicit payload-stale invalidation, `Synchronizing/NotReady`, newer same-session recovery, ambiguous multi-door resolution loss, explicit source-session restart, sequence restart, and delayed old-session rejection;
- an actual unconfigured universal-floor mapping error enters the Mapping diagnostic layer with source/session/native-object context. Resolved, unresolved, stale, readiness, and inactive-session events retain the corresponding generic source/session/car/role context without becoming elevator faults;
- a BACnet-origin stale incident capsule replays in a fresh host, passes the same deterministic pseudonymization/manual-review path, removes the private source/session/car identifiers, and reproduces the same trigger after sanitization;
- source-specific BACnet semantics remain outside Engine: `Object_Name` stays native context, universal floors require configured mapping, and disagreeing car-door entries remain `DoorEvidence::Unknown` rather than being fused or guessed;
- M5.7's executable checklist and this host-level check therefore cover adapter mapping, canonical ownership, runtime health, diagnostics, incident replay, and sanitization with a second independent family. They do not implement or claim BACnet transport, COV/liveliness semantics, discovery, live freshness timing, controller compatibility, writes, conformance, certification, or second live production-source validation.

Validation: the focused recorded-BACnet headless test, `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 110 workspace tests, all seven TUI scenarios, all three non-live headless scenarios, normal-dependency-tree isolation, canonical dependency/wall-clock/write-boundary checks, diff/whitespace review, temporary-artifact cleanup, and empty Podman container/image inventories pass.

External validation pending: no live BACnet Lift endpoint, sanitized real BACnet capture, documented controller API, CiA 417/SocketCAN evidence, or other supplied second-family production input is available. Inventing packets, scanning networks, installing privileged interfaces, or promoting recorded evidence to a live claim remains forbidden. HUMAN accepted that this external live-validation item does not block M5.9; no BACnet-live, production-ready, or multi-protocol-production claim is implied.

### M5.9a — Pure Experimental Read-model Projection

**IMPLEMENTED.**

Implemented behavior and evidence:

- an API-boundary-owned `ExperimentalReadModel` projects existing `ReadOnlyHost`/Engine facts without deriving serde on canonical model, Core, Engine, runtime, or adapter types and without serializing `HostEvent`, `EngineEvent`, ingestion errors, or Rust `Debug` output;
- JSON carries `api_version=experimental/v0`, host-local monotonic revision, lifecycle/readiness, sorted operational cars, source/session/transport/payload/availability state, binding roles, and the already bounded recent host diagnostic identities/context;
- tagged wire representations preserve floor-reference versus continuous location and Known/None/Unknown reported destinations. Motion direction, door state, operating mode, source-session activity, binding role, and observation availability remain explicit rather than inferred or collapsed;
- revision begins at 1 for each new Host process, advances with projected host changes, is independent of an observation sequence fixture of 99, and has no persistence, database, session, or canonical-model meaning;
- the executable `headless-experimental-read-model` scenario emits and inspects one complete JSON projection without opening a socket. No score, confidence, estimate, universal health value, generic message bus, client SDK, or stable/public API claim was added.

Validation: focused projection tests, warning-free headless all-target clippy, `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 113 workspace tests, the pure projection executable scenario, canonical serde/dependency checks, and diff/whitespace review pass.

### M5.9b — Experimental HTTP Snapshot

**IMPLEMENTED.**

Implemented behavior and evidence:

- axum 0.8 and Tokio 1 are dependencies only of `liftassistant-headless`; the workspace Rust 1.85 resolver selected compatible axum 0.8.9/Tokio 1.54.1 without changing MSRV or rewriting the synchronous RMF host;
- `GET /api/experimental/v0/snapshot` returns one complete `ExperimentalReadModel` as JSON with `Cache-Control: no-store`; no canonical/internal structure is serialized directly;
- the router defines only a GET route. POST is rejected with HTTP 405, unknown paths remain absent, and the response contains no wildcard or implicit CORS header;
- the server is disabled unless the explicit developer fixture command is invoked. It accepts an explicit socket address, uses no implicit `0.0.0.0`, has no authentication/TLS/RBAC fiction, and exits cooperatively on SIGINT/SIGTERM;
- a real child server process listened on `127.0.0.1`, an independent `curl` process fetched and parsed the response, verified version/readiness/revision/car state and `no-store`, observed POST 405 and no CORS, then terminated the server cleanly;
- a real-loopback focused regression also exercises the listener and route without handler mocking. Client or HTTP failure cannot mutate Engine truth because the boundary owns only an immutable projected snapshot.

Validation: real-listener focused test, process-level server plus independent `curl` JSON proof, `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 114 workspace tests, explicit read-only route and dependency/MSRV isolation checks, and diff/whitespace review pass.

### M5.9c — SSE Current-state Stream

**IMPLEMENTED.**

Implemented behavior and evidence:

- `GET /api/experimental/v0/events` returns `text/event-stream`; every `snapshot` event contains one complete `ExperimentalReadModel`, beginning promptly with the current value and continuing when host-local read-model revision advances;
- a Tokio watch channel provides one bounded latest value rather than a per-client history queue. Slow consumers may coalesce intermediate revisions and recover from the newest complete state; the stream makes no every-event, durable-cursor, backlog, replay, or exactly-once promise;
- reconnecting starts a new subscription and immediately receives the then-current complete snapshot. `Last-Event-ID`, cursor persistence, and resume-from-revision behavior were deliberately not added;
- transport keepalive exists only to maintain the HTTP connection. Client presence, disconnection, backpressure, and keepalive do not enter source freshness, Engine readiness, diagnostics, Flight Recorder, or Incident Capsule behavior;
- focused real-listener tests prove initial delivery, change push without client polling, a burst of coalesced revisions, latest-value observation by a deliberately slow receiver, and reconnect recovery. A separate child-server plus independent `curl` proof observed an initial snapshot, a pushed newer revision, and the same latest revision after reconnect;
- HTTP/Tokio dependencies remain confined to `liftassistant-headless`, no write route or CORS policy was added, and the developer fixture remains disabled unless invoked explicitly.

Validation: focused SSE and snapshot listener tests, process-level child server plus independent `curl` streaming/reconnect proof, `cargo fmt --all -- --check`, and warning-free headless all-target clippy pass.

### M5.9d — Production Host Integration

**IMPLEMENTED.**

Implemented behavior and evidence:

- the real long-running RMF read-only composition accepts optional `--api-listen <ip:port>`; omission leaves the API fully disabled, and invalid/duplicate addresses fail configuration explicitly;
- an explicit non-loopback bind is allowed but emits a clear `experimental unauthenticated read-only API` warning. No implicit `0.0.0.0`, authentication/TLS/RBAC fiction, wildcard CORS, or write endpoint was added;
- HTTP serving runs in its own headless-boundary thread and consumes only complete projected models. Binding failure fails the explicitly requested startup, while later server/client failure can only emit an API warning and cannot mutate Engine, source health, operational truth, or readiness;
- every real host mutation path publishes the latest projection: subscriber creation, discovery/mapping outcome, canonical ingestion, transport failure, and stale invalidation. Unchanged revisions are ignored and the SSE boundary retains only the newest complete value;
- the server shares cooperative SIGINT/SIGTERM shutdown without converting the existing RMF composition to Tokio. HTTP dependencies remain absent from model, Core, Engine, runtime, and RMF adapter crates;
- a real production-host process with no RMF publisher exposed the explicitly configured loopback API, served `Synchronizing/NotReady` snapshot and SSE state from the actual Host, retained no CORS header, then exited successfully on SIGTERM with its API listener gone. The API-disabled default is also covered by configuration regression.

Validation: production configuration tests, warning-free focused headless all-target clippy, real production Host plus independent `curl` process proof, cooperative shutdown/resource cleanup checks, and diff/whitespace review pass.

### M5.9 — Experimental Read-only API

**IMPLEMENTED — M5.9a through M5.9d complete.**

The bounded API exits are satisfied: snapshot and SSE work across real process boundaries, state changes are pushed without client polling, complete latest-value snapshots permit reconnect recovery without unbounded history, serving is disabled by default, client/server concerns cannot alter Engine truth, and no write/CORS/canonical/protocol-specific/stable-public boundary was introduced.

### M5.10 — Independent External CLI Client

**IMPLEMENTED — MANDATORY HUMAN-CHECK BOUNDARY REACHED.**

Implemented behavior and evidence:

- new `liftassistant-cli` is a separate executable crate with direct dependencies only on a blocking HTTP client and serde/JSON. It has no dependency on LiftAssistant model, Core, Engine, runtime, RMF, simulator, TUI, or headless crates and cannot instantiate or read Host memory;
- `snapshot <http://host:port>` requests and validates one `experimental/v0` JSON snapshot. `watch <http://host:port>` continuously consumes complete SSE `snapshot` events, with an optional positive `--max-snapshots` bound for deterministic automation;
- its deliberately local parser and text renderer display revision, lifecycle/readiness, operational cars, source/session/transport/payload/observation summary, and recent diagnostic identity/context. Unsupported API versions, non-HTTP or path-bearing base URLs, HTTP failures, wrong content types, invalid JSON, and premature stream end fail explicitly;
- no SDK crate, schema generation, OpenAPI client, generic client framework, FRB, WebUI, Flutter assumption, write path, reconnect backlog, or stable/public compatibility claim was introduced;
- process proof 1 ran the real long-running RMF Host on an otherwise idle ROS domain and an independent CLI process fetched its actual `Synchronizing/NotReady` state plus active source session, then the Host shut down cooperatively;
- process proof 2 ran the independent headless API fixture and independent CLI `watch` process; the client received and rendered the initial complete car state plus a pushed newer diagnostic revision without polling. All proof processes, listeners, and temporary artifacts were cleaned.

Validation: three focused CLI parser/renderer/SSE tests, `cargo fmt --all -- --check`, warning-free workspace all-target clippy, all 119 workspace tests, all seven TUI scenarios, all four non-live headless scenarios, process-level production Host plus independent CLI snapshot proof, process-level Host fixture plus independent CLI multi-snapshot SSE proof, dependency-isolation inspection, and cleanup checks pass. Cargo selected reqwest 0.12.28 and its transitive graph at the highest versions compatible with the workspace Rust 1.85 policy; the workspace MSRV was not changed.

### M5.11 — LiftAssistant Client

**IMPLEMENTED — MANDATORY HUMAN-CHECK BOUNDARY REACHED.**

Implemented behavior and evidence:

- `client/` (`liftassistant_client`) is an independent Flutter graphical client targeting Android, Linux, and Web with zero Cargo/FFI/crate dependency on LiftAssistant model, Core, Engine, runtime, RMF, simulator, TUI, or headless internals;
- clean four-layer client architecture (`HostTransport` -> `ExperimentalV0Decoder` -> `ClientController`/`ClientState` -> adaptive Flutter UI) shares API models, strict validation, SSE framing, synchronization/reconnect state, and UI across all three platforms, using conditional imports only where native `dart:io` `HttpClient` streaming differs from browser `dart:js_interop` `fetch` + `ReadableStreamDefaultReader` + `AbortController`;
- synchronization is strictly complete-snapshot centric (`GET /api/experimental/v0/snapshot` followed by `GET /api/experimental/v0/events` SSE `event: snapshot` replacement) with bounded exponential-backoff reconnect across both snapshot and SSE-attach failures, immediate manual reconnect, inactivity watchdog over SSE keepalives, non-following HTTP 3xx redirect enforcement, bounded snapshot body and SSE event payload reads, non-live marking of retained snapshots during disconnection, clearing of prior Host state when switching endpoints, acceptance of smaller revisions across Host restarts, and explicit `UnsupportedApiVersionException` / `MalformedReadModelException` reporting (even alongside retained snapshots) without field guessing;
- UI preserves LiftAssistant semantic boundaries without inventing backend truth: discrete `FloorLocation` versus genuine `ContinuousLocation`, `ReportedDestination` (`Known`/`None`/`Unknown`) as external source evidence rather than desired target or call assignment, `Operational` versus `Auxiliary` binding authority (with explicit non-failover semantics), six distinct health dimensions (client link, Host lifecycle, Host readiness, source session, transport discovery, payload activity, and per-car observation availability), and recent diagnostics as machine-readable Host evidence rather than equipment-fault or repair assertions;
- strictly read-only: exposes zero call, target, park, door, mode, or protocol write controls or endpoints;
- real persistent SSE client integration exposed a latent bug in `liftassistant-headless` (`crates/liftassistant-headless/src/http_api.rs`): `--serve-experimental-snapshot-fixture` dropped its last `watch::Sender` after its 300 ms update (prematurely closing SSE streams), while cooperative shutdown waited indefinitely on active SSE connections until clients closed sockets. Fixed in a standalone commit by keeping a shutdown-bounded relay watch channel inside `http_api::serve`, covered by a focused Rust regression test;
- **CLIENT API PRESSURE / Architecture Pressure recorded (no speculative backend rewrite performed):**
  1. *Browser Same-Origin vs unauthenticated no-wildcard-CORS Host API*: `liftassistant-headless` rightly omits wildcard `Access-Control-Allow-Origin` on unauthenticated plain HTTP. Direct cross-origin browser requests are therefore blocked by browser Same-Origin Policy; Web deployments currently require same-origin hosting/reverse proxying (verified in Chrome) until explicit origin configuration and/or authenticated TLS deployment semantics are reviewed.
  2. *Plain-HTTP cleartext boundary on mobile OS policies*: Android API 28+ blocks cleartext HTTP by default; `AndroidManifest.xml` explicitly enables `usesCleartextTraffic="true"` for the M5.11 local/LAN `experimental/v0` scope without claiming TLS or authentication.
  3. *Projected topology / unresolved reason detail*: when `operational_cars` is empty (for example `Starting`/`Synchronizing`), `experimental/v0` intentionally projects per-car availability (`awaiting`/`incomplete`/`unresolved`/`revoked`) and diagnostic IDs (`observation.unresolved`, `mapping.rejected`) without served-floor topology metadata or human-formatted diagnostic prose; M5.11 needs neither to satisfy read-only operational visibility, so no additive DTO expansion was introduced.

Platform build and runtime validation against real `liftassistant-headless`:

- **Linux desktop (`VALIDATED — BUILD + REAL HOST RUNTIME`)**: `flutter build linux --release` succeeded; native GTK release binary launched before Host startup (survived offline), auto-connected to `--serve-experimental-snapshot-fixture`, received live pushed SSE revision, survived Host `SIGTERM`, and auto-reconnected across Host restart/revision reset to `--run-rmf-read-only --api-listen ...`.
- **Web (`VALIDATED — JS & WASM BUILDS + REAL CHROME + REAL HOST RUNTIME`)**: both `flutter build web --no-web-resources-cdn` and `flutter build web --wasm --no-web-resources-cdn` succeeded; real `/usr/bin/google-chrome-stable` booted both JS and `skwasm`/`main.dart.wasm` bundles via a same-origin static/API reverse proxy, rejected 302 redirects without contacting redirect targets, survived initial 502 while Host was offline, auto-connected and held a steady `fetch` `ReadableStream` SSE connection without polling, survived Host stop, and reconnected across Host restart/revision reset to `--run-rmf-read-only`.
- **Android (`VALIDATED — DEBUG/RELEASE APK BUILD + REAL ANDROID EMULATOR + REAL HOST RUNTIME`)**: `flutter build apk --debug` and `flutter build apk --release` succeeded; booted `Medium_Phone_API_36.1` (Android API 36.1), installed APK via `adb`, verified cleartext HTTP routing (`http://10.0.2.2:<port>` and `adb reverse` loopback) while Host was offline, live SSE snapshot push from `--serve-experimental-snapshot-fixture`, process survival across Host stop, and automatic reconnect across Host restart/revision reset to `--run-rmf-read-only`.

Validation: `cargo fmt --all -- --check`, warning-free workspace all-target `cargo clippy`, all 126 Rust workspace tests, `dart analyze --fatal-infos` (zero issues), all 28 Flutter VM tests plus 21 Chrome-platform tests, and real process-level runtime proofs on Linux, Web (Chrome JS & Wasm), and Android Emulator pass. All test processes, listeners, temporary Chrome profiles, and Android emulator instances were cleaned up.

The M5.11 first-version checkpoint was accepted for backend progression on 2026-11-18. Its three-platform evidence applies to the validated client, Host and test environment.

### M5.12 — HA/BMS Observation Integration

**DEFERRED / CONDITIONAL.** Outside the M5 completion gate.

### M5.13 — Failure / Soak Matrix

**FOUNDATION CORRECTIONS IN PROGRESS.** Human approval assigns fresh incarnation tokens to the ingestion-lifecycle owner. M5.14 opens after A–F and the matrix rerun.

- **Rust 1.85.0:** The actual compiler initially rejected let chains in the existing workspace. Equivalent 1.85-compatible conditions restore a passing local `cargo check --workspace --all-targets --locked`. The CI workflow now runs that command with Rust 1.85.0 alongside its existing stable job.
- **Source/session and data plane:** The synthetic matrix passed 32 rounds each at 4 cars / 2 sources, 32 / 8 and 128 / 16. Interleaved cars preserve source-global order across same-session staleness and sequence restart across explicit session changes. Late retired observations leave current evidence unchanged. Existing Host policy records Operational late samples as degradation until a valid current-session update. Conflicting healthy Auxiliary evidence neither fills missing Operational door evidence nor keeps a stale Operational source ready. Transport failure is diagnostic evidence; explicit invalidation revokes operational state while preserving the active session and discovery evidence. The compacted stale capsule replayed at each scale.
- **Retention finding:** Session churn used to grow the Flight Recorder's sequence detector map despite eviction from the recorder. The detector now drops each retired stream's watermark on session replacement. A 1,024-session regression retains one active detector watermark, 128 Flight Recorder entries and eight incident snapshots. Engine's separate retired-identity set still grew to 1,023 entries: it implements permanent rejection of reactivating an arbitrary opaque session ID. Exact indefinite rejection needs growing history under the current contract. The RMF Host uses one adapter session per process and reconnects under that session, but unconstrained explicit rollover requires a HUMAN retention/lifecycle decision before this resource gate can pass.
- **Correction A.** The Host now obtains 128 random bits from the system RNG for each ingestion lifetime. Engine fences on the current token and retains one active slot per source. The 2,048-rollover regression verifies active state and detector cardinality and fences delayed samples. The bounded Capsule validation rejects reused begin tokens.
- **Corrections B–D.** RMF supervises arrival deadlines per configured car on payloads and idle ticks, alongside feed-wide silence. A bound car can lose evidence while its sibling and source session stay active. Newer samples rebuild under one source-global watermark. Unconfigured lifts are diagnosed and ignored, while mapping and floor membership errors revoke only the mapped car. Different in-building floor subsets pass. The bounded Capsule records, compacts, replays and sanitizes single-car invalidations without reviving stale evidence or resetting ordering.
- **API shutdown:** A loopback SSE peer that stopped reading with a pending socket write reproduced an indefinite graceful drain. The API first attempts graceful shutdown, then closes remaining connections after a two-second drain deadline. The same stalled-peer regression now finishes while the peer is still open and verifies listener release.
- **API/client churn.** The real RMF Host on an idle domain reported subscriber-created, payload-awaiting and NotReady through five independently started CLI subscribers. SIGTERM closed an active SSE response and the listener. The separate synthetic Host process served all static profiles, kept authoritative state independent of client churn, and delivered the newest complete snapshot to both a fast client and a paused client after a 192-publication burst. The paused client received 36 post-initial snapshots. Both connections closed on shutdown. Restart on the same endpoint reset revision from 1707 to the same fresh-fixture value of 411.
- **Resource soak.** A 45.44-second Linux loopback run used five waves of 20 SSE clients at a target 20 Hz, 900 publications and five Auxiliary stale/failure/session rollovers. Host RSS started at 11,964 KiB and ranged from 17,040 to 17,304 KiB at the post-wave idle samples. At 20 clients the Host held 27 FDs and 21 sockets, returning to seven FDs and one listener every wave. Thread count stayed at 19. The rolling watch channel retained its latest value, Host diagnostics capped at 256, Engine Flight Recorder entries at 128 and incidents at eight. The Host recorder uses a 128-action rolling window with a compacted baseline; stale capsules at all three profiles replayed within the existing format bounds.
- **Performance.** On Linux x86-64 with an unoptimized Rust build, warm snapshots for 4/2, 32/8 and 128/16 measured 47,896, 57,110 and 86,500 bytes. Compact JSON serialization p95 was 2.476, 2.677 and 4.628 ms, respectively, from 15 samples per profile. The 32/8 run crossed 1, 5 and 20 clients with target rates of 1, 10 and 20 Hz and observed 0.99–19.88 Hz. Update through publish p95 was 0.605–1.066 ms, publish to localhost receipt 6.498–32.725 ms and Python client JSON parse 1.205–1.784 ms. At 20 clients and 20 Hz Host process CPU was about 3.13 core-equivalents. Each SSE connection encodes its own complete snapshots. These are local measurements with no production capacity or latency guarantee.

Reproduce the synthetic Linux process proof with `python3 crates/liftassistant-headless/tests/m5_soak.py` from the repository root. Formatting, locked all-target workspace Clippy, 129 workspace tests, all seven TUI scenarios and four non-live headless scenarios passed. The ignored Rust soak child was exercised by the Python driver. The hosted `msrv` job passed on the initial workflow change, and the final local all-target workspace check passed with actual Rust 1.85.0. Official Hotel live payload and timing proofs were **NOT RE-RUN IN THIS PASS**. No new second-family live evidence was supplied.

### M5.14 — M5 Completion Audit

**NOT READY.** A–F and the M5.13 matrix rerun precede the completion audit.
