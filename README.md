# LiftAssistant

**LiftAssistant is an open, vendor-neutral software platform for elevators.**

The long-term goal is to make elevator controllers, protocols, capabilities, policies, diagnostics, simulation, and building integrations available through one common operational model — without replacing standards that already solve part of the problem.

LiftAssistant is an early semantic-core and simulation project. **It is not approved for controlling passenger elevators or any safety-critical equipment.**

## Current target: M5 — Production read-only platform

M1 Interactive Simulator is implemented and human-accepted.

The current working vertical path is:

~~~text
interactive hall call
        |
        v
deterministic dispatch
        |
        v
high-level intent
        |
        v
runtime boundary
        |
        v
fixed-step simulator
        |
        v
state + semantic event trace
        |
        v
developer TUI
~~~

M2 evolved this from one-shot dispatch into persistent call semantics: call identity/lifecycle, multiple pending calls, runtime-feedback-driven completion, explainable decisions, and explicit served-floor topology. That semantic vertical path is implemented and machine-reviewed.

M3 turned the deterministic simulator and replay path into a small research laboratory with executable scenarios, trace inspection, expected-outcome checks, and one-shot integration-failure injection. M4 established native-semantic recorded/read-only inputs, canonical incremental observations, explicit source bindings and sessions, observation authority isolation, Engine-owned operational state, diagnostics/replay, and a headless read-only lifecycle proof. M5.1 now adds a Rust-native, subscription-only Open-RMF adapter that maps explicitly configured native lift names through the same canonical Engine path. It has been exercised against the official Open-RMF Hotel simulation, but none of this implies passenger-elevator control, certification, or conformance claims.

The TUI is a composition/debugging client, not the architecture.

## Run the simulator

Requires Rust stable (minimum 1.85) and a terminal at least 70 columns by 24 rows.

~~~bash
cargo run -p liftassistant-tui
~~~

Run the built-in M2 lifecycle reference scenario without a TTY:

~~~bash
cargo run -p liftassistant-tui -- --scenario m2-call-lifecycle
~~~

The command executes the same developer composition path twice, rejects a deterministic or expected-outcome mismatch, and prints the semantic trace plus final call/car state. It is a research tool, not a conformance or certification claim.

Inspect the recorded/read-only runtime boundary:

~~~bash
cargo run -p liftassistant-tui -- --scenario recorded-read-only-observation
~~~

This scenario replays native semantic observation updates at explicit elapsed times. The recorded feed provides observations and deterministic replay only; it has no intent-submission interface. Engine, rather than the feed, owns the operational view consumed by the scenario. It performs no protocol, network, device, or hardware I/O.

Inspect the recorded Open-RMF state-mapping boundary:

~~~bash
cargo run -p liftassistant-tui -- --scenario rmf-recorded-read-only
~~~

The experiment maps recorded `LiftState` fields into canonical partial evidence with source identity and deterministic sequence. It preserves floor-level location without inventing a continuous coordinate, keeps undirected door movement and source-specific modes unresolved, and produces a complete `CarState` only when every required operational field is sufficiently established. It does not connect to ROS or send lift requests.

Inspect the independent recorded BACnet Lift-object mapping boundary:

~~~bash
cargo run -p liftassistant-tui -- --scenario bacnet-recorded-read-only
~~~

This experiment maps a typed in-memory projection of decoded BACnet Lift properties into the same canonical observation/Engine ingestion path. Universal floor numbers remain configured floor references; multi-door status is not collapsed when doors disagree; and BACnet object `Out_Of_Service` decoupling makes operational evidence unknown rather than pretending the car is in LiftAssistant's `OutOfService` mode. It implements no BACnet packet codec, transport, discovery, property write, live connection, or conformance claim.

Inspect bounded diagnostic capture and deterministic incident replay:

~~~bash
cargo run -p liftassistant-tui -- --scenario diagnostic-incident-replay
~~~

