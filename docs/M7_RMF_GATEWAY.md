# M7 Open-RMF intent gateway

Status: M7.0 + M7.1a are HUMAN-accepted. M7.1b explicit recovery, retirement
and safe resumption are implemented and deterministically validated; the
required official-Hotel late-restart rerun remains pending. This is a private,
experimental process boundary. It is not approved for HIL, passenger equipment,
a real controller, a public write API, certification, or deployment.

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
  "recovery_operator": {
    "allowed_uid": 1000,
    "principal": "site-a-recovery-operator"
  },
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

`recovery_operator` is optional so an existing installation fails closed with
no recovery permission. When present, both its peer UID and explicitly named
Principal must match a recovery request. Ordinary command permission does not
imply recovery permission. A recovery Principal is not installed as the
ordinary command Principal and recovery does not create a command grant. If
several applications share one Unix service UID, `SO_PEERCRED` cannot
independently distinguish them; separate UID ownership or an explicitly
accepted shared-peer context is required. This limitation is why the private
socket is not a public control API or a general authentication system.

The ingress protocol accepts the ordinary private request:

```json
{"version":1,"operation":"serve_floor","logical_car":"A","floor":2}
```

It is not a public or stable schema. The existing Headless HTTP/SSE API, CLI,
and Flutter client remain read-only and expose no route to this socket.

The same bounded private socket supports three recovery-only operations. Each
requires the configured recovery Principal:

```json
{"version":1,"operation":"inspect_recovery","principal":"site-a-recovery-operator"}
{"version":1,"operation":"retire_unknown","principal":"site-a-recovery-operator","reason":"operator-reviewed outcome remains unknown"}
{"version":1,"operation":"resume_control","principal":"site-a-recovery-operator","reason":"resume current control while preserving historical uncertainty","accept_residual_uncertainty":true}
```

Inspection returns the operation identity and immutable car/floor, RMF holder
correlation, durable delivery and reconciliation progress, current private
readiness, current canonical Operational snapshot, observable current holder,
configuration revision, commissioning/overload state and bounded recent audit.
It contains no backend credential, grant or ROS identity.

A socket-adjacent lock still protects exact ingress cleanup. Independently, an
owner-only advisory lock derived from `RMF domain_id + native_lift` fences the
actual local control domain, so two processes cannot cover the same lift through
different socket or state paths. Different native control domains can coexist,
and a crashed process releases the kernel lock. The stable lock filename is only
a local identifier; its file also stores and verifies the full domain string,
failing closed on a collision. This does not solve cross-host split brain, which
remains a HIL/deployment gate.

## Durable unresolved operation

The version-2 private JSON store contains a configuration revision, monotonic
`last_intent_id`, at most one unresolved operation, at most one
resumption-required record, and at most 32 retirement/resumption audit records.
Version-1 M7.1a state is read conservatively and upgraded on its next mutation.
An unresolved record contains the immutable logical/native target and identities
plus conservative delivery/correlation and reconciliation evidence. A recovery
audit records the action, Principal, configuration/control domain, prior durable
progress, current canonical snapshot, observable holder, bounded operator reason
and whether remote inactivity remained uncertain or was explicitly accepted.
It does not contain grants, credentials, bearer tokens, ROS identity, or an
observation source incarnation.

Before publication the gateway creates a fresh Engine `IntentId`, commits the
unresolved record through a new mode-`0600` temporary file, `fsync`s it,
atomically renames it, and `fsync`s the parent directory. Only then may the RMF
publisher run. State and request payloads are size-bounded. Symlinks,
group/world-accessible state, non-regular files, unknown/incompatible versions,
configuration-revision mismatches, malformed identities, and oversized or
unreadable state fail closed rather than starting with an empty history.
The stored operation mapping and retired control domain are rechecked against
current configuration even when a revision string was mistakenly reused.

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

Atomic rename plus file and directory synchronization provide crash
consistency, not cryptographic rollback resistance. Normal in-process updates
cannot rewind the watermark or audit sequence. An external actor able to replace
the owner-only state file with an older but syntactically valid copy is outside
this local storage guarantee and requires a stronger deployment trust anchor
before a real-controller decision.

## Observation, readiness, and recovery

The gateway consumes `/lift_states` through the existing explicit
`lift_name -> CarId` and native-floor mapping. Only canonical Operational
observations can establish receipt correlation and physical reconciliation.
Command transport state never overrides the observed state. Unknown native
lifts are reported and isolated; configured mapping failures invalidate the
configured car rather than fabricate state.

