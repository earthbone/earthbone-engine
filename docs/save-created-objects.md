# Created Objects tables

SAV-65 implements the source-defined type-4 reader, but remains blocked on
populated vanilla validation fixtures as of 2026-09-20. All 60 available saves
contain empty Created Objects tables. The predeclared corpus test requires at
least one populated block and fails that requirement. SAV-17 remains blocked
on SAV-65, with its original live-record restoration completion check unchanged.
Its other prerequisite, SAV-07, is complete on this branch.

## Source and implementation boundary

The local UESP `Created_Objects.wikitext` defines four consecutive tables:
weapon enchantments, armor enchantments, potions and poisons. Each table has a
VSVal object count. Each object has a three-byte RefID, a uint32 field named
`timesUsed`, a VSVal effect count and the effects. Each effect contains a RefID,
float magnitude, uint32 duration, uint32 area and float price, in that order.
An effect therefore occupies 19 bytes. Duration precedes area here, while the
plugin EFIT subrecord puts area before duration.

The local skymp revision `756fb86b05ab5c2fae4f9bc3c6d52ca580b8bdd3`
independently supplies matching reader and writer implementations in
`savefile/src/SFReader.cpp` (`FillCreatedObjects`) and
`savefile/src/SFWriter.cpp` (`WriteCreatedObjects`, `WriteEnchantment` and
`WriteMagicEffect`). `savefile/include/savefile/SFStructure.h` declares the
same fields. The component's `savefile/LICENSE` is GPL version 3. That source
was checked before implementing this GPL save-crate reader. No code or schema
from it was added to the MIT shim crates, and no dependency was added.

The UESP file SHA-256 is
`80787bef9a28c0f0cffa0513f37f77c5bcf4b15a69794df8da5959a74b80f287`.
The inspected skymp license SHA-256 is
`3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986`.
xEdit revision `6dbbed51d92456e39ffb916eece56040e91114b5` keeps type 4 as
raw bytes in `Core/wbDefinitionsTES5Saves.pas`. It supplies no competing typed
layout. Public source was read directly. No executable was disassembled.

`Save::created_objects` decodes the optional type-4 block in table one and
rejects duplicate blocks. `CreatedObjects::read` consumes exactly one block,
retains all RefID bits, float bits and original VSVal widths, and bounds
allocations by the remaining input size. It rejects truncated fields, invalid
count widths, impossible counts and trailing bytes. `data_bytes` rebuilds the
payload from fields and refuses inconsistent list lengths.

The parser does not normalize non-finite values or resolve form identities.
Those are validation decisions for restoration. The meaning of `timesUsed`
is uncertain in UESP, so it remains a stored integer. CommonLibSSE-NG revision
`b93280e832f263dbef44e44cbe2936622a02f91a` names a runtime reference count in
`BGSCreatedObjectManager::CreatedMagicItemData`. That memory structure does not
prove the saved field's runtime semantics.

## Validation and failed coverage requirement

Four synthetic tests cover all four tables, distinct duration and area values,
zero effects, empty tables, wider count encodings, signed zero and non-finite
float bits. They check every truncation point in the populated fixture,
impossible counts, reserved count-width tags, extra bytes and modified fields.
A fifth synthetic test in `roundtrip.rs` checks the optional Save accessor,
duplicate blocks and invalid payloads.

The external survey's thresholds were written before its first run: at least
50 saves, exactly one type-4 block per save and at least one populated block.
The first run passed four synthetic cases and decoded every external block,
then failed the populated-block requirement. Aggregate printing was moved
before the same assertion to retain the complete evidence. Population is
counted from raw table lengths, separately from reference resolution.

| Corpus observation | Result |
| --- | ---: |
| Saves and type-4 blocks | 60 |
| Save form version | 78 |
| Total type-4 payload bytes | 240 |
| Nonempty blocks | 0 |
| Weapon enchantments, armor enchantments, potions, poisons | 0 in each table |
| Effects | 0 |
| Exact block re-encodings | 60 |
| Created ACHR change forms | 1,572 |
| Created REFR change forms | 21,999 |
| Created PARW change forms | 1,406 |
| Created PHZD change forms | 1,286 |
| Created PMIS change forms | 40 |
| Created PCON change forms | 1 |
| Created RELA change forms | 547 |
| Created NPC_, ALCH or ENCH change forms | 0 |

The corpus run with `SOE_TEST_SAVES` exits 101, with four tests passing and
one failing its coverage requirement. The decoder does not fail on any of
the 60 blocks. Empty-block equality does not validate real object or effect
fields. No source save was copied into the repository or modified.

## Controlled fixtures needed for acceptance

Keep fixtures outside the repository and record the exact Skyrim executable
build, save form version, plugin list and crafting operations. Start with a
baseline save before crafting, then make paired saves after creating one weapon
enchantment, one armor enchantment, one potion and one poison. Record the
selected base item or ingredients, known effects, character skills and perks,
and visible results so each newly populated category can be identified
independently. Preserve the baseline and intermediate saves for comparison.

For each resulting type-4 block, record its outer declared length and all four
table counts. Confirm the changed table and independently inspect each object's
RefID, `timesUsed`, effect count, effect RefIDs, magnitude, duration, area and
price using the published skymp reader or another source-grounded reader.
UI rounding is not an independent observation of stored float bits. The Rust
typed encoder must reproduce the complete block, including original count
widths, and the test must pass with at least one populated block in a corpus
of at least 50 saves. Include all four populated categories to extend the
synthetic coverage to vanilla data. Do not lower the existing threshold.

The fixture's data boundary is the entire type-4 global block. A craft-produced
inventory delta, a changed record or a remembered runtime memory layout is
not a replacement for that block. Keep only sanitized observations and test
code in the repository. The positive vanilla fixture evidence is still missing,
so the original SAV-65 completion check is not claimed.

## SAV-17 restoration boundary

Type 4 stores magic effect lists rather than complete ENCH or ALCH plugin
records. It does not store an authored source record, a full name, a model or
all other plugin properties. The source audit does not establish how vanilla
chooses every missing property when it materializes a created base. The
CommonLib created-object methods forward to engine functions rather than
providing those construction rules. Synthetic base templates would guess them.

The existing world record-override reader also requires an authored NPC origin.
It cannot currently persist standalone created magic-item records. SAV-17
must establish the required construction semantics and store the resulting
records in the shared form-backed world before placement and inventory
references resolve. This decoder does not allocate live records, alter the
world's dynamic-ID counter or satisfy that restoration contract.