This scenario records canonical observations and Engine events in a bounded in-memory flight recorder, detects a deliberately repeated source sequence, freezes a small before/after evidence window, and replays the incident through the same deterministic detector. An anomaly is evidence of an invariant violation, not an equipment-fault or root-cause claim. No database, file format, cloud telemetry, or public diagnostic schema is involved.

Exercise the internal versioned incident-capsule path, including a fresh replay process:

~~~bash
cargo run -p liftassistant-headless -- --scenario headless-incident-capsule-replay
~~~

The production RMF command may additionally receive `--incident-capsule-out <path>` to create one bounded capsule at the first payload-stale trigger. Files are created without overwriting an existing path and can be replayed with `liftassistant-headless --replay-incident-capsule <path>`. The JSON wire representation is private, experimental, size-bounded, and explicitly versioned; it is not a stable/public schema or an unbounded telemetry archive.

Create a separately written candidate for manual review with `liftassistant-headless --sanitize-incident-capsule <private.json> <candidate.json>`. The deterministic private profile pseudonymizes source/session/car identities and remaps floor relationships, then requires the candidate to reproduce the same trigger in a fresh host before writing it. The source file is never modified. The candidate embeds `manual_review_required=true`; this path does not claim perfect anonymization, security certification, or permission to upload/publish the result.

Run the non-interactive headless/read-only lifecycle proof:

~~~bash
cargo run -p liftassistant-headless -- --scenario headless-read-only-lifecycle
~~~

This scenario uses an internal recorded canonical-observation source and exercises no transport. It proves that process existence is distinct from readiness: partial evidence remains `Synchronizing`, complete evidence reaches `ReadyReadOnly`, and a repeated source sequence moves the host to `Degraded`/not-ready while retaining the last known state for inspection. The headless binary has no TUI, simulator, `IntentSink`, passenger-elevator write path, deployment claim, or stable public API.

Exercise the live Open-RMF subscription proof against an independently running official Hotel simulation:

~~~bash
ROS_DOMAIN_ID=47 cargo run -p liftassistant-headless -- --scenario headless-rmf-live-proof
~~~

The proof requires explicit `Lift1 -> A` and `Lift2 -> B` configuration, starts an adapter-owned source session, consumes `/lift_states` with BestEffort/Volatile QoS, and feeds canonical updates into Engine-owned operational state. It pauses for an operator-controlled source restart and an independent simulation-only movement request so it can verify session invalidation/recovery plus real motion and door transitions. LiftAssistant creates no `/lift_requests` publisher and exposes no live RMF `IntentSink`.

Exercise the live payload-freshness proof with an explicit RMF-source silence policy:

~~~bash
ROS_DOMAIN_ID=47 \
LIFTASSISTANT_RMF_MAX_SILENCE_MS=3000 \
cargo run -p liftassistant-headless -- --scenario headless-rmf-freshness-proof
~~~

This proof keeps one subscription active, asks the operator to stop and restart only the official simulation source, and verifies `ReadyReadOnly -> Synchronizing/NotReady -> ReadyReadOnly`. The configured duration is deliberately source-specific and has no default. It begins measuring silence only after the subscription has received a payload, so DDS discovery delay remains distinct from a previously active data plane becoming stale.

Run the long-lived experimental RMF read-only host with explicit minimal configuration:

~~~bash
cargo run -p liftassistant-headless -- --run-rmf-read-only \
  --domain-id 47 \
  --source-id rmf/site-main \
  --max-silence-ms 3000 \
  --floor L1=1 --floor L2=2 --floor L3=3 \
  --bind Lift1=A:operational \
  --bind Lift2=B:operational \
  --agv-as-normal
~~~

The host creates an adapter-owned source session, reports lifecycle/readiness changes, revokes stale operational truth, reconnects for same-session recovery, and handles SIGINT/SIGTERM cooperatively even before the first payload arrives. Every floor, native-to-logical binding, binding role, interaction-mode projection, and freshness duration is explicit. This CLI is internal/experimental rather than a stable configuration or public API, and it remains subscription-only.

