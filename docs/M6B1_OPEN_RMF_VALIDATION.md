# M6B.1 Open-RMF isolated validation contract

The target is the official Open-RMF Hotel simulation on ROS 2 Kilted. The command harness and the simulator must have independent lifetimes inside a private network boundary. Only the fixed Hotel lift bindings are eligible for test requests. Default hosts, clients, and live observation interfaces stay read-only.

## Required evidence

| Case | Acceptance evidence |
| --- | --- |
| A. Happy path | Record LiftAssistant admission and local dispatch separately. Show explicit evidence of RMF receiving the request, then independent lift observations and an Engine outcome. A send result or a receipt leaves physical reconciliation pending. |
| B. Duplicate and replay | Repeat one IntentId across the caller, adapter, transport and observation reconnect boundaries. Record native command attempts and physical transitions so the test can distinguish one operation from a second actuation. |
| C. Stale state | Reach a target once, then submit a different IntentId for the same target. Replay a previously generated sample across the adapter boundary. Keep the new operation pending until evidence belongs to the current execution lifecycle. |
| D. Restart | Restart before dispatch, after dispatch before arrival, and after arrival before observation. Preserve the original IntentId, trace authority decisions and avoid resubmission. Show recovery or an explicit unresolved outcome. |
| E. Disconnect | Isolate the link while the simulator runs independently. Preserve the local operation and delivery uncertainty, then prove recovery against external evidence. |
| F. Wrong state | Present independent evidence for a different target or incompatible execution. The intended result stays unsatisfied and the discrepancy is recorded. |
| G. Rejection and failure | Distinguish a local send failure, uncertain delivery, an explicit external rejection when the interface supplies one, observed divergence and a test deadline. Capture the original evidence. |
| H. Lift mapping | Configure `Lift1 → A` and `Lift2 → B` explicitly. Use the other official lift to exercise an unconfigured input. Source-native names and sessions stay in the RMF boundary. |
| Software authority | Prove that unavailable or higher-priority state and stale or missing grants block ordinary admission before publication. Treat any simulator alarm evidence as simulated software semantics only. |

Maintain a causal trace with the IntentId, logical CarId, grant checks, native correlation, publish count, observation provenance and source time, controller evidence and final Engine outcome. Use bounded deadlines that fail a test instead of implying success. A mock or separately replayed sample can verify a boundary and contributes no independent external-transition evidence.

## Identity and completion boundary

`PrincipalId`, `AuthorityGrantId` and `IntentId` come from LiftAssistant. RMF lift names, holder sessions, message identities and simulator clocks belong to the adapter. A mapped sample may supply only the evidence its source actually observed. The adapter must establish usable freshness before promoting it to the M6A Operational stream. Received transport data, holder assignment, and physical arrival are distinct facts. The command path may report indeterminate delivery. It must not retry an uncertain operation by allocating another identity.

Complete M6B.1 only after the required matrix and workspace/CI gates pass against the official simulator. Record a minimal reproduction and leave the dependent milestone blocked if an environment or upstream limit prevents the required evidence. A change to M6A public semantics, security identity, authority or its guarantees requires HUMAN review.

## Evidence limits

The only execution target is the isolated official simulation. Real elevators, hardware, CAN/CANopen/CiA 417, KONE, LCI, OEM compatibility, field safety, production readiness and certification remain unverified. `M6B.2` is a separate HUMAN gate.

## Executed Hotel evidence

The 2026-11-19 validation used the official ROS 2 Kilted Hotel image pinned at `ghcr.io/open-rmf/rmf/rmf_demos@sha256:601a6bb7c04d559fb9ec654d9b7a41650918990e35712036d836f072a6430db5`. The simulator and the Rust probe ran as independent processes in one temporary rootless Podman container attached only to a gateway-free internal subnet. The probe refuses to run when it sees a default route, an unexpected interface set, the wrong fixed test address, or a ROS domain other than 47. The normal Host, HTTP API and clients gained no write path.

The simulation-only `hotel_intent_probe` executable supplies five bounded cases:

| Case | Result and external evidence |
| --- | --- |
| Happy / duplicate | `Lift1 -> A` moved from L1 to L2. A holder-bearing state established acceptance while Engine reconciliation remained Pending. One logical submission, ten Engine retries and one identical transport republish produced one trip and completion only after a stopped L2 observation. |
| Same-target stale state | `IntentId(1)` reached L2. The adapter ended that RMF holder, began a new LiftAssistant source session, replayed the old-session completion and proved it remained fenced. Only a different `IntentId(2)`, new adapter-local holder and new closed-to-open Hotel door cycle completed the second L2 operation. |
| Restart / disconnect | The source session restarted after dispatch and revoked operational observations. The probe withheld the data plane for 20 seconds while Hotel independently moved L1 to L3. Reconnection observed final L3 in the new session and reconciled the original Intent without a second command. |
| Wrong state | LiftAssistant desired L2 while the fault harness requested L3. The observed stopped L3 state produced `Diverged { observed: 3F }`, never Satisfied. |
| Native rejection | Hotel logged `Received request for unavailable floor [not-a-hotel-floor]`. Its interface supplied no typed rejection receipt. LiftAssistant retained Pending reconciliation and converted Awaiting delivery to Indeterminate at the bounded adapter deadline; publication was never treated as completion. |

`Lift2 -> B` was observed concurrently through the same source-global stream. Applying its native name to an adapter configured only for `Lift1 -> A` produced `UnconfiguredLift`; no name-based car was created. RMF lift names, holder strings, lift clocks and source sessions remained adapter context. The LiftAssistant Principal, grant and Intent identities stayed independently allocated in Engine.

The official Hotel path reported normal AGV interaction mode and did not supply credible emergency semantics for this run. Emergency or real safety behavior is therefore not verified. The software-only regression proves that an OutOfService Operational state blocks controlled admission before backend submission, and existing mapper regressions preserve Fire, Offline, Unknown and Emergency distinctions.

Transport discovery was observably imperfect: some first RustDDS reader or writer generations saw no payload or peer even while a ROS CLI subsequently proved the topic active. The probe records this negative evidence and recreates a bounded transport generation without changing `ObservationSessionId`, authority, Principal, grant or Intent. Discovery was never promoted to acceptance or completion evidence. No raw simulator logs are committed.
