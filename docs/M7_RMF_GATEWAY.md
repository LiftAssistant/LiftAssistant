# M7 Open-RMF intent gateway

Status: implemented for software/simulation review on 2026-11-20. This is a
private, experimental process boundary. It is not approved for passenger
equipment, a real controller, a public write API, certification, or deployment.

## Boundary

The first M7 path is intentionally concrete:

```text
Unix peer UID
    -> configured PrincipalId -> fresh Engine AuthorityGrant
    -> car-specific Intent::ServeFloor / IntentId
    -> durable unresolved operation
    -> Open-RMF /lift_requests

Open-RMF /lift_states
    -> existing RMF mapper -> canonical Operational observations
    -> Engine receipt/reconciliation -> durable terminal cleanup
```

Engine still owns authority, intent identity, receipt meaning, canonical
observation authority, and physical reconciliation. The gateway owns the local
process ingress, RMF transport/session mechanics, native correlation, explicit
logical/native/floor mapping, source-specific payload supervision, reconnect,
private readiness, and the bounded restart record. ROS participant identity,
RMF holder/session identity, connectivity, and successful publication do not
grant authority and do not establish physical completion.

The gateway supports exactly one configured logical car and
`Intent::ServeFloor`. Its capability declaration is car-specific and
LiftAssistant-owned, lists the configured floors and observation source,
declares holder correlation, declares that withdrawal is unsupported, and
declares reconnect without resend. It is not a generic adapter or capability
framework.

## Private ingress and configuration

`liftassistant-rmf-gateway` accepts only:

```bash
cargo run -p liftassistant-rmf --bin liftassistant-rmf-gateway -- \
  --config /private/path/rmf-gateway.json
```

A minimal non-secret shape is:

```json
{
  "socket_path": "/run/user/1000/liftassistant/rmf-gateway.sock",
  "state_path": "/var/lib/liftassistant/rmf-gateway-state.json",
  "config_revision": "site-a-rmf-v1",
  "allowed_uid": 1000,
  "principal": "site-a-orchestrator",
  "domain_id": 47,
  "source_id": "site-a-rmf-gateway",
  "logical_car": "A",
  "native_lift": "Lift1",
  "floors": {
    "L1": 1,
    "L2": 2
  },
  "max_payload_silence_ms": 4000
}
```

This example is not a deployment recommendation and contains no credential.
The operator must choose private paths, permissions, identities, topology, and
an evidence-backed RMF payload-silence interval. Configuration is fail-closed,
bounded, and rejects unknown fields. The local Unix socket is mode `0600`, each
request is limited to 8 KiB with bounded I/O time, and `SO_PEERCRED` supplies
the kernel-authenticated UID. That UID must exactly match the configured caller
mapping before the gateway uses the configured `PrincipalId` and a fresh,
process-local Engine grant. Grants are never restored from disk.

The ingress protocol currently accepts one private JSON request shape:

```json
{"version":1,"operation":"serve_floor","logical_car":"A","floor":2}
```

It is not a public or stable schema. The existing Headless HTTP/SSE API, CLI,
and Flutter client remain read-only and expose no route to this socket.

An advisory lock adjacent to the socket prevents two gateway processes from
covering the same configured ingress. A crashed process releases the kernel
lock; its successor may then remove only the stale socket at that exact path.
This is local single-instance fencing, not a distributed lock or permission to
operate a second competing control path for the same physical lift.

## Durable unresolved operation

The private JSON store contains only a version, configuration revision,
monotonic `last_intent_id`, and at most one unresolved operation. An unresolved
record contains the immutable logical/native target and identities plus the
conservative delivery/correlation and reconciliation evidence needed after a
restart. It does not contain grants, credentials, bearer tokens, ROS identity,
or an observation source incarnation.

Before publication the gateway creates a fresh Engine `IntentId`, commits the
unresolved record through a new mode-`0600` temporary file, `fsync`s it,
atomically renames it, and `fsync`s the parent directory. Only then may the RMF
publisher run. State and request payloads are size-bounded. Symlinks,
group/world-accessible state, non-regular files, unknown/incompatible versions,
configuration-revision mismatches, malformed identities, and oversized or
unreadable state fail closed rather than starting with an empty history.

The crash contract is conservative:

