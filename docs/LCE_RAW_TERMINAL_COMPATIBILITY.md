# LCE-R1/R2 raw terminal compatibility

Status: R1 `OFFLINE_TRANSCRIPT_PARTIAL`; R2 `PTY_READONLY_CONNECTOR_PARTIAL`

Reviewed: 2026-11-21

## Scope

LCE-R1 adds a bounded, offline byte-stream decoder in `liftassistant-lce` and
feeds its native records through the existing LCE-R0 mapper and Engine path.
It is a parser foundation for independently supplied transcript bytes, not a
serial adapter, terminal client, controller tool, production compatibility
claim, or KONE-supported integration.

The implemented path is:

```text
arbitrary offline byte chunks
  -> bounded line/transcript decoder
  -> exact bounded raw block + existing RecordedLceFrame
  -> existing conservative LCE-R0 mapper
  -> CarObservationUpdate
  -> explicit Engine binding/session
  -> Engine-owned operational state
```

R1 introduced no serial dependency, terminal command generation, controller
query, LCEUIO communication, unlock/counter operation, memory or parameter
access, call generation, hardware access, or write/control path. R2's later
typed synthetic-PTY query surface is documented separately below.

## Relation to LCE-R0

R1 does not introduce a second `STA`, `CMD`, `MDE`, or `GRP` semantic model.
It extracts the existing `RecordedLceFrame` records, after which every R0
identity, floor, ambiguity, provenance, session, and canonicalization rule
continues to apply. In particular, source-native lift identifiers and floor
tokens still require explicit caller configuration. Fault values, controller
calls, and reported destinations do not become equipment diagnoses,
LiftAssistant calls, or desired intents.

## Community capture hunt

The bounded search covered ordinary web search, public GitHub/code-index
searches, elevator-maintenance forums, public document/file indexes, and exact
searches for the known LCE terminal titles and KCM831 HyperTerminal reports.
No login, paywall, private message, access control, or non-public attachment was
circumvented.

The strongest new result is a 2013 VatorTrader field report by Scott Davidson.
It says a computer was connected to a KCM831/LCE controller, HyperTerminal text
capture was used, and parameter plus LON information was saved. The public
thread contains the report, not the resulting `.txt` capture or the separately
offered write-up. A 2024 VatorTrader discussion independently shows field
interest in exchanging LCE HyperTerminal instructions, again without a public
raw capture attachment.

Result:

```text
REAL_CAPTURE_EXISTENCE_CORROBORATED
PUBLIC_RAW_CORPUS_NOT_FOUND
```

No actual public real-capture file was located. No external capture, manual,
PDF, screenshot, terminal program, or proprietary material was copied into the
repository. Because no external bytes were obtained, there is no external
capture checksum to record.

## Evidence classification

