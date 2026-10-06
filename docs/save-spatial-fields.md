# Saved cell, location and encounter-zone fields

SAV-27 decodes CELL fields with serialized layouts in xEdit, including its
seen-data shape. The named CELL flags 28 to 30 are exposed as typed markers,
and bytes after the defined fields remain an exact opaque suffix. ECZN and
LCTN expose their named changed markers and common form flags. Their
placeholder-union bytes remain an exact opaque tail. SAV-28 applies supported
fields to live records and retains opaque bytes through engine save/load.
Opaque bytes do not identify field widths or internal values.

## Source boundary

SAV-66 audit result: the named public sources establish several flag names and
runtime fields, but they do not establish the requested serialized boundaries
for every high-flag combination. Do not treat these runtime declarations as a
save payload schema.

Local xEdit `6dbbed51d92456e39ffb916eece56040e91114b5`,
`Core/wbDefinitionsTES5Saves.pas`, supplies CELL's u16 flags, length-prefixed
full name, owner RefID and two seen-data shapes. Its CELL dispatch orders
the common form flags, cell flags, seen data, full name and ownership. CELL
flags 28 to 30 have named unions with `wbNull` alternatives, but xEdit omits
those unions from the CELL dispatch. They therefore add typed flag state
without a serialized field width in this schema.
The exterior seen-data shape has 32 bytes. The interior shape has counted
entries containing two single-byte fields and a 32-byte array.

`ChangedFormCellIsInteriorDecider` selects a shape from the remaining payload
length after adjusting for name and ownership. It does not consult an authored
cell record. This heuristic needs validation against independent cell identity
and complete payload boundaries before it becomes an engine decoder contract.

ECZN's Zone Flags and Game Data unions and LCTN's KeywordData and Cleared unions
are `[wbNull, wbNull]`. The decoder treats these as zero-payload markers and
retains remaining bytes without assigning them to a marker. These bytes still
need controlled fixtures or a public serialized schema before they can be
split into typed fields.

The CommonLibSSE-NG headers confirm runtime CELL flag names, the LCTN runtime
keyword-data entry shape and cleared booleans, and ECZN runtime data and game
data member types. They declare `SaveGame` and `LoadGame`, but the checked
headers do not define those method bodies. Those runtime shapes therefore do
not establish the corresponding ESS encodings. FallrimTools
`ChangeForm.java::getData` also has no typed dispatch for these records and
retains their bytes as raw payloads. No controlled vanilla save fixtures in
this worktree isolate the flagged sections.

FallrimTools `61bb57d895f023617cb55045a4907849fd1ff566`,
`src/main/java/resaver/ess/ChangeForm.java::getData`, has no CELL, LCTN or ECZN
typed dispatch. Its fallback retains raw bytes. The local UESP
`Save_File_Format.wikitext` identifies these change-form type indices, but the
consulted save-format pages do not provide their complete payload schemas.
The local skymp savefile readers supply no additional decoder for these types.

CommonLibSSE-NG `b93280e832f263dbef44e44cbe2936622a02f91a`,
`TESObjectCELL.h`, `BGSLocation.h` and `BGSEncounterZone.h` name runtime flags,
fields and save/load method declarations. They do not provide those method
bodies or serialized field order. In particular, a 16-byte runtime encounter
zone structure does not prove that a 16-byte ESS payload has the same fields
or padding. No Bethesda executable was inspected or disassembled.

## Corpus measurement

`crates/save/tests/spatial_change_coverage.rs` requires at least 50 saves,
with that threshold set before the first run. It visits every change form,
groups every CELL, LCTN and ECZN by save version, change version and flags,
and records counts, decompressed sizes and size distributions. It reparses
the rewritten save and checks exact target headers, raw payloads and inflated
payloads. This proves container preservation, not typed field interpretation.

The 2026-09-20 run passed in 16.65 seconds after compilation. It inspected
60 saves and 1,344,172 total change forms. All target records have save and
change-form version 78, and none have individually compressed payloads.

| Type | Records | Payload bytes | Observed flag combinations |
| --- | ---: | ---: | ---: |
| CELL | 78,806 | 1,676,067 | 9 |
| LCTN | 4,135 | 65,820 | 2 |
| ECZN | 573 | 9,168 | 1 |

Of the CELL records, 71,027 set at least one of the high flag fields.
The 50,511 records with flags `60000000` each have eight bytes. That length does
not assign those bytes to Exterior Char or Detach Time. No observed CELL sets
Exterior Short or Full Name, so those combinations also lack corpus coverage.

All 573 ECZN records set `80000000` and have 16 bytes. LCTN has 4,008 records
with `40000000` and lengths 1, 8, 15, 22 or 29 bytes, and 127 records with
`80000000` and two-byte payloads. These lengths do not identify a count type,
reference encoding, flag operation or serialized member order.

## Evidence needed to continue

SAV-66 requires public Skyrim field definitions or controlled vanilla fixtures
that isolate the missing CELL fields, LCTN keyword/cleared state and ECZN flags
and game data. Record the runtime and save versions, exact operations and
independently observed state. Include single flags, combinations, interior and
exterior cells, variable counts and unchanged control saves. Account for every
intervening field before assigning offsets or meanings.

Synthetic tests assert the source-defined layouts and each named zero-payload
marker. They check truncation rejection for CELL fields with known widths and
the common form flags. The census accounts for every target record, but it does
not turn opaque suffixes into typed fields or establish internal truncation
rules for those suffixes. No user save was modified or copied into this
repository.

`ChangeForm::cell_change` decodes CELL bits 0 to 3 and 31 in xEdit's order:
common flags, cell flags, seen data, full name and ownership. It checks the
remaining bytes against the 32-byte exterior shape or repeated interior
entries of two one-byte fields plus 32 bytes, then validates the following
name and owner. A truncated interior that leaves exactly 32 bytes remains
indistinguishable from exterior data. Bits 28 to 30 are typed presence markers
whose bytes are not laid out in xEdit's CELL schema. A suffix on such a form is
kept opaque and re-encoded exactly.

`ChangeForm::encounter_zone_change` accepts ECZN bits 0, 1 and 31, and
`ChangeForm::location_change` accepts LCTN bits 0, 30 and 31. They decode the
six-byte common form flags when present and expose their named zero-payload
markers. Remaining bytes stay opaque and re-encode exactly.
`sav_27_spatial_changes.rs` covers source-defined flag combinations, marker
state, exact re-encoding and truncation of fields with known widths. The
external corpus survey reports per-type flags and payload sizes and tries the
typed decoders for every target record.

The survey also checks exact bytes for every successfully decoded record. Its
60-save run accounts for all CELL, LCTN and ECZN records. Opaque tails are
preserved as bytes and are not counted as decoded fields.
