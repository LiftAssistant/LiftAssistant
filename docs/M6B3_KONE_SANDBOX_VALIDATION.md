# M6B.3 — KONE Official Sandbox semantic validation

## Scope and access gate

Test whether LiftAssistant's existing controlled-intent, identity, receipt and reconciliation semantics can represent an OEM group controller that chooses the car. Freeze this contract before implementing or running a live probe. A finding that needs a new semantic model goes to HUMAN review.

The approved environment is one explicitly allowlisted building and group in the **official KONE Sandbox**, with simulated resources. Before any live call, confirm the official Sandbox application and the exact simulated building/group through official metadata where available or explicit operator confirmation. Mixed, unknown or production resources are out of scope. Use the exact approved pair throughout. Select areas from the approved group's returned configuration and actions. Real buildings, lifts, controllers, passenger equipment, production resources, commercial onboarding, and spending require separate approval.

Supply the Sandbox application's client ID and secret through an approved external secret source such as process environment. Supply the exact approved building ID and group ID separately. The process environment names for this validation are `KONE_CLIENT_ID`, `KONE_CLIENT_SECRET`, `KONE_BUILDING_ID` and `KONE_GROUP_ID`. The selected WebSocket examples use building/group selectors. The KEN selector used by separate Operational API examples is outside this contract. Verify exact group scope and request only the required inventory access. Avoid wildcard group scopes and discovery calls outside the approved pair. If setup is incomplete, finish only contract and source work and mark live cases BLOCKED.