| Evidence | Source / date | Production claim and acquisition | Raw/reformatted; visible session content | Corroboration | Redistribution / sensitivity | Classification / confidence |
| --- | --- | --- | --- | --- | --- | --- |
| KCM831 parameter/LON capture report | [VatorTrader thread](https://www.vatortrader.com/forums/ubbthreads.php?Board=1&main=913&type=thread&ubb=printthread), Scott Davidson, 2013-01 | Author claims a real KCM831/LCE connection using Windows XP, RS232/null-modem and HyperTerminal text capture | No capture bytes are public; report says screen text was continuously saved. Banner, echo, commands and tables cannot be inspected | Specific controller context and method; later field discussions corroborate use of the terminal path | Public post, but the capture/write-up was offered privately and has no redistribution grant | `FIELD_REPORT_ONLY`; medium confidence that real captures existed, zero content-corpus confidence |
| Later HyperTerminal discussions | [2024 discussion](https://www.vatortrader.com/forums/ubbthreads.php?Number=37544&ubb=showflat) and [2024 LCE thread](https://www.vatortrader.com/forums/ubbthreads.php?Board=1&main=6770&type=thread&ubb=printthread) | Field users discuss KCM831/LCE connection and privately shared instructions | No public raw log, parameter dump, banner, prompt, or table body | Consistent with the 2013 report and KONE-origin interface evidence | Public posts only; private material was not requested or accessed | `FIELD_REPORT_ONLY`; low-to-medium content confidence |
| LCE terminal instructions | [Publicly readable community upload](https://www.scribd.com/document/379489546/LCE-Terminal-using-instructions-with-logo-KONE-pdf), historical document, uploader Daud Jiong KIM | KONE-branded historical instructions with sample `STA`, `CMD`, `CAL`, `GRP`, `MDE`, `N`, prompt, pagination, and active-function descriptions | Manual examples are formatted/OCR-derived, not an independently captured field session; keystrokes and output are interleaved in explanatory text | Vocabulary agrees with R0 evidence and field reports | Marked all rights reserved; nothing copied into the repository | `VENDOR_MANUAL_SAMPLE`; medium semantic confidence, low byte-layout confidence |
| LCEUIO manual, EDMS 884634en.A.2 | [Public mirror of KONE-origin document](https://v6-file.globalso.com/file_manage/1579/20240913/kone-service-tool.pdf), first issue 2006-02-28 | Identifies LCECPU40/LCECPU561/LOPCB, LCEUIO and LCETERM interface/lock boundary | Not a terminal transcript and does not define the runtime table grammar | KONE-origin identity and independently mirrored document metadata | External reference only; no repository copy | `VENDOR_ORIGIN_INTERFACE_EVIDENCE`; high interface-boundary confidence, no grammar confidence |
| R0 and R1 test inputs | Independently authored in this repository, 2026-11-20/21 | Synthetic normalized tables and malformed/boundary inputs | Self-contained LF/CRLF byte streams with deliberately synthetic values and session noise | Executable deterministic tests only | Repository-owned test material; no site/controller identifiers | `LIFTASSISTANT_SYNTHETIC`; high test-provenance confidence, no field-compatibility claim |

No candidate qualified as `REAL_FIELD_CAPTURE` or
`REAL_FIELD_CAPTURE_REFORMATTED`. Search results that merely repeated the
historical manual were not promoted to independent evidence.

## Corpus decision

The repository corpus remains entirely LiftAssistant-authored. The historical
manual supplies family names, title vocabulary, line-oriented/paginated
behavior, and the distinction between monitoring and active functions. The
field report establishes that terminal text capture was actually practiced.
Neither supplies inspectable raw bytes for a controller-output grammar.

R1 therefore implements a conservative fallback rather than claiming capture
compatibility. The accepted semantic block is a narrow three-row projection:

1. an `STA STATUS OF LIFT`, `CMD COMMAND OF LIFT`, or `MDE MODE OF LIFT`
   title, with the existing underscore form also accepted;
2. the exact normalized R0 field row;
3. one value row.

This projection is independently authored and deliberately easier to delimit
than the OCR/manual layout. It is not asserted to be byte-for-byte output from
any LCE firmware. A future real capture may require an additive adapter-local
layout extractor before the existing native records; it must not change the R0
semantic mapper merely to fit screen formatting.

## Raw and session findings

| Assumption | Evidence basis | R1 treatment |
| --- | --- | --- |
| Output is line-oriented | Historical manual samples | Incremental bounded line decoder |
| `PRINTER MODE`, software/version text, `?`, `VTEST1`, help and command echoes may occur | Historical manual samples only | Preserved as bounded unknown raw lines; never used as car state or a required frame boundary |
| User echo behavior and exact prompt are stable | Not established | No prompt/echo state machine and no prompt-dependent completion |
| LF and CRLF occur in redistributable real captures | Not established | Both are accepted as reversible offline compatibility tolerance; fragmented CRLF is handled; bare CR is rejected explicitly |
| One OS read equals one line/table | Contradicted by normal byte-stream behavior, not evidenced by LCE | Arbitrary chunks are supported; whole, bytewise and irregular partitions are equivalent |
| `STA`/`CMD`/`MDE` screen columns always occupy the R1 three-row layout | Not established | Only the explicitly normalized fallback projection is accepted; no universal firmware claim |
| `GRP` has a stable row count or terminator | Ambiguous manual sample | Raw `GRP` extraction is rejected as unsupported; R0's already-delimited semantic fixture remains available |
| `CAL`, `N`, `SPB`, or `RST` can safely delimit canonical records | Not established and several are interactive/variable | Recognized as unsupported research/native context; no canonical output |

`ObservationSourceId`, `ObservationSessionId`, and source-global
`ObservationSequence` are still supplied at the LiftAssistant ingestion
boundary. A transcript's lift, group, node, CPU, banner, prompt, or software
version text cannot create those identities.

## Implemented offline decoder

`LceTranscriptDecoder::push(&[u8])` accepts arbitrary chunks without a transport
trait or event queue. `finish()` handles a final unterminated line and reports
an incomplete pending frame. A successful event contains both the exact bytes
of the recognized bounded block (including observed LF/CRLF endings) and the
existing native record. Unknown lines and structural failures remain bounded
adapter-local evidence and never emit an observation.

Bounds are explicit:

- 8 KiB per caller-supplied chunk, which also bounds the synchronous returned
  event vector;
- 512 bytes per pending line;
- 1 KiB and 16 physical lines per pending block;
- the existing R0 limits of 64 bytes per semantic token, 32 semantic lines,
  and eight group rows where the R0 parser is used;
- no internal backlog, history, wall clock, network, or hardware state.

Oversize input, invalid bytes, bare carriage returns, malformed columns,
truncation, unsupported families, and incomplete end-of-stream all produce
deterministic rejection events. The decoder resynchronizes at a bounded line or
new supported header and never fabricates a partial observation.

## Native evidence preservation

Successful extraction produces two independent results:

```text
native parse = SUCCESS
canonical projection = COMPLETE, PARTIAL, NONE, or ERROR
```

The exact raw block remains on `DecodedLceTranscriptFrame`. Every successfully
parsed native value remains in `LceStaRecord`, `LceCmdRecord`, or
`LceMdeRecord`, and mapping returns the complete record in `LceSourceContext`.
Thus a value such as `LOAD` or `FLT_ID` survives even when no canonical type
exists or canonical mapping rejects another value. No generic vendor property
bag or KONE field was added to the canonical model.

## Canonical projection coverage

| Family / native fields | Classification | Treatment |
| --- | --- | --- |
| `STA SPD` + `DIR` | `CanonicallyMapped` | Map only consistent stopped/up/down evidence; contradictions are Unknown |
| `STA DOOR` | `CanonicallyMapped` | Exact simple states only; encoded/composite values are Unknown |
| `STA LVL_NO` | `CanonicallyMapped` with qualifiers | Floor reference only with stopped motion plus `AT_DZN` and documented `AT_LEVEL` evidence |
| `STA AT_DZN`, `AT_LEVEL` | `UsedAsQualifier` | Qualify floor evidence and remain in native context |
| `STA ADV_POS`, `SRV`, `FLT_ID`, `TERM_LIM`, `SPDA_VID`, `SPDB_VID`, `LOAD`, `CORR_DRV`, `CC_PREFF` | `PreservedSourceOnly` | Retained exactly; no canonical field or diagnosis invented |
| `CMD DEST` | `CanonicallyMapped` | Explicit floor token becomes reported destination, never desired Intent |
| Other `CMD` fields | `PreservedSourceOnly` | Planned direction/door/opening/timing context stays LCE-local |
| `MDE LIFT_MODE` | `CanonicallyMapped` for exact equivalents; otherwise source-specific/unknown | Only Normal, FireService and OutOfService exact cases map |
| `MDE DOOR_STATE` | `CanonicallyMapped` for exact simple states; otherwise Unknown | Encoded fault/time combinations are not decoded |
| Other `MDE` fields | `PreservedSourceOnly` | Group/call-record/fault/advance-position context stays LCE-local |
| R0 `GRP` position/destination | `CanonicallyMapped` only after explicit native-lift/floor mapping | R1 raw extraction remains unsupported |
| `CAL`, `N`, `SPB`, `RST`, prompt/banner/help/pagination | `Unsupported` or `UnknownMeaning` for canonical projection | Bounded raw/research context only |

## Core Gap Audit

| Data pressure | Classification | R1 conclusion |
| --- | --- | --- |
| Position, motion, simple directed door state, broad operating mode, reported destination | A — already canonical | Existing evidence types remain sufficient and conservative |
| Door-zone and accurate-level indicators | B — qualifier only | They strengthen a floor reference but are not independent Core state |
| Advance position, speed-plate IDs, correction drive, car-call preference, native record flags | C — correctly vendor-local | Preserve in LCE source context |
| Fault IDs, terminal limits, controller service flag | D — diagnostics candidate | More source/version and cross-vendor evidence is required before normalized diagnostics; never infer root cause or mode now |
| Encoded/composite values, pagination variants, group traffic details, node/roll-call content | E — unknown | Preserve raw/native context; do not guess |
| Load class and possibly a cross-vendor service-availability concept | F — plausible cross-vendor canonical gap | Requires a second independent source/standard mapping, an actual product/operator use case, and established vendor-neutral terminology before any Core proposal |

Group serving/traffic state may later pressure dispatch or diagnostics, but the
manual sample and one LCE family are insufficient to create generic group
capability semantics. No Core or Engine type or behavior changed in R1.

## Validation

Focused tests cover supported `STA`, `CMD`, and `MDE`; whole, bytewise,
two-byte/irregular and split header/column/value/CRLF input; multiple blocks per
chunk; leading/trailing banner, prompt, echo, help and unknown text; malformed,
truncated, oversize, non-ASCII/binary, unsupported-family and end-of-stream
paths; deterministic arbitrary bytes; exact raw preservation; and native
source-only field survival.

The Engine regression exercises:

```text
chunked bytes -> native records -> R0 mapper -> explicit binding/session -> CarState
```

It proves that native IDs do not create logical cars, floor mapping is
mandatory, source-only fields survive, destination stays reported, session
replacement clears prior evidence, delayed old-session input is rejected, and
new-session partial evidence cannot inherit old fields.

Repository tests use no private attachment, paid source, external network,
hardware, OEM binary, or local secret.

Closing validation passed `cargo fmt --all -- --check`, warning-free workspace
all-target Clippy, 221 workspace tests with one expected ignored soak child, the
exact Rust/Cargo 1.85.0 locked all-target workspace check, all nine TUI
scenarios, all four non-live Headless scenarios, the 21 focused LCE tests, and
normal-dependency inspection showing only `liftassistant-model`. No live LCE or
external peer test was run or claimed.

## Result and next evidence gap

LCE-R1 is complete at `OFFLINE_TRANSCRIPT_PARTIAL`. It establishes a bounded,
chunk-independent and evidence-preserving offline foundation. It does not
establish compatibility with a real LCECPU40/LCECPU561/KCM831 transcript,
HyperTerminal newline/echo behavior, firmware layout variants, `GRP`
termination, repeated/paginated acquisition, serial lifecycle, freshness, or
query safety.

The next evidence needed is an independently retained, legally inspectable raw
capture from a positively identified non-passenger bench/controller version,
with provenance for line endings, banner/prompt/echo, idle and changing
`STA`/`CMD`/`MDE`, pagination, interruption, and preferably group output. Any
hardware, serial, LCEUIO, unlock, or active-query work remains a separate HUMAN
gate. A real capture must be reviewed for permission and site/private data
before any minimized derivative fixture could enter the repository.

## R2 evidence dossier

R2 rechecked the narrow interface/session claims instead of treating the task
summary as a protocol specification:

| Source | Durable evidence used | Classification / exclusion |
| --- | --- | --- |
| KONE *LCEUIO Dongle User Manual*, EDMS 884634en.A.2, first issue 2006-02-28 | The KONE-origin document identifies the LCE 6.X.X lock boundary, D9 Port A for LCECPU40/LCECPU561/LOPCB, Port B for LCETERM, 10–24 V external power, and the CPU connection as RS232 | `VENDOR_ORIGIN_INTERFACE_EVIDENCE`; establishes interface/lock existence only. It does not establish target lock state, terminal grammar, or permission to unlock |
| Historical KONE-branded *LCE Terminal Using Instructions* | Historical runtime profile 9600 baud, 8 data bits, no parity, one stop bit and null-modem wiring; monitoring families include `STA`, `CMD`, `MDE`; the same terminal surface also includes calls, memory, parameters, roll call and interactive/paginated operations | `VENDOR_MANUAL_SAMPLE`; supports a strict positive allowlist and stateful/fail-closed session, not byte-for-byte firmware compatibility |
| KONE AM-11.65.040-USK, *KCM831 with KDM drive*, 2022-09-27 | CPU561 XDB1 is explicitly called the Serial Port in the commissioning procedure | `VENDOR_ORIGIN_INTERFACE_EVIDENCE`; connector identity only. No pinout, field procedure, or controller access was derived |
| [2013 KCM831 report](https://www.vatortrader.com/forums/ubbthreads.php?Board=1&main=913&type=thread&ubb=printthread) | A real PC/RS232/null-modem/HyperTerminal session and `.txt` capture of parameters/LON information were reported | `FIELD_REPORT_ONLY`; `REAL_CAPTURE_EXISTENCE_CORROBORATED`, but no public raw bytes |
| [2024 LCE discussion](https://www.vatortrader.com/forums/ubbthreads.php?Board=1&main=6770&type=thread&ubb=printthread) | Independent field report says RS232, 9600, 8-bit and null modem worked on LCE and distinguishes newer KCE | `FIELD_REPORT_ONLY`; profile corroboration, not complete grammar evidence |
| KONE AS-10.22.028, LCECPU561 software-upload/SD servicing instruction | Its 115200 baud output belongs to the SD/bootloader/service information path | `VENDOR_ORIGIN_INTERFACE_EVIDENCE`, explicitly excluded from the runtime connector. No baud scan, 115200 fallback, bootloader or upload support exists |

This leaves the R1 corpus result unchanged:

```text
REAL_CAPTURE_EXISTENCE_CORROBORATED
PUBLIC_RAW_CORPUS_NOT_FOUND
REAL_FIELD_CAPTURE absent
```

No source document, screenshot, binary or external terminal transcript was
copied into the repository. All executable R2 bytes are marked
`LIFTASSISTANT_SYNTHETIC`.

## R2 typed terminal boundary

The public query surface is exactly:

```text
LceReadOnlyQuery::Status       -> STA + carriage return
LceReadOnlyQuery::CommandState -> CMD + carriage return
LceReadOnlyQuery::Mode         -> MDE + carriage return
```

There is no raw/string command, generic write, memory, parameter, call, test,
escape, pagination or unlock API. `LceTerminalTx` has private construction and
can only expose bytes generated from one of those three variants. The session
never automatically polls or retries. R2.3 distinguishes the historical
main-level `?` prompt from the `VTEST1` observation context: `?` may
establish interface availability but cannot authorize `STA`/`CMD`/`MDE`;
only an exact `VTEST1` boundary permits those typed queries. Returning to
`?` revokes query readiness. The connector still never sends the historical
printer/V-test entry sequence, RETURN-for-next-page, Escape or Ctrl-C.

`LceRuntimeTerminalProfile::historical_9600_8n1()` is an explicit profile type,
not parser state. The reviewed 9600/8N1 value is a historical/vendor-evidenced
default and remains unvalidated on the target CPU561. No constructor or
fallback exists for the separate 115200 servicing profile.

`LceInterfaceState` preserves `Unknown`, `Available`, and
`LockedOrUnavailable`. An exact known boundary can move an unknown synthetic
session to available. A configured locked/unavailable interface fails before
query generation. The connector contains no LCEUIO algorithm, counter reset,
unlock sequence or license/tool bypass.

Capture metadata continues to distinguish construction-time interface
knowledge from later terminal evidence. `initial_interface_state` is exactly
the caller's knowledge when the session/capture is created and is never
rewritten. R2.3 capture format v4 replaces the formerly ambiguous generic
boundary with `MainLevelBoundary` and `VTestBoundary`: either can establish
that an initially unknown interface responded, while only `VTestBoundary`
authorizes observation queries. No redundant final-state field or second
interface-state history is stored.

## R2 session and evidence lifecycle

The synchronous session represents:

```text
Disconnected
  -> SessionStarting
  -> AwaitingKnownBoundary
  -> MainLevelReady        on exact "?"
  -> VTestReady            only on exact "VTEST1"
  -> Transmitting(query)
  -> AwaitingResponse(query)
  -> ReceivingResponse(query)
  -> VTestReady

VTestReady -> MainLevelReady on exact "?"

any unsupported/ambiguous/timeout path -> FailedClosed(reason)
```

It accepts arbitrary RX fragmentation and delegates every table byte to the R1
decoder. Banners and non-interactive noise are preserved without becoming
state; exact fragmented echo is classified but optional. A supported-family
mismatch, `GRP`/`CAL`/`N`/`SPB`/`RST`, unknown interactive prompt, malformed or
oversized input, timeout, partial table or disconnect mid-response stops query
issuance. End-of-stream completion is retained as raw evidence but is not
released as an observation-bearing frame. Reconnection requires a new
`ObservationSessionId`; a disconnected old session rejects late bytes, while
Engine source-session fencing remains authoritative for canonical ingestion.

The response timeout is supplied explicitly by the caller. R2 establishes no
universal LCE timing value and has no background clock, task or retry loop. In
R2.2 it starts only after the complete typed query has been reported accepted
by the caller-owned transport, not when the query is merely prepared. The
caller supplies a per-session monotonic relative clock beginning at zero.

Every session owns a bounded internal evidence recorder containing:

- format version, session ID, capture origin, initial interface state and profile;
- monotonic relative microsecond offsets;
- exact RX bytes plus transport-accepted TX prefixes with typed query correlation;
- structured session/parser markers;
- explicit completion/disconnect/fail-closed reason;
- SHA-256 over the canonical capture body.

The internal JSON envelope is versioned and experimental. R2.1 introduced v2
for `initial_interface_state`; R2.2 introduced v3 for prepared versus
transport-accepted TX evidence; R2.3 bumps the format to v4 because terminal
context is now explicit. Older synthetic schemas are rejected rather than
silently reinterpreted. Artifact loading requires `VTestBoundary` before a
prepared observation query; `MainLevelBoundary` alone is insufficient.
Changing terminal context while a query is pending is rejected.
Transport-accepted bytes must remain an exact non-empty prefix of the private
typed query, and response correlation begins only after all four bytes were
accepted. A short accepted prefix may exist only as evidence on a fail-closed
path. Limits remain 128 bytes for session identity,
4,096 entries, 64 KiB per raw entry, 1 MiB aggregate raw bytes and 5 MiB
serialized form. Deserialization revalidates version, profile, timestamps,
typed-query lifecycle, cardinality, sizes and SHA-256. The checksum detects
artifact mutation; it is not a digital signature or proof of source identity.
Replay feeds only RX entries through a fresh R1 decoder; TX and markers never
become observations. No site, building, passenger or controller identity is
recorded by default.

## R2 PTY validation boundary

The Linux integration test creates a real kernel pseudo-terminal with `rustix`,
puts its slave in raw mode, and runs a scripted fake LCE peer. It exercises
whole, bytewise and irregular reads; fragmented CRLF; startup noise;
main-level `?` and V-test `VTEST1` context boundaries; echo/no echo;
`STA`/`CMD`/`MDE`; delayed response; multiple sequential records; partial
response; disconnect; reconnect/new session; and deterministic R1 replay.
R2.2 uses a single PTY `write` result as transport-acceptance evidence before
advancing the session, rather than recording TX at query preparation. R2.3
proves that main-level `?` alone cannot prepare a query and that a later
`?` revokes V-test query readiness. The fake and every fixture are explicitly
`LIFTASSISTANT_SYNTHETIC`.

Portable session/recorder tests retain the semantic coverage when a non-Linux
target does not compile the PTY integration test. `rustix` is a dev-only PTY
dependency; there is no serial/USB runtime dependency or device enumeration.
Normal dependencies add existing-workspace `serde`/`serde_json` for the bounded
internal evidence artifact and the small RustCrypto `sha2` implementation for
artifact integrity. No async runtime, daemon, SDK or adapter framework was
introduced.

Closing validation passes formatting, warning-free workspace all-target
Clippy, 245 workspace tests with one expected ignored soak child, the exact
Rust/Cargo 1.85.0 locked all-target workspace check, 42 focused LCE unit tests,
two Linux PTY integration tests and the compile-fail public-API regression.
The existing scenario suites were not changed or separately rerun by these
focused R2.2/R2.3 patches. No live RMF,
Podman, serial device, CPU561, XDB1, LCEUIO or external peer was used.

R2 still routes accepted bytes only through:

```text
terminal session -> exact RX -> R1 decoder -> R0 native mapper
  -> CarObservationUpdate -> explicit Engine binding/session
```

No Core or Engine domain type, authority rule, Intent, Principal, grant or
write path changed.

## R2.2 transport-evidence hardening

R2.2 closes the distinction between application intent and transport evidence:

```text
KnownBoundary
  -> QueryPrepared(query)
  -> Transmitting(query)
  -> TRANSPORT_ACCEPTED_TX exact prefix
  -> QueryTransportAccepted(query) only after all 4 bytes
  -> AwaitingResponse(query)
```

`LceTerminalTx` remains privately constructible and is bound to its
`ObservationSessionId` plus a session-local sequence, so a stale token from
another session cannot confirm a new query. Callers report only the number of
bytes one write attempt accepted; the session derives the corresponding prefix
from the typed token and never accepts arbitrary caller-provided TX bytes.
Zero/failed and partial writes fail closed, and no automatic retry can turn a
partially accepted terminal command into a different interaction.

This distinction follows ordinary byte-stream semantics rather than a KONE
claim. Rust `Write::write` may accept fewer bytes than requested; serial APIs
commonly distinguish write acceptance from a later flush/drain; Linux serial
and SocketCAN documentation likewise distinguish queue/driver acceptance from
stronger transmission confirmation. Modbus serial guidance keeps a single
request pending and starts response waiting after request transmission. These
are architecture priors only; they do not establish CPU561 runtime behavior.

R2.2 intentionally stops at **transport accepted**. A future real serial backend
may record a separate drain confirmation if its platform can support one, but
neither transport acceptance nor OS drain is to be mislabeled as independent
physical-wire observation. No serial backend, `/dev/tty*` access, USB
enumeration, LCEUIO behavior or controller access is introduced here.

## R2.3 terminal-context hardening

The historical KONE terminal instructions distinguish the terminal's main
level from the V-test monitoring context. The main level presents `?` and
lists test-program selectors, while the documented V-test flow reaches a
`VTEST1` context in which `STA`, `CMD`, `MDE` and other V-test
functions are used. Because the whole terminal surface also contains active
functions, LiftAssistant must not treat a bare main prompt as observation
authority.

R2.3 therefore models only evidence that the controller itself supplied:

```text
?       -> MainLevelBoundary -> MainLevelReady
VTEST1  -> VTestBoundary     -> VTestReady
```

`MainLevelReady` records an available terminal interface but
`issue_query()` fails there. Only `VTestReady` permits the existing
`STA\r`, `CMD\r` and `MDE\r` allowlist. Observing `?` again moves the
session back to `MainLevelReady` and immediately revokes that query
readiness. No R2.3 code generates the historical `V`/space entry sequence,
printer-mode entry, RETURN pagination, Escape, Ctrl-C or another navigation
keystroke.

This means a future first field session can truthfully stop after capturing a
main-level prompt instead of gambling that an observation command is valid in
that context. If the controller is already in or independently exposes
`VTEST1`, the typed observation-query path can be considered separately under
the R3 HUMAN gate. If entry into V-test is required, that navigation remains a
new evidence/authorization decision rather than an implicit R2 behavior.

## R2 result and next evidence gap

R2 is complete at `PTY_READONLY_CONNECTOR_PARTIAL`. This means the typed
observation-only query surface, bounded fail-closed session, synthetic PTY
transport proof, raw evidence finalization/checksum/replay and existing
R1/R0/Engine path are implemented. It does not mean `REAL_LCE_COMPATIBLE`,
`CPU561_VALIDATED`, `LCEUIO_VALIDATED`, `LIVE_SERIAL_VALIDATED` or
`PRODUCTION_READY`.

The next gate is `LCE-R3 — FIRST REAL READ-ONLY EVIDENCE ACQUISITION`. It needs
separate HUMAN authorization and must begin with evidence preservation. Exact
firmware banner, prompt variants, echo, CR/LF, screen layout, timing,
pagination, lock state and LCEUIO pass-through remain unknown. The reviewed
historical LCE terminal profile establishes baud/data/parity/stop-bit and
null-modem prior art, but the current evidence dossier does not independently
establish a runtime hardware/software flow-control setting; any real serial
backend must therefore make that choice explicit rather than inherit an
unstated library or OS default.

## HUMAN gate

No authorization follows from R1/R2 for real serial hardware, `/dev/tty*`,
USB-RS232, XDB1, LCEUIO, CPU561 connection, unlock, live terminal queries,
passenger equipment, controller probing or controller writes. Crossing any of
those boundaries requires a separate explicit HUMAN decision.
