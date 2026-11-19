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

## Executed preflight

The preflight ran on Manjaro Linux 6.18.55-1 in an unprivileged user and network namespace with only loopback and the newly created `vcan-m6b2` link. It verified the link kind before starting the peer or opening a SocketCAN socket. A separate run with a same-name dummy link exited before either action and produced an empty CAN capture.

The independent peer was the CANoopEnDemo Linux CiA 417 call-panel sample at `e26f4ed600b55d2472d240bad85b422c21b84c7d` with the CANoopEn submodule at `b421de2b80c69becb24f17e02512ba5e9188bae7`. The only source edit selected the test interface. The preflight observed peer boot-up, sent a generic CANopen SDO read of Device Type, and observed the peer's response. A separate `candump` process recorded both directions. This exchange supplies transport and peer evidence. The application-level hall-call mapping and acknowledgement remain untested.

| Case | Preflight verdict | Evidence |
| --- | --- | --- |
| A | BLOCKED | The [CiA 417 catalog](https://www.can-cia.org/can-knowledge/cia-417-series-profile-for-lift-control-systems) lists Parts 1, 2, 3-1 and 4 at 2.0.0 as PAS with login required. Official PDFs, local checksums and the profile subset are pending. |
| B | PASS | Kernel link kind `vcan`, plus refusal of a same-name `dummy` in a separate namespace. |
| C | PASS | The independent pinned panel replied to the generic CANopen query. Boot-up and the request/response appear in `can.log`. |
| D | BLOCKED | The transport read records send and response separately. CiA 417 application acknowledgement remains untested. |
| E | BLOCKED | Application-level duplicate and retry behavior requires the sourced slice. |
| F | BLOCKED | Application lifecycle restart and stale-sample fencing require the sourced slice. |
| G | BLOCKED | Unsupported application objects and wrong application states require a source-backed mapping. |
| H | BLOCKED | The preflight has no LiftAssistant identity mapping or authority path. Those boundaries still require review against the implemented application slice. |
| I | PASS | Evidence v0 retains all seven files and individual verdicts for the preflight. The program returns nonzero while the required application cases are blocked. |

### Repeat the preflight

Use Python 3.11 or later, Git, iproute2, unshare, a C/C++ build toolchain and a Linux kernel that supports `vcan` in an unprivileged network namespace.

1. Check out [CANoopEnDemo](https://github.com/xyntos-ch/CANoopEnDemo/tree/e26f4ed600b55d2472d240bad85b422c21b84c7d) at the stated pin, and initialize its `CANoopEn` submodule. At `Samples/CanOpenClient/CiA417/Linux/Source/Target/Linux/Start/main.cpp`, change only the `CanInterfaceName` initializer from `"can1"` to `"vcan-m6b2"`. Build with `make -C Samples/CanOpenClient/CiA417/Linux/Make BUILD_DIR=build-m6b2` in that checkout.
2. Check out [can-utils](https://github.com/linux-can/can-utils/tree/834ceef1f1eabec67625389172eebf1050d2f42b) at the pinned commit and run `make candump` in its root.
3. From the LiftAssistant checkout, set `peer_dir`, `can_utils_dir` and `evidence_dir` to absolute paths outside the checkout. Run:

   ~~~bash
   timeout 20s unshare --user --map-root-user --net sh -ec '
     ip link add dev vcan-m6b2 type vcan
     ip link set dev vcan-m6b2 up
     exec python3 tools/m6b2_vcan_preflight.py --peer "$1" --candump "$2" --evidence-root "$3"
   ' sh "$peer_dir" "$can_utils_dir/candump" "$evidence_dir"
   ~~~

4. Read the printed bundle path and check `verdicts.json`. To exercise refusal, repeat in a fresh namespace with `type dummy` in the first command. Expect B to report the kernel kind mismatch and `can.log` to be empty.

## Hall Call Up normative baseline

HUMAN supplied the CiA PAS documents directly from CiA. Each cover identifies version 2.0.0, dated 2011-02-02, and each local SHA-256 matched the supplied value. The source files remain outside the repository.

| Part and title in the CANopen application profile for lift control systems | SHA-256 | Selected references |
| --- | --- | --- |
| 417-1, General definitions | `3fa6fc651d6239da5685c711989d824da6321efccdb0dec84de69f91a13d89d4` | §§4.2.2–4.2.4, §6.2 Table 1 |
| 417-2, Virtual device definitions | `512df32879811591a8df7c1bae87af293c691218a38264210db071bf278b8638` | §§5.3.3–5.3.5, Tables 6–11 |
| 417-3-1, Pre-defined PDOs for lift application 0 | `22d7c1d4c5b6f734fcb3a426796d080b5061d4ef9baab691eb04572a5074088a` | §§4.2.2–4.2.3, 4.3.2–4.3.3, Tables 6, 8, 10, 12, 82, 84, 86, 88 |
| 417-4, Detailed application data objects | `9990d1906042ed3114ab220c0fba05cef7000432e46946c7149d45b6a6e16d68` | §§4.3.8–4.3.9, 4.4.1, 4.4.6, Figures 6–10, 16, 20–23, Tables 23–24, 29–31, 40, 42, 50–54 |

### Pinned peer comparison

This slice pairs the panel and Call Controller at the preflight pins. The peer's remote-PDO image indexes the panel's published mapping. Its callback processes virtual input for the request, and its output path publishes the virtual output record through the panel's receive mapping. The PDO labels `tpdo 1` and `rpdo 1` in sample code are local slots.

| Selected field | CiA 417 2.0.0 and fixture interpretation | Pinned upstream result |
| --- | --- | --- |
| Basic function and sub-function | Standard hall call and Up, `02/01` on each direction's defined PDO | MATCH |
| Lift field | Lift 1 bit, `01` | MATCH |
| Floor/panel field | Floor panel 1, `01` | MATCH |
| Node-ID assignment | Part 1 §6.2 Table 1 recommends 21–85 for floor panels in shaft networks. | DIFFERENCE from guidance: the isolated upstream panel uses node 10. Kept that fixture assignment. Part 3-1's PDO formula uses the selected node ID. |
| Door assignment | Door 1 in the low nibble with zero upper nibble, `01` | DIFFERENCE: `11` uses a populated upper nibble. Adjusted both selected call encoders and comparators. |
| Request data | Unlocked, set input `01`. Cleared input is `00`. | MATCH for this fixture |
| Confirmation data | Indication status is bit 0. Affirmation is bit 7. This fixture selects both, `81`. | DIFFERENCE: `01` indicates status only. `IsStatusOn()` checks bit 0. Adjusted the controller's positive output. The harness requires the predicate on the captured frame in addition to the panel log. |
| Request PDO | TPDO 130 carries 48 bits from `6010:00`, wire ID `48A` for panel node 10 | MATCH on the wire and in the panel OD at `1881/1A81`. The controller receives it via its remote panel image and processes `6010:00`. |
| Request receiver mapping documentation | Part 2 Table 11 lists `6010:00` for RPDO 130–256. Part 3-1 Table 84 prints `6011:00` as the mapping default despite §4.3.2 naming the path virtual input. | DIFFERENCE in published references. The fixture exercises the role and TPDO 130 mapping defined by Part 2 and Part 3-1 §4.2.2. It makes no determination about the contradictory receiver default. |
| Confirmation PDO | TPDO 257 and RPDO 257 carry 48 bits from `6011:00`, wire ID `400` for lift 1 | MATCH. Controller sends using the remote mapping at `1500/1700`, and the panel consumes it via its own corresponding entries. |

The upstream two-process run, with only the interface binding changed, showed receipt at both peers but emitted the reserved door nibble and clear affirmation predicate. Its normative D verdict is FAIL. The subsequent `2.0.0 conformance fixture adjustment` changes only those selected sample data values and door comparators. It does not change the stack, peer control flow, or the published mapping discrepancy.

### Executed Hall Call Up matrix

The adjusted pair ran in separate Linux processes, plus a separate `candump` process, on the verified `vcan-m6b2` interface in a fresh unprivileged network namespace on Manjaro Linux 6.18.55-1. The panel and controller used separate PTYs. The peer source files, both binaries, `candump`, and the temporary external patch are identified by SHA-256 in the private bundle. The adjusted run verifies the retained upstream negative bundle and records its hashes.

Closing evidence is `2026-11-19-hall-fixture-35706b2e` on implementation commit `90d3c68` (45 causal events, 140 captured frames). The upstream negative control is `2026-11-19-hall-upstream-566e631f`. The external fixture diff SHA-256 is `096fc20c9fbc763e6dd37a648c4d97a610d56299e2414c8f3e930342b04b30c3`. Raw bundles are retained outside Git.

| Case | Adjusted run | Evidence |
| --- | --- | --- |
| A | PASS | Four verified PAS documents, exact peer pins, selected Hall Call Up profile, and local environment in the manifest. |
| B | PASS | Positive kernel link-kind and isolation checks before bus access. |
| C | PASS | Independent controller and panel PIDs, NMT Start and operational panel heartbeat, and separate capture. |
| D | PASS | Panel request on `48A` and controller input callback, then at least 350 ms with no `400` frame. A separate controller trigger produced an affirmed `400` output and panel callback. |
| E | PASS | An identical wire repeat remained the same fixture exchange. Withdrawal and a new request produced a separate exchange. Existing `call_ids_are_deterministic_and_only_active_duplicates_are_reused` passed for LiftAssistant caller semantics. |
| F | PASS | Separate panel and controller process restarts, and a separate harness validation-generation transition with fresh peer evidence. Each interrupted exchange remained pending. Cached and deliberately replayed old-generation evidence was rejected; a new peer-originated exchange completed. |
| G | PASS | The matcher excluded a different direction, lift bit, floor, door, reserved door nibble and indication-only output. Withdrawal ended the pending exchange. |
| H | PASS | Node ID, COB-ID, NMT, PDO, SDO, lift, floor, door, CANopen Identity object and peer PID stay protocol context. No mapping to domain identity, receipt, completion or authority. |
| I | PASS | All seven Evidence v0 files, individual A–I verdicts and the external fixture patch, with upstream evidence retained separately. |

Two intermediate runs exposed value-change behavior worth preserving. An identical request PDO appeared on the bus without a second controller application log. A panel-only restart left the previous input value in the live controller's remote image, so the same pressed value did not produce a new callback. The fixture explicitly withdraws that interrupted input after re-establishing NMT, then requires a new panel button edge. The harness does not infer a new request or successful acknowledgement from unchanged state. Generation and replay provenance apply to this controlled validation only. The wire record has no operation or incarnation token, so arbitrary byte-identical traffic cannot be aged by its payload.

CiA 417 Hall Call Up and LiftAssistant `HallCall` have structural correspondence. Dispatch ownership remains different. The external controller owns its exchange and the acknowledgement is a virtual output, distinct from an `IntentReceipt` or physical completion. This slice makes no normative claim about the rest of the peer, the disputed receiver default, hardware interoperability or certification.

### Repeat the Hall Call Up run

Apply the preflight toolchain and isolation setup. Build both pinned Linux samples with `BUILD_DIR=build-m6b2` under each sample's `Make` directory. Bind only the two `CanInterfaceName` initializers to `vcan-m6b2` for the upstream run. Use the four authorized local PDFs in `normative_dir`. For the negative control run the following command, with the same absolute peer, can-utils and evidence paths as in the preflight:

~~~bash
timeout 65s unshare --user --map-root-user --net bash -ec '
  ip link add dev vcan-m6b2 type vcan
  ip link set dev vcan-m6b2 up
  exec python3 -B tools/m6b2_hall_call.py --peer "$1" --candump "$2" \
    --normative-dir "$3" --evidence-root "$4" "${@:5}"
' bash "$peer_dir" "$can_utils_dir/candump" "$normative_dir" "$evidence_dir" --variant upstream
~~~

The negative control returns nonzero with a FAIL for D. Retain its printed bundle path.

In each sample's selected `Source/CanOpenDevice/CanOpen{Slave,Master}Device.cpp`, replace only the two selected `CoCia417::DoorA` uses with `0x01`. In the master's positive acknowledgement, use `0x81` in place of `CoCia417::FunctionDataStatusOn`, keeping its clear value. Rebuild both samples, then rerun the same command with `--variant fixture --upstream-evidence <upstream-bundle>`. The harness verifies the source deltas against the pins before launching the peers. The fixture should exit zero only with all A–I verdicts PASS.
