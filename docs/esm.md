# soe-esm: plugin reader and form table

Reads Skyrim SE plugin files (`.esm`, `.esp`, `.esl`) into a group tree of records,
resolves FormIDs across a load order, and answers "which record wins" for any form.
Typed views cover the initial world records, race phonemes, scoped dialogue,
magic, crafting fields and HAZD hazards. Other data stays in raw subrecords and
is written back unchanged.

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

`Plugin::set_masters` reorders existing masters or adds new ones while retaining
all current masters. It relocates valid master and plugin-self FormIDs while
preserving null, light-plugin and out-of-range unresolved FormIDs. It relocates
record FormIDs, FormID group labels, TES4
`ONAM`, and recognized four-byte links in the typed world record set. It updates
the TES4 `MAST` and paired `DATA` entries, keeping existing master data with its
name. Duplicate and removed masters, malformed recognized links and lists over
255 masters return an error before changing the plugin. Unknown subrecords and
unmodeled link fields remain byte-for-byte unchanged. In particular this method
does not inspect nested `VMAD` links.

The operation follows local Mutagen `Plugins/FormID.cs` for the master-index
and object-id split, and `docs/plugins/Remapping-FormLinks.md` for the rule that
links must retain their target identity while a mod is reorganized. Spriggit's
`docs/spriggit-file.md` describes stable plugin identity across text
serialization. The Rust plugin model retains raw subrecords instead of
Mutagen's complete typed record graph, so this implementation relocates its
known typed links and preserves other fields unchanged.

Each `LoadedFile` exposes a SHA-256 fingerprint of its plugin's serialized
definition before the load order adds engine-owned forms. It is independent of
regular and light slot positions. For parsed files, serialization preserves
record content but normalizes compressed records, so the fingerprint describes
the loaded definition rather than the exact source file bytes. A changed
definition changes its fingerprint. This is dependency evidence, not an
authenticity check. `tests/sav_67_fingerprints.rs` covers slot reordering,
same-name replacements and the engine-added global.

`LoadOrder::fingerprint` hashes each plugin's ASCII case-folded name and
serialized-definition fingerprint in load order. It is suitable as a cache
identity for data derived from this resolved order. It does not fingerprint
loose assets or archive contents.

OpenMW's `apps/openmw/mwstate/savegame.hpp` writes content file names to saves,
and `loadgame.hpp` checks dependencies and maps saved indices on load.
`statemanagerimp.cpp` passes the active content list to its character manager.
These paths do not provide content fingerprints, so this crate hashes the
definition it loads.

Light plugins (`ESL` header flag) do not take a top-byte slot at runtime. The game packs
them as `FE:sss:ooo`, a twelve-bit slot and a twelve-bit object. `LoadOrder::runtime_id`
produces that 32-bit form for save files and scripts. The table itself never needs it.

Created forms use `CREATED_FILE`, the internal `u16::MAX` namespace, and
`runtime_id` packs them as `FF:oooooo`. Load-order positions include both
regular and light files, so position 255 can belong to either plugin kind.
It cannot reserve created identities. All 254 regular and 4096 light slots
fit below the created namespace without reducing the supported load order.

The source for created RefIDs is local
`engine-refs/uesp-savefile/Save_File_Format.wikitext`, FormID and FormIDs Array.
Light masks and object widths follow local Mutagen
`Mutagen.Bethesda.Core/Plugins/FormID.cs` and `docs/plugins/Compaction.md`.
These describe the external packing. The existing Rust identity and loader
are retained, with a separate internal sentinel rather than another form table.

When the first file is Skyrim.esm, the runtime load order supplies the missing
engine-defined GLOB `00000063`, `PlayCredits`, as a short with initial value
zero. The layout comes from local xEdit `Core/Hardcoded/Skyrim.esp`. A record
already present in Skyrim.esm or an override in a later plugin takes precedence.
This changes the in-memory loaded plugin, not its source file or the standalone
parser. It supplies only this global, which is required by the real ESS corpus,
not the complete set of Skyrim hardcoded forms.

## Typed views