Its line-oriented internal diagnostics use bounded monotonic host sequences and machine-readable IDs while keeping transport, source/session, mapping, observation/resolution, and readiness evidence distinct. Each applicable record carries source, session, logical car, binding role, and source-native entity context; the Engine Flight Recorder continues to retain bounded canonical observations and semantic events. These records are evidence for operators/developers, not elevator-fault, root-cause, repair, or stable public-schema claims. DDS-backed ROS domains are rejected above the ROS 2 maximum of 232 before transport creation.

Controls:

- Up/Down select a floor; U/D create an UP/DOWN hall call.
- 1F permits UP only; 8F permits DOWN only.
- Space pauses/resumes; `.` advances exactly one 500 ms step while paused.
- A/B toggle the corresponding simulated car between Normal and OutOfService.
- P replays the recorded session twice from fresh simulators, verifies deterministic semantic output/live state, and leaves the result paused.
- R resets the simulator/UI.
- Q, Esc, or Ctrl-C quits.

The building shows floor references; car details distinguish floor-level location from genuine continuous position and also show mode, controller-reported destination, door state, and serve-floor capability. LiftAssistant's desired destination remains visible through call/intent state rather than being conflated with that report. The latest 100 trace entries show calls, dispatch, intents, runtime events, and errors.

Recording is capped at 2,048 commands. Pauses and floor selection do not affect physics and are not scenario commands.

Calls now have deterministic identity and persistent Core-owned state. Multiple calls can coexist: idle cars receive pending work deterministically, while excess work remains pending rather than replacing an active target. Runtime acceptance, arrival, and door-open feedback advance calls through serving/completion and release cars for deterministic pending-work reevaluation. Each dispatch reports per-car mode, busy/capability constraints, distance, rejection reasons, and the deterministic selection basis. An explicit served-floor topology drives call validation, TUI selection/rendering, reset, and replay. M2 semantic implementation and machine review are complete. Human review of the internal terminology remains welcome before any normative public schema is frozen, but it does not block reversible internal bridge work.

## Repository layout

~~~text
AGENTS.md                  authoritative AI-agent workflow
agent.md                   compatibility/navigation pointer

crates/
  liftassistant-model/     canonical domain types
  liftassistant-core/      deterministic semantic decisions
  liftassistant-engine/    shared semantic orchestration
  liftassistant-headless/  internal read-only host/lifecycle and live proofs
  liftassistant-rmf/       Open-RMF mapping and subscription-only adapter
  liftassistant-runtime/   semantic runtime contract
  liftassistant-sim/       deterministic reference and recorded backends
  liftassistant-tui/       developer simulator composition / UI

docs/
  PROJECT_MEMORY.md        durable decisions / validated long-lived facts
  STATUS.md                current milestone / READY work / blockers
  ROADMAP.md               milestone sequence / future scope
  ARCHITECTURE.md          architecture and safety boundaries
  PRIOR_ART.md             research triggers / standards-project index
  PRODUCTION_DESIGN_NOTES.md non-normative production/debugging ideas

scenarios/                 simulator/replay assets as real needs emerge
spec/                      future language-independent public semantics
~~~

Do not add more crates merely to represent hypothetical future layers. Extract a shared application/service layer only when a second frontend or runtime composition actually needs it.

## Architecture rules

1. Stable semantics, replaceable implementations.
2. Intent, not actuator control.
3. Core stays vendor-neutral.
4. Simulation/replay first.
5. Integrate existing standards instead of inventing replacements.
6. Safety authority stays below LiftAssistant.
7. No real passenger-elevator control in early milestones.
8. Working vertical slices come before framework expansion.

See [AGENTS.md](AGENTS.md), [docs/PROJECT_MEMORY.md](docs/PROJECT_MEMORY.md), [docs/STATUS.md](docs/STATUS.md), [docs/ROADMAP.md](docs/ROADMAP.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and [docs/PRIOR_ART.md](docs/PRIOR_ART.md).

## License

A license has **not** been selected yet. This is an explicit HUMAN-REVIEW decision.
