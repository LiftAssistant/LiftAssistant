# Prior art map

Research index, not normative specification. Verify standards/licensing before implementation.

## Device/controller interoperability
- CiA 417 / CANopen Lift
- THOR / NeXt

## Semantic/building models
- ETSI SAREF4LIFT
- BACnet elevator objects

## Cross-brand orchestration
- T/GSET 001-2025
- Open-RMF / RFA / LCI ecosystems

## Legacy multi-vendor aggregation
- UniTech OPENlift / KUBE
- NASEUL NELIOT
- Aethon ReadyElevator
- similar multi-brand monitoring/robot-elevator platforms

## Software-defined elevator direction
Toshiba, TKE, Schindler/Inventio and industrial-computing vendors have explored OS/application layers, containers/OTA, high-level application requests, and separation from real-time/safety control.

## LiftAssistant opportunity

Do not claim individual ideas above are novel.

The intended contribution is to combine useful layers into an open, executable, vendor-neutral platform:
- common operational model;
- capability negotiation;
- intent/runtime boundary;
- portable policy;
- normalized diagnostics;
- deterministic simulation;
- public conformance tests;
- adapters to existing standards/controllers.

The goal is closer to a Home Assistant/HAOS-style neutral platform for elevators than to another proprietary controller.