`records::{cell, refr, wrld, land, lctn, navm, npc, stat, race, hazd}` and `records::gameplay`
expose `from_record`. Views copy
the named fields and leave the record intact, so an unknown subrecord is never lost.
FormIDs inside views are raw, as written in the file, so a view can be built without a
load order and resolved later.

`Cell::lighting` decodes XCLL's ambient, directional, fog, rotation and six
directional ambient-color fields. Color values retain all four bytes,
including xEdit's unused fourth byte. The optional fields after those six
colors are specular color, Fresnel power, far-fog color, fog maximum, light
fade distances and inheritance flags. Float values encode through their
original IEEE-754 bits. The reader accepts complete prefixes of the optional
fields from 64 to 92 bytes. This follows xEdit's `wbAmbientColors` and
`SetOptionalFrom(11)` definitions in `wbDefinitionsTES5.pas`. UESP
`CELL.wikitext` describes the same fields and notes a 64-byte duplicate cell.
Repeated XCLL fields and partial optional fields are errors. No existing Rust
reader exposes these fields with byte-exact re-encoding.
`tests/fmt_01_cell_lighting.rs` checks malformed sizes, each optional-field
prefix, float bit preservation and every winning installed CELL XCLL when
`SKYRIM_DATA` is set.

`Plugin::remap_form_ids` applies a caller-provided map to record identities,
group labels that contain FormIDs and the TES4 ONAM override list. It also
updates the FormID fields currently decoded by the CELL, WRLD, LCTN, REFR,
LTEX, NAVM, MGEF, HAZD and selected INFO views. This covers cell water,
owner, location, image space, music, acoustic space, encounter zone, weather
and region links. It covers worldspace parent, climate, water, location,
encounter zone and music links, plus location parents and markers. REFR base,
owner, location, layer, encounter zone, emittance, teleport door, enable
parent, lock key, linked reference, navmesh door, location ref type and idle
links are remapped. Texture set and grass links, navmesh base object lists,
magic effect DATA links, keywords and counter effect lists, SCEN and DIAL
quest links, INFO topic, response and response-sound links, WEAP and ARMO
enchantments, and HAZD image-space and DATA
links are remapped at their documented offsets. Scalar values remain intact.
The REFR field layouts follow local xEdit `wbDefinitionsTES5.pas` and UESP
`REFR.wikitext`. VMAD object
properties and arrays in object formats 1 and 2 are remapped while trailing
fragment bytes are preserved. An invalid or unsupported VMAD property makes
the operation fail without changing the plugin. Other raw subrecords remain
untouched because their fields have not been modeled as FormIDs here.

`records::npc::NpcFace` reads NPC_ `PNAM` head-part links in file order and the
19 floats in `NAM9`. `edit_record` replaces those fields while preserving the
other NPC subrecords, checks each link against the active load order for a
winning `HDPT`, and rejects malformed links or non-finite morph values. The
`NPC_` `PNAM` links are relocated with other recognized FormIDs when a plugin's
master order changes. These fields are defined by local xEdit's NPC_ record
definition. TiltedEvolution's `FaceGenSystem.cpp` confirms that runtime face
assembly resolves NPC head-part records before finding mesh nodes. The PyNifly
TRI source named by the editor milestone is absent from the local reference
corpus, and this view does not generate TRI morph meshes or FaceGen textures.

`Refr::lock` reads XLOC level, key and flags. Presence means initially locked,
including level zero. Level 255 requires a key. The reader accepts the 20-byte
layout and the 9- and 12-byte forms obtained by omitting xEdit's optional unused
tail fields. Other lengths return a size error. The serialized flags retain
bit 2 for leveled locks. They are distinct from the runtime locked bit.
Sources are local xEdit's REFR XLOC, UESP REFR, Mutagen PlacedObject.xml and
OpenMW's esm4 loadrefr.cpp. A bounds-checked Rust view fits the existing parser.

