# Project status

Updated: 2026-11-16

## Current milestone

**M4 — Real-world bridges (recorded/read-only first)**

M1 Interactive Simulator is closed and human-accepted.

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
- runtime-provided per-car capabilities remain the dispatch input; no speculative building/door/capability DSL was introduced;
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
- capabilities advertise position/door observation while `serve_floor` and `park` are false;
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
- a closed door does not confirm the exact at-floor position required by `CarState`, so that complete-state mapping is rejected;
- `MOTION_UP`/`MOTION_DOWN` are rejected because a string `current_floor` does not provide LiftAssistant's continuous position; `MOTION_UNKNOWN` is also rejected;
- Human/AGV control modes are rejected rather than conflated with LiftAssistant operating modes; unsupported/unknown/emergency modes remain explicit errors;
- mapped observations feed the existing recorded/read-only runtime, which still rejects every intent;
- `cargo run -p liftassistant-tui -- --scenario rmf-recorded-read-only` verifies two identical mappings/playbacks, retained session context, final observed state, no-write behavior, and visible ambiguity errors;
- no live ROS/network connection, request publication, hardware I/O, public protocol schema, or write path was introduced.

Self-review evidence preserved: the first moving-state implementation mapped `MOTION_UP` plus `current_floor=L3` to an exact `3.0F` position. That fabricated precision was removed; moving states now fail mapping until a source provides continuous position or the canonical model explicitly represents qualitative position. The review then applied the same rule to stopped/closed states, which do not confirm exact at-floor position; the accepted fixture is stopped/open. A subsequent focused build also exposed and removed one unused import.

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
- a floor reference becomes exact at-floor position only for the currently proven stopped/open evidence combination;
- the Open-RMF mapper now produces canonical observations and separately retains raw current/destination labels, available/current source modes, and session identity;
- declared Open-RMF `MOTION_UNKNOWN` and Human/AGV/Emergency modes are valid partial evidence, while unrecognized enum values and invalid/unconfigured floor labels remain mapping errors;
- the real `rmf-recorded-read-only` scenario runs the mapper/resolver flow twice and checks the expected incomplete `L3 + UP + DOOR_MOVING` result, the resolved `L4 + STOPPED + OPEN` result, source-specific AGV-mode non-resolution, provenance/sequence, and the existing no-write boundary;
- the model remains internal/reversible: no generic quality/confidence/timestamp system, storage format, ROS dependency, live transport, or public schema was added.

Focused prior-art review rechecked the official Open-RMF `LiftState` definition and OPC UA `DataValue`. The implementation preserves the demonstrated source semantics and provenance distinction without copying OPC UA quality/timestamp machinery that has no current caller.

Validation: formatting, all-target linting, 43 workspace tests, and the real Open-RMF recorded/read-only scenario passed.

### M4.4 — Separate observation, intent, and simulation-driving contracts

**Implemented.**

Implemented behavior:

- the all-in-one `ElevatorRuntime` trait is replaced by three internal contracts: `ObservationSource`, `IntentSink`, and `SimulationDriver`;
- `ObservationSource` exposes resolved operational state and capabilities without granting a write method;
- `IntentSink` accepts only LiftAssistant high-level `Intent`; no low-level motion, drive, door-force, or safety primitive was introduced;
- `SimulationDriver::advance(Duration)` owns explicit-time simulator/replay progression and is not required by observation sources or intent sinks in general;
- `SimRuntime` implements all three contracts because it is an active deterministic simulator;
- `RecordedRuntime` implements only observation and replay-driving contracts; the previous always-error `submit_intent` method and its `IntentSink` capability are gone;
- TUI composition imports only the contracts it actually uses, and recorded/Open-RMF scenarios exercise observation/replay without attempting a write;
- existing simulator replay, runtime feedback, timing, partial-observation resolution, and scenario behavior remain deterministic.

No async runtime, database, message bus, network protocol, IPC, plugin ABI, or new package was added.

Validation: formatting, all-target linting, 43 workspace tests, and all four executable developer scenarios passed.

### M4.5 — Extract the demonstrated Engine boundary

**Implemented.**

Implemented behavior:

- a focused `liftassistant-engine` crate now owns building topology, call registry/lifecycle, dispatch, pending scheduling, high-level intent submission/result handling, runtime-feedback transitions, and machine-readable `EngineEvent` output;
- Engine exposes canonical observation resolution through the existing deterministic resolver rather than making the TUI or source adapter decide operational truth;
- Core remains deterministic and I/O-free; Engine depends on Core/model/runtime but not simulator, terminal UI, networking, storage, or wall-clock time;
- the TUI `App` retains only simulator instance/control, selection/pause/step state, bounded display trace, and replay input recording;
- interactive execution and non-interactive simulator scenarios now use the same Engine orchestration; the recorded Open-RMF scenario uses the same Engine resolution boundary;
- simulator-specific mode/rejection controls remain in the developer composition and do not leak into Engine events;
- one direct Engine test protects intent generation, Assigned/Serving/Completed lifecycle, and runtime-feedback processing, while all existing replay/scenario tests protect migration parity.

The sixth crate was justified by actual reuse across interactive, non-interactive, resolver, and upcoming headless callers; no service framework or generic application platform was introduced.

Validation: formatting, final all-target linting, 44 workspace tests with the updated lockfile, and all four executable developer scenarios passed.

### M4.6 — Diagnostics substrate

**READY — next implementation slice.**

Promotion evidence: Engine now has stable machine-readable observation-resolution and semantic-event boundaries used by both interactive and scenario callers, while diagnostics still collapse into a TUI-only bounded `String` trace and the Open-RMF evidence path has no shared recorder. This is concrete caller pressure for a bounded structured flight recorder and a very small deterministic detector set.

Implement incrementally:

- add a bounded in-memory flight recorder over canonical observations, semantic transitions, decisions and intents;
- define a small internal machine-readable diagnostic-event shape;
- add deterministic invariant/sequence anomaly detection for one or two demonstrated cases;
- make an incident snapshot replayable through the existing deterministic tooling;
- do not add ML, a database, cloud telemetry, or a large diagnostic taxonomy.

The purpose is to validate that production failures can leave useful, replayable evidence.

### M4.7 — Headless/read-only production host proof

Only after the previous slices:

- prove the Engine can run without the TUI;
- introduce lifecycle/readiness concepts sufficient for startup, synchronization, read-only readiness and degraded state;
- connect only recorded/virtual/read-only sources;
- no passenger-elevator writes;
- no public stable API yet;
- no production deployment claim.

## Preserved production ideas

Longer-term production/debugging ideas are preserved in `docs/PRODUCTION_DESIGN_NOTES.md`. They are constraints/candidates, not a mandate to implement them all during M4.

The highest-priority future themes include observation quality/freshness, reconciliation, shadow debugging, incident capsules/replay, stable diagnostic codes, adapter validation profiles, sandboxing, semantic maturity/versioning, safe update/rollback, sanitized diagnostic bundles, and a community recorded-fixture corpus.
