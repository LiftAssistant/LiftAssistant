# M6C.2 — KONE landing-call Sandbox validation

M6C.0, M6C.1a and M6C.1b are HUMAN-accepted. This document freezes the bounded
M6C.2 experiment before any live landing call is issued. It is an evidence
contract, not a product adapter design or permission to alter the accepted
generic external-dispatch model to fit KONE.

## Scope and stop boundary

The only live environment is the official KONE Sandbox exposed to one approved
Sandbox application. The operation is an Elevator Call API v2 **landing call**
selected from current inventory, configuration and actions/capabilities. The
harness may use only official Sandbox resources returned to that application.
It must not contact production/commercial resources, real elevators, passenger
equipment or controller cabinets.

The validation uses a reversible, validation-only transport harness outside the
product runtime. Sanitized correlated evidence is applied through the real M6C
Engine APIs. No KONE dependency enters model, Core or Engine, no production
adapter or public write route is added, destination-call Core semantics remain
out of scope, and M7 does not begin.

Stop early only when approved credentials are unavailable, the current Sandbox
exposes no usable landing-call action, commercial/production/real-equipment
access is required, or evidence requires a fundamental generic
Core/authority-model redesign. A semantic mismatch does not stop independent
evidence collection.

## Source provenance

Primary authority is the current official KONE API Portal and live official
Sandbox behavior. The official `konecorp/kone-api-examples` repository is pinned
independently at commit `e597b46aac8870e6e85ee2b842abbeb36471ac74` and tree
`149a6c4ca201eb2f5a6855d6a83f0d1607dfed69`. The pinned README states the
high-level distinction between source-plus-destination and
source-plus-direction calls, but it is not authority for the current landing
payload. Current configuration, action descriptors, actual responses and live
lifecycle events decide the experiment's wire mapping. Any disagreement is
retained rather than reconciled silently.

## Fixture discovery and mapping

The harness first authenticates for minimum inventory access, re-discovers all
Sandbox buildings/groups and audits their current configuration/actions. It
prefers the same logical group selected by M6B.3 when it still exists and has an
enabled direct landing-call action. Otherwise eligible groups rank by:

1. at least two lifts, then lift count;
2. monitoring coverage;
3. number of valid normal landing/direction fixtures;
4. stable lexical resource identity.

The chosen source area must map explicitly through current configuration to one
configured LiftAssistant `Floor`; the selected native UP/DOWN action must map
unambiguously to `Direction::Up` or `Direction::Down`. Area, side, terminal,
action and native lift/deck values remain vendor-local. Names, numeric
coincidence, list order and topic order never create `CarId`.

Prefer a normal landing/direction served by multiple lifts and, when passive
monitoring can establish it safely, one where no eligible lift already occupies
the landing. This is a preference, not permission for repeated calls.

## Call and time budget

The default and intended maximum is **two normal landing calls**:

- Call A: construction, external reservation, delivery, native response,
  lifecycle, assignment investigation, serving, independent Operational
  evidence, native terminal state, one controlled disconnect/resume and
  LiftAssistant physical completion evaluation.
- Call B: one supported delete/cancel attempt at the earliest useful safe point,
  followed by bounded lifecycle and physical observation.

A third call is forbidden unless one precise unresolved required case is
documented before sending. There are no automatic network retries and no call
resubmission after `Submitted` or `Indeterminate`. A same-`CallId` retry is
eligible only if the harness proves `NotSubmitted`; otherwise the live retry
case remains unexercised. Every network operation and observation window is
bounded and the protocol log retains at most 512 sanitized messages.

## Evidence boundaries

Record these as separate causal milestones:

~~~text
HallCall created
-> external request reserved
-> socket send attempted
-> delivery result
-> native API/action response
-> receipt interpretation
-> first native call state
-> native assignment evidence
-> configured assignment mapping, if authoritative
-> separate Serving evidence
-> fresh Operational evidence
-> native terminal lifecycle
-> LiftAssistant physical completion, if its predicate is satisfied
~~~

`send`, action success, `Receipt::Accepted`, assignment, Serving, native
`served`, and physical `Completed` are never synonyms. A native response maps to
`Accepted` only when official semantics plus correlated live response establish
controller acceptance rather than request processing. Assignment needs an
official relationship from allocated native identity through configuration and
monitoring identity to an explicitly configured logical `CarId`. Serving needs
separate, exact-call lifecycle evidence after that assignment. Native `served`
cannot complete a HallCall; only fresh canonical Operational evidence after
Serving may exercise the existing M6C.1b completion predicate.