`Refr::primitive` decodes XPRM's 32 bytes as three bounds floats, four color
floats and a raw 32-bit kind. It preserves float bit patterns when encoded,
including values it does not interpret. xEdit names kinds 0 through 4 as none,
box, sphere, portal box and line. UESP describes 1 through 3 as box, sphere and
portal box, and lists 4 as unknown, so the view keeps the numeric kind. XPRM
must occur at most once and must have exactly 32 bytes. The layout follows local
xEdit `Core/wbDefinitionsTES5.pas` and UESP `REFR.wikitext`, cross-checked with
UESP `Raw_Data.wikitext`. `tests/fmt_16_reference_primitives.rs` checks field
decoding, exact encoding, invalid sizes, duplicates and the installed winning
REFR XPRM survey when `SKYRIM_DATA` is set.

`Land::heights` keeps the gradient form and `decode` produces the 33 by 33 absolute
heights in world units, using the row-start plus running-sum rule OpenMW uses.
`Heights::encode` is the inverse, quantised to the byte grid. `navm::Geometry` parses
the packed `NVNM` block, refuses counts that cannot fit in the remaining bytes before
allocating, and writes itself back byte for byte.

`records::gras::Grass::from_record` reads a `GRAS` record's `EDID`, generic `MODL`
and 32-byte `DATA` block (density, min and max slope, water fields, position and
height range, wave period, flags), and `fits_slope` filters a candidate placement's
ground slope against the record's declared range. `records::ltex::LandTexture::from_record`
reads an `LTEX` record's `EDID`, `TNAM` texture set FormID through the existing
`form_id` helper, and its repeated `GNAM` grass FormIDs through `form_id_list`, in
record order. Field layout for both from `TES5Edit/Core/wbDefinitionsTES5.pas`, cross
checked against OpenMW's `components/esm4/loadltex.cpp` for `GNAM`'s repeated single
FormID shape. `godot-host` resolves a cell's `LAND` alpha layers through `LTEX` to
`GRAS` for procedural grass placement (`docs/godot-host.md`, "Grass placement").

## Parsed plugin cache

`read_plugin_cached(path, cache_dir)` stores and reuses the parsed `Plugin`
tree in the caller's cache directory. The cache filename hashes the canonical
source path, source length, modification time at nanosecond resolution and
cache format version. The entry repeats those fields and stores the full
parsed tree, including placements, offsets, record order and group structure.
It uses a compact binary encoding of the owned plugin structs, so no
serialization crate is added.
The local `engine-refs/godotwind/src/native/ESMCache.cs` uses a versioned
binary ESM cache with source validation and a parse fallback. Its cache stores
selected host-facing record projections, so it cannot restore this crate's
complete raw records, subrecords and group tree. The cache here keeps the
existing `Plugin` model intact.

Each cache file has a magic value, payload length and SHA-256 digest. A cache
hit requires matching identity fields, a valid digest and complete decoding of
the payload. Invalid, truncated or stale entries are ignored. A successful
source parse replaces the current entry through a temporary file and rename.
Cache directory and cache write failures do not prevent source parsing.
`tests/fmt_29_cache.rs` covers fixture equality, canonical-path reuse, changed
file identity and modification time, cache format version, corrupt and
truncated entries, and all five installed masters.

## Tests

- Unit tests in every module: round trips, compressed and `XXXX` records, truncation
  and bad sizes at each layer, each typed view against a synthetic record.
- `tests/hundred_plugins.rs`: a base master with a 5 by 5 exterior grid, 99 patches
  (every seventh one light) overriding cells and references, checks the winner for each
  touched form, that every reference base resolves, and the runtime slot packing.
- `tests/real_files.rs`: gated on `SKYRIM_DATA`. Loads the five base masters, prints
  `file  TAG  count` lines for diffing against xDump, and checks that every CELL and
  REFR under Tamriel resolves and every REFR base is in the table.

## Scripted patch passes

`PatcherSession` applies a callback to winning top-level `STAT` records in resolved
FormID order. The output plugin must include each changed record's owner as a master and
preserve the source plugin's master order, with that source immediately after its masters.
Changed records keep their FormID and are appended to the output, while unchanged records
are omitted. An existing output override is reused on later passes, and the TES4 record
count is synchronized. The pass rejects nested records, incompatible master layouts,
known FormID subrecords and `VMAD` rather than moving group paths or remapping embedded
FormIDs. It currently applies only to top-level `STAT` records.

