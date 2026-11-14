# Project status

Updated: 2026-11-14

## Current milestone

**M2 — Semantic Core**

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

## Current limitation driving M2

M1 call handling was one-shot:

~~~text
HallCall
  -> immediate dispatch
  -> ServeFloor
  -> runtime target
~~~

M2.1 through M2.4 now provide persistent call identity/lifecycle, multiple pending calls without target replacement, runtime-feedback-driven serving/completion/reassignment, and structured dispatch explanations. Focused capability/configuration refinement is the next semantic-core gap.

This is the primary M2 problem.

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

**READY — next implementation slice.**

Only after the lifecycle path is working:
- reduce TUI-only hard-coded semantic assumptions where there is a demonstrated need;
- refine building topology/capability/configuration inputs;
- compare terminology and concepts with existing standards before freezing public semantics.

Any safety/authority-model change remains HUMAN-REVIEW.

## Explicit non-goals in M2

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
