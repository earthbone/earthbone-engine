# Saved base-object value and name fields

SAV-53 is established from xEdit's public Skyrim save definitions. The
`CHANGE_BASE_OBJECT_VALUE` field uses mask `0x00000002`, and
`CHANGE_BASE_OBJECT_FULLNAME` uses mask `0x00000004`. Both flags select a
zero-byte field. The optional generic form-flags field comes first and uses six
bytes. This fixes the boundary before later type-specific fields without
claiming that the value or name is serialized in the save.

## Established and missing boundaries

| Section | Source evidence | Boundary |
|---|---|---|
| Generic form flags, bit 0 | xEdit `wbUnionCHANGE_FORM_FLAGS` and UESP ChangeFlags | One 32-bit word followed by one 16-bit word, six bytes in total |
| Base-object value, bit 1, mask `0x00000002` | xEdit `wbUnionCHANGE_BASE_OBJECT_VALUE` | Zero bytes |
| Base-object full name, bit 2, mask `0x00000004` | xEdit `wbUnionCHANGE_BASE_OBJECT_FULLNAME` | Zero bytes |

xEdit orders the generic flags, value and name unions in that sequence for
the base-object types it describes. Both alternatives of each value/name
union are `wbNull`. In the same file, `wbNull` is defined as
`wbByteArray('Unused', -255)`. xEdit's `TwbByteArrayDef.GetSize` maps `-255`
to zero with the comment `Explicitly null for wbNull`. Thus the flags consume
no payload bytes, and the next field begins directly after the optional
generic form-flags section. `ChangedFlag01Decider` selects the value union
from bit 1, while `ChangedFlag02Decider` selects the name union from bit 2.

The save crate exposes this boundary through
`ChangeForm::base_object_fields`. It reads the optional six-byte form-flags
section, reports whether either zero-byte field is flagged, and returns the
remaining byte count for the dependent decoder. It does not claim to recover
the value or name themselves.

The same skeleton appears for ACTI, TACT, ARMO, BOOK, CONT, DOOR, LIGH, MISC,
APPA, STAT, MSTT, FURN, WEAP, AMMO, KEYM, ALCH, IDLM, NAVM, WOOP, MGEF and
ENCH. SAV-33 uses this boundary for ARMO, WEAP and AMMO. Their named value
and full-name fields are zero bytes in xEdit, while unnamed flag bits and
trailing bytes remain uninterpreted. INGR, NOTE and PACK have different
type-specific sections. Other types still need their own complete field and
flag evidence, including TACT's speaker and BOOK's teaches/read sections.

No field width is inferred from an in-memory C++ member, a plugin record's
`DATA` or `FULL`, a different game's save, or a successful raw-byte round
trip. NPC names and reference display-name extras are separate contracts.

## Source audit

Local sources are under `../engine-refs/` relative to the main checkout.
Public source files were inspected at the revisions below. No reference
repository was modified and no external implementation was copied.