The API follows Mutagen's local `docs/loadorder/Winning-Overrides.md` and
`docs/plugins/Create,-Duplicate,-and-Override.md` for winner selection and preserving form
identity in an override. Wrye Bash's local `[dev]-PBash-Patching-Procedure.md` informed the
ordered passes and omission of records no patcher changed. The Rust implementation uses
the existing raw record model because the crate does not provide Mutagen's generated typed
record graph. `tests/edt_24_patcher.rs` verifies deterministic output, reloads the patch,
checks the new winner, and confirms unchanged records are not emitted.

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

- Typed gameplay views do not evaluate conditions or implement game systems.
  Their coverage and remaining raw fields are listed below.
- `Plugin::set_masters` relocates structural identities and the recognized
  links listed above. It does not inspect nested VMAD links or arbitrary
  unmodeled fields. `Plugin::remap_form_ids` applies an explicit map to the
  typed VMAD object links and other fields listed above.
- Typed `XCLL` lighting and `XPRM` primitives retain exact byte encoding.
- No fuzzing harness beyond the malformed-input unit tests. `cargo-fuzz` over
  `Plugin::parse` and `Geometry::parse` is the obvious next step.
- The xDump diff is printed, not automated: xDump is a Windows binary.

## Cell index (2026-09-18, branch `cell-index`)

`CellIndex` answers "what is in this cell" and "which cell sits at this grid
position" as hash lookups. `CellIndex::build` walks every winning record once,
`LoadOrder::build_cell_index` keeps the result on the order and `add` drops it
again, since a new file can move any placement.

The host integration stores the cache in `OnceLock`, so immutable readers can
share one `Arc<LoadOrder>` and one index. Repeated `build_cell_index` calls
reuse that index. `add` still requires exclusive access and invalidates it.
This also avoids rebuilding the full index on each game reference lookup.

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

## Dirty plugins (2026-09-18, branch `dirty-records`)

xEdit's Quick Auto Clean repairs three things in a plugin: an override that
repeats its master byte for byte (it wins over every plugin between the two),
a deleted placed object (the vanilla engine crashes when a script or a linked
reference still names it, so the cleaner undeletes it, sets the initially
disabled flag and moves it to z minus 30000, `wbUDRSetZValue` in
`wbInterface.pas`), and a deleted navmesh (same crash through the edge links).
Bethesda's own Update, Dawnguard, HearthFires and Dragonborn masters carry all
three. The engine takes the cleaned meaning from the file as shipped, so nobody
cleans anything:

- `CellIndex` keeps a placed object whose winner is deleted under its last known
  cell in `deleted_references`, apart from the `references` slice a loader
  draws. It resolves through the load order like any form, and the game's
  `World::load_ref` starts it disabled (the one line in `crates/game` this
  branch touched). The hot lookup is unchanged.
- A deleted `NAVM` winner is left out of the index and the navmesh crate cuts
  the links that pointed at it, see `docs/navmesh.md`.
- An identical override is ordinary data, the load order already applied it as
  the winner.
- `LoadOrder::dirty_report` counts all three per file (`DirtyReport`,
  `PluginDirt`), and `soe-bench dirty` prints it. Identical means the record
  flags apart from the compressed bit and the subrecord list equal those of the
  record in the highest of the file's own masters that carries the id. xEdit
  compares element by element under its conflict rules and skips fields it
  marks as ignorable, so its count sits at or above this one.

`tests/dirty.rs` runs a master, a dirty patch and a third plugin that links to
the deleted reference through `XLKR`: the link resolves, the index files the
reference apart, the navmesh is gone, and the counts per plugin match.

### Live run (2026-09-18)

`tests/real_files.rs::the_shipped_masters_are_dirty_and_the_report_says_where`
over the five masters as installed. The LOOT column is the masterlist's
v1.7.9900.0 row, the one whose deleted counts fit these files. The installed
CRC32 values (Update.esm 0x2207CF03, Dawnguard 0x6CEC879A, HearthFires
0x1EABC985, Dragonborn 0xE4A4F6EA) match no row in the masterlist, so the build
is close to that row and not the file LOOT hashed.