Credentials and access tokens must stay out of command arguments, logs, errors, Git, fixtures and evidence. The pinned shared helper prints the token response and constructs a WebSocket URL containing the token. Use the official examples as the first behavioral reference where practicable, with credential-safe output and errors checked before any authentication. Unmodified token logging is unsuitable for this run. Preserve sample semantics and record any differences from the Sandbox. [Pinned shared helper](https://github.com/konecorp/kone-api-examples/blob/e597b46aac8870e6e85ee2b842abbeb36471ac74/src/common/koneapi.ts)

## Official source provenance

- The [KONE API Portal](https://dev.kone.com/) and [Elevator Call API overview](https://dev.kone.com/api-portal/elevator-call-api/) provide the official product and API context. The public sources reviewed here do not attest to the setup or entitlement of a specific Sandbox application.
- [KONE's official examples](https://github.com/konecorp/kone-api-examples/tree/e597b46aac8870e6e85ee2b842abbeb36471ac74) are pinned at commit `e597b46aac8870e6e85ee2b842abbeb36471ac74`, committed 2025-11-13 UTC. The official GitHub commit record and a separate external checkout agree on tree `149a6c4ca201eb2f5a6855d6a83f0d1607dfed69`. The checkout remains external to LiftAssistant. The pin uses Authentication v2 and the Elevator WebSocket `stream-v2` route. Actual Sandbox protocol behavior must be observed.
- Review the [pinned Elevator Call API examples](https://github.com/konecorp/kone-api-examples/tree/e597b46aac8870e6e85ee2b842abbeb36471ac74/src/examples/elevator-call-api) for configuration, controller actions, Elevator Call v2, site monitoring, combined call and monitoring, cancellation and session creation/resume. The selected operation is the v2 destination-call example. Its submission identifies a building, group, origin area and destination area and has no car selector. This raises the hypothesis of OEM-owned assignment. The group's effective dispatch semantics, specific-car options, assignment timing and changes require live evidence.
- Track three vocabularies separately. The product overview uses Allocated, Arriving, Served and Canceled. The [pinned types](https://github.com/konecorp/kone-api-examples/blob/e597b46aac8870e6e85ee2b842abbeb36471ac74/src/common/types.ts) include `created`, `approved`, `served` and `cancelled` in `lift-call-state`. They include an optional `liftId` and define an `ok`/error response separately. Capture the exact live event names and ordering before establishing any equivalence.

## Semantic comparison

LiftAssistant's `HallCall` carries a floor and direction. Its `CallId` and `CallState` remain Engine-owned. Controlled `ServeFloor` and `ParkAt` name a car before admission. Admission uses a configured controller, explicit grant, execution capabilities and Operational observations for that car. A delivery receipt and `Reconciliation::Satisfied` are independent outcomes. Satisfaction depends on qualifying fresh Operational evidence of the requested result.

Record request construction, WebSocket send completion, response, first call lifecycle event, assignment, independent monitoring and terminal state as separate evidence. Keep reported Sandbox disposition distinct from LiftAssistant physical reconciliation. Even coherent motion and door events from the approved environment are **SIMULATED OEM SANDBOX EVIDENCE**.

Keep request, session, connection, building, group, lift, deck, area and door identifiers vendor-local. OAuth application identity and a token's safe subject/context establish only the applicable vendor authentication context. They confer no LiftAssistant `CarId`, `CallId`, `PrincipalId`, `AuthorityGrantId`, `IntentId`, `ObservationSessionId`, `ControllerId` or write authority. Any future explicit identity mapping is a separate decision. Keep uncertain delivery and unsupported correlation visible.

Classify each semantic boundary from evidence as CLEAN, PARTIAL, MISMATCH or NOT_APPLICABLE. Keep BLOCKED cases distinct from findings. The overall verdict is CLEAN_MAPPING when the operation fits existing semantics, PARTIAL_MAPPING when some semantics align but ownership or execution remains unfit for a production mapping, or SEMANTIC_MISMATCH when the observed operation contradicts an assumption required by the existing model. A demonstrated mismatch goes to HUMAN semantic review without changing Core. This contract authorizes only a reversible Sandbox experiment. Product runtime, Core and Engine semantics, public write routes, a production KONE adapter and M7 remain outside this scope.

## A–K validation matrix

Each case ends with PASS, FAIL, ERROR or BLOCKED and a reason. Select the fewest bounded Sandbox calls needed to distinguish the cases. Set explicit time and attempt limits before execution and respect published Sandbox rate limits. Leave unsupported behavior explicit.

| Case | Required evidence |
| --- | --- |
| A — Provenance | Verify official source pin, API/version where known, Sandbox app and exact resource provenance. Record LiftAssistant revision, OS/kernel, harness/runtime versions and harness hash, and selected operation. |
| B — Authentication and hygiene | Authenticate using the approved external source. Verify redaction in both normal and deliberate error paths and check Git, commands, output and evidence for credential exposure. Treat unguaranteed redaction as FAIL. |
| C — Configuration and actions | Inspect the approved pair via official configuration and actions queries. Capture sanitized lift membership, floors/areas, decks/doors where available, actions and submission scope. |
| D — Submission and response | Record one bounded call's construction, send, response and first lifecycle event separately. Preserve native correlation in its original context. |
| E — Assignment | Establish whether submission selects a lift, what the effective group scope is, whether and when `liftId` first appears, and what evidence exists for reassignment and specific-car choice. Compare with preselected-car admission. |
| F — Lifecycle and monitoring | Record actual states in order alongside distinct monitoring evidence where available. Evaluate position, direction, door and status evidence against the freshness and independence required for reconciliation. |
| G — Duplicate and retry | Use only documented or otherwise clearly supported, bounded duplicate/retry or resume behavior. Preserve whether a replay is suppressed, rejected, creates work, or remains undefined. Avoid resending calls after ambiguous delivery without a supported safe contract. |
| H — Cancellation and non-success | Use a supported cancellation path or harmless in-scope error case. Preserve exact typed rejection/cancellation/uncertainty. Compare the delivery disposition with LiftAssistant withdrawal, including what physical effect remains unknown. |
| I — Reconnect and resume | Interrupt a supported session once under controlled conditions and inspect resume and any requested latest-state window. Preserve observed duplicate/stale limits. Keep KONE session identity separate from the LiftAssistant ingestion incarnation. |
| J — Identity and authority | Audit each listed native identity against LiftAssistant identities and grants. No automatic identity conversion or authorization. |
| K — Evidence v0 | Write a private, bounded experimental bundle with a terminal verdict for every case and redacted evidence. Preserve absent observations and negative findings. |

## Private evidence and stop conditions

Use `evidence/m6b3/<run-id>/` outside the repository for `manifest.json`, `verdicts.json`, `events.jsonl`, `commands.txt`, `stdout.log`, `stderr.log` and `protocol.log`. Retain only bounded causal events and redact secrets, token-bearing URLs, headers, response bodies and errors before output. Leave protocol logs empty when there has been no exchange. Keep the provenance manifest distinct from case verdicts. This is the existing experimental Evidence v0 approach, scoped to this validation.

Stop at HUMAN review for absent Sandbox setup, required commercial or production onboarding, a demonstrated semantic mismatch, or completion of the matrix. This contract cannot validate real motion, physical door behavior or position accuracy, passenger safety, controller cabinets, production resources or behavior, OEM SLA or rate limits, network reliability, broad product compatibility, production gateway security, regulatory compliance or certification.

## Setup preflight

The official example pin is verified. The approved external credentials and verified Sandbox resource pair are unavailable in the validation environment. No Sandbox topology, authentication, lifecycle, assignment, physical-reconciliation, retry, cancellation or resume result has been observed. Live cases remain BLOCKED. Source examples support a dispatch hypothesis only. Commercial onboarding requirements for this application remain undetermined.