| Source and exact revision | Files or symbols inspected | Finding |
|---|---|---|
| xEdit, `6dbbed51d92456e39ffb916eece56040e91114b5` | `Core/wbDefinitionsTES5Saves.pas`, `wbChangeFlags012`, both target unions, `wbNull` | Defines masks 2 and 4, the field sequence, and zero-byte alternatives. |
| xEdit, `6dbbed51d92456e39ffb916eece56040e91114b5` | `Core/wbInterface.pas`, `TwbByteArrayDef.GetSize` | Defines `-255` as zero bytes explicitly for `wbNull`. |
| xEdit public file revision, `973b80ec555f2fbdc5ee8c1a15113d23ecb6ea2d` | [Skyrim save definitions](https://github.com/TES5Edit/TES5Edit/blob/973b80ec555f2fbdc5ee8c1a15113d23ecb6ea2d/Core/wbDefinitionsTES5Saves.pas#L5526), plus the five most recent file-history entries | The latest returned file revision retains the same zero-byte alternatives. Recent save-loader changes do not add serialized values or names. |
| xEdit, `6dbbed51d92456e39ffb916eece56040e91114b5` | `Core/wbDefinitionsFNVSaves.pas`, the two corresponding unions | Defines a 32-bit value and a length-prefixed name for Fallout New Vegas. These definitions are not Skyrim evidence. |
| FallrimTools, `61bb57d895f023617cb55045a4907849fd1ff566` | `src/main/java/resaver/ess/ChangeForm.java::getData` and `ChangeFormDefault.java` | No typed decoder for these base-object fields. The fallback retains uninterpreted bytes. |
| skymp, `756fb86b05ab5c2fae4f9bc3c6d52ca580b8bdd3` | `savefile/include/savefile/SFStructure.h::ChangeForm` and the `SFChangeFormNPC`/`SFChangeFormACHR` source inventory | Generic payload is a byte vector. Specialized change-form classes cover NPC and ACHR, not the target fields. |
| Wrye Bash, `e192d181aaf2f889fb2e4ae55cacb62d3ce0ba7a` | `Mopy/bash/bosh/_saves.py` | This body reader explicitly covers Oblivion. It cannot establish the Skyrim payloads. |
| Skyrim Save Lab, `dbb0b4e3243c7753ff4b5b049cef644e21d294a8` | [app/core/skyrim_ess.py](https://github.com/xcier/Skyrim-Save-Editor/blob/dbb0b4e3243c7753ff4b5b049cef644e21d294a8/app/core/skyrim_ess.py#L543), `raw_editor.py`, `inventory_lab.py` | The generic reader records headers, payload offsets and a hex sample. The inspected editing paths cover player data and inventory, not typed base-object value/name payloads. |
| TES-parser, `a8d69c595bf921c533aecc9ee1d95b3d5d3373bc` | [Parser_code.cpp](https://github.com/Larkin-man/TES-parser/blob/a8d69c595bf921c533aecc9ee1d95b3d5d3373bc/Parser_code.cpp#L189), `Headers.cpp`, `Headers.h`, `TableLoader.cpp`, `Locations.cpp` | Its inspected Skyrim detection follows a TES4 plugin header and HEDR. It supplies no definition of these ESS fields. |

The local UESP save-format snapshot names the change types and distinguishes
change payloads from plugin records. Its SHA-256 is
`dce068365d729f58b9f092fbe696e8161df9b5a7042fe5c9cf3d9cfe9c5983bb`
for `uesp-savefile/Save_File_Format.wikitext`. No matching value/name layout
was found in the local UESP save pages.

The public [UESP ChangeFlags revision 2308895](https://en.uesp.net/w/index.php?title=Skyrim_Mod:ChangeFlags&oldid=2308895),
dated 2021-01-07, was also retrieved through the wiki API. It identifies
the two base-object masks and defines the generic six-byte section. UESP's
local format conventions define null fields as zero bytes, consistent with
xEdit's explicit `wbNull` size handling.

## Why setter-only fixtures would be inconclusive

At SKSE64 revision `4cd2e34face74d5247fa38888c7542ad45d4a1d2`,
`skse64/PapyrusForm.cpp::SetName` and `SetGoldValue` assign the in-memory
components without calling `AddChange`. The local CK pages explicitly
describe those changes as unsaved:
[SetGoldValue revision 15466](https://ck.uesp.net/w/index.php?title=SetGoldValue_-_Form&oldid=15466)
and [SetName revision 15565](https://ck.uesp.net/w/index.php?title=SetName_-_Form&oldid=15565).
Their lack of an ESS payload would not establish that a flagged value/name
section is empty.

CommonLibSSE-NG revision `b93280e832f263dbef44e44cbe2936622a02f91a`
declares `TESForm::AddChange` in `include/RE/T/TESForm.h`. That declaration
is a possible route for a future controlled probe, not a serialization
specification or proof that a given form honors either flag. No such probe
was built or run in this checkpoint. No Bethesda executable was disassembled.

## Corpus evidence and the completion requirement

The SAV-33 survey, documented in its `docs/save.md` section
`ARMO, WEAP and AMMO change coverage`, read all 60 external saves and found
zero ARMO, WEAP or AMMO change forms. Its test is
`sav_33_corpus_reports_item_change_field_coverage` in
`crates/save/tests/item_change_coverage.rs`. The survey reports type, flags,
payload size and count, then decodes and re-encodes observed item payloads.
It does not establish coverage for other base-object types. No user save is
copied or changed.

The public xEdit definitions and UESP null-field convention establish the
SAV-33 layout for the named value and name bits. Controlled reference-game
fixtures would be needed if a future source changes either field from `wbNull`.
Such fixtures must establish the following before a decoder can consume such
new content:

1. Record game build, save version, change-form version, load order, source
   form identity, authored probe and original/new values. Preserve the
   external saves in place and record file hashes for reproducibility.
2. From the same baseline, emit value-only, name-only and combined changes.
   Verify the actual saved masks and decoded record identities independently
   of the operation used to request each change. A missing change record or
   missing target bit is a coverage failure, not a zero-byte field.
3. Use several distinct numeric values and names of different byte lengths,
   including empty and non-ASCII cases where accepted. Isolate any length
   prefix, terminator, byte order and field order. Account for every byte
   and verify boundaries with and without the generic form-flags section.
4. Reload with the reference game and verify restored values independently
   of the new parser. Reset probe state between samples. Artificially
   marking a flag must be reported separately from ordinary game behavior.
5. Establish which record types and versions share the observed layout.
   Preserve uncertainty for unobserved combinations. Dependent decoders must
   still pass exact re-encoding, truncation rejection and their corpus checks.

Only sanitized observations and authored probe material belong in this
repository. No fixtures with this provenance were located during the audit.
SAV-31, SAV-35, SAV-37, SAV-39, SAV-41, SAV-43, SAV-47 and SAV-49 can use
the established zero-byte boundaries. Their payload decoders still need
their own field evidence and corpus checks. SAV-33's ARMO, WEAP and AMMO
decoder is documented in `save.md`.
