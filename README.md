# LiftAssistant

**LiftAssistant is an open, vendor-neutral software platform for elevators.**

The long-term goal is to make elevator controllers, protocols, capabilities, policies, diagnostics, simulation, and building integrations available through one common operational model — without replacing standards that already solve part of the problem.

LiftAssistant is an early semantic-core and simulation project. **It is not approved for controlling passenger elevators or any safety-critical equipment.**

## Current milestone — M7 software/simulation gateway

M1 Interactive Simulator is implemented and human-accepted.

M5 is complete at its read-only milestone boundary and M6's controlled intent
laboratory is HUMAN-accepted and complete. M7.0 + M7.1a are HUMAN-accepted, and
M7.1b completes the private, process-isolated Open-RMF `ServeFloor` gateway's
software/simulation recovery boundary. Outcome-unknown retirement remains
separate from explicit safe resumption, and the official Hotel proof confirms
that neither a late target-floor snapshot nor retirement alone restores write
readiness. The project is stopped for a HUMAN HIL/bench-controller environment
decision. This is not a public write API: Headless HTTP/SSE, CLI and Flutter
remain read-only, and no passenger elevator, real controller cabinet, CAN/OEM
network or OPC installation is authorized.

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

Exercise the offline KONE LCE typed-session and real Linux PTY proof:

~~~bash
cargo test -p liftassistant-lce --test pty_readonly_connector
~~~

The test creates only a kernel pseudo-terminal and a scripted
`LIFTASSISTANT_SYNTHETIC` peer. The connector generates only typed `STA`, `CMD`
and `MDE` observation queries, preserves bounded exact TX/RX evidence, finalizes
and verifies an internal SHA-256 capture, and deterministically replays RX
through the existing R1/R0 path. It contains no real serial backend, device
enumeration, LCEUIO/unlock procedure, controller connection or KONE control
path. The historical 9600/8N1 profile and `?` boundary remain unvalidated on a
real CPU561; see
[`docs/LCE_RAW_TERMINAL_COMPATIBILITY.md`](docs/LCE_RAW_TERMINAL_COMPATIBILITY.md).

Build and validate the Linux software-RX-only acquisition foundation without
opening a real device:

~~~bash
cargo build -p liftassistant-lce --example lce_passive_serial_capture --locked
cargo test -p liftassistant-lce --test passive_serial_capture --locked
cargo run -p liftassistant-lce --example lce_passive_serial_capture --locked -- \
  --plan \
  --assurance software-rx-only \
  --profile historical-9600-8n1 \
  --device /deliberately/nonexistent/lce-plan-tty \
  --output /tmp/lce-passive-capture.json \
  --duration-seconds 30
~~~

The example requires explicit `--assurance software-rx-only` and
`--profile historical-9600-8n1` acknowledgements as well as an absolute tty
path. `--plan` validates and prints the configuration without opening a device,
creating the output, generating a session, issuing an ioctl, or probing the
host. Real execution prints the weaker software-only assurance boundary before
reserving a create-new output name and opening the tty. It then opens the tty
`O_RDONLY | O_NOCTTY | O_NONBLOCK | O_CLOEXEC`, requests exclusive mode, applies
and records an explicit local 9600/8N1 raw-capture policy, and stores only
bounded `TTY_DELIVERED_RX` chunks. The PTY regression proves the descriptor is
read-only, a synthetic write fails `EBADF`, no tty echo reaches the peer, and
offline R2.3 replay produces no query/TX. A duration-complete zero-byte artifact
is explicitly `PassiveSilent` and inconclusive. A normal pre-commit error
removes the reserved incomplete output; SIGKILL, abort, kernel crash, or power
loss may still leave an empty or partial file, and only artifact parsing and
integrity verification establish validity. Software RX-only is not
physical passivity: no real `/dev/tty*`, CPU561, XDB1, LCEUIO, controller, or
passenger equipment was used. The XDB1 evidence freeze is common to two
explicit assurance paths: a separately HUMAN-approved software-RX-only field
observation may use a reviewed normal RS-232 interface while retaining
`PHYSICALLY_PASSIVE=false/unproven`; the R3A.0.3/R3A.0.4 physical RX-only
design/bench path remains the stronger prerequisite for a physical-passivity
claim. Neither path authorizes active query, unlock or control. See
[`docs/LCE_R3A_PASSIVE_ACQUISITION.md`](docs/LCE_R3A_PASSIVE_ACQUISITION.md)
and
[`docs/LCE_R3A_SOFTWARE_RX_ONLY_FIELD_PATH.md`](docs/LCE_R3A_SOFTWARE_RX_ONLY_FIELD_PATH.md).

Run the internal observation-adapter validation profiles:

~~~bash
cargo run -p liftassistant-tui -- --scenario adapter-validation-profiles
~~~

The scenario applies one bounded checklist to the concrete Open-RMF adapter and recorded BACnet mapper: explicit native-to-logical identity, Engine binding, source sessions, source-global sequence, ambiguity preservation, restart invalidation, canonical Engine ownership, and an observation-only path. Its report keeps RMF's official live-plus-recorded evidence separate from BACnet's recorded-only evidence. These are deterministic engineering checks, not certification, conformance, safety validation, or evidence of a live BACnet endpoint.

Run the M6A virtual write-adapter profile without a TTY:

```bash
cargo run -p liftassistant-tui -- --scenario m6a-write-adapter-profile
```

The profile checks the simulated `ServeFloor` and `ParkAt` boundary for controller binding, per-car execution floors, readiness, grant fencing, IntentId idempotency, correlated receipts, canonical observation reconciliation, cancellation, and restart without resend. It includes the deterministic fault matrix and bounded diagnostic replay. Its evidence applies to VirtualController and SimRuntime inside this process. The headless API and its clients continue to expose read-only observations.

M6B.1 additionally challenges that contract with the independently running official Open-RMF Hotel simulation. The simulation-only `liftassistant-rmf` `hotel_intent_probe` example is intentionally not a normal Host command: it refuses to publish outside its fixed gateway-free test network and is absent from the read-only Host, API and clients. The executable validation cases and their evidence limits are recorded in [`docs/M6B1_OPEN_RMF_VALIDATION.md`](docs/M6B1_OPEN_RMF_VALIDATION.md). A Hotel pass is simulator semantic evidence, not real-elevator, hardware, safety, OEM, CAN/CANopen, production or certification evidence.

M7.1a adds the separate `liftassistant-rmf-gateway` binary. It has a local-only,
kernel-peer-authenticated Unix socket, one explicit logical/native car and floor
mapping, a fresh Engine grant, a bounded durable unresolved-operation record,
and no automatic resend after restart. Run it only with a private operator-owned
configuration:

```bash
cargo run -p liftassistant-rmf --bin liftassistant-rmf-gateway -- \
  --config /private/path/rmf-gateway.json
```

The configuration shape, failure/recovery contract, explicit private recovery
operations and completed official Hotel evidence are documented in
[`docs/M7_RMF_GATEWAY.md`](docs/M7_RMF_GATEWAY.md). This binary is an
experimental software/simulation gateway, not a passenger-production service
or a general-purpose command interface.

Inspect bounded diagnostic capture and deterministic incident replay:

~~~bash
cargo run -p liftassistant-tui -- --scenario diagnostic-incident-replay
~~~

This scenario records canonical observations and Engine events in a bounded in-memory flight recorder, detects a deliberately repeated source sequence, freezes a small before/after evidence window, and replays the incident through the same deterministic detector. An anomaly is evidence of an invariant violation, not an equipment-fault or root-cause claim. No database, file format, cloud telemetry, or public diagnostic schema is involved.

Exercise the internal versioned incident-capsule path, including a fresh replay process:

~~~bash
cargo run -p liftassistant-headless -- --scenario headless-incident-capsule-replay
~~~

The production RMF command may additionally receive `--incident-capsule-out <path>` to attempt one bounded capsule at the first feed-wide payload-stale trigger. A failed attempt warns and observation continues. Files are created exclusively and can be replayed with `liftassistant-headless --replay-incident-capsule <path>`. The JSON format is private, experimental, size-bounded and versioned.

Create a separately written candidate for manual review with `liftassistant-headless --sanitize-incident-capsule <private.json> <candidate.json>`. The deterministic private profile pseudonymizes source/session/car identities and remaps floor relationships, then requires the candidate to reproduce the same trigger in a fresh host before writing it. The source file is never modified. The candidate embeds `manual_review_required=true`; this path does not claim perfect anonymization, security certification, or permission to upload/publish the result.

Run the non-interactive headless/read-only lifecycle proof:

~~~bash
cargo run -p liftassistant-headless -- --scenario headless-read-only-lifecycle
~~~

This scenario uses an internal recorded canonical-observation source and exercises no transport. It proves that process existence is distinct from readiness: partial evidence remains `Synchronizing`, complete evidence reaches `ReadyReadOnly`, and a repeated source sequence moves the host to `Degraded`/not-ready while retaining the last known state for inspection. The headless binary has no TUI, simulator, `IntentSink`, passenger-elevator write path, deployment claim, or stable public API.

Inspect the pure `experimental/v0` read-model projection without opening a socket:

~~~bash
cargo run -p liftassistant-headless -- --scenario headless-experimental-read-model
~~~

