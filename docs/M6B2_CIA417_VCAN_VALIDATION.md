# M6B.2 CiA 417 / CANopen isolated protocol-layer validation contract

M6B.2 validates a deliberately narrow CANopen Lift protocol boundary over Linux virtual CAN. It is a software/protocol experiment only. It does not authorize real CAN hardware, a controller cabinet, an OEM network, a passenger lift, or a production write gateway.

## Approved environment

The only approved bus target is Linux SocketCAN `vcan`.

The validation harness must refuse to run unless the selected interface is positively identified as a virtual CAN interface. A name such as `vcan0` is not sufficient by itself: the run must verify the link kind. No `can*`, `slcan*`, USB/PCI CAN adapter, physical CAN controller, bridge to a site network, controller cabinet, PLC, OEM endpoint, or passenger equipment is eligible.

LiftAssistant and the CANopen peer must run as independent processes. A loopback/self-echo test does not count as external protocol evidence.

## Normative and executable baselines

Normative claims for this milestone are limited to the officially published CiA 417 version 2.0.0 series unless HUMAN review explicitly selects another legally available edition.

The 2026 CiA 417 2.4.0 documents are a newer draft proposal series available to CiA members. They are not an implicit upgrade target and must not be reconstructed from third-party constants or examples.

Before any profile-specific mapping is implemented, record:

- the exact CiA 417 part/version used for each normative claim;
- the source location and local checksum of the obtained document;
- the exact external peer implementation and commit/tag;
- the CANopen/CiA 417 subset exercised.

Do not commit standards PDFs or other material whose redistribution terms do not permit it.

The preferred executable prior art is a pinned Linux/SocketCAN CiA 417 peer from an independent implementation such as Xyntos CANoopEn/CANoopEnDemo. CANopenNode/CANopenLinux is useful for SocketCAN/CANopen behavior and tooling. Neither implementation is the normative definition of CiA 417.

## Goal

Challenge the protocol boundary that a future lower-controller adapter would need while preserving the existing M6A/M6B.1 guarantees:

- local submission is not application-level acknowledgement;
- protocol acknowledgement is not physical completion;
- native CANopen/CiA 417 identifiers remain adapter-local;
- retries and restarts do not create replacement LiftAssistant identities;
- stale protocol evidence cannot complete a new LiftAssistant lifecycle;
- unsupported or ambiguous profile behavior is reported rather than guessed;
- no profile mapping grants LiftAssistant authority by itself.

M6B.2 does not need to prove physical lift motion. M6B.1 already supplied independent simulated physical-state evidence. M6B.2 adds protocol-layer evidence only.

## Mapping rule

Implement only the smallest CiA 417 application slice that is both:

1. supported by the selected normative 2.0.0 material; and
2. independently exercisable by the pinned peer.

A hall-call/call-controller exchange is a useful executable reference because public CANoopEnDemo provides Linux CiA 417 call-panel and call-controller samples, but it must not be silently reinterpreted as LiftAssistant `ServeFloor`.

If the selected CiA 417 function does not have the same semantics as an existing LiftAssistant HallCall or controlled Intent, keep the mapping adapter-local or stop with a documented semantic mismatch. Do not change Core/M6A public semantics merely to fit the profile.

## Required validation matrix

| Case | Acceptance evidence |
| --- | --- |
| A. Baseline provenance | Record LiftAssistant commit, kernel, Rust/Cargo, selected interface, CiA 417 document parts/versions/checksums, external peer project + commit/tag, and the exact exercised profile subset. |
| B. Virtual-bus isolation | Positively identify the selected link as Linux `vcan`; refuse a physical/unknown CAN link. Record the verification result before any protocol write. |
| C. Independent peer | Start a separate CANopen/CiA 417 peer process and prove bidirectional bus activity through SocketCAN. A self-loop, in-process fake, or local send result alone is insufficient. |
| D. Submission / acknowledgement separation | Record local LiftAssistant/adapter submission separately from independent peer-observed application acknowledgement/state. A successful socket write or CAN frame transmission is not promoted to an application receipt. |
| E. Duplicate and retry | Repeat the same logical operation/correlation at caller and transport boundaries. Demonstrate that LiftAssistant does not allocate replacement Principal, grant, Intent, or call identity merely because a frame/request is repeated. |
| F. Restart and stale evidence | Restart the adapter and separately restart the external peer. Old frames/state from a retired adapter/source lifecycle must not satisfy a new lifecycle. Preserve or explicitly terminate unresolved work according to the existing contract; do not infer success after restart. |
| G. Unsupported / wrong protocol state | Feed an unmapped node/function/object or an independently observed state inconsistent with the requested semantic. The adapter must reject, keep pending, or report divergence/unsupported evidence; it must not fabricate a canonical success. |
| H. Identity / authority boundary | CAN node IDs, COB-IDs, NMT state, object indexes/subindexes, PDO/SDO correlation, profile virtual-device identifiers, and peer process identity stay protocol/adapter context unless an explicit reversible mapping exists. None becomes `CarId`, `PrincipalId`, `AuthorityGrantId`, `IntentId`, or write authority by name equality. |
| I. Evidence bundle | Produce the bounded Evidence v0 bundle described below and make every required case independently PASS/FAIL/ERROR/BLOCKED. A required case may not disappear behind a skipped test. |