| File | Identical, engine | LOOT ITM | Deleted refs, engine | LOOT UDR | Deleted navmeshes, engine | LOOT nav |
|---|---|---|---|---|---|---|
| Skyrim.esm | 0 | no row | 0 | no row | 0 | no row |
| Update.esm | 366 | 476 | 93 | 93 | 6 | 6 |
| Dawnguard.esm | 418 | 853 | 90 | 82 | 59 | 59 |
| HearthFires.esm | 224 | 256 | 11 | 11 | 5 | 5 |
| Dragonborn.esm | 39 | 142 | 8 | 8 | 1 | 1 |

The deleted counts match LOOT outright for three files of four, and the
Dawnguard difference is accounted for: of its 90 deleted placed records, 82 are
Skyrim.esm forms and 8 are Update.esm forms, and LOOT's 82 is the first group
alone. The engine counts a deleted placed object whichever master owns it. The
90 are 80 `REFR` and 10 `ACHR`, none of them with an empty body.

The identical counts sit below LOOT's throughout, which is the expected
direction. Byte equality over the subrecord list is stricter than xEdit's
element comparison, which decodes each field and skips the ones it marks
ignorable, so xEdit calls records identical that this rule keeps apart. The
report never treats the gap as a defect in a plugin, it only says what the file
holds.

Report time 0.007 s over 1,178,642 records. Deleted placed objects the index
kept apart: 202, which is the sum of the four files' deleted references, each
resolving to a winner carrying the flag and absent from the drawn list. Cell
index build 0.154 s and one `references` lookup 9 ns, against 0.285 s and 10 ns
recorded before this branch on the same data. The build gained a push per
deleted winner and a sort per cell, the lookup path is untouched, so treat both
figures as run to run variance rather than a speedup.

## Scoped gameplay views (2026-09-19, branch `gameplay-views`)

`records::gameplay` exposes six record views. They read a record on demand
and do not create another form database. They introduce no dependencies.

| View | Decoded fields |
|---|---|
| `DialogueTopic`, DIAL | Text, priority, branch, owning quest, category, repeat flag, effective subtype representation and declared INFO count |
| `DialogueInfo`, INFO | Legacy DATA or modern ENAM flags/reset time, previous-topic and INFO links, shared INFO, follow-up topics, favor level, prompt, speaker, walk-away topic, output override, ordered response emotion/number/sound/text/idle fields, conditions and raw VMAD |
| `Spell`, SPEL | Name, description, equip type, all 36 SPIT bytes, ordered EFID/EFIT groups and each group's conditions |
| `Ingestible`, ALCH | Name, description, ENIT value, flags, addiction, addiction chance and consume sound, plus ordered EFID/EFIT groups and each group's conditions |
| `MagicEffect`, MGEF | Name, description, keywords, counter effects, conditions and the DATA subset below |
| `Recipe`, COBJ | Declared ingredient count, ordered signed ingredient quantities, per-ingredient COED owner/union/condition, conditions, optional output object/count and workbench keyword |

The MGEF DATA view reads flags, base cost, associated item, magic skill,
resistance, declared counter-effect count, taper weight, minimum skill, area,
casting time, taper curve and duration, secondary actor-value weight, archetype,
both actor values, projectile and explosion links, cast type, delivery, skill
usage multiplier, dual-cast scale, equip ability, perk, and script AI score and
delay. Other DATA fields remain in the source record. This is not a full MGEF
API.

`Condition` preserves CTDA flags, comparison number or GLOB FormID, function,
raw parameter unions, run-on fields and following CIS1/CIS2 strings. Conditions
remain in file order and are attached to their record or individual spell
effect. No function table, condition evaluator or eligibility decision is
provided. Unknown numeric enum values and flags are retained.

`Condition::push` is public for record readers in other crates. It groups
CTDA and its following CIS1/CIS2 without assigning execution semantics. The
game crate uses it for QUST dialogue conditions before the first NEXT marker.

Sources read before implementation:

- `../engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas`, definitions for DIAL,
  INFO, SPEL, MGEF, COBJ, `wbConditions`, `wbEFIT`, `wbCNTONoReach` and
  `wbCOED`. These specify layouts, condition unions and repeated groups.
- `../engine-refs/uesp-modfileformat/{DIAL,INFO,MGEF,COBJ}.wikitext`, which
  document DIAL parse order, INFO reset-time formats, MGEF offsets and optional
  COBJ output objects. These clarify details where xEdit's editing schema and
  the installed records differ.

