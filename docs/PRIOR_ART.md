# Prior art and research policy

## M6A controlled intent semantics

CiA's [417 application profile](https://can-cia.org/can-knowledge/cia-417-series-profile-for-lift-control-systems) and the implementer's [CANoopEn demonstration](https://xyntos.ch/en/canopen/configuration/) distinguish a landing call from its acknowledgement. [ASHRAE BACnet Lift](https://bacnet.org/wp-content/uploads/sites/4/2022/08/Add-135-2012aq.pdf) separates call registration, assignment, car position and door-side properties. [Open-RMF LiftRequest](https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftRequest.msg) carries a requested floor and requester session and [LiftState](https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftState.msg) reports a lift's available floors and observed progress. These support separate call and operation identities, typed receipts, explicit execution coverage, and observation-based satisfaction.

| Reviewed interface | Request and holder | Arrival, cancellation, failure and recovery implications |
| --- | --- | --- |
| [CiA 417 / CANoopEnDemo](https://github.com/xyntos-ch/CANoopEnDemo) | Panel call and separate call-controller acknowledgement | The demonstration confirms acknowledgement independently from arrival. The example provides no generic holder, retry or restart contract to import. |
| [Open-RMF](https://osrf.github.io/ros2multirobotbook/integration_lifts.html) | Supervisor mediates shared requests. LiftRequest carries session, destination and release | LiftState is separate observed evidence. Mode and door semantics remain adapter context. The message contract supplies no generic per-operation deduplication or restart proof. |
| [Octa LCI](https://github.com/octarobotics/lci-rmf-adapter) | RMF shared session and adapter-local origin/destination representation | Arrival uses floor and door evidence. Front/rear suffixes and backend ride encoding vary. Portable retry/recovery guarantees were not established. |
| [QuikSync](https://github.com/quikbot/open-rmf-adapters-quiksync/tree/main/packages/lift_adapter_quiksync) | Per-lift session occupant lock, request execution identifier and conflicting-session rejection | Server report reconciles the adapter's holder view. Release and lock TTL belong to the controller integration. No blanket safe-resend claim follows from owning a session. |
| [Plynth](https://github.com/plynthbuild/plynth-robot-kit/tree/main/plynth-ros2/src/plynth_rmf_adapter) | Passage ID and robot-specific decisions | Same-ID retries, observed arrival, and distinct cancel-before-boarding versus exited handling. Heartbeat period and passage wire fields stay local. |
| [KONE RMF OPC integration](https://github.com/chart-sg/lift_kone_opccontroller) | Controller holder session, per-lift floor and door-side configuration | Configured I/O timeouts and separate status publication support treating failed delivery separately from physical outcome. No controller restart guarantee was established. |

[Kafka protocol fencing](https://kafka.apache.org/35/design/protocol/) is supporting evidence for stable retry identity and a bounded active generation. [AWS Device Shadow](https://docs.aws.amazon.com/iot/latest/developerguide/iot-device-shadows.html) provides a separate desired/reported pattern. The virtual lab adopts typed correlation, authority fencing and observation reconciliation. Vendor topic names, physical safety actions, door encoding/holding, service registration, timeouts, offline/emergency policy, transport response codes and production restart guarantees stay outside the canonical vocabulary.

[Kafka's message format](https://kafka.apache.org/21/implementation/message-format/) retains producer sequence information for partition duplicate checking. The analogous local scope is the controlled car. Its execution watermark can fence retired operation IDs without coupling admission across independent cars.

## M6C dispatch-ownership finding

The accepted M6B.3 KONE Sandbox evidence demonstrated a group-owned
source/destination request with no car selector and later
`allocated_lift_deck` evidence, but no proven mapping from that value to a
LiftAssistant `CarId`. In contrast, official Open-RMF
[`LiftRequest`](https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftRequest.msg)
names a lift before execution and reports progress separately through
[`LiftState`](https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftState.msg).
The [KABAM/CHART KONE RMF adapter](https://github.com/KABAM-Robotics/lift_kone_adapter)
is implementation evidence that a multi-lift KONE group can be translated into
that named-lift interface; it does not make its adapter-local translation a
vendor-neutral rule. The separate
[KONE OPC integration](https://github.com/chart-sg/lift_kone_opccontroller)
is car-specific evidence, so capability follows the configured backend rather
than the vendor name.

[Octa LCI](https://github.com/octarobotics/lci-rmf-adapter) retains explicit
lift/requester context and adapter-local origin/destination translation while
separating requests from observed results. [QuikSync](https://github.com/quikbot/open-rmf-adapters-quiksync)
uses explicit session/execution correlation and performs vendor translation
behind its connector. These reinforce correlation and boundary separation, but
their transport, retry, locking, platform and lifecycle policies are not Core
semantics.

Together the evidence supports dispatch ownership as a real independent axis:
LiftAssistant-owned selection produces the existing car-specific `Intent`,
whereas an external owner receives a car-free service request and may provide a
mapped assignment later. LiftAssistant therefore reuses its `Pending ->
Assigned(CarId)` call lifecycle, requires explicit native-to-logical mapping and
uses a separate dispatch-scope authority grant. Vendor identity, credentials,
transport acknowledgement, assignment and physical completion remain distinct.
This M6C decision is internal and reversible.

## M5 read-only lifecycle corrections

- [Kubernetes object UIDs](https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#uids) and [systemd invocation IDs](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html) distinguish stable names from individual lifetimes. [Kafka producer fencing](https://kafka.apache.org/35/design/protocol/) rejects superseded producers using current identity/epoch. LiftAssistant takes the fresh opaque incarnation and current-token fence. Its token carries no numeric epoch or event order. Bounded replay artifacts can check uniqueness separately.
- [DDS 1.4 deadline and liveliness QoS](https://www.omg.org/spec/DDS/1.4/PDF) distinguish an instance update contract from writer liveliness. [ROS 2 QoS](https://raw.githubusercontent.com/ros2/ros2_documentation/kilted/source/Concepts/Intermediate/About-Quality-of-Service-Settings.rst) also separates deadline, liveliness and connection matching. In the observed Hotel feed, one subscription carries multiple lifts. An application arrival timer keyed by explicit lift binding can revoke one car while the feed still receives data. Discovery evidence belongs to the current subscriber. Recreating that subscriber leaves the adapter incarnation and existing ordering intact.
- [Kubernetes controllers](https://kubernetes.io/docs/concepts/architecture/controller/) scope reconciliation to linked entities. [Open-RMF LiftState](https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftState.msg) identifies a lift and supplies its own floor list. LiftAssistant scopes mapping faults to the configured entity and treats unbound names as diagnostic evidence. Feed transport and impossible adapter failures retain their wider scope.
- [Open-RMF LiftState](https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftState.msg) supplies available floors per lift. [ASHRAE BACnet Lift object](https://bacnet.org/wp-content/uploads/sites/4/2022/08/Add-135-2012aq.pdf) describes individual lift and door/landing characteristics. An observed RMF floor set can therefore fit inside the configured building floor universe without defining execution capability.
- [OpenTelemetry error handling](https://opentelemetry.io/docs/specs/otel/error-handling/) isolates observability failures from application work. An optional Capsule write is a one-shot diagnostic side effect after semantic invalidation. The Host reports a write error and continues according to source/transport disposition.

This file is a compact research index for semantic and interface design. It is not a specification and must not become a substitute for implementation.

Verified/reviewed: 2026-11-16

## When prior-art research is required

Do targeted research before introducing or materially changing:

- core elevator-domain semantics;
- call lifecycle/state vocabulary;
- capability models;
- normalized diagnostics/fault semantics;
- protocol/standard mappings;
- external/public API or schema concepts;
- adapter/runtime boundary semantics;
- configuration concepts that may later become public;
- terminology that existing standards may already define.

Research is recommended, but not mandatory, for:

- internal Rust API organization;
- TUI/UX details;
- test organization;
- performance implementation choices.

Research is normally unnecessary for:

- clear bug fixes;
- formatting/renames;
- local refactors that preserve semantics;
- straightforward implementation of already-approved semantics.

## Source priority

Prefer sources in this order:

1. official standards / standards-body documentation;
2. official project repositories and documentation;
3. documented vendor/public APIs;
4. mature community implementations and issue/discussion history;
5. blogs/posts only when stronger sources are unavailable.

Do not treat a single external project as authoritative for LiftAssistant.

## Research output discipline

Research should answer only what is needed for the next working vertical slice.

For each meaningful semantic design question, capture briefly:

- what existing terminology/concepts exist;
- what is directly reusable;
- what does not fit LiftAssistant and why;
- any important divergence LiftAssistant intentionally makes;
- whether the decision is still internal/reversible or is approaching a HUMAN-REVIEW boundary.

Do not create long research reports unless the topic genuinely requires one.

A useful rule:

~~~text
research enough to avoid reinvention
-> implement
-> validate
-> only then deepen research if the implementation exposes ambiguity
~~~

## M6B.2 isolated CANopen/CiA 417 validation findings

The M6B.2 prior-art pass deliberately separates normative profile material, executable peers, Linux transport behavior, and test-evidence practice.

- **Normative baseline.** CiA states that CANopen Lift CiA 417 version 2.0.0 was published in 2011 and can be downloaded free of charge. The revised 2026 draft proposal series (including 2.4.0 parts and the 2.3.0 pre-defined-PDO part currently listed by CiA) is member-only. M6B.2 therefore freezes 2.0.0 as the default normative baseline and requires exact part/version/checksum provenance before profile-specific implementation. A future legally available edition requires a separate HUMAN decision rather than silent drift.
  - https://www.can-cia.org/can-knowledge/cia-417-series-profile-for-lift-control-systems
  - https://www.can-cia.org/news/pr-english?tx_news_pi1%5Baction%5D=detail&tx_news_pi1%5Bcontroller%5D=News&tx_news_pi1%5Bnews%5D=295
- **Virtual transport.** Linux documents `vcan` as a virtual local CAN interface that transmits and receives CAN frames without CAN controller hardware. CANopenNode/CANopenLinux explicitly supports SocketCAN and documents testing without a real CAN interface through Linux virtual CAN. This supports an isolated protocol experiment, not any conclusion about electrical timing or controller/transceiver behavior.
  - https://docs.kernel.org/networking/can.html
  - https://github.com/CANopenNode/CANopenLinux
- **Independent CiA 417 peer.** Xyntos CANoopEn/CANoopEnDemo provides Apache-2.0 Linux/SocketCAN CiA 417 examples, including a lift hall-call panel and lift call-controller sample. This makes it useful as an independently running executable peer. Its constants, mappings and behavior remain implementation evidence, not the normative definition of CiA 417.
  - https://github.com/xyntos-ch/CANoopEn
  - https://github.com/xyntos-ch/CANoopEnDemo
  - https://xyntos.ch/en/canopen/samples/
  - https://xyntos.ch/en/canopen/configuration/
- **Evidence practice.** PX4 bench testing treats simulation and hardware as different evidence layers, requires bounded timeouts and explicit PASS/FAIL outcomes, and retains timestamped raw evidence. Zephyr Twister separates terminal test status from structured recordings and can preserve nested JSON records alongside ordinary logs. M6B.2 borrows only these test-evidence disciplines: bounded waits, provenance, structured verdicts and raw-artifact separation. It does not import either project's runtime architecture.
  - https://docs.px4.io/main/en/test_and_ci/bench_testing
  - https://docs.zephyrproject.org/latest/develop/twister/twister_statuses.html
  - https://docs.zephyrproject.org/latest/develop/twister/harness/console.html

LiftAssistant deliberately does **not** infer that a CAN socket write is application acknowledgement, that a CiA 417 acknowledgement is physical completion, or that a source-native CAN identifier is a LiftAssistant identity/authority token. It also does not add a general CANopen framework merely because the isolated validation needs an external peer.

## Current elevator-domain references

### CiA 417 — CANopen application profile for lift control systems

Official overview:
https://www.can-cia.org/can-knowledge/cia-417-series-profile-for-lift-control-systems

Use it to understand existing lift-control functional boundaries and terminology such as call controller, car drive controller, car door controller, car position, load measuring, and related virtual-device concepts.

Do not copy CANopen transport/object-dictionary details into LiftAssistant Core semantics.

### BACnet elevator objects / landing calls

Official ASHRAE addendum introducing elevator-related object concepts:
https://www.ashrae.org/File%20Library/Technical%20Resources/Standards%20and%20Guidelines/Standards%20Addenda/135_2012_aq_20160302.pdf

Use it as prior art for elevator-group representation, landing-call status/control, group membership, and building-facing semantics.

Do not assume BACnet's object model should become LiftAssistant's internal architecture.

### ETSI SAREF4LIFT

Official ETSI technical specification:
https://www.etsi.org/deliver/etsi_ts/103400_103499/10341011/02.01.01_60/ts_10341011v020101p.pdf

Ontology portal:
https://saref.etsi.org/saref4lift/v2.1.1/

Use it to compare domain vocabulary, lift entities, states, operating concepts, faults, doors, stops, and semantic interoperability terminology.

Do not treat ontology structure as a mandatory runtime data model.

### Open-RMF lift integration

Official lift request message:
https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftRequest.msg

Use it as prior art for how an upper-layer system requests lift service, identifies sessions/requesters, specifies destination floors, and separates requester semantics from lift implementation.

Open-RMF is robotics/building-infrastructure middleware, not the target architecture for LiftAssistant. Reuse concepts only where they fit.

The official Hotel lift plugin provides reliable `lift_requests` input and separate reliable `lift_states` output. Its granted requester session remains in reported state, and releasing that holder is a distinct `REQUEST_END_SESSION` request. The plugin rejects an unavailable floor with a simulator warning rather than a typed per-request rejection message. M6B.1 therefore keeps the holder as adapter-local correlation, uses independent observed state for physical reconciliation, performs an explicit session release before a second same-floor lifecycle, and treats a request with neither acceptance nor completion evidence as indeterminate at a host-owned deadline. It does not manufacture a rejection receipt or reinterpret the holder as a LiftAssistant Principal, grant, Intent, or source session.

Official plugin source: https://github.com/open-rmf/rmf_simulation/blob/main/rmf_building_sim_gz_plugins/src/lift.cpp

### Redfish — architectural prior art, not lift-domain semantics

Developer hub:
https://redfish.dmtf.org/

Use Redfish only as architectural inspiration for separating stable semantic models, implementation backends, schemas/examples, registries, and interoperability profiles.

Do not import Redfish resource models into LiftAssistant.

## Current M2 research targets

Before finalizing M2 call-lifecycle semantics, compare at least:

- CiA 417 call/controller concepts;
- BACnet landing-call/group concepts;
- SAREF4LIFT terminology for lift state/operation;
- Open-RMF request/session concepts.

The purpose is not to force one standard's lifecycle onto LiftAssistant. The purpose is to avoid inventing conflicting terms unnecessarily and to document deliberate differences.

M2 remains internal/reversible unless a public normative schema is being frozen. Freezing or materially changing a normative public schema remains HUMAN-REVIEW.

## M2 call-semantics finding

Targeted review for M2.1 found that BACnet represents landing calls as currently active group-level calls and separately exposes calls assigned to a lift; CiA 417 treats call control as a distinct functional element; SAREF4LIFT primarily supplies lift/device/state/command vocabulary rather than a request lifecycle; and Open-RMF uses requester-provided session identity for a robot's lift session rather than identity for each landing call.

LiftAssistant therefore uses deterministic per-call identity and the internal lifecycle `Pending -> Assigned(car) -> Serving(car) -> Completed`. This deliberately models persistent orchestration state above a runtime, rather than copying a transport object, ontology shape, or requester session. The terminology remains an internal, reversible implementation decision; it is not a frozen public schema.

## M2 dispatch-explanation finding

CiA 417 diagnostic objects, BACnet lift fault/status/mode properties, and SAREF4LIFT fault/state concepts describe device or protocol observations. Open-RMF likewise keeps lift requests separate from reported lift state. A car being busy, outside Normal mode, missing a capability, or losing a deterministic distance tie-break is an orchestration decision explanation, not necessarily a lift fault or alarm.

LiftAssistant therefore keeps M2 dispatch explanations separate from normalized equipment diagnostics: candidate mode, busy status, capability availability, distance, rejection reasons, and `OnlyEligible`/`Nearest`/`CarIdTieBreak` selection basis. This remains an internal, reversible Core result rather than a standards mapping or policy DSL.

## M2 served-floor configuration finding

BACnet defines a universal floor number as a bottom-up contiguous building index and uses it for floor text, positions, stops, and landing calls. SAREF4LIFT separately models car-stop spaces, building spaces, door connection points, and car services, which can express topology beyond a flat floor range. CiA 417 keeps call/input/position functions behind implementation-independent virtual-device interfaces.

LiftAssistant M2 needs only the demonstrated common denominator: a canonical ordered nonempty set of served `Floor` values. Core validates membership and terminal hall-call direction from that set. It intentionally does not yet model public floor names, doors, car-specific service maps, spatial graphs, or BACnet array indexes; those require real adapter/integration evidence.

## M3 simulator rejection-injection finding

BACnet elevator `Fault_Signals` and SAREF4LIFT `FaultSignal`/fault-state concepts represent observed lift or equipment fault conditions, while SAREF4LIFT models commands separately. Open-RMF likewise separates lift requests from reported lift state. A deliberately rejected simulator intent is therefore not evidence of a controller, drive, door, safety-chain, or other equipment fault.

LiftAssistant keeps the M3 control as an explicit one-shot simulator-only intent rejection. It surfaces through the existing runtime submission error path, leaves Core call state pending for deterministic retry, and never produces a normalized equipment fault/alarm. The control and its developer trace event remain internal and reversible; they are not a public diagnostic schema or protocol mapping.

## M4 recorded Open-RMF state-mapping finding

The official Open-RMF `LiftState` message reports a lift name, string floor labels, current/destination floor, discrete door and motion states, available/current modes, and the session granted control. `DOOR_MOVING` does not distinguish opening from closing. `MOTION_UP`/`MOTION_DOWN` do not carry continuous position. `MODE_HUMAN`/`MODE_AGV` describe RMF interaction modes, while the same field also reports fire, offline, and emergency states.

LiftAssistant therefore requires an explicit string-label-to-`Floor` configuration and first maps recorded fields into canonical partial evidence. A floor label remains a floor reference; moving directions remain known without inventing continuous position; `DOOR_MOVING` remains movement with unknown direction; Human/AGV/Emergency values remain source-specific context rather than LiftAssistant operating modes. Declared source unknowns remain explicit evidence, while unrecognized enum values and unadvertised or unconfigured labels remain structured mapping errors. The resolver preserves a floor reference as floor-level `CarLocation::Floor` and promotes state only when every required operational field is established; it does not manufacture a continuous at-floor coordinate. The input remains an in-memory recording with no ROS dependency, live transport, request publication, or normative protocol schema.

Official message source: https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftState.msg

## M4 live Open-RMF destination/busy finding

The official `LiftState` message reports `current_floor`, `destination_floor`, and motion independently, while the separate `LiftRequest` message uses `destination_floor` as a requested floor. Neither message defines the mere presence of a reported destination as proof that a lift is currently occupied or moving. Official Hotel simulation evidence additionally showed unchanged idle snapshots with current floor and destination both equal to `L1`, stopped motion, and a closed door.

LiftAssistant therefore keeps reported destination separate from Engine call ownership and uses the smallest conservative dispatch rule supported by both sources: motion, Engine Assigned/Serving state, unknown destination, or a known different destination are Busy evidence; a stopped car reporting its current floor is not Busy from that field alone. This remains internal and reversible rather than a generalized activity model.

Official sources:
- https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftState.msg
- https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftRequest.msg

## M4 recorded BACnet Lift-object mapping finding

The official ASHRAE Addendum 135-2012aq defines a Lift object whose externally
visible properties include required `Car_Position`, `Car_Moving_Direction`,
`Car_Door_Status`, and `Out_Of_Service`, plus optional `Car_Mode` and
`Next_Stopping_Floor`. `Car_Position` is a universal floor number rather than a
continuous position. `Car_Door_Status` is an array because a car may have multiple
doors. Most importantly, object `Out_Of_Service=true` means the BACnet object is
decoupled from the represented lift and no longer tracks or controls it; this is not
the same assertion as a lift car operating in LiftAssistant `OutOfService` mode.

LiftAssistant therefore uses an explicit universal-floor-number configuration,
keeps position as a floor reference, aggregates door status only when all mapped
doors agree, and retains the exact per-door/status/mode/object values as source
context. A decoupled object produces unknown position, motion, door, and mode
evidence regardless of the values still present in its other properties. Only
clearly equivalent car modes map into current internal operating modes; other
standard BACnet modes remain source-specific. The implementation is an internal
typed projection of decoded property values, not a BACnet object/packet model,
transport, write client, or conformance profile.

Official source: https://www.ashrae.org/File%20Library/Technical%20Resources/Standards%20and%20Guidelines/Standards%20Addenda/135_2012_aq_20160302.pdf

## Cross-domain production architecture references

These are architectural prior art, not LiftAssistant domain standards.

### OPC UA DataValue

Official reference:
https://reference.opcfoundation.org/specs/OPC-10000-4/7.11

Useful idea: a value is accompanied by status/quality and source/server timestamps rather than treated as an unqualified scalar. Use this as prior art for observation provenance/quality; do not copy OPC UA types wholesale.

### ROS 2 QoS: Deadline, Liveliness, Lifespan

Official design:
https://design.ros2.org/articles/qos_deadline_liveliness_lifespan.html

Useful idea: freshness expectations, publisher liveliness and sample lifespan are distinct concepts. This is relevant to deciding when elevator observations are still usable.

M5.2 applies that distinction without copying a DDS policy into Core. The official ROS 2 design defines Deadline as an allowed interval between messages, Liveliness as entity-aliveness reporting, and Lifespan as sample validity. LiftAssistant's first RMF supervision therefore uses an explicitly configured adapter/host payload-silence interval only after payload has actually flowed. Subscriber creation or discovery alone remains separate evidence, source timestamps are not the host clock, and a missed interval revokes live operational truth rather than asserting an elevator fault. This remains internal and source-specific rather than a universal deadline or stable health schema.

### ROS 2 DDS domain bounds

Official ROS 2 documentation derives DDS domain IDs from 16-bit UDP port allocation: the possible DDS-backed range is 0 through 232, while narrower ranges are recommended on each platform to avoid ephemeral-port conflicts. M5.4 runtime validation exposed that the current RustDDS version performs its default-port arithmetic in `u16` and panics above that possible maximum. LiftAssistant now rejects values above 232 both at its headless CLI and RMF transport boundary. This is transport input validation, not a new LiftAssistant domain semantic or a claim that every value through 232 is operationally conflict-free on every host.

Official source: https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/nodes/About-Domain-ID.rst

### Kubernetes controllers/reconciliation

Official documentation:
https://kubernetes.io/docs/concepts/architecture/controller/

Useful idea: controllers repeatedly reconcile desired state against observed current state rather than assuming one command permanently establishes reality. Use only the control-loop principle; LiftAssistant is not a Kubernetes-style distributed control plane.

### OpenTelemetry semantic stability

Official semantic-convention documentation:
https://opentelemetry.io/docs/specs/semconv/general/semantic-convention-groups/

Useful idea: semantic groups progress through explicit maturity/stability levels and stable groups are not casually broken. This is prior art for future LiftAssistant semantic maturity/versioning.

Additional production patterns such as diagnostic registries, integration quality profiles, process sandboxing, update rollback and sanitized diagnostic bundles should be researched only when their implementation milestone becomes READY.

## M7 RMF gateway implementation review

M7.0 reviewed the 2026-11-20 current source of five concrete elevator integrations before freezing the first process boundary. These projects are implementation prior art, not normative authorities:

- Open-RMF `lift_adapter_template` (`ca5445ad14a7b38fa6f7333bcee0bb586406baba`) owns one lift API, accepts only requests for its configured lift, rejects unsupported floors, polls state independently, and reports the granted RMF session through `LiftState`. Its template deliberately leaves controller connectivity and command semantics to the integrator; it has no durable operation record and its synchronous API calls can share the ROS executor path.
- KABAM/CHART `lift_kone_adapter` (`9bc3a87a01eea66c606e0a256e1cd8a8c60bc596`) owns KONE token renewal, topology discovery, WebSocket state monitoring and request IDs. It demonstrates that one adapter can own shared building/group transport for several cars. It also logs credential material and uses wall-clock/reconnect loops without a durable command identity, so its credential handling, retry constants and outcome claims are explicit anti-patterns for LiftAssistant.
- Octa `lci-rmf-adapter` (`a91b2ada76e98d763809621e5b5ca2638369edf7`) separates MQTT receive activity, availability and command contexts, uses locks/conditions and monotonic elapsed time, and keeps state publication alive through a dedicated receiving path. Its device-specific availability/session behavior is not evidence for LiftAssistant authority, receipt or retry semantics.
- QuikSync `open-rmf-adapters-quiksync` (`e4d797c500664526ae51858156ce757f124c3680`) has one process manage multiple lifts, separates background WebSocket state pumps from request routing, reads secrets from a file, refreshes tokens, bounds reconnect/backoff, and supplies an explicit execution ID to a server that documents idempotent deduplication. LiftAssistant reuses the separation and explicit-correlation lessons, but does not copy its HTTP API, Auth0/session-manager authorization or retries because plain RMF topic publication provides no equivalent deduplication proof.
- CHART `lift_kone_opccontroller` (`158b5925544e1d9f7270c1fceb552aad1d65e1f8`) keeps configured per-lift floors and uses OPC acknowledgement/state reads after writes, with command work moved to per-request threads while periodic state polling continues. Its blocking tag sequences, process-local busy/session flags and lack of crash recovery are not sufficient production operation ownership.

The compatible common denominator is narrow: the gateway owns backend transport, credentials, reconnect, native correlation and backend readiness; observation ingestion continues independently of command-channel work; unsupported mappings fail explicitly; and retries are allowed only when that backend proves idempotency or non-submission. Transport identity and credentials do not grant LiftAssistant authority. M7 therefore keeps Principal/grant, `IntentId`, receipt, canonical observations and physical reconciliation in Engine, records unresolved work durably before an RMF publish, and never automatically resends it after restart. A ROS publish remains local delivery evidence, not a controller receipt or physical completion.

Source repositories:

- https://github.com/open-rmf/lift_adapter_template
- https://github.com/KABAM-Robotics/lift_kone_adapter
- https://github.com/octarobotics/lci-rmf-adapter
- https://github.com/quikbot/open-rmf-adapters-quiksync
- https://github.com/chart-sg/lift_kone_opccontroller

## LCE legacy recorded/read-only compatibility

Reviewed 2026-11-20 for the independent LCE-R0 research slice. The KONE-origin
*LCEUIO Dongle User Manual* (EDMS 884634en.A.2, first issue 2006) establishes
the LCECPU/LOPCB diagnostic-interface and lock boundary and places LCETERM
behind that boundary; it does not establish terminal table grammar. Current
ELFIN eleon S1 installation material and EDNL Sense documentation independently
show commercial monitoring over KONE LCE diagnostic/RS232 paths. ELFIN's MQTT
Consumer API 4.0.0 exposes normalized floor, movement, door, status and error
information, but that normalization is interoperability evidence rather than a
description of the LCE wire format.

The weaker-provenance historical *LCE Terminal Using Instructions* documents
read families `STA`, `CMD`, `CAL`, `GRP`, `MDE`, and `N`. It also documents
active call, memory and parameter operations, so availability of read tables
does not imply write authority. The durable mapping lessons are narrow:

- level, advance-position and destination fields have distinct meanings;
- speed and direction must agree before motion is known;
- controller calls and fault identifiers are context, not LiftAssistant calls,
  equipment diagnosis or operating mode;
- group lift and node identifiers require explicit logical-car binding;
- commercial normalized output cannot fill gaps in historical table semantics.

The focused evidence matrix, source URLs and exact implementation limits are in
[`LCE_RECORDED_READONLY_COMPATIBILITY.md`](LCE_RECORDED_READONLY_COMPATIBILITY.md).
The result is `RECORDED_READONLY_PARTIAL`: an original-synthetic recorded parser
is justified, while serial transport, raw capture compatibility, hardware and
production claims are not.

### LCE-R1 community-corpus finding

A bounded 2026-11-21 search covered public web/code indexes, elevator forums and
file/document indexes. The strongest field evidence remains the
[2013 KCM831 report](https://www.vatortrader.com/forums/ubbthreads.php?Board=1&main=913&type=thread&ubb=printthread):
its author says HyperTerminal text capture saved real LCE parameter and LON
output. [Later discussion](https://www.vatortrader.com/forums/ubbthreads.php?Number=37544&ubb=showflat)
corroborates field use and private circulation of instructions. Neither page
publishes the captured `.txt` bytes, and no independently verifiable public raw
capture file was located.

The durable distinction is `capture existence evidence != capture content
evidence`. Historical manual samples can justify family vocabulary,
line-oriented/paginated expectations and preservation of prompt/banner/echo
context, but not a universal byte grammar, exact newline/echo behavior or
variable-row termination. LCE-R1 therefore keeps its accepted three-row input
as a LiftAssistant-authored normalized fallback, preserves raw/native/canonical
levels separately, and leaves real-capture compatibility unclaimed. The full
classification and projection/Core-gap matrices are in
[`LCE_RAW_TERMINAL_COMPATIBILITY.md`](LCE_RAW_TERMINAL_COMPATIBILITY.md).

### LCE-R2 interface/profile/session finding

The KONE-origin [LCEUIO manual, EDMS 884634en.A.2](https://v6-file.globalso.com/file_manage/1579/20240913/kone-service-tool.pdf)
establishes an LCE interface/lock boundary, an RS232 CPU561 connection and
LCETERM pass-through, but not target availability or terminal grammar. KONE
AM-11.65.040-USK independently calls CPU561 XDB1 the Serial Port. The
historical [LCE terminal instructions](https://www.scribd.com/document/379489546/LCE-Terminal-using-instructions-with-logo-KONE-pdf)
and the independent [2024 field report](https://www.vatortrader.com/forums/ubbthreads.php?Board=1&main=6770&type=thread&ubb=printthread)
support a 9600/8N1/null-modem runtime profile and `STA`/`CMD`/`MDE` vocabulary.
The manual also exposes calls, parameters, memory, roll call and interactive
pagination, so the terminal as a whole is not a read-only protocol.

The durable design lesson is `interface identity != availability`, `runtime
profile != servicing profile`, and `typed observation query != arbitrary
terminal access`. CPU561 service/SD documentation associates 115200 with the
bootloader/information path; it is not a fallback for runtime acquisition.
LiftAssistant therefore models interface state explicitly, exposes only the
three typed observation queries, requires a known boundary, records evidence
before interpretation and fails closed on unknown interaction. PTY behavior is
`LIFTASSISTANT_SYNTHETIC`; absent real raw bytes, exact prompt, echo, line
ending, timing, pagination, firmware and LCEUIO behavior remain unknown.


### LCE-R2.2 generic transport-evidence comparison

R2.2 compared the connector's TX evidence semantics with established
byte-stream and request/response interfaces rather than treating a successful
application call as proof of physical transmission:

- Rust `std::io::Write::write` explicitly permits short writes:
  https://doc.rust-lang.org/stable/std/io/trait.Write.html
- pySerial separates `write()` from `flush()`, where the latter waits for
  output completion:
  https://pyserial.readthedocs.io/en/latest/pyserial_api.html
- POSIX/Linux `tcdrain()` is a stronger "wait until transmitted" operation
  than simply handing bytes to a write call:
  https://man7.org/linux/man-pages/man3/tcdrain.3p.html
- Linux SocketCAN exposes `MSG_CONFIRM` only where the driver can provide a
  stronger local transmission-confirmation signal:
  https://docs.kernel.org/networking/can.html
- Modbus serial guidance models a single pending request and moves to response
  waiting after the request is sent:
  https://www.modbus.org/file/secure/modbusoverserial.pdf

The durable design lesson is `prepared != transport accepted != drained !=
independently observed on wire`. LiftAssistant records only the level it can
actually support. R2.2 therefore labels accepted write bytes
`TRANSPORT_ACCEPTED_TX`, starts response timeout after complete acceptance,
preserves short-write prefixes before failing closed, and leaves any future
drain/wire evidence as a separate real-transport concern. SHA-256 protects
capture integrity only; provenance authentication would require a distinct
trust/signature design and is not claimed.


## M4 semantic-boundary audit findings — reported/current/desired state

### Reported state and desired intent must remain distinct

Open-RMF exposes reported lift state and lift requests as separate message types. `LiftState` reports `destination_floor`, current floor, motion, door state, modes, and the granted session; `LiftRequest` separately carries a requester/session, request type, requested `destination_floor`, and requested door state.

The official `LiftState` message declares `destination_floor` as a string but does not define an empty-string sentinel. LiftAssistant therefore cannot treat an empty recorded value as proof of "no destination"; it remains unknown unless a source contract supplies stronger semantics.

AWS IoT Device Shadow similarly separates `desired` and `reported` state and versions updates so stale writers can be forced to resynchronize. Eclipse Ditto explicitly distinguishes reported, desired, and current twin state.

LiftAssistant should therefore not use one optional `target` field to simultaneously mean Engine desired target, controller-reported next stop, and "no target". External destination evidence belongs on the reported/observation side; Engine intent/call state belongs on the desired/orchestration side.

Official/reference sources:
- https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftState.msg
- https://github.com/open-rmf/rmf_internal_msgs/blob/main/rmf_lift_msgs/msg/LiftRequest.msg
- https://docs.aws.amazon.com/iot/latest/developerguide/device-shadow-document.html
- https://eclipse.dev/ditto/

### Omitted update is different from explicit unknown

OPC UA `DataValue` associates an individual value with status and source/server timestamps rather than requiring every device property in one monolithic snapshot. AWS Device Shadow updates affect only fields supplied in an update; omitted fields are not automatically rewritten as "unknown".

This matters for live BACnet COV, CAN/CiA 417 PDOs, and other incremental streams. LiftAssistant needs a deterministic distinction between "this update did not carry/change this field" and "the source explicitly says the field is unknown/unavailable". The current whole-`CarObservation` proof is therefore insufficient as the final live-ingestion contract.

### Floor identity and measured continuous position are different evidence

Open-RMF `LiftState.current_floor` is a floor label. BACnet Lift `Car_Position` is a universal floor number. CiA 417 defines a separate car-position unit whose role is measuring car position and which may optionally provide speed/acceleration/jerk.

A floor reference plus stopped/open evidence may establish that the car is at a landing for operational purposes, but it does not establish an exact continuous encoder coordinate. LiftAssistant should preserve this precision distinction instead of manufacturing `Position::at_floor` from floor-level sources.

### Lifecycle/readiness and diagnostics comparison

ROS 2 managed nodes distinguish lifecycle states and allow an external supervisor to configure/activate/deactivate/recover components. Kubernetes separately distinguishes startup, liveness, and readiness. These support LiftAssistant's current direction that "process alive" is not equivalent to "read-only state is synchronized and ready".

Redfish messages use stable machine-readable `MessageId` plus severity and resolution metadata. Home Assistant separates downloadable diagnostics from actionable Repairs and maintains an explicit integration quality scale. These support keeping LiftAssistant anomalies machine-readable and evidence-backed while delaying a public diagnostic registry, repair taxonomy, or adapter quality tier until real M5 deployment pressure exists.

THOR's event fingerprint/logbook and KONE E-Link traffic-history playback independently validate the value of bounded pre-event evidence and historical playback for elevator field debugging, but LiftAssistant should keep its implementation open, deterministic, and source-neutral rather than copy either product's data model.
