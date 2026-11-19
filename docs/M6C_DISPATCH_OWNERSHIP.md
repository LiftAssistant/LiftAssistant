# M6C dispatch ownership contract

Status: M6C.0 semantic freeze for the internal, reversible M6C.1a proof.

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

M6C.1a reuses the established call lifecycle only through `Assigned` and keeps
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

## Current limits

M6C.1a is a deterministic in-memory Engine proof. It has no KONE or other vendor
type, network transport, credential, public write API, production adapter,
database, broker, plugin framework, simulator claim, physical completion claim,
or M7 implementation. Serving, completion, cancellation, receipt mapping,
reconciliation, reconnect and durable recovery require later evidence and HUMAN
review.