These tools provide schema descriptions rather than a Rust view library, so
local readers use the existing `Fields` bounds checks. No source code was
copied. DIAL retains the last DATA numeric subtype or SNAM signature encountered.
It does not convert signatures through a version-dependent subtype table. INFO
TPIC is the previous topic, not its owner. The enclosing plugin group identifies
ownership, and this view does not build that index. VMAD fragments remain raw.

Callers must select a winning record with `LoadOrder::winner_ref`, obtain its
localized flag from that winning file, and resolve every raw link using the
winner's file index. The owning FormID's file index can differ from the file
that supplied its override. Callers must also handle deleted winners and
missing linked forms. A missing COBJ output is `None`, not a fabricated item.
A view is a snapshot and must be read again after the source record changes.

`tests/gameplay_views.rs` covers effect/response grouping, subtype parse order,
both reset-time formats, signed quantities, ownership unions, condition strings,
malformed sizes and orphaned/duplicate groups, localized overrides for all four
text-bearing views, missing forms and recipe overrides with reordered masters.
Unknown subrecords remain in the source record.

The gated `tests/real_gameplay_views.rs` survey loads five installed masters,
decodes every non-deleted winner of these types, and requires every non-null
spell effect, topic quest and recipe output/ingredient link to resolve:

| Measurement | Result |
|---|---|
| DIAL / INFO winners | 19,557 / 40,678 |
| SPEL / MGEF / COBJ winners | 1,354 / 1,573 / 1,385 |
| Conditions / responses | 79,710 / 44,030 |
| Spell effects / ingredients | 2,264 / 2,616 |
| Checked links | 25,815, all resolved |
| Recipes with absent output object | 7 |

The seven incomplete recipes include unused steel-plate shields and gilded-elven
armor recipes. UESP marks CNAM optional, while xEdit marks it required. The view
retains absence and does not claim these definitions can craft an item. Counts
are measurements of this install, not limits imposed on another load order.

No dialogue selection or playback, spell casting, active-effect lifecycle,
crafting consumption, UI or progression is implemented by these readers.
Fragment decoding, full MGEF fields and function-specific condition semantics
still need their own scopes.

## DLBR and SCEN views (2026-09-23, FMT-04)

`DialogueBranch` reads DLBR: owning quest, numeric category, numeric flags
and the starting topic. All four fields are required by xEdit's schema but
stay `Option` here, matching every other view in this module, since a
malformed or partial file should still produce a readable snapshot.

`Scene` reads SCEN's flat field list and its three repeated, uncounted lists:

- Phases (`ScenePhase`): name, start conditions and completion conditions
  split at the phase's first NEXT marker, and display width.
- Actors (`SceneActor`): alias ID, flags and behavior flags.
- Actions (`SceneAction`): type, name, alias ID, index, flags, start and end
  phase, plus `SceneActionData`, the type-specific union. Dialogue actions
  keep their topic FormID, Package actions their ordered package FormIDs,
  Timer actions their duration. SNAM is the shared start-phase index for
  every action type, and a Timer action carries a second SNAM after it for
  the type-specific duration float, as UESP's SCEN page notes. On a Timer
  action a SNAM followed by ENAM or by a second SNAM is the start phase, and
  a SNAM after those is the duration. A Timer with a single SNAM and no ENAM
  after it keeps that value as the duration, since the union field is what
  makes it a Timer, and leaves start_phase unset.

VMAD stays raw, its INFO and SCEN fragment sections belong to FMT-03. Parent
quest, last action index, the four actor-behavior "set all" settings and
scene-level conditions follow the actions list.

None of DLBR, Phases, Actors or Actions carry an explicit count. Reading
walks the record once, tracking which of five sections (header, phases,
actors, actions, trailer) the current subrecord tag belongs to, since a few
tags are shared: ANAM starts an action with data and ends one when empty,
ALID appears on both actors and actions, INAM and PNAM close out an open
action or, once no action is open, belong to the trailer instead. Every
transition is driven by a field only the next section defines, per
`SCEN.wikitext`'s "no terminator, each list ends where the next begins" note.