Private readiness distinguishes startup/synchronization, ready, degraded,
recovery-required and resumption-required conditions. It is not a global health
framework. A command
requires current complete Operational evidence, the current grant, the
configured controller/capability/floor, an available RMF request subscriber,
and no prior unresolved operation. Observation loss invalidates Operational
truth, blocks admission and completion, and preserves unresolved work.

Configured `available_floors` are commissioning evidence for this concrete
write gateway. The configured native lift must be observed and its current
floor set must match the explicitly mapped served-floor labels before Ready.
Duplicate, missing or foreign labels invalidate that car and keep writes
degraded. This adapter check does not promote RMF floors into generic Engine
execution authority.

`retire_unknown` atomically moves the durable unresolved operation into
`ResumptionRequired`, appends an outcome-unknown audit, and fences the old
process command grant. It does not call Engine completion/prevention/rejection
and cannot make Ready. The old in-memory operation is not rewritten; a normal
gateway restart establishes a new process-local grant while the durable
resumption gate remains closed. `resume_control` separately requires recovery
authorization, the same local control-domain lease and configuration, fresh
complete compatible Operational evidence, available command transport, an
empty current RMF holder, no active operation in the new Engine lifetime, and
an explicit acceptance of residual historical uncertainty. Empty holder is not
recorded as proof of historical inactivity. Successful resumption clears only
the gate and preserves the retired Intent and audit as outcome unknown; the next
accepted Intent must use a greater identity.

Command publisher reconnect and observation subscription run independently.
Both use bounded retry/backoff. Reconnect keeps the same unresolved operation
and never resends it. A subscriber generation that receives no first payload
within the configured source-specific interval is recreated without changing
the adapter SourceSession or declaring payload recovery. If payload had flowed
earlier in the process lifetime, the loss also revokes Operational truth. Only
a real new payload restores it. This distinction was required by a live Hotel
case where DDS graph discovery succeeded but a newly created RustDDS reader
received no data.

The observation channel is bounded. A full queue sets a dedicated fail-closed
signal rather than blocking silently: queued evidence is discarded,
Operational state is invalidated, and the gateway remains degraded until a
fresh compatible payload is ingested. Observation enqueue and command admission
share one narrow local critical section so a newly detected overload cannot be
crossed by a concurrent publish. Ingress processing is also capped per main-loop
iteration to avoid starving observations.

## Deterministic and external evidence

Focused unit and independent-child-process tests cover caller/grant/floor/
observation/backend rejection, mapping isolation, state corruption and config
revision mismatch, crash windows, bounded state, local singleton coverage, and
restart recovery with one captured backend send. The child process is killed
after a possible send; the next process restores `IntentId(1)`, replies
`recovery_required`, consumes correlated canonical observations, clears the
record, retains the watermark, and leaves the send log at exactly one entry.

M7.1b regressions additionally cover distinct recovery permission, wrong
Principal/UID, authorized inspection, outcome-unknown retirement, grant fencing,
restart-stable `ResumptionRequired`, stale/transport/holder/overload resumption
blocks, explicit residual-uncertainty acceptance, no old resend, monotonic next
Intent, configuration contradiction, local control-domain exclusion through
different paths, crash-consistent retirement/resumption windows, and 80 audit
actions retaining only the newest 32 records.

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

A deliberately late M7.1a restart supplied an important negative result: Hotel had
already cleared the holder and reached the target before the new subscriber
observed progress. A target-floor snapshot alone was insufficient, so the
gateway correctly retained `RecoveryRequired` indefinitely instead of guessing
completion or resending. This is an evidence limit, not a successful recovery
claim. A backend with no replayable correlation may require an explicit future
recovery decision. M7.1b implements that policy without changing the negative
result. Its required Hotel recovery/retirement/resumption rerun has not yet been
performed at this document state.

The proof used only software processes and the official simulator. It accessed
no passenger elevator, real controller, CAN/CANopen interface, OEM endpoint,
OPC server, or site network, and it establishes no HIL, safety, production, or
certification claim.

## Remaining review boundary

After the M7.1b official-Hotel rerun and completion audit, the hard stop is a
HUMAN HIL/bench-controller environment decision. Remaining items include:

- operational packaging, service supervision, filesystem ownership, and
  deployment hardening;
- HIL/bench-controller evidence and a second concrete backend;
- public control clients, withdrawal, multiple controlled cars/processes,
  cross-host authority coverage, and real credentials;
- any passenger or real-controller write authorization.

No HIL, real-controller, passenger or later item is implied by completion of
this software/simulation slice.