The output is a separate API-boundary JSON DTO rather than serialized canonical Rust structs. It preserves floor-reference versus continuous location, all three reported-destination states, explicit motion/door/mode/readiness/binding-role values, source/session health, and bounded recent diagnostic identities. Its revision is monotonic only within the current host process and is not an observation sequence, source session, persistent version, or stable API promise.

For an isolated developer-only HTTP/SSE boundary proof, explicitly start the fixture on a chosen address:

~~~bash
cargo run -p liftassistant-headless -- \
  --serve-experimental-snapshot-fixture 127.0.0.1:8080
~~~

`GET /api/experimental/v0/snapshot` returns the complete current JSON projection. `GET /api/experimental/v0/events` sends complete `snapshot` events when the read-model revision changes. The stream retains only the latest complete value, so reconnecting recovers current state without a durable cursor or forensic event backlog. Both routes are read-only, return no wildcard CORS policy, and are experimental rather than stable/public API contracts. The fixture is disabled unless this explicit command is used.

Consume that boundary from the independent read-only CLI process:

~~~bash
cargo run -p liftassistant-cli -- snapshot http://127.0.0.1:8080
cargo run -p liftassistant-cli -- watch http://127.0.0.1:8080
~~~

The CLI prints Host lifecycle/readiness and revision, operational cars, source/session/observation summaries, and recent diagnostic identities. `watch` continuously consumes complete SSE snapshots; `--max-snapshots <n>` is available for bounded automation. The client has its own small `experimental/v0` parser and communicates only over HTTP/SSE: it neither links nor instantiates Engine, the RMF adapter, a write boundary, or a shared client SDK.

Consume the same boundary from the independent Flutter graphical client (`client/`, targeting Android, Linux, and Web):

~~~bash
cd client
flutter run -d linux
~~~

The graphical client connects to an explicitly configured `http://host:port`, loads a complete `experimental/v0` snapshot, subscribes to continuous SSE `snapshot` events, recovers across disconnects and Host restarts (including Host-local revision resets), and remains strictly read-only against normal `liftassistant-headless` Hosts. Because `liftassistant-headless` intentionally omits wildcard CORS headers on unauthenticated HTTP, browser deployments should serve `flutter build web --no-web-resources-cdn` from the same origin (for example behind a local/LAN reverse proxy forwarding `/api/experimental/v0/*`).

For local development only, an isolated Simulator server example can expose the same read-only `experimental/v0` snapshot/SSE endpoints together with an explicitly opted-in, loopback-only Simulator `ServeFloor` workflow (`/api/dev-sim/v0/*`):

~~~bash
cargo run -p liftassistant-headless --example sim-dev-server -- \
  --listen 127.0.0.1:8080 \
  --enable-dev-sim-control
~~~

Without `--enable-dev-sim-control`, the example serves only read-only `experimental/v0`. When enabled on loopback, `client/` exercises preview, transaction-bound review, client-correlated submission, Engine admission/`IntentId` execution against `VirtualController(SimRuntime)`, canonical observation reconciliation, reconnect status query without automatic resend, and honest process-restart knowledge loss. This example is strictly Simulator-only and development-only: `/api/experimental/v0/*` stays read-only, RMF/LCE/real hardware are unreachable, mode control and settings/parameters are absent, and no public/production control API or real-elevator write path is claimed.

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
  --api-listen 127.0.0.1:8080 \
  --agv-as-normal
~~~

The configured floors define the building universe. Each RMF lift may report a different subset. `--max-silence-ms` applies to the feed and individually to each bound lift after its first payload. A silent or malformed lift can lose its own read-side state while other lifts continue updating.

The host creates an adapter-owned source session, reports lifecycle/readiness changes, revokes stale operational truth, reconnects for same-session recovery, and handles SIGINT/SIGTERM cooperatively even before the first payload arrives. Every floor, native-to-logical binding, binding role, interaction-mode projection, and freshness duration is explicit. The API is disabled when `--api-listen` is omitted. Binding a non-loopback address is allowed only as an explicit operator choice and emits a warning because this experimental read-only API has no authentication, TLS, or RBAC; it also does not enable wildcard CORS. This CLI and wire format are internal/experimental rather than stable public contracts, and the host remains subscription-only.

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

client/                    independent Flutter read-only graphical client (Android, Linux, Web)

crates/
  liftassistant-cli/       independent experimental HTTP/SSE read-only client
  liftassistant-model/     canonical domain types
  liftassistant-core/      deterministic semantic decisions
  liftassistant-engine/    shared semantic orchestration
  liftassistant-headless/  internal read-only host/lifecycle and live proofs
  liftassistant-lce/       offline legacy-LCE parser/session/evidence/PTY research
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