## Evidence v0

M6B.2 introduces a deliberately experimental run artifact, not a framework or public schema.

A run should produce one bounded directory such as:

~~~text
evidence/m6b2/<run-id>/
  manifest.json
  verdicts.json
  events.jsonl
  commands.txt
  stdout.log
  stderr.log
  can.log
~~~

Required properties:

- `manifest.json` records provenance/environment identity, not semantic truth;
- `verdicts.json` records each validation case and one of `PASS`, `FAIL`, `ERROR`, or `BLOCKED`;
- `events.jsonl` contains only the small causal trace needed to explain verdicts;
- raw process/CAN logs remain raw evidence and are not parsed into new domain semantics;
- every operation has a bounded deadline; a timeout is a named non-success result;
- a failed required case causes a non-zero validation exit;
- raw evidence is not committed by default;
- the repository stores the contract, reproduction recipe, concise completion audit, and any small deterministic fixture needed for regression.

Reuse existing Engine events / Flight Recorder / incident-capsule concepts where they already describe canonical behavior. Do not create a generic validation event bus, telemetry service, database, artifact server, plugin framework, or stable evidence schema.

An incident capsule may be attached to a failed validation run, but a validation evidence bundle is not itself an incident capsule.

Evidence v0 remains private/experimental until at least two different external validation families demonstrate a genuinely shared stable shape. Any later public/stable evidence schema is an M8/HUMAN-REVIEW concern.

## Implementation constraints

Prefer the smallest reversible vertical slice.

Allowed in SELF-ITERATION:

- adapter-local SocketCAN/vcan transport;
- one narrow, source-backed CiA 417 mapping;
- an independent pinned peer harness;
- bounded deterministic unit/recorded tests;
- protocol capture for the isolated run;
- Evidence v0 generation local to the validation harness;
- bug fixes and regressions needed to preserve existing M6 semantics.

Do not introduce without a new HUMAN gate:

- real CAN hardware access;
- generic production CAN daemon/service;
- production Host/API/CLI/Flutter write routes;
- vendor/OEM protocol support;
- controller-cabinet or site-network access;
- low-level brake, motor/drive, safety-chain, door-lock, or comparable safety actuation;
- a new Core authority/security model;
- normative public protocol/evidence schemas;
- a claim of CiA 417 conformance/certification;
- M7 production gateway work.

Do not add a general CANopen stack to LiftAssistant merely because M6B.2 needs a test peer. Prefer an independent implementation for the peer and a narrow adapter boundary for LiftAssistant.

## Reproduction discipline

Follow the same evidence hierarchy used in mature simulation/bench projects:

- pin the software under test and external peer;
- make every wait bounded;
- print/record explicit case verdicts;
- retain raw evidence separately from the concise verdict;
- make the environment identity part of the run;
- distinguish test-infrastructure failure (`ERROR`) from behavior mismatch (`FAIL`) and unavailable prerequisite (`BLOCKED`).

The hosted CI does not have to perform the live `vcan` run unless the runner can provide the required isolated virtual interface safely and deterministically. Deterministic unit/recorded regressions remain required in normal CI. The completion audit must state exactly where the live protocol run executed.

## Completion gate

Declare `M6B.2 COMPLETE` only when:

- the approved CiA 417 baseline and peer version are pinned;
- the selected profile subset is documented without semantic invention;
- the run is positively restricted to `vcan`;
- an independent peer supplies protocol evidence;
- cases A-I have explicit terminal verdicts and all required cases PASS;
- local send, application acknowledgement, canonical observation, and physical completion remain distinct concepts;
- no M6A public contract, Principal/grant/Intent identity semantics, or production write boundary was weakened;
- normal workspace regression and repository-defined Rust/MSRV gates pass;
- the completion audit lists what remains unverified.

Still NOT VERIFIED after M6B.2:

- real CAN electrical/timing behavior;
- CAN controller/transceiver behavior;
- bus arbitration/error confinement under real hardware;
- real lift/controller compatibility;
- OEM behavior;
- physical motion or passenger safety;
- production gateway behavior;
- CiA conformance/certification.

Stop after M6B.2 for HUMAN completion review. Do not automatically enter M7, real hardware, or a passenger-production environment.