Unused, pre-SE leftover subrecords inside phases and actions (SCHR, SCDA,
SCTX, SCRO, a leftover QNAM) are read past and dropped, as is a second CK
scripting leftover block after Actions and before PNAM. HTID, DMAX, DMIN,
DEMO and DEVA, the head-tracking and looping extras on Dialogue actions, are
not decoded. A SCEN with an unclosed phase, actor or action group at record
end (a truncated file, since the three lists have no length prefix to
validate against) still surfaces that last, partial entry: the reader flushes
whichever of phase, actor and action is still open once the subrecord loop
ends, instead of dropping it. An unclosed phase followed by ALID, ANAM or
PNAM is closed at that field, and the next section starts there. PNAM counts
as package data only inside a Package action (type 1), matching TES5Edit's
wbRUnion, and a stray PNAM inside any other open action is ignored.

Sources read before implementation: `wbDefinitionsTES5.pas` lines 7114 to
7130 for DLBR and 7232 to 7428 for SCEN, and `uesp-modfileformat/DLBR.wikitext`
and `SCEN.wikitext` for the Phases, Actors and Actions field tables and the
no-terminator note. No source code was copied, both are schema references.
No existing crate in `engine-refs` was reused, wrye-bash's `skyrimse/records.py`
SCEN reader was consulted for confirmation of the phase and action boundaries
and led to the same section split UESP already documents.

`tests/gameplay_views.rs` covers DLBR and SCEN round trips (`fmt_04_dlbr_branch_fields_round_trip`,
`fmt_04_scen_phases_survive_the_next_marker_split`,
`fmt_04_scen_actions_keep_type_specific_data_by_action_type`).

## Localized strings, 2026-09-20

`strings::StringTable` reads the indexed `.strings`, `.dlstrings` and
`.ilstrings` layouts. It checks each directory offset and length against the
declared data section. The language mapping follows xEdit
`Core/wbLocalization.pas`, `Core/wbInterface.pas` and `xEdit/xeInit.pas`.
For SE, English uses Windows-1252. Other languages try UTF-8 first and then
their language-specific fallback. Japanese and Chinese require UTF-8.
Tests cover all three layouts, invalid offsets and termination, UTF-8 text,
and a legacy French fallback.

## HAZD view, FMT-28 (2026-09-27)

`records::hazd::Hazard::from_record` reads HAZD's required 40-byte DATA block:
limit, radius, lifetime, image-space radius, target interval, flags, spell,
light, impact data set and sound. It also reads the optional MNAM image-space
modifier. DATA and MNAM links remain raw FormIDs for resolution with the file
that supplied the winning override. Unknown flag bits and unknown subrecords
are retained. Missing or repeated DATA, repeated MNAM and incorrect fixed
field sizes return errors.

`Plugin::remap_form_ids` remaps MNAM and only the four FormID words at the end
of DATA. It preserves the scalar values and flag word. The field sizes and
link types follow local xEdit `wbDefinitionsTES5.pas` HAZD, UESP
`HAZD.wikitext`, Mutagen `Hazard.xml` and CommonLibSSE-NG `BGSHazard.h`.
These sources describe the format and runtime structure rather than a Rust
reader, so the implementation uses the existing bounds-checked `Fields` API.
The gated HAZD survey checks all non-deleted winners in the five installed
masters and resolves each non-null link relative to the winning plugin.

## GRAS and LTEX views, RND-11 (2026-09-24)

`records::gras` and `records::ltex` are new typed views, added so `godot-host`
can scatter grass from a cell's `LAND` texture layers rather than from a
placed `REFR`, which vanilla never uses for grass. `records::ltex::from_record`
was rewritten during this change to call the crate's existing `form_id`
helper for `TNAM` instead of a hand-written length match, which also fixes a
latent gap: a malformed `TNAM` now returns a size error like every other
fixed-length subrecord in the crate, instead of being silently read as
absent. Both views are exercised by `godot-host`'s cell streaming path, not
by a unit test added here, since this row's instructions withhold new tests
for a later breaker and judge pass. See `docs/godot-host.md`, "Grass
placement", for the placement algorithm and its open gaps.