The controlled reconnect uses the same native KONE session. It records the new
connection, resume request/acknowledgement, replay/duplicates, fresh
subscriptions and later lifecycle independently. It must not resubmit Call A.
Native session identity stays vendor-local.

A delete/cancel acknowledgement maps to `WithdrawalOutcome::Prevented` only if
it proves execution will not start. Otherwise the result remains Requested,
Unsupported or insufficient/indeterminate, and later assignment/Serving remains
eligible observed truth.

## Private Evidence v0 and secret policy

The seven-file bundle is written outside Git under a private temporary
`evidence/m6c2/<run-id>/`-equivalent directory:

~~~text
manifest.json
verdicts.json
events.jsonl
commands.txt
stdout.log
stderr.log
protocol.log
~~~

`events.jsonl` retains bounded causal events; sanitized raw messages belong in
`protocol.log`. Stable aliases replace vendor resource identifiers where useful.
Credentials, bearer tokens, Authorization headers, OAuth response bodies and
token-bearing URLs are forbidden from source, Git, commands, stdout/stderr,
Evidence v0 and hosted CI. Credentials enter the one-shot process only through
the already approved hidden local input channel and are cleared after use.

## Frozen A--L matrix

Each row ends in `PASS`, `FAIL`, `ERROR`, `BLOCKED` or `PARTIAL`. A negative or
partial finding is valid evidence and must not be forced green.

| Case | Required evidence |
| --- | --- |
| A — Provenance & hygiene | Official pin, current API/Sandbox provenance, harness/runtime revisions and secret scans |
| B — Sandbox capability | Live discovery of the exact enabled landing-call action and payload fields |
| C — HallCall mapping | Explicit configured floor and UP/DOWN mapping with no guessed identifier |
| D — Delivery & receipt | Separate request reservation, send/delivery and controller disposition |
| E — Assignment | Authoritative native allocation/configuration/monitoring correlation, or preserved inability |
| F — Serving | Separate correlated assignment-to-Serving evidence, or preserved inability |
| G — Physical completion | Fresh Operational evidence through the actual M6C.1b predicate; native terminal kept separate |
| H — Native terminal | Exact observed lifecycle ordering and absent states retained |
| I — Withdrawal | Delete/cancel construction, acknowledgement, later lifecycle and conservative outcome |
| J — Reconnect/resume | Controlled recovery, replay/duplicate findings and explicit proof of no call resubmission |
| K — Identity/authority | Explicit audit of every native identity against LiftAssistant identities/grants |
| L — Evidence/regression | Complete bounded Evidence v0 and the smallest deterministic Engine-API regression for actual findings |

## Claims deliberately excluded

This run cannot establish real or production KONE behavior, physical
door/motion accuracy, passenger safety, broad KONE compatibility, production
reliability, cancellation guarantees, certification or conformance. Hosted CI
remains secret-free and does not run live KONE tests.

## Executed official Sandbox validation

Run `2026-11-20T011559Z` completed the two-call budget against the current
official Sandbox. The official examples pin was independently rechecked at
commit `e597b46aac8870e6e85ee2b842abbeb36471ac74`, tree
`149a6c4ca201eb2f5a6855d6a83f0d1607dfed69`. The current KONE Portal describes
real-time call and assigned-elevator information, while the pinned README says
landing calls use a source and direction. Neither public source supplied the
current exact landing payload. Live actions were therefore authoritative for
this run.

All five inventory candidates returned current configuration and actions. Each
exposed direct enabled LandingCall descriptors `LdgCall UP`/action `2001` and
`LdgCall DOWN`/action `2002`. The same nine-lift logical fixture class selected
in M6B.3 again won by lift count, then monitoring richness and valid landing
count. Its configured building floor universe was 1--60. The deterministic
fixture mapped source short name `2`, side `1`, and configured group-floor `2`
to LiftAssistant `Floor::new(2)`. Nine configured lifts could serve both normal
directions there. Vendor area, group, terminal, request and session identities
remain private aliases and did not become LiftAssistant identities.

### Call A — delivery, response and resume

The validation-only Rust bridge invoked the real M6C Engine sink. Engine
reserved external ownership for `CallId #1` before the harness attempted the
UP WebSocket send. Send completed and the sink returned `Submitted`. The
correlated native response contained `success: true` and a call session, but no
explicit controller-acceptance disposition. The bridge therefore recorded
`ExternalDispatchDisposition::Indeterminate`; the mapping result is
`NO_RECEIPT_MAPPING`, not Accepted.

