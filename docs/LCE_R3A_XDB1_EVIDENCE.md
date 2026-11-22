# LCE-R3A.0.2 — XDB1 serial-path evidence freeze

Status: `COMPLETE — EVIDENCE_FROZEN; NO_REAL_HARDWARE_AUTHORIZATION`

Reviewed: 2026-11-23

## Scope

This record freezes the evidence that may be used when reasoning about the
LCECPU561/XDB1 serial path before a physical acquisition design is accepted.
It does not define a field cable, authorize a connection, establish that an
interface is unlocked, or show that an idle controller emits useful data.

Evidence states in this record have exact meanings:

- **DIRECTLY CONFIRMED** — the identified KONE-origin source states the scoped
  fact. It is not automatically transferable to another board, connector or
  software generation.
- **STRONGLY SUPPORTED / HIGH-CONFIDENCE INFERENCE** — multiple scoped facts and
  conventional RS-232 interpretation support the conclusion, but the reviewed
  material does not state the CPU561/XDB1-specific claim directly.
- **UNKNOWN / NOT DEMONSTRATED** — the current sources do not distinguish the
  relevant possibilities. Silence and absence of evidence cannot choose one.
- **OUT OF SCOPE** — the behavior is deliberately excluded from this passive
  research track even if a source describes it.

## Source register

The source documents remain external. No KONE manual, training PDF, screenshot
or substantial quotation is copied into this repository.

| Source identity | Version/date and relevant location | Retrieval/provenance boundary | Use in this record |
| --- | --- | --- | --- |
| KONE *LCEUIO Dongle User Manual* | EDMS `884634en.A.2`, first issue 2006-02-28; sections 1–3, pages 3–4 | KONE-origin document rechecked through a public mirror; the mirror is not the author | CPU561 RS-232 path; LCEUIO unlock and active communication behavior |
| KONE AM-11.65.040-USK, *KCM831 with KDM drive* | 2022-09-27; CPU561 commissioning procedure naming XDB1 as `Serial Port` | Previously reviewed KONE-origin source identity; document bytes are not retained in Git | CPU561 XDB1/serial connector identity only |
| Later KONE LCEUIO instruction | Later than EDMS `884634en.A.2`; LOPCB RS232-enable note | HUMAN-reviewed source states that the LOPCB switch step is not needed for LCECPU40/LCECPU561. Publicly retrievable JX15138 section 3 independently scopes the conditional switch action to the LOPCB board. Exact publication metadata for the HUMAN-reviewed copy is not frozen here. | Prevents the LOPCB switch procedure from becoming a CPU561 prerequisite |
| KONE CHN internal training material | 2011; UIO/HyperTerminal cable section | HUMAN-reviewed KONE training evidence; source bytes and diagrams are not copied into Git | LCE CPU/UIO cable uses conductors 2, 3 and 5; UIO-to-CPU segment is straight-through; computer connection crosses 2 and 3 |
| KONE-branded *LCE Terminal Using Instructions* | Historical; `Connecting a PC to the LCE controller` section; publication revision/date not identified in the accessible copy | Community-hosted KONE-origin document; source origin and retrieval host remain distinct | Classic terminal 9600/8N1 profile, 2/3 crossing and D-type PC-convention context; not universal CPU561 configuration |

The public retrieval references used for provenance review are:

- LCEUIO EDMS `884634en.A.2`:
  <https://file.globalso.com/file_manage/1579/20240913/kone-service-tool.pdf>
- later JX15138 LCEUIO instruction:
  <https://www.donati.it/sites/default/files/commerce_product/product/attachment/JX15138_Kone%20advanced%20Tool%28LCEUIO%29%20Instruction.pdf>
- historical LCE Terminal instructions:
  <https://www.scribd.com/document/379489546/LCE-Terminal-using-instructions-with-logo-KONE-pdf>

Retrieval availability does not upgrade a community mirror into normative
authority. The document origin, revision and interface scope control each
claim.

## Frozen evidence matrix