| Window | Restart result |
| --- | --- |
| before durable reservation | no unresolved operation and no send |
| after reservation, before send | same operation remains uncertain; no automatic send |
| during/after a possible send | same operation remains uncertain; no automatic resend |
| after correlated receipt | same operation and correlation evidence are restored for observation-driven reconciliation |
| during withdrawal | unsupported in this slice, so no withdrawal command can be issued |

This is not exactly-once delivery. A possible external side effect always
survives as uncertainty. Startup restores the same operation identity and
blocks incompatible new work with `recovery_required`. A new process creates a
fresh observation SourceSession and a fresh grant but does not create a
replacement Intent and does not publish the unresolved request again.

## Observation, readiness, and recovery

The gateway consumes `/lift_states` through the existing explicit
`lift_name -> CarId` and native-floor mapping. Only canonical Operational
observations can establish receipt correlation and physical reconciliation.
Command transport state never overrides the observed state. Unknown native
lifts are reported and isolated; configured mapping failures invalidate the
configured car rather than fabricate state.

Private readiness distinguishes startup/synchronization, ready, degraded, and
recovery-required conditions. It is not a global health framework. A command
requires current complete Operational evidence, the current grant, the
configured controller/capability/floor, an available RMF request subscriber,
and no prior unresolved operation. Observation loss invalidates Operational
truth, blocks admission and completion, and preserves unresolved work.

Command publisher reconnect and observation subscription run independently.
Both use bounded retry/backoff. Reconnect keeps the same unresolved operation
and never resends it. A subscriber generation that receives no first payload
within the configured source-specific interval is recreated without changing
the adapter SourceSession or declaring payload recovery. If payload had flowed
earlier in the process lifetime, the loss also revokes Operational truth. Only
a real new payload restores it. This distinction was required by a live Hotel
case where DDS graph discovery succeeded but a newly created RustDDS reader
received no data.

## Deterministic and external evidence

Focused unit and independent-child-process tests cover caller/grant/floor/
observation/backend rejection, mapping isolation, state corruption and config
revision mismatch, crash windows, bounded state, local singleton coverage, and
restart recovery with one captured backend send. The child process is killed
after a possible send; the next process restores `IntentId(1)`, replies
`recovery_required`, consumes correlated canonical observations, clears the
record, retains the watermark, and leaves the send log at exactly one entry.

The external proof used the official Open-RMF Hotel simulation image pinned at
`sha256:601a6bb7c04d559fb9ec654d9b7a41650918990e35712036d836f072a6430db5`
on an isolated ROS domain. It demonstrated this path:

```text
kernel-authenticated local caller
  -> configured Principal / fresh grant
  -> IntentId(1) ServeFloor(A, L2)
  -> durable reservation
  -> real /lift_requests publication
  -> Hotel Lift1 movement/holder/door/floor evidence
  -> canonical Operational observations
  -> Engine reconciliation
  -> durable unresolved cleanup
```

A second run killed the real gateway after `IntentId(1)` had reached correlated
progress. The successor started `RecoveryRequired`, did not republish, recovered
the same identity from new canonical observations, and cleared the record. The
next accepted operation used `IntentId(2)`. Separately, pausing Hotel produced
repeated first-payload generation timeouts; unpausing it caused a later
generation to ingest Lift1 without changing the source lifetime. An unconfigured
Lift2 continued to be isolated and produced only one diagnostic per process.

A deliberately late restart supplied an important negative result: Hotel had
already cleared the holder and reached the target before the new subscriber
observed progress. A target-floor snapshot alone was insufficient, so the
gateway correctly retained `RecoveryRequired` indefinitely instead of guessing
completion or resending. This is an evidence limit, not a successful recovery
claim. A backend with no replayable correlation may require an explicit future
recovery decision, but that policy is not authorized here.

The proof used only software processes and the official simulator. It accessed
no passenger elevator, real controller, CAN/CANopen interface, OEM endpoint,
OPC server, or site network, and it establishes no HIL, safety, production, or
certification claim.

## Remaining review boundary

M7.1a deliberately leaves these questions to HUMAN review or a later bounded
slice:

- operational packaging, service supervision, filesystem ownership, and
  deployment hardening;
- an approved explicit retirement/operator-recovery path when RMF correlation
  is no longer observable;
- HIL/bench-controller evidence and a second concrete backend;
- public control clients, withdrawal, multiple controlled cars/processes,
  cross-host authority coverage, and real credentials;
- any passenger or real-controller write authorization.

No later item is implied by completion of this software/simulation slice.
