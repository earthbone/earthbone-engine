# Saved cell, location and encounter-zone fields

SAV-27 remains blocked on SAV-66 as of 2026-09-20. The source audit and corpus
census below do not implement a typed decoder or live restoration. SAV-28
continues to depend on SAV-27. Its original completion check is unchanged.

## Source boundary

Local xEdit `6dbbed51d92456e39ffb916eece56040e91114b5`,
`Core/wbDefinitionsTES5Saves.pas`, supplies CELL's u16 flags, length-prefixed
full name, owner RefID and two seen-data shapes. Its CELL dispatch orders
the common form flags, cell flags, seen data, full name and ownership.
The exterior seen-data shape has 32 bytes. The interior shape has counted
entries containing two single-byte fields and a 32-byte array.

`ChangedFormCellIsInteriorDecider` selects a shape from the remaining payload
length after adjusting for name and ownership. It does not consult an authored
cell record. This heuristic needs validation against independent cell identity
and complete payload boundaries before it becomes an engine decoder contract.

CELL bits 28, 29 and 30 are named Exterior Short, Exterior Char and Detach Time,
but their unions contain `wbNull` placeholders and the CELL dispatch omits them.
ECZN's Zone Flags and Game Data unions are placeholders. LCTN's KeywordData and
Cleared unions are also placeholders. A flag name does not establish its field
width or a zero-length payload. The current corpus contains nonempty payloads
using these flags.

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

Of the CELL records, 71,027 set at least one of the undefined high flag fields.
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

Once those boundaries are established, SAV-27 can implement typed fields,
exact encodings and malformed-input checks for all supported combinations.
The census does not satisfy that decoder completion check. No user save was
modified or copied into this repository.
