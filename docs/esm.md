# soe-esm: plugin reader and form table

Reads Skyrim SE plugin files (`.esm`, `.esp`, `.esl`) into a group tree of records,
resolves FormIDs across a load order, and answers "which record wins" for any form.
Typed views exist for the six record types the first milestone needs, everything else
is kept as raw subrecords and written back unchanged.

## What the file format is

A plugin is a `TES4` header record followed by top-level groups, one per record
signature. Groups nest: a `WRLD` record is followed by a world-children group holding
exterior blocks, sub-blocks, `CELL` records and each cell's children group, which in
turn holds a persistent and a temporary group of `REFR`, `LAND` and `NAVM` records.
The parser keeps that tree (`Plugin::groups`) and stamps every record with the
worldspace and cell it sits under (`Record::placement`), because a `LAND` record has
no other link to its cell.

Records are lists of subrecords, each a four-byte tag plus a 16-bit size. Three quirks
are handled and tested: records with flag `0x00040000` hold a u32 inflated size and a
zlib stream, a subrecord larger than 65535 bytes is announced by an `XXXX` subrecord
carrying the real size, and player-visible strings in a localized file are string table
ids rather than text (`records::Text` keeps whichever it found).

Sources used for the layouts, in the order they were consulted:

- `engine-refs/openmw/components/esm4` for the header structures, group types, and the
  CELL, REFR, LAND and NAVM loaders.
- `engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas` for field definitions and flag
  names (TES4 header flags, LAND `DATA` flags, `wbNVNM`, STAT flags and `MNAM`).
- `engine-refs/TES5Edit/Core/wbImplementation.pas` for the light-plugin object id
  rule (`$00FFF000` must be zero).
- UESP's Mod File Format pages, from memory of earlier hand verification since the
  site blocks automated fetches.

## FormIDs and the load order

A FormID in a file is `index:object`. The index byte counts into that file's master
list, and an index at or past the end means the file itself. The same object is written
with a different index in every file that mentions it, so `LoadOrder` converts each to a
`GlobalFormId { file, object }` where `file` is the owner's load order position.

The form table maps every global id to the list of records that carry it, in load
order. `winner` is the last one, which is the rule the game, xEdit and Mutagen all apply.
`overridden` lists the ids whose winner is not their owner.

Light plugins (`ESL` header flag) do not take a top-byte slot at runtime. The game packs
them as `FE:sss:ooo`, a twelve-bit slot and a twelve-bit object. `LoadOrder::runtime_id`
produces that 32-bit form for save files and scripts. The table itself never needs it.

## Typed views

`records::{cell, refr, wrld, land, navm, stat}` each expose `from_record`. Views copy
the named fields and leave the record intact, so an unknown subrecord is never lost.
FormIDs inside views are raw, as written in the file, so a view can be built without a
load order and resolved later.

`Land::heights` keeps the gradient form and `decode` produces the 33 by 33 absolute
heights in world units, using the row-start plus running-sum rule OpenMW uses.
`Heights::encode` is the inverse, quantised to the byte grid. `navm::Geometry` parses
the packed `NVNM` block, refuses counts that cannot fit in the remaining bytes before
allocating, and writes itself back byte for byte.

## Tests

- Unit tests in every module: round trips, compressed and `XXXX` records, truncation
  and bad sizes at each layer, each typed view against a synthetic record.
- `tests/hundred_plugins.rs`: a base master with a 5 by 5 exterior grid, 99 patches
  (every seventh one light) overriding cells and references, checks the winner for each
  touched form, that every reference base resolves, and the runtime slot packing.
- `tests/real_files.rs`: gated on `SKYRIM_DATA`. Loads the five base masters, prints
  `file  TAG  count` lines for diffing against xDump, and checks that every CELL and
  REFR under Tamriel resolves and every REFR base is in the table.

```text
SKYRIM_DATA="/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data" \
  cargo test -p soe-esm --release --test real_files -- --nocapture
```

## Not done yet

- Only six typed views. Everything else is raw subrecords.
- No writer beyond the round trip: no FormID renumbering, no master list editing.
- `XCLL` lighting and `XPRM` primitives are kept as bytes.
- No fuzzing harness beyond the malformed-input unit tests. `cargo-fuzz` over
  `Plugin::parse` and `Geometry::parse` is the obvious next step.
- The xDump diff is printed, not automated: xDump is a Windows binary.
