# Saved package runtime state

Work in progress on ess-package-state, based on master 0baadcf and the
14 save-crate prerequisite files from the reviewed ess-npc-changes tree.
The reviewed 57-file tree remains unchanged. This branch does not contain
its game-side inventory and NPC restoration changes.

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
denied. The full workspace gate passes with 1,114 Rust tests and 11 Python
tests. No optional tools were skipped and no warnings were reported.

Named procedure-state decoding, nonzero package-data prefixes and other
package kinds remain unfinished. Runtime package restoration and execution
are separate requirements. These decoder results do not establish full
actor inventory restoration or a resumed game. The zero-failure criterion
must remain in place as the missing layouts are implemented.
