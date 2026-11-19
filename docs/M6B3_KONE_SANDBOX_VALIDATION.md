# M6B.3 — KONE Official Sandbox semantic validation

## Scope and access gate

Test whether LiftAssistant's existing controlled-intent, identity, receipt and reconciliation semantics can represent an OEM group controller that chooses the car. Freeze this contract before implementing or running a live probe. A finding that needs a new semantic model goes to HUMAN review.

The approved environment is the **official KONE Sandbox** and any destination call is limited to one verified, locally allowlisted simulated building/group pair. The operator confirmed that the application is labeled SANDBOX in KONE Portal. Resources returned by that application's official inventory are eligible validation candidates unless official metadata identifies a production, real-customer or otherwise incompatible resource. Candidate audit and deterministic fixture selection inside this envelope are SELF-ITERATION work. Contradictory, ambiguous or production evidence stops the run. Real buildings, real lifts, controllers, passenger equipment, production resources, commercial onboarding and spending remain outside this validation.

The initial prerequisite is the Sandbox application's client ID and secret supplied through an approved local channel, such as process environment (`KONE_CLIENT_ID` and `KONE_CLIENT_SECRET`) or a masked local form that passes the values directly to a one-shot probe. The original four-variable gate also required `KONE_BUILDING_ID` and `KONE_GROUP_ID` from the operator. The API now supplies candidate building/group pairs. Audit every returned candidate with bounded read-only official requests, exclude individual unusable candidates, and select the best eligible fixture deterministically. Operator selection is unnecessary for ordinary equivalent Sandbox fixtures. Rotate any secret previously exposed outside the approved local environment before use. While credentials are missing, finish contract and source work and mark live cases BLOCKED.

