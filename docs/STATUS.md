# Project status

Updated: 2026-11-17

## Current milestone

**M5 — Production Read-only Platform**

M1 Interactive Simulator is closed and human-accepted.

M4 is COMPLETE. M4.10 HUMAN-CHECK was accepted on 2026-11-17 after all four slices and the live-evidence-exposed dispatch regression passed review.

M5.1 is implemented: the Rust-native Open-RMF adapter has passed official Hotel live read-only validation through Engine-owned state. **M5.2 — Source Lifecycle / Freshness / Health is READY.** No later M5 slice is READY merely because it appears in the roadmap.

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

**READY.**

Implement host-observed, source-specific, configurable freshness without moving wall-clock time into Core or treating observation sequence as freshness. The next slice must distinguish transport/discovery, payload activity, source/session activity, observation resolution, and Engine operational readiness at least enough that discovery alone cannot claim healthy ingestion.

An Operational source becoming stale must explicitly revoke its current live operational truth/readiness while retaining bounded inspectable evidence; an Auxiliary source becoming stale must not revoke a healthy Operational authority. Recovery must pass through synchronization and require sufficient new canonical evidence. Do not add automatic failover, a universal timeout, an equipment-fault inference, or a stable public health taxonomy.

M5.3 and later dependent slices remain not READY until M5.2 supplies the lifecycle/freshness evidence required by a production read-only host.
