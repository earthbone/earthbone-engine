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

## Live run (2026-09-14)

The gated test loaded the five base masters against the installed Steam copy. The
form table held 1168671 ids. Tamriel resolved to 13849 cell records, 263415
references, 0 unresolved reference bases, 13074 land records, 13713 navmesh records
and 12107 static records in the form table. The test finished in 5.07 seconds. No
parser change was needed, every record type the checks touch read cleanly.

## Not done yet

- Only six typed views. Everything else is raw subrecords.
- No writer beyond the round trip: no FormID renumbering, no master list editing.
- `XCLL` lighting and `XPRM` primitives are kept as bytes.
- No fuzzing harness beyond the malformed-input unit tests. `cargo-fuzz` over
  `Plugin::parse` and `Geometry::parse` is the obvious next step.
- The xDump diff is printed, not automated: xDump is a Windows binary.

## Cell index (2026-09-18, branch `cell-index`)

`CellIndex` answers "what is in this cell" and "which cell sits at this grid
position" as hash lookups. `CellIndex::build` walks every winning record once,
`LoadOrder::build_cell_index` keeps the result on the order and `add` drops it
again, since a new file can move any placement.

- `references(cell)`: the placed objects (`REFR`, `ACHR`, `PGRE`, `PHZD`, `PMIS`,
  `PARW`, `PBEA`, `PFLA`, `PCON`, `PBAR`) whose winning record sits in the cell,
  persistent ones first, each part sorted by id. `persistent_references` and
  `temporary_references` are the two halves.
- `land(cell)` and `navmeshes(cell)` from the temporary group.
- `exterior_cell(world, x, y)` and `grid_of(cell)` from `XCLC` under the
  worldspace's children group. `world_persistent_cell(world)` is the cell placed
  directly under the `WRLD` group that holds the worldspace's persistent
  references. The index keeps it apart, it does not spread those references over
  the grid, so an exterior loader adds them itself.
- `cell_by_editor_id`: interior wins over an exterior cell with the same editor
  id, then the lowest id.
- `is_interior(cell)` from the winning `DATA` flag.

Override rules follow the form table. A placed object's cell is read from its
winning record's placement, so a plugin that moves a reference into another
cell moves it in the index, and a winner with the deleted flag is dropped.
`tests/cell_index.rs` checks both on a synthetic master plus a patch, round
tripped through bytes so the placements come from the parser, and that adding a
file drops the cached index.

`soe-godot-host`'s `DataSource` builds the index at open and streams a cell from
it, see `docs/performance.md` for what that changed.

### Live run (2026-09-18)

`tests/real_files.rs::the_cell_index_covers_whiterun` over the five base masters:

| Measurement | Value |
|---|---|
| Build time | 0.285 s (release, after the parse) |
| Cells indexed | 73,723 |
| Placed objects | 872,220 |
| Grid cells | 72,917 in 47 worldspaces |
| WhiterunBanneredMare | 697 placed objects, 690 `REFR`, 35 persistent, 1 navmesh |
| WhiterunWorld persistent cell | 495 placed objects |
| 3 by 3 block around WhiterunOrigin (4, -2) | 980 placed objects, 9 cells, land in 3 of them |
| One `references` lookup | 10 ns |

Every id filed under the Bannered Mare resolves back to that cell through its
winning record's placement. The 690 `REFR` count matches what the linear scan in
`DataSource` found before the index, so the two agree on the vanilla data.