No correlated call-state event arrived in the first eight-second window. The
first connection was then closed deliberately. A new connection resumed the
same KONE session and received `ok`/201. The requested 30-second latest-state
lookback supplied no replay or duplicate during the bounded five-second replay
observation window. A fresh call-state subscription then produced no event
during the separate bounded 90-second window.
The Engine status stayed `Pending`, with one submission, `Submitted` delivery
and Indeterminate receipt. Reconnect did not resubmit.

The independent monitoring subscription retained 22 position and 40 door
messages across the nine native lift topics. One native topic showed a complete
closed/opening/opened/closing/closed door cycle and stopped position changes.
However, the call stream supplied no allocation, `liftId`, deck mapping,
Serving state or native terminal event. Those monitoring changes could not be
correlated authoritatively to Call A. No `CarId`, canonical Operational state or
physical completion was fabricated. The already-at-floor edge therefore
remained indeterminate rather than demonstrating a completion-predicate
mismatch.

### Call B — conservative withdrawal

The same bridge reserved `CallId #2` before the DOWN send. Send completed and
the correlated `success: true`/session response again mapped only to an
Indeterminate receipt. The official delete request was sent once at the first
useful point and Engine recorded `WithdrawalOutcome::Requested`. No correlated
delete acknowledgement arrived in 20 seconds, and no call-state, assignment,
Serving, cancellation or terminal event arrived in the following 30-second
window. Call B correctly remained Pending and observable. The absence of proof
was not promoted to `Prevented`.

### A--L terminal matrix

`PASS` means the bounded case was executed and its positive or negative result
was preserved. `PARTIAL` is an evidence result, not a test-infrastructure
failure.

| Case | Verdict | Evidence-backed conclusion |
| --- | --- | --- |
| A — Provenance & hygiene | PASS | Official pin/tree, current Portal, Sandbox provenance, harness/bridge hashes, bounds and secret scans are retained privately. |
| B — Sandbox capability | PASS | Live configuration/actions exposed current direct LandingCall UP and DOWN descriptors; no destination call was substituted. |
| C — HallCall mapping | PASS | Configured area/floor/side and live action descriptors established `Floor(2)` plus UP/DOWN without guessing a resource identifier. |
| D — Delivery & receipt | PARTIAL | Both sends were Engine-reserved then Submitted. Both correlated `success: true` responses lacked explicit controller acceptance, so receipt stayed Indeterminate. |
| E — Assignment | PARTIAL | No allocation event or exact official allocation/configuration/monitoring correlation arrived; no `CarId` was fabricated. |
| F — Serving | PARTIAL | No correlated native Serving lifecycle arrived, so neither call advanced from Pending. |
| G — Physical completion | PARTIAL | Uncorrelated position/door changes were retained, but no assignment/Serving/complete Operational evidence existed; native or LiftAssistant completion did not occur. |
| H — Native terminal | PASS | The exact observed lifecycle is empty for both calls; all absent states and terminal absence are retained. |
| I — Withdrawal | PASS | One delete send completed; no acknowledgement or terminal evidence arrived. Engine conservatively retained Requested rather than Prevented. |
| J — Reconnect/resume | PASS | Resume returned `ok`/201, no replay/duplicate was observed, and Engine submission count proved no resubmission. |
| K — Identity/authority | PASS | OAuth, resource, request, KONE session, connection, monitoring and possible allocation identities stayed vendor-local and conferred no LiftAssistant authority. |
| L — Evidence/regression | PASS | Seven bounded private files contain 52 causal events and 88 sanitized protocol messages with no drops. A focused Engine regression preserves Submitted + Indeterminate + Pending, no resume resend, and Requested-withdrawal behavior. |

Overall classification is **PARTIAL_MAPPING**. This does not reveal a generic
M6C semantic contradiction. The conservative model accurately retained remote
uncertainty and refused to invent receipt acceptance, assignment, Serving,
prevention or physical completion. The run also did not establish those later
lifecycle mappings positively, so it is not a production adapter proof.

The private Evidence v0 bundle remains outside Git. Its run retained seven
files, 52 causal events and 88 protocol messages with zero drops; normal/error
redaction and exact credential/token scans passed. The live harness source hash
was `0d7683e8446592ec914c068177c935a0c4a14422be5fd1df2d4f40ea4ab0eff8` and
the validation bridge source hash was
`e8701f73ac5d9605d0eacfc352f39ad9e9fed51f0d48ac0f1f403f960e95e40a`.

No `Intent`, `HallCall`, `CallState`, authority, receipt, delivery, assignment,
Serving, withdrawal or completion semantic changed. Core and Engine gained no
KONE, network, wall-clock or filesystem dependency. No production adapter,
public write path, destination-call Core expansion, M6C.3 or M7 work began.
