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

## M4 — Production boundary and real-world bridges — current

M4 should first make the prototype architecture honest about real external observations before adding live control.

Ordered direction:

1. canonical partial observation proof while keeping operational state complete;
2. separate observation-source, high-level intent-sink, and simulator/replay driving responsibilities;
3. extract a shared Engine boundary once duplicated orchestration demonstrates the need;
4. prove a minimal diagnostics/flight-recorder/replay substrate;
5. prove a headless read-only host/lifecycle;
6. expand recorded/virtual/read-only adapters and protocol mappings;
7. controlled non-passenger HIL/bench work;
8. only later consider high-level writes under explicit HUMAN-REVIEW.

Candidate bridge work after the boundary is sound:

- virtual CAN / SocketCAN;
- CiA 417 mapping experiments;
- BACnet/building observation mappings;
- T/GSET 001-2025 gateway research;
- Open-RMF live/read-only integration;
- documented controller APIs;
- recorded/read-only adapters before writes.

Any passenger-elevator control, safety-authority change, normative public-schema freeze, or first real write path requires HUMAN-REVIEW.

## M5 — User/building integrations

Only after stable enough semantics:

- stable external API;
- Home Assistant integration;
- BACnet/building integration;
- production web UI where justified;
- installation/update/rollback story;
- later evaluate appliance/OS distribution.

## Long-running constraints

- integrate existing standards instead of replacing them;
- LiftAssistant expresses WHAT, validated lower systems decide HOW;
- simulation/replay before real hardware;
- working vertical slices before infrastructure expansion;
- no premature microservices, plugin marketplace, database, or large policy DSL;
- research existing standards/projects before inventing durable semantics, but do not turn research into a milestone by itself.
