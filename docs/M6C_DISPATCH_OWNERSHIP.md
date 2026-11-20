# M6C dispatch ownership contract

M6C.0 and M6C.1a passed HUMAN architecture/quality review. The internal,
reversible M6C.1b lifecycle and recovery proof is authorized.

This contract records the smallest conclusion supported by the accepted M6B.3
KONE Sandbox evidence and the targeted prior-art review. It is not a public
schema, production adapter contract, or authorization for live elevator work.

## Independent ownership axis

Dispatch ownership is independent from an `Intent`:

~~~text
LiftAssistant-owned
HallCall -> LiftAssistant selection -> CarId -> ServeFloor -> execution

External-controller-owned
HallCall -> external request -> Pending
         -> native assignment evidence -> explicit mapping
         -> Assigned(CarId)
~~~

The existing `Intent::ServeFloor { car, floor }` and `Intent::ParkAt { car,
floor }` remain car-specific. They are used only when LiftAssistant owns car
selection. M6C does not add an unknown/optional car or a vendor-specific intent.

The external boundary starts with the existing `HallCall` and `CallId`. It must
not manufacture a car, create an `Intent`, or pass through `ControlledIntentSink`.
Successful local submission leaves the call `Pending`; submission is not
assignment, service, completion, or physical reconciliation.

## Assignment evidence and identity

An external controller may report a source-native assigned entity after
submission. That opaque identity remains controller context. Only an explicit,
configured mapping in the dispatch scope may resolve it to a logical `CarId`.
List position, integer/deck value, name similarity, credentials, sessions and
transport identities never create a logical car.

For M6C.1a:

- a mapped assignment advances `Pending -> Assigned(CarId)`;
- an unconfigured identity returns a typed rejection and leaves the call pending;
- an identical repeated assignment is idempotent;
- a different later assignment is an explicit conflict and cannot overwrite;
- evidence for a terminal, retired, wrong-scope or wrong-controller operation
  cannot mutate or resurrect the call;
- assignment alone cannot advance to `Serving` or `Completed`.

Reassignment policy is deliberately absent because no accepted evidence defines
one.

## Dispatch-scope authority

External submission requires a separate current grant proving that one
LiftAssistant `Principal` may submit to one configured external dispatch scope
through one configured `ControllerId`. The grant is explicit, scope-bound,
revocable and fenced when replaced. It does not name or borrow authority from a
future assigned car.

Existing per-car `AuthorityGrant` semantics remain unchanged and cannot
authorize this group-level path. OAuth credentials, native group IDs, successful
connections, request/session IDs and vendor acknowledgements confer no
LiftAssistant authority.

## Lifecycle separation

M6C.1a reused the established call lifecycle through `Assigned` and kept
external delivery state local to the new boundary. It does not reinterpret the
existing controlled-intent `Receipt`, `WithdrawalOutcome`, or
`Reconciliation`.

The following M6B.3 findings remain deferred constraints:

- KONE `success: true` has no approved mapping to controlled-intent receipt;
- `being_served` and `served` are native lifecycle evidence, not canonical
  service or physical satisfaction;
- delete acknowledgement does not prove cancellation or physical prevention;
- session resume proves neither exactly-once delivery, replay, freshness,
  ordering nor LiftAssistant observation incarnation;
- call-submission idempotency was not established;
- `allocated_lift_deck` has no production-safe mapping to monitoring identity or
  `CarId`;
- destination-call origin/destination, door/landing side and journey semantics
  are not added to Core.

## M6C.1b external dispatch lifecycle and recovery

- A HallCall duplicate reuses its active identity without sending. An explicit
  retry may send the same request and CallId only after proven `NotSubmitted`,
  using the current grant, scope and configured controller. `Submitted` and
  `Indeterminate` are fenced against resend, including after virtual restore.
  Engine reserves external ownership and uncertainty before entering the sink.
  An interrupted send that unwinds to the caller retains that fence.
- Correlated controller receipts record Accepted, Rejected or Indeterminate
  independently of delivery. An identical receipt is idempotent. A definitive
  disposition may resolve Indeterminate. A late Indeterminate cannot erase a
  definitive disposition, and conflicting definitive receipts are reported.
  A Rejected receipt records the controller's disposition. Correlated
  assignment and service evidence remain independently observable.
- Assignment requires the configured native mapping. Only the same native
  assignment is a duplicate. Separate serving evidence must match that
  assignment and its call, scope and controller. Only then may `Assigned`
  advance to `Serving`. Non-submitted operations have no eligible remote
  evidence. Evidence for an already-sent operation remains eligible after
  command authority is revoked.
- Physical completion uses the shared call registry's arrival/open-door
  semantics. The assigned car needs fresh, resolved Operational position and
  motion at the exact requested floor, stopped, and fresh open-door evidence.
  A post-serving observation of travel or of a closed door at the target
  distinguishes current service from an already open, idle snapshot. Evidence
  from before serving, before observation invalidation or before virtual
  restore cannot satisfy completion. Door evidence must be observed at or
  after the position and stopped evidence. A shared car/floor sample leaves
  multiple matching serving calls unresolved. Vendor terminal states and legacy
  uncorrelated car-specific feedback carry no external completion authority.
- Withdrawal rechecks the current grant, scope and controller. Requested
  and Unsupported keep the operation active and observable. A proven Prevented
  before serving retires the operation and removes its active call without
  recording physical completion. A prevention claim after serving conflicts
  with observed progress. An already recorded withdrawal outcome suppresses
  repeat commands in this proof.
- Physical completion and proven prevention remove active external records.
  Each terminal outcome retains up to 128 recent tombstones with call identity
  and dispatch scope. After
  eviction, delayed evidence can become Unknown and still cannot recreate a
  call. The existing CallId allocator never rewinds in virtual recovery.
- Consume-and-restore checkpoints preserve logical work, delivery uncertainty,
  receipt, assignment, serving and retirement. Restore fences grants and
  invalidates physical observation evidence. It performs no controller I/O.

## Current limits

M6C is a deterministic in-memory Engine proof. It has no KONE or other vendor
type, network transport, credential, public write API, production adapter,
database, broker, plugin framework, simulator claim, physical completion claim,
or M7 implementation. Vendor receipt, service and withdrawal mapping, native
allocation correlation, live reconnect and durable recovery require later
evidence and HUMAN review.
