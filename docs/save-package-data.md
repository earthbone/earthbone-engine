# Saved package runtime state

The empty-state decoder originated on ess-package-state. It is now prepared
for integration on remaining-work-merge, based on master e9cc2f9. That base
already contains the inventory, NPC restoration and melee increments. The
original package review worktree remains unchanged.

## Sources and unresolved layout

The local UESP `uesp-savefile/Extra_data.wikitext` documents ExtraPackageData
as type 108, followed by a byte. Byte 255 ends the entry. The nonempty
payload is explicitly left undocumented. Local xEdit
`Core/wbDefinitionsTES5Saves.pas` names PackageData but does not define that
payload. FallrimTools `ChangeFormExtraDataData.java` has no case for it.

CommonLibSSE-NG `TESPackage.h` names package type 18. Its `ExtraPackageData.h`
points to ActorPackageData. The alandtse fork adds ActorPackageData and
CustomActorPackageData declarations, including a package-data list and an
unidentified pointer. They do not supply the saved field order. Those
in-memory declarations were not treated as a serialization format.

An external corpus survey found 169 rejected kind-18 entries across 60 saves
and 13 distinct actor identities. After the kind byte, every entry starts
with a zero byte followed by a length-prefixed name. The names are empty in
88 entries, Sequence in 72 and Stacked in nine. The meaning of the prefix
byte and the contents of named procedure state remain unverified.

## Observed empty variant

The reader now accepts type 108, kind 18, prefix byte zero and an empty name.
It retains the kind, prefix and empty string as typed fields. It rejects
other prefix values, other nonempty kinds and named procedure state. Type
255 retains the previously documented one-byte payload. No search for a
plausible next extra or inventory boundary is performed.

Before implementation, a diagnostic replaced each proposed empty entry with
the documented type-255 entry in memory. Every following extra and inventory
section then decoded and re-encoded exactly. This was evidence for the
three-byte boundary in all 88 observed entries, not a save-file edit or a
claim about the runtime semantics of the prefix byte. The 73 following
inventories contained 344 item entries, all with resolvable saved identities.
No save or changed save payload was copied into this repository.

Synthetic tests exercise both empty variants before a following AshPileRef
and a two-item inventory, preserving negative deltas and distinct stacks.
Every truncated prefix fails. Nonzero prefix bytes, unknown kinds and named
Sequence, Stacked and unknown trees remain errors.

## Validation and remaining work

The strict corpus acceptance test still requires zero decoding failures and
still fails. It now reports 81 failures, all named procedure trees, compared
with 169 before this increment. It reproduces 1,154,445 reference-content
sections exactly, including 174,623 inventories, 865,513 item entries and
844,557 stack lists. Six synthetic reference-content checks and all nine NPC
decoder checks pass. The latter include both external NPC corpus surveys.
The save crate's all-features/all-targets Clippy check passes with warnings
denied. The original isolated workspace gate passed with 1,114 Rust tests and
11 Python tests. No optional tools were skipped and no warnings were reported.

The integrated candidate based on e9cc2f9 passes the full tools/lint.sh gate
with 1,165 Rust tests and 11 Python tests, no warnings and no skipped optional
tools. Its repeated strict corpus run still fails on exactly 81 named procedure
states and reproduces the counts above. All nine NPC decoder checks and six
synthetic reference-content checks pass with the corpus available.

All 42 game-side save-restoration tests pass with the installed game and the
60-save corpus. The actor survey restores 114,769 inventories, 921,587 items
and 678,758 distinct stacks, excluding 53 records with unsupported contents.
This recovers 73 actor inventories compared with the prerequisite's 126
excluded records. Base inventory composition makes these runtime item totals
different from the raw decoder's saved entry counts. These remaining exclusions
are reported by the survey and do not weaken the strict decoder test.

Named procedure-state decoding, nonzero package-data prefixes and other
package kinds remain unfinished. Runtime package restoration and execution
are separate requirements. These decoder results do not establish full
actor inventory restoration or a resumed game. The zero-failure criterion
must remain in place as the missing layouts are implemented.

## SAV-21 Stacked source audit, 2026-09-20

SAV-21 is blocked on the serialized Stacked payload after its name. The
sources checked below identify runtime types but provide no saved field
widths, field order, child-state framing or entry termination. This audit
changes no decoder, encoder or acceptance criterion.

Paths below are relative to the external `engine-refs` directory.

| Source checked | Evidence and limit |
|---|---|
| `uesp-savefile/Extra_data.wikitext`, type 108 | Documents the kind byte and 255 terminator. The nonempty payload is explicitly unfinished. |
| `TES5Edit/Core/wbDefinitionsTES5Saves.pas`, `PackageData` | Names the extra type without defining its nonempty payload. |
| `FallrimTools/src/main/java/resaver/ess/ChangeFormExtraDataData.java` | No case 108. Cases 106 and 111 are adjacent, and unsupported types reach the default error. |
| `CommonLibSSE-NG-alandtse/include/RE/Offsets_RTTI.h` | Names `BGSProcedureTreeStackedExecState` and its `BGSTypedItem` specialization over `BGSProcedureTreeOneChildExecState`. Type identity and inheritance do not establish a saved schema. |
| `CommonLibSSE-NG-alandtse/include/RE/I/IProcedureTreeExecState.h` and `RE/C/CustomActorPackageData.h` | Declare unknown virtual methods and runtime storage. They contain no Stacked serialization implementation. |
| `skse64/skse64/GameRTTI.h` | Declares the Stacked and StackedExecState runtime type identifiers, without fields or a reader. |

The original CommonLibSSE-NG fork contains the same Stacked type names in its
RTTI table. Neither inspected fork has a Stacked class or saved-state reader
body. The public [libSkyrim RTTI header](https://github.com/himika/libSkyrim/blob/master/Skyrim/include/Skyrim/RTTI.h)
also registers the two types without defining a payload. No third-party code
or dependency was added. No game executable was inspected or disassembled.

The Stacked-specific local snapshots have these SHA-256 digests:

```text
CommonLibSSE-NG-alandtse/include/RE/Offsets_RTTI.h
5fd9d32567ec089ba153504edacda263de8f51bf631952312e17f7d46c611bce
skse64/skse64/GameRTTI.h
1e4b6fd36d42d84f1d6c58957d17c01df6a5ac6176b8062f7325748f4e10aaee
```

On branch `task-sav-21`, based on `4e03b82`, the repeated
`cargo test -p soe-save --test reference_contents -- --nocapture` with
`SOE_TEST_SAVES` set to all 60 external saves passed six synthetic checks.
The strict corpus check failed with exit 101 on exactly nine Stacked and
72 Sequence entries. Six Stacked entries begin at decompressed change-form
offset 59 and three at offset 60. These are entry starts, not their ends.
The accepted sections re-encoded 1,154,445 reference-content sections,
174,623 inventories, 865,513 entries and 844,557 stacks exactly. No save was
copied or changed.

Unblocking SAV-21 requires a published layout, a public reader or writer, or
controlled runtime observations that establish every field after the Stacked
name, the nested-state framing and the following extra-data boundary. The
resulting typed encoder must reproduce all nine observed entries and preserve
that boundary. Copying an opaque continuation or choosing a plausible next
extra does not establish the required layout.

SAV-12 owns Sequence decoding and SAV-22 owns the unresolved-variant manifest.
Neither supplies the missing Stacked schema, and this audit establishes no
prerequisite code dependency between the two decoders. SAV-13's package
restoration still requires both. The zero-failure corpus acceptance criterion
remains unchanged.