| Claim | State | Evidence | Consequence |
| --- | --- | --- | --- |
| LCECPU561 has a KONE-documented RS-232 port | **DIRECTLY CONFIRMED** | EDMS `884634en.A.2`, section 3, places Port A on the RS232 port of LCECPU40, LCECPU561 or LOPCB | A CPU561 serial-service path exists; this says nothing about current availability, passive data, pin direction or permission to connect |
| CPU561 XDB1 is the serial-port connector | **DIRECTLY CONFIRMED** | AM-11.65.040-USK identifies CPU561 XDB1 as `Serial Port` in its commissioning context | `XDB1` may be used as the connector identity for this board family; no pinout or field procedure follows from the name |
| The documented LCE CPU serial connection uses DE-9 conductors 2, 3 and 5 | **DIRECTLY CONFIRMED** | 2011 KONE CHN UIO/HyperTerminal cable evidence identifies those three conductors, a straight-through UIO-to-CPU segment and a computer-side 2/3 cross | Future design analysis is bounded to the documented three-conductor data/return family; this is not yet a field wiring diagram |
| The logical signal family is one RS-232 transmit conductor, one receive conductor and signal ground | **STRONGLY SUPPORTED / HIGH-CONFIDENCE INFERENCE** | The documented 2/3 crossing, conductor 5, RS-232 identity and the terminal manual's D-type PC-convention statement agree | Useful for design threat analysis, but not a substitute for a CPU561/XDB1-specific directional pin table |
| XDB1 pin 2 is approximately CPU receive, pin 3 CPU transmit and pin 5 signal ground | **STRONGLY SUPPORTED / HIGH-CONFIDENCE INFERENCE** | Conventional PC/DTE interpretation plus the documented crossing behavior supports this assignment | R3A.0.3 must treat direction as unverified; no live connection or field cable may rely on this inference |
| LCEUIO opens/unlocks the LCE user interface and CPU serial port and communicates with the controller | **DIRECTLY CONFIRMED** | EDMS `884634en.A.2`, introduction, LED states and section 3 | LCEUIO is active equipment, not a passive tap. Emulation, unlock and counter handling are excluded |
| An LOPCB RS232-enable switch is not a CPU561 prerequisite | **DIRECTLY CONFIRMED** | The later HUMAN-reviewed KONE instruction says the switch operation is not needed for LCECPU40/LCECPU561; JX15138 independently makes the switch step conditional and names the LOPCB board | Do not copy an LOPCB-specific mode-switch procedure into CPU561/XDB1 preconditions |
| Classic LCE Terminal uses 9600 baud, 8 data bits, no parity and 1 stop bit | **DIRECTLY CONFIRMED** within that terminal document's scope | Historical KONE-branded LCE Terminal instructions | The existing `historical_9600_8n1` profile is evidence-based for that context only; it is not a universal CPU561/XDB1 baud contract |
| Every CPU561/XDB1 service context uses 9600/8N1 | **UNKNOWN / NOT DEMONSTRATED** | Other KONE servicing contexts use distinct profiles; the classic terminal source does not claim universality | No active baud scan, fallback, or inference from unrelated bootloader/gateway material |
| An idle CPU561 XDB1 emits useful unsolicited telemetry | **UNKNOWN / NOT DEMONSTRATED** | The reviewed LCE terminal evidence is interactive and does not establish idle periodic output | A full-duration zero-byte capture is scientifically valid and inconclusive |
| Passive silence proves wrong baud, wrong pin, a broken capture, a locked port, or request/response-only behavior | **UNKNOWN / NOT DEMONSTRATED** | The same observation is compatible with all of those and other hypotheses | `PASSIVE_SILENT` must not be promoted to a hardware or protocol fault |
| CPU561 XDB1, LOPCB/XL17-style paths, older CPU serial interfaces, TTL/adapter paths, CAN and LON are interchangeable | **OUT OF SCOPE** | They are distinct physical and protocol contexts | Evidence from one path cannot fill another path's connector, electrical or lifecycle gaps |

## Explicit non-claims

This evidence freeze does **not** claim that:

- passive monitoring will produce bytes;
- the CPU serial port is currently unlocked or available;
- 9600/8N1 is universal across CPU561 firmware or service modes;
- the inferred CPU561 pin direction is directly documented;
- pins 2/3/5 alone form an approved or field-ready cable;
- LCEUIO is passive or may be emulated;
- opening a Linux tty is electrically passive;
- a zero-byte result diagnoses hardware, wiring, baud, lock or protocol state;
- XDB1 evidence applies to LOPCB, older LCE CPUs, adapter boards, CAN or LON;
- any real controller, elevator or passenger equipment may now be connected.

The frozen artifact term `PASSIVE_SILENT` means only:

```text
requested capture interval completed
+ no nonempty Linux userspace read result
+ LiftAssistant requested no active protocol transaction
= inconclusive passive-silence evidence
```

It does not prove that the controller emitted nothing on the wire, because the
Linux tty/adapter path is not independent waveform evidence.

## Gate result

`LCE-R3A.0.2 COMPLETE — EVIDENCE FROZEN; NO REAL-HARDWARE AUTHORIZATION.`

Completion permits further design/review of either the software-RX-only field
assurance path or the higher-assurance R3A.0.3 physical RX-only path. It does
not itself permit R3A.0.4 bench work, opening a real `/dev/tty*`, attaching to
CPU561/XDB1, using LCEUIO, unlocking an interface, transmitting terminal bytes,
or starting R3A.1. Any real field capture remains a separate HUMAN decision.
