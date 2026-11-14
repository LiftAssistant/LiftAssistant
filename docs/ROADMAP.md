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

## M2 — Semantic Core — current

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

### M2.4 — Decision reasons / normalized diagnostics
- structured selected/rejected reasons;
- capability/mode/distance/tie-break evidence;
- no large policy DSL.

### M2.5 — Capability/configuration refinement
- move demonstrated semantic assumptions out of UI-only code where appropriate;
- refine topology/capability/configuration vocabulary against existing standards;
- HUMAN-REVIEW before changing safety/authority semantics or freezing normative public schemas.

## M3 — Research and conformance laboratory

- reusable scenario representation only when real use requires it;
- trace inspection;
- controlled fault injection for simulator/bench work;
- standards mappings;
- backend semantic conformance checks;
- virtual/read-only adapter experiments.

Do not turn M3 into a certification claim.

## M4 — Real-world bridges

Controlled non-passenger work first:

- virtual CAN / SocketCAN;
- CiA 417 mapping experiments;
- T/GSET 001-2025 gateway research;
- HIL/bench environments;
- documented controller APIs;
- read-only/recorded adapters before writes.

Any passenger-elevator control or safety-authority change requires HUMAN-REVIEW.

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