Credentials and access tokens must stay out of command arguments, logs, errors, Git, fixtures and evidence. The pinned shared helper prints the token response, constructs a WebSocket URL containing the token and can log resource data. Use a credential-safe harness or a documented output-only modification to the external example checkout. Keep sample request semantics intact. Check normal and error output for redaction before any live authentication. [Pinned shared helper](https://github.com/konecorp/kone-api-examples/blob/e597b46aac8870e6e85ee2b842abbeb36471ac74/src/common/koneapi.ts)

## Discovery and request authorization

1. Obtain the minimum inventory-only token supported by the Sandbox, using `application/inventory` without callgiving scope. Use `GET /api/v2/application/self/resources` to discover buildings accessible to this application. Record only sanitized identifiers needed for this experiment. A successful, verified empty inventory is `BLOCKED_ON_KONE_SANDBOX_RESOURCE_PROVISIONING`. Keep authentication, transport and response errors distinct from an empty inventory.
2. Determine eligible groups for those Sandbox buildings from the inventory response, documented resource metadata, official topology/configuration APIs or the current official examples where they establish a discovery path. The pinned helper reads building IDs from `buildings[].id`. Never guess IDs or enumerate unrelated resources. For all exact pairs returned by inventory, use bounded configuration and actions requests without issuing elevator calls.
3. An eligible pair must expose destination-call capability, two distinct connected areas and no incompatible production or special-service requirement. Rank eligible pairs by at least two lifts, lift count, multi-lift routes, monitoring richness, then stable lexical building/group identity. Within the winner choose a distinct, normal source/destination route served by at least two lifts where possible, with stable lexical ordering as the final tie-breaker. Record scores and rationale. A failure excludes only that candidate. Stop at `BLOCKED_ON_KONE_SANDBOX_RESOURCE_CAPABILITY` only when all candidates are unusable.
4. Only after selecting the exact pair, obtain the minimum access needed for that group, preferring `callgiving/group:<BUILDING_ID>:<GROUP_ID>` and only the other scopes required by the official API. A broader scope requires documented Sandbox necessity and an unchanged local allowlist restricting every operation to the selected pair. Revalidate topology and supported actions before submitting the first elevator call. The KEN selector used by separate Operational API examples is outside this path.

## Official source provenance

- The [KONE API Portal](https://dev.kone.com/) and [Elevator Call API overview](https://dev.kone.com/api-portal/elevator-call-api/) provide the official product and API context. The public sources reviewed here do not attest to the setup or entitlement of a specific Sandbox application.
- [KONE's official examples](https://github.com/konecorp/kone-api-examples/tree/e597b46aac8870e6e85ee2b842abbeb36471ac74) are pinned at commit `e597b46aac8870e6e85ee2b842abbeb36471ac74`, committed 2025-11-13 UTC. The official GitHub commit record and a separate external checkout agree on tree `149a6c4ca201eb2f5a6855d6a83f0d1607dfed69`. The checkout remains external to LiftAssistant. The pinned defaults are `https://dev.kone.com/api/v2/oauth2/token`, `https://dev.kone.com/api/v2/application/self/resources` and `wss://dev.kone.com/stream-v2`. Actual Sandbox protocol behavior must be observed. [Endpoint definitions](https://github.com/konecorp/kone-api-examples/blob/e597b46aac8870e6e85ee2b842abbeb36471ac74/src/common/koneapi.ts#L23-L27)
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
| C — Inventory, configuration and actions | Discover the building via application inventory and determine the group using official evidence or operator selection when required. Inspect the exact pair via official configuration and actions queries. Capture sanitized lift membership, floors/areas, decks/doors where available, actions and submission scope. |
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

Stop for missing approved credentials, all Sandbox candidates lacking the required capability, required commercial or production onboarding (`BLOCKED_ON_COMMERCIAL_OR_PRODUCTION_ONBOARDING`), or completion of the matrix. A semantic mismatch is fully documented while independent cases continue. HUMAN review begins before implementing any resulting Core, identity, authority or public-schema change. This contract cannot validate real motion, physical door behavior or position accuracy, passenger safety, controller cabinets, production resources or behavior, OEM SLA or rate limits, network reliability, broad product compatibility, production gateway security, regulatory compliance or certification.

## Setup preflight

Historical run `2026-11-19-sandbox-preflight` stopped at `BLOCKED_ON_HUMAN_SANDBOX_SETUP` under the original four-variable contract. Its private evidence remains as recorded.

The revised-gate run `2026-11-19-sandbox-inventory-preflight` reverified the pin and recorded the operator's Sandbox confirmation. It stopped at `BLOCKED_ON_HUMAN_SANDBOX_CREDENTIALS` before any live request.

## Executed read-only inventory

On 2026-11-20 the operator confirmed use of the existing Sandbox credential pair and supplied it directly to a one-shot probe through a masked local form. The probe independently reproduced the pinned authentication and inventory requests. A synthetic authentication error checked the status-only diagnostic path before the live request. The local probe retained no credential values, token, resource identifiers or raw response bodies. There was no group-scoped token or WebSocket connection.

The Authentication v2 request for `application/inventory` and the authenticated application resource GET each returned HTTP 200. Inventory returned five distinct `buildings[].id` entries. Each building's `groups[]` metadata contained one identified group. No explicit boolean Sandbox or simulation marker was observed on those building records. Provenance is the HUMAN-confirmed Sandbox application and the official inventory returned for that application.

Run `2026-11-20-sandbox-live-inventory` retained counts only and correctly stopped at `BLOCKED_ON_HUMAN_GROUP_SELECTION` under the then-frozen contract. That gate was unnecessarily conservative. Inventory had already identified five Sandbox building candidates and one identified group in each. Ordinary candidate audit, fixture scoring and route selection are now SELF-ITERATION work. The historical evidence remains unchanged. At that point no detailed topology or actions had been queried and no elevator call had been constructed or submitted.

Private Evidence v0 run `2026-11-20-sandbox-live-inventory` records the bounded read-only result. A–J remain BLOCKED at their full acceptance boundaries. K passes for this run's artifact completeness. Earlier preflight bundles are retained separately.

## Executed live Sandbox validation

The 2026-11-20 local run used the HUMAN-confirmed official Sandbox application, the pinned official examples and a one-shot external Python harness. The harness was SHA-256 pinned in the private manifest and remained outside LiftAssistant. It used Python 3.13.9, `websockets` 17.1, no network retries, ten-second per-operation transport bounds, a 512-message protocol-log limit, a two-call limit, and exact group-scoped authorization. It contacted only resources returned by the application inventory. Raw evidence remains in a private seven-file Evidence v0 directory outside Git.

All five inventory candidates accepted their exact group scope and returned configuration plus actions. Each exposed direct enabled DestinationCall action `2` and a valid multi-lift route. Stable aliases below identify only this run; no vendor identity became a LiftAssistant identity.

| Candidate | Lifts | Areas | Multi-lift directed routes | Deck/door monitoring score | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| candidate-1 | 8 | 126 | 15,750 | 48 | eligible |
| candidate-2 | 8 | 80 | 6,320 | 24 | eligible |
| candidate-3 | 8 | 126 | 8,790 | 48 | eligible |
| candidate-4 | 8 | 80 | 6,320 | 24 | eligible |
| candidate-5 | 9 | 120 | 5,960 | 54 | selected |

Candidate 5 won at the earlier lift-count ranking criterion before route count was compared. Its lexically first eligible normal route used pseudonyms `area-405101758459` to `area-39e5b4830d4d`, was shared by three lifts and used the group's first stable terminal. Configuration reported nine lifts, eighteen decks, thirty-six doors and 360 per-lift floor entries. Source and destination were derived from explicit floor/side membership; no identifier was guessed.

### Call A, lifecycle and resume

The v2 action request named the exact building/group, origin, destination, terminal and DCS action `2`. It carried no lift/car selector. Evidence records these milestones independently:

1. local request constructed;
2. WebSocket send completed;
3. native action response returned `success: true` with vendor request and call-session correlation;
4. the first subscribed call-state event was `being_served`, containing a one-element `allocated_lift_deck` list;
5. independent group monitoring showed simulated door and position transitions, including opening/closing and upward movement on one lift topic;
6. a later call-state event was `served`.

No field named `liftId` appeared in the captured action, call-state or monitoring messages. The experiment does not decode `allocated_lift_deck` into `CarId`; it records that assignment-like vendor evidence first appeared after submission. No `being_allocated` or `served_soon` event was observed. The group monitoring was coherent simulated OEM evidence, but the run did not establish an explicit vendor correlation from the allocated deck value to the monitored lift topic at the destination. `served` therefore remains OEM lifecycle completion and does not prove `Reconciliation::Satisfied`.

After the first lifecycle event, the harness closed the first WebSocket in a controlled way, opened a second connection and sent the official `resume-session` request for the same KONE session with `resendLatestStateUpToSeconds: 30`. KONE returned `type: ok` and status 201. No replay or duplicate arrived during the bounded five-second replay observation. A fresh read-only call-state subscription on the resumed connection later delivered `served`. The evidence proves the session handshake and subsequent use of the resumed connection; it does not claim that latest-state replay occurred, that ordering/freshness was supplied, or that KONE session identity is an `ObservationSessionId`.

### Call B and cancellation uncertainty

The second and final allowed normal call used the reverse valid route. Its action response returned `success: true`. The harness immediately sent the official v2 delete payload with that call's vendor session value. The same connection returned an adjacent status-201 acknowledgement about 0.4 seconds later. The official delete example carries no client `requestId`, and the acknowledgement did not carry the deleted call session or a typed cancellation disposition. No `cancelled`, `canceled`, rejection or other terminal event arrived during the remaining bounded window.

This is terminal evidence for the cancellation case: API-level delete acknowledgement was observed, while semantic cancellation and physical effect remain unconfirmed. The harness did not retry, issue a third call or reinterpret the result as LiftAssistant withdrawal. Call-submission idempotency remains undocumented. Session resume was used for duplicate/retry evidence without resubmitting Call A; no duplicate event was observed.

### Terminal A–K matrix

`PASS` below means the required bounded case was executed and its positive or negative result was retained. It does not turn a semantic mismatch into compatibility.

| Case | Verdict | Evidence-backed conclusion |
| --- | --- | --- |
| A — Provenance | PASS | LiftAssistant revision, OS/kernel, harness/runtime versions and hash, official source commit/tree, Sandbox provenance and operation are in the private manifest. |
| B — Authentication and hygiene | PASS | Formal authentication used hidden standard input. Credentials, tokens, authorization headers and token-bearing URLs are absent from Git and the seven retained files. Synthetic normal/error redaction checks and post-run scans passed. An earlier local candidate-audit PTY echo flaw is retained as negative process evidence; it did not enter Git or Evidence v0. |
| C — Configuration/actions | PASS | All five candidates were audited read-only; the winner was scored deterministically and revalidated before Call A under exact group scope. |
| D — Submission/response | PASS | Construction, send completion, successful native response and first call-state are distinct causal events. |
| E — OEM assignment | PASS | Submission had no lift selector. Assignment-like `allocated_lift_deck` appeared later in `being_served`; no `liftId` field appeared and no `CarId` was inferred. |
| F — Lifecycle/monitoring | PASS | `being_served`, independent simulated position/door changes and `served` were captured. Explicit allocation-to-monitoring correlation and LiftAssistant physical satisfaction remain unproven. |
| G — Duplicate/retry | PASS | Official session resume was exercised without call resubmission. No replay duplicate was observed; submission idempotency remains undocumented. |
| H — Cancellation | PASS | One immediate official delete received an adjacent 201 acknowledgement; no correlated typed cancellation or terminal state arrived. The uncertainty is preserved. |
| I — Reconnect/resume | PASS | Controlled disconnect and `resume-session` succeeded. No requested latest-state replay was observed before a fresh subscription produced `served`. |
| J — Identity/authority | PASS | Request, connection, session, building, group, lift/deck/area/door and OAuth identities stayed vendor-local and conveyed no LiftAssistant authority. |
| K — Evidence v0 | PASS | Seven bounded private files preserve causal events, sanitized protocol traffic, verdicts and negative findings. Known credential and token-bearing URL/header scans passed. |

### Semantic classification and implementation footprint

Overall classification is **SEMANTIC_MISMATCH**. The observed OEM operation accepts a group-owned origin/destination request and decides an allocated deck after submission. Current `Intent::ServeFloor { car, floor }` requires a logical car before controlled admission. A production mapping cannot honestly claim LiftAssistant selected that car, while treating `allocated_lift_deck` as a pre-existing `CarId` would invent identity and ownership.

`HallCall`, `CallState`, `Intent`, `Receipt`, `Reconciliation`, Core dispatch, Principal/Authority semantics and production write surfaces are unchanged. No KONE product crate or fixture was added, no production/real installation was contacted, and no M7 work began. This run cannot establish real motion/door behavior, production reliability, cancellation effects, call idempotency, regulatory safety or conformance.

Closing local regression passed formatting, locked all-target Clippy with warnings denied, all 144 workspace tests with one expected ignored test, the actual Rust/Cargo 1.85.0 locked all-target workspace check, all nine TUI scenarios and all four non-live headless scenarios. Secret-free hosted [CI run 44575372614](https://github.com/LiftAssistant/LiftAssistant/actions/runs/44575372614) passed the Stable format/Clippy/test job and the Rust 1.85 workspace job on the evidence commit.

The validation terminal is:

> **M6B.3 VALIDATION COMPLETE — HUMAN SEMANTIC REVIEW REQUIRED. Full Sandbox evidence collected; no Core redesign implemented.**
