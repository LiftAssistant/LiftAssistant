# M6 completion audit

Updated: 2026-11-20

## Decision posture

This audit evaluates the M6 goal:

> validate high-level write semantics without using passenger production as the
> experiment.

M6C.2 is HUMAN-accepted with its executed result unchanged:
`PARTIAL_MAPPING`. The missing KONE receipt, assignment, Serving, terminal and
prevention mappings are preserved as negative evidence. The audit did not
repeat the same Sandbox experiment, contact production equipment, add a
production adapter or begin M7.

The implementation and accepted evidence support **RECOMMEND_M6_COMPLETE**.
This is an engineering recommendation, not the HUMAN milestone-exit decision.
M6C.3 is **NOT REQUIRED**: the remaining positive vendor lifecycle mappings are
backend-specific evidence gaps, and another equivalent KONE run would not test
a new generic M6 hypothesis.

## Evidence classes

- **Implementation:** the current source and deterministic state transitions.
- **Deterministic regression:** repeatable unit, scenario, replay or isolated
  process evidence in this repository.
- **External validation:** accepted evidence from an independently running
  simulator, protocol peer or official Sandbox.
- **Residual:** a limit that the current evidence does not erase.

No external result is promoted beyond its environment. Open-RMF Hotel remains
simulation evidence, the CiA 417 subset remains `vcan` protocol evidence, and
KONE remains official-Sandbox evidence rather than production behavior.

## Semantic evidence matrix

| Concern | Implementation and deterministic evidence | Accepted external evidence | Residual classification |
| --- | --- | --- | --- |
| Principal and authority | Per-car `AuthorityGrant` and external-scope `ExternalDispatchAuthorityGrant` are explicit, revocable and fenced. Controller, credential and vendor identity grant no authority. Wrong, replaced and restored grants fail closed. | Hotel transport/holder recreation did not create a Principal or grant. KONE OAuth/resource/session identities did not create LiftAssistant authority. | Production authentication-to-Principal mapping is `M7_ENTRY_REQUIREMENT`. |
| Operation identity | `CallId` names one hall call. `IntentId` names one car-specific lower operation. External-owned calls retain their `CallId` without manufacturing an `Intent`. Per-car watermarks and bounded terminal tombstones fence old identities. | CiA 417 Hall Call Up carried no LiftAssistant operation/incarnation ID. KONE native request/session identity remained vendor-local. | Backend correlation strategy is `BACKEND_SPECIFIC`; durable identity storage across a real process crash is `M7_ENTRY_REQUIREMENT`. |
| Dispatch ownership | LiftAssistant-owned calls select a logical car before `ServeFloor`. External-owned calls submit car-free and accept assignment only through explicit native-to-`CarId` mapping. | KONE destination-call evidence demonstrated group ownership and late allocation; landing-call evidence did not provide an authoritative allocation mapping. | Concrete backend ownership/capability declaration and rejection of overlapping physical authority coverage are `M7_ENTRY_REQUIREMENT`. |
| Delivery uncertainty | Engine reserves active identity and conservative uncertainty before entering a sink. `NotSubmitted`, `Submitted` and `Indeterminate` stay distinct. A caught sink interruption cannot expose a false unsent state. | Hotel unavailable-floor delivery remained Indeterminate. KONE sends were Submitted while native success lacked an approved acceptance meaning. | Backend send-boundary classification is `BACKEND_SPECIFIC`. Durable crash recovery is `M7_ENTRY_REQUIREMENT`. |
| Receipt | Receipt is correlated by controller plus operation identity and is separate from delivery, assignment, service and satisfaction. Definitive evidence cannot be overwritten by late uncertainty. | Hotel holder observation supplied simulation-only receipt evidence. KONE `success: true` remained Indeterminate because it did not establish controller acceptance. | Positive KONE receipt mapping is `FUTURE_EVIDENCE`; each production backend mapping is `BACKEND_SPECIFIC`. |
| Retry and duplicate behavior | Same identity and meaning deduplicate. Only explicit authorized retry after proven non-submission can resend. Submitted/Indeterminate work, interrupted sends, restored work and retired identities remain fenced. | Hotel retry/republish did not create a second trip. KONE reconnect/resume did not resubmit. | Exactly-once transport is not claimed. Backend idempotency/correlation is `BACKEND_SPECIFIC`. |
| Expiry, cancel and withdrawal | Expiry/cancel withdraw desired work without fabricating a stop. `Prevented` retires only unstarted work; `Requested`/`Unsupported` retain uncertainty. Withdrawal reserves `Requested` before sink entry so interruption cannot trigger a duplicate command. | Hotel holder release was explicit simulation behavior. KONE delete lacked correlated acknowledgement and correctly remained Requested. | Vendor prevention/cancellation proof is `BACKEND_SPECIFIC`; KONE positive mapping is `FUTURE_EVIDENCE`. |
| Late assignment and conflict | Assignment is eligible only for already-sent external work in the configured scope/controller. Unknown mapping is non-destructive, identical evidence idempotent, conflicting reassignment rejected, and terminal evidence cannot resurrect work. | KONE supplied no authoritative landing-call allocation. Destination-call allocation remained unmapped vendor evidence. | Reassignment policy, if a backend needs one, is `BACKEND_SPECIFIC` and requires new evidence. |
| Serving | Separate correlated evidence is required after assignment. Assignment or receipt alone cannot start service. | KONE supplied no correlated landing-call Serving event, so both calls remained Pending. | Positive Serving mapping is `FUTURE_EVIDENCE` and production mapping is `BACKEND_SPECIFIC`. |
| Desired versus reported | Desired floor remains in the immutable request; reported destination is observation evidence and cannot rewrite desired work. Unknown or divergent reports retain uncertainty/diagnostics. | Hotel wrong-target and unavailable-floor cases preserved divergence/uncertainty. | Controller-specific destination semantics are `BACKEND_SPECIFIC`. |
| Physical completion | Accepted/Serving/native terminal states do not imply completion. Fresh Operational arrival, stopped motion and ordered open-door evidence are required; ambiguous shared evidence does not choose between calls. | Hotel supplied simulation movement/door completion and wrong-floor divergence. KONE uncorrelated position/door activity had no completion authority. | Physical predicates for a chosen real backend are `M7_ENTRY_REQUIREMENT`; real equipment evidence is `FUTURE_EVIDENCE`. |
| Stale observation | Only current Operational authority can reconcile work. Invalidation and restore remove stale physical evidence; Auxiliary evidence cannot complete or fail over. | Hotel source-session restart and delayed-old evidence exercised fencing and observation-only recovery. | Real backend freshness and recovery policy are `BACKEND_SPECIFIC` and part of the M7 gateway gate. |
| Restart and reconnect | Reversible checkpoints preserve unresolved identity, delivery, receipts and external progress, fence grants, invalidate physical evidence and perform no I/O or automatic resend. | Hotel transport recreation and source restart preserved identity without resend. KONE resume produced no replay and no resubmission. | Durable process-crash persistence/reconciliation is `M7_ENTRY_REQUIREMENT`; M6 proves only in-memory/virtual recovery. |
| Concurrent conflict | One unresolved controlled intent per logical car; incompatible active Core work blocks controlled admission. External assignment conflicts are explicit and multiple matching serving calls remain unresolved rather than guessed. | No accepted external evidence establishes a universal mixed-backend arbitration policy. | Deployment-level exclusion of overlapping per-car and group controller authority is `M7_ENTRY_REQUIREMENT`, not a generic M6 arbitration framework. |
| Diagnostics and replay | Bounded Flight Recorder/incident windows cover authority rejection, identity/receipt conflicts, divergence and readiness loss. Terminal histories retain 128 recent records. | Private bounded M6B.3/M6C.2 Evidence v0 preserves sanitized causal evidence and negative results. | Production audit persistence, retention and export are `M8_HARDENING`. |
| Backend boundary | Model/Core/Engine contain no transport, protocol, wall-clock, filesystem, HTTP or vendor dependency. Existing HTTP/SSE and Flutter/CLI surfaces remain read-only. | All external write validation used isolated test-only harnesses; none entered the product Host. | A concrete isolated production gateway is `M7_ENTRY_REQUIREMENT`. |

