# Prior art and research policy

This file is a compact research index for semantic and interface design. It is not a specification and must not become a substitute for implementation.

Verified/reviewed: 2026-11-14

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
