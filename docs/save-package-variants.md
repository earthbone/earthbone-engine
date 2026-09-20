# Saved package variant manifest

SAV-22 classifies the undecoded PackageData in the 60-save corpus at the
`4e03b82` baseline. The versioned
[manifest](save-package-variants-v1.tsv) accounts for all 1,154,526 reference
sections. Of these, 1,154,445 decode and re-encode exactly. Every one of the
81 failures has an occurrence row and an explicit unsupported reason below.
The two observed discriminator/name classes are Sequence, with 72 occurrences,
and Stacked, with nine. No other decoder error is accepted by this survey.

These are header classes, not proof that their opaque payloads have one layout
per name. Every occurrence also carries `OPAQUE-CONTINUATION`, accounting for
all unclassified bytes through the enclosing change-form end. This includes
possible nested procedures, further extras, inventory and actor state whose
individual boundaries cannot yet be established. None is treated as decoded,
discarded or assigned a guessed package length.

## Classification and unsupported reasons

Offsets below are relative to the extra-data type byte. The manifest's
absolute offsets are relative to the decompressed change-form payload.

| Identifier | Observed header | Boundary or explicit unsupported reason |
|---|---|---|
| `PD18-SEQUENCE` | Type 108, kind 18, prefix 0, eight-byte Sequence name | `SEQUENCE-LAYOUT`: observed header occupies 13 bytes including the type. S1 leaves nonempty data undocumented. S2 and S3 have no saved schema. S4 and S5 expose classes and unknown virtual functions, not serialized fields. The node-state count, recursion and end are unknown. SAV-12 owns decoding |
| `PD18-STACKED` | Type 108, kind 18, prefix 0, seven-byte Stacked name | `STACKED-LAYOUT`: observed header occupies 12 bytes including the type. S1, S2 and S3 supply no nonempty schema, and S4 does not establish a Stacked wire layout. Child counts, child boundaries and the outer end are unknown. SAV-21 owns decoding |
| `OPAQUE-CONTINUATION` | Every byte after either observed name, through the recorded enclosing payload length | No source-backed package end exists. This region may contain additional package subvariants and later fields. Its enclosing end follows the change-form length, not a guessed package boundary. No byte scan, byte replacement or resynchronization is used to classify it |

The known prefix/name widths are observations already used by the empty-state
reader. They are not source-backed semantics for the prefix byte or named
procedure bodies. A matching name cannot authorize decoding its suffix.

Other discriminator classes are explicitly outside the observed set:

| Class | Corpus evidence | Handling and reason |
|---|---|---|
| Kind 255 | Already supported | S1 documents the one-byte terminator after type 108 |
| Kind 18, prefix 0, empty name | Already supported | The earlier 88-entry empty-state survey established the five-byte entry including type and kind. See [package state](save-package-data.md#observed-empty-variant) |
| Kind 18, nonzero prefix | No encountered failures | Unsupported because the prefix's meaning and dependent field order are undocumented in S1 to S5 |
| Kind 18, other nonempty name | No encountered failures | Unsupported because no saved schema identifies its state fields or end |
| Kind other than 18 or 255 | No encountered failures | S1 allows nonempty data but supplies no layout. An in-memory package enum does not establish saved field order |
| Truncated kind, prefix or name | No encountered failures | Malformed input, not a supported variant. Existing truncation tests require an error |

The absence claims concern encountered headers. They do not classify bytes
inside `OPAQUE-CONTINUATION`. An additional kind, prefix, name, truncation or
unrelated error makes the survey fail instead of entering a catch-all bucket.

## Local source evidence

Paths are relative to `../engine-refs/` from the repository root. These files
were read directly. No new parser dependency or reference implementation was
needed for this classification, which reuses the existing Rust reader.

| Source | File and inspected declaration | What it establishes |
|---|---|---|
| S1 | `uesp-savefile/Extra_data.wikitext`, type 108 | A discriminator byte, with 255 terminating the entry. The nonempty payload is explicitly undocumented |
| S2 | `TES5Edit/Core/wbDefinitionsTES5Saves.pas`, `wbExtraTypeEnum` and `wbChangedExtraData` | Names PackageData but does not define its payload. Other package extras are separate types |
| S3 | `FallrimTools/src/main/java/resaver/ess/ChangeFormExtraDataData.java`, switch cases 106, 111 and default | No case 108. Its reader cannot supply a package boundary |
| S4 | `CommonLibSSE-NG-alandtse/include/RE/C/CustomActorPackageData.h` and `RE/I/IProcedureTreeExecState.h` | Runtime package data and procedure-state interfaces. Unknown virtual methods do not identify serialized widths or order |
| S5 | `CommonLibSSE-NG-alandtse/include/RE/B/BGSProcedureTreeSequence.h` | A Sequence procedure class with Execute and an unknown method. No saved-state implementation |

Source snapshots are identified by SHA-256 of the files actually read. This
also identifies local declarations that might differ from their Git revision.

```text
S1 Extra_data.wikitext
25c39c2df50cc44dfa76bafcdcdfa55e15abf59b50a5da27aae7f5fde156b66f
S2 wbDefinitionsTES5Saves.pas
229cc3df9a8daacf68f0f1da511acff02a96e3803ba158726aac1748a9b7fdec
S3 ChangeFormExtraDataData.java
a5aa4670617e3acafca429f9021b5d202f8d0717866b76141bc61f6f31bda0eb
S4 CustomActorPackageData.h
7f2599e0d35b687e942dbacf126e4ffe4b22e26a5495303587a049ec7e7b22a6
S4 IProcedureTreeExecState.h
463a2175d1d84cda7eb49ba4639762b8c83d73418441812403306279d991f42c
S5 BGSProcedureTreeSequence.h
55a094023d499d55289b1e7708b57cfbc886e3d2eac2ebb3cbb04beefa054ad7
```

## Manifest schema and acceptance

Version 1 is UTF-8, tab separated, with one record per line and no quoting.
Paths are relative to `SOE_TEST_SAVES`. The manifest retains filenames,
reference identities, flags, counts and enclosing offsets. It contains no
save payload bytes. It is a diagnostic index, not a redistributed save.

| Row type | Columns after the row type |
|---|---|
| `manifest` | Format identifier, version |
| `corpus` | Save count, reference-section count, exact decoded-section count |
| `variant` | Variant identifier, occurrence count, support status, reason identifier |
| `occurrence` | Save path, raw saved RefID in hex, resolved saved FormID in hex, change flags in hex, package start offset, enclosing payload end offset, variant identifier, continuation reason |
| `save` | Save path, reference-section count, failure count |

Occurrence and save rows are sorted as complete text lines. The manifest
lists saves with zero failures as well as saves with blockers. Saved FormIDs
are in the saved load-order namespace, not a current runtime identity.

Before the first run, the test fixed the existing baseline: exactly 60 saves,
1,154,526 reference sections, 1,154,445 exact typed re-encodings, 72 Sequence
failures and nine Stacked failures. It validates each error offset against
the already consumed header, then compares every ledger row. A mismatch
prints a candidate ledger for review. Updating a manifest is a reviewed
evidence change, not a reason to relax the zero-failure decoder check.

```bash
rtk proxy env SOE_TEST_SAVES=/path/to/external/saves \
  cargo test -p soe-save --test package_manifest -- --nocapture
```

Without the corpus, two synthetic/manifest checks run and the corpus check
returns early with a skip message. The existing
`corpus_reference_contents_decode_with_exact_typed_reencoding` test still
requires zero errors. Its expected 81 failures remain a decoding blocker.
SAV-22 establishes a complete ledger of encountered failures and explicit
reasons for their unresolved continuations. It does not complete SAV-12,
SAV-21, runtime package restoration or full ESS resumption.