## Correctness defects found and fixed

The audit found two directly coupled failure-boundary defects:

1. A controlled sink could unwind after possible transmission while the active
   record still said `NotSubmitted`. The Engine now reserves
   `Receipt::Indeterminate` immediately before sink entry. A caught unwind
   retains the active identity, accepts later correlated receipt evidence and
   cannot resend.
2. Controlled and external withdrawal sinks could unwind after possible
   transmission before recording a withdrawal outcome. Both paths now reserve
   `WithdrawalOutcome::Requested` before sink entry. A caught unwind makes no
   prevention claim and fences a duplicate withdrawal.

Focused regressions exercise interruption with `catch_unwind`. These fixes use
existing accepted states and do not add authority, alter public semantics or
claim that panic recovery is a production process-crash durability mechanism.

## Gap ledger

Every open item is assigned exactly one class for this audit.

### M6_BLOCKER

None after the interruption-boundary fixes and closing validation.

### BACKEND_SPECIFIC

- exact send-boundary, receipt, assignment, Serving, cancellation/prevention,
  destination and physical-satisfaction mappings for a selected backend;
- transport idempotency/correlation and any evidenced reassignment policy;
- controller-specific freshness, reconnect and reconciliation behavior;
- CiA 417 operation correlation and the exact profile encoding used by a chosen
  controller.

### M7_ENTRY_REQUIREMENT

- an approved concrete gateway/backend and first non-passenger environment;
- authenticated host identity mapped explicitly to the existing Principal and
  grant model;
- a minimal backend capability/ownership declaration, including explicit
  rejection of overlapping physical authority coverage;
