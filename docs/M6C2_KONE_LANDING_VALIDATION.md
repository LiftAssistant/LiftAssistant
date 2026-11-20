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