- least-privilege process/network isolation and fail-closed readiness;
- durable unresolved-operation recovery and controller query/reconciliation;
- backend-specific physical completion and stale-observation policy;
- request-to-observation audit correlation sufficient for the approved
  environment.

### M8_HARDENING

- production retention/export for command diagnostics and audit evidence;
- supervision, upgrade/rollback, release signing and long-term telemetry;
- production TLS/auth/origin/rate-limit policy and public compatibility work.

### FUTURE_EVIDENCE

- positive KONE receipt, allocation, Serving, terminal and prevention mapping;
- HIL, bench-controller and controlled non-passenger lifecycle/soak evidence;
- real controller/OEM behavior, real CAN and physical safety behavior;
- additional backend families where they test a distinct hypothesis.

An equivalent repeat of the current KONE Sandbox conditions is not scheduled.
A future run needs a concrete falsifiable hypothesis or changed official
capability that can supply the missing lifecycle evidence.

## Private evidence retention

The final M6B.3 and M6C.2 Evidence v0 bundles were available during this audit.
They were copied to a user-private archive outside Git with directory mode 0700
and file mode 0600. Per-file SHA-256 manifests verify every retained evidence
file, and credential/token-bearing header, URL and value scans passed. The raw
bundles, credentials and host-specific archive paths are not committed. This is
bounded retention, not a new evidence service or public format.

## Architecture pollution audit

- Core still depends only on model and contains no ROS/RMF, CAN, KONE, HTTP,
  wall clock, filesystem or network knowledge.
- Engine still depends only on Core/model/runtime and contains no vendor,
  transport, HTTP, filesystem or clock policy.
- Observation authority remains independent from write authority. Auxiliary
  evidence cannot dispatch, reconcile or fail over.
- `CallId`, `IntentId`, Principal/grant, controller identity, source session,
  observation sequence and client revision retain separate meanings.
- Active state follows current configured work. Controlled history, Core
  completed calls, external completion and external prevention each retain a
  bounded recent window; no M6 correctness set grows with all historical
  operations.
- No generic gateway, lifecycle/health framework, plugin ABI, broker, database,
  confidence score, automatic failover or public write API was added.

## Planning-only M7 entry proposal

This proposal records the next HUMAN decision surface. It is not M7 approval or
implementation.

### Gateway boundary

Use one concrete backend-specific gateway outside model/Core/Engine. The
gateway owns transport, credentials, wall clock, controller correlation and
durable recovery. Engine remains the deterministic semantic authority. The
first gateway must not add a generic plugin SDK, broker or public write route.

### Reuse existing authority

Map an authenticated gateway/operator identity to the existing Principal plus
per-car `AuthorityGrant` or external-scope `ExternalDispatchAuthorityGrant`.
Do not create a parallel vendor authorization model. Credential possession and
controller connectivity remain insufficient to grant LiftAssistant authority.

### Minimal backend capability shape

Declare only evidenced facts: ownership mode (LiftAssistant-selected or
external-dispatch), supported high-level operations, explicit floor/scope,
delivery and receipt correlation, withdrawal/prevention support, assignment
mapping when applicable, observation source used for reconciliation, and
whether unresolved state can be queried after reconnect. This is a concrete
configuration boundary, not capability negotiation or a universal DSL.

### Isolation and failure behavior

Run the concrete gateway in a narrow, least-privilege process/network boundary
with an explicit controller allowlist. Unavailable or stale gateway state blocks
new commands. Uncertain delivery never triggers automatic resend. Observation
loss blocks new work and physical completion. Reconnect must query/reconcile the
same operation identity. Revoked authority fences commands while correlated
evidence for already-sent work remains eligible. Unresolved remote work stays
visible and blocks incompatible work.

### First controlled environment and gates

Start with one HIL or bench-controller environment whose high-level command and
observation semantics can be correlated without passenger service. Require a
HUMAN architecture/environment decision before implementation and a separate
HUMAN gate before any controlled non-passenger or real-controller execution.
Passenger-production writes, raw motion/safety primitives, public command APIs,
automatic failover and certification claims remain out of scope.

## Closing validation

- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: PASS.
- `cargo test --workspace --locked`: 166 passed, 1 expected ignored, 0 failed.
- Rust/Cargo 1.85.0 locked workspace/all-target check: PASS.
- All nine existing TUI scenarios: PASS.
- All four existing non-live Headless scenarios: PASS.
- Dependency and resource-bound scans: PASS; no Cargo dependency changed.
- Private evidence checksum, permission and credential-pattern checks: PASS.

No live Open-RMF, KONE, real CAN, controller or equipment run was repeated in
this closure pass. The accepted external evidence retains its original scope.

## Recommendation

The generic high-level write semantics have implementation evidence,
deterministic regression and accepted isolated external challenges without
using passenger production as the experiment. Remaining gaps belong to a
concrete backend, the M7 gateway gate, later hardening or future evidence rather
than an unresolved M6 semantic contradiction.

**RECOMMEND_M6_COMPLETE. Awaiting HUMAN milestone-exit and M7-entry decision.**
