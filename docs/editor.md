# The editor: a Creation Kit replacement on the Godot editor

Written 2026-09-06 for the round-2 editor item of `ENGINE-PLAN.md`. Sources: the CKPE patch
list and its Skyrim SE configuration (`engine-refs/Creation-Kit-Platform-Extended`, the
`CKPE.SkyrimSE.Patch.*` headers and `Stuffs/SSE/CreationKitPlatformExtended.toml`), the
Skyrim Behavior Editor README (`engine-refs/Skyrim-Behavior-Editor-`), the Godot editor
plugin tutorials (`engine-refs/godot-docs/tutorials/plugins/editor`), sections 6 and 8 of
`research/skyrim-open-engine-map.md`, and `docs/design-system.md`, `docs/esm.md`,
`docs/godot-host.md` in this repo. The UESP Creation Kit wiki refuses automated fetches, so
its workflow descriptions are used from memory and every claim that rests on them is marked.

## 1. What the Creation Kit is, and why it still works

The Creation Kit is not a level editor with a game attached. It is a form editor with a
render window. Everything it edits is a record in the form table described in
`design-system.md`: the Object Window is a tree of every form by type, the Cell View is a
list of cells and the references in each, the Render Window shows one cell, and every dialog
is a property sheet over one record. Saving writes the changed records to the active plugin
with the load order the editor was started with. That is the whole model, and it is why one
author with no engine knowledge can ship a quest mod in an evening.

The workflow verbs that authors depend on are visible in CKPE's rebindable hotkey table,
which is the best public inventory of them: drop selection to ground, align to position and
rotation, snap reference, duplicate, load the selected object's parent cell, edit landscape,
edit navmesh with add polygon, merge vertices, flood fill, find cover, finalize cell, flag
water triangles, import object navmeshes, object palette, multibound references, link and
break rooms and portals. Each one is a small operation over references and their transforms,
landscape heights, or navmesh triangles. None of them needs the Creation Kit specifically,
and all of them need the form table.

Three areas have no substitute anywhere and must be reproduced whole:

- **Quest and dialogue editing.** Quest stages, aliases, scenes, dialogue topics with their
  branches and conditions, and the response table with its voice file and lip sync
  generation. The condition system (`CTDA`, over 700 function slots, see section 3 of the
  map) is edited in every dialog through the same condition grid.
- **Navmesh editing and generation.** Triangle-level editing plus automatic generation from
  collision, with cover finding and cell finalization for the door and edge links that the
  AI needs across cell boundaries.
- **Landscape editing.** Height painting, texture layer painting on the quadrant grid, and
  vertex colours, all over `LAND` records.

Everything else (mesh preview, FaceGen, packages, perks, spells, the Papyrus fragment editor)
is a form dialog and a preview.

## 2. What breaks, read from what CKPE patches

CKPE is over 130 binary patches to the Skyrim SE Creation Kit, and the patch names are a
map of where the tool fails. Grouped:

| Group | Patches (examples) | What it says about the tool |
|---|---|---|
| Crashes on ordinary data | `CrashInventoryIterators`, `CrashMergeForms`, `CrashDuplicateWorldspace`, `CrashUsingMore16NPCForFaceGen`, `CrashMHDTMoreThan70`, `FixCrashDuplicateForm`, `FixCrashLightingShader`, `FixNiSkinInstance` | Normal operations on normal-sized mods crash it. Authors learn superstitions instead of workflows. |
| Hard limits | `BSPointerHandleManager` (reference cap, raised to 8,388,608 as an experimental option), `PapyrusEditorLimit`, `FixLoadMore32KAnimation`, `ResponseIgnoreMax`, `AllowCreateBitArray`, `AllowMultipleWindowAndMaster` | Fixed-size tables from 2011 that large modlists and total conversions exceed. |
| Load and save | `LoadOptimization`, `SkipTopicInfoValidation`, `BSArchiveManager` with an own archive loader, `AllowSaveESMandMasterESP`, `NewFormat171`, ONAM generation from 65,536 to 4 billion forms, `FixFormIDsInLoadFile`, `FixLoadArchiveActivePlugin`, `VersionControlMergeWorkaround` | Load time is minutes for a full order, masters cannot be edited directly, the save path has form-count ceilings, and version control cannot merge with more than two masters. |
| Navmesh | `NavMeshPseudoDelete`, `NavMeshInfoMap`, `NavMeshWindow`, the "No Landscape" ground height test switch | Deleting triangles corrupts data, so a pseudo-delete was added. Generation is brittle to worldspace flags. |
| Rendering | `RenderWindow60FPS`, `ReEnableFog`, `ReEnableLandShadows`, `FixBSShadowDirectionalLight`, `FixRenderPass`, D3D11 imagespace AA and shaders, `CheckD3D11` | The render window is a cut-down copy of the game renderer that lost fog, land shadows and a frame cap. What the author sees is not what the player sees. |
| Text and locale | `Unicode`, `BNetConvertUnicodeString`, `FixInvalidStrings` | ANSI code pages in 2026. Non-Latin mods need a translation layer on open and save. |
| Threading and memory | `MemoryManager`, `MemoryLeakBSString`, `MemoryLeakInPreviewWindow`, thread management capped at 85 percent of the processor | Single-threaded with leaks. |
| UI | `MainWindow`, `ObjectWindow`, `CellViewWindow`, `DataWindow`, `ProgressWindow`, `UIHotkeys`, dark theme, docking, `FixRemoteDesktop`, `FixCellViewObjectList`, `FixTabControl` | Win32 dialogs from the Oblivion era. Docking and hotkeys had to be reimplemented from outside. |
| Version control and workflow gaps | `AddChangeRef`, `AlteredFormList`, `EnableStateParentWorkaround`, `RefLinkGeometryHangWorkaround`, `FixDialogueBranch`, `FixMoveToTopic` | Multi-author work is a separate proprietary system nobody outside Bethesda uses, and the reference-link visualisation hangs. |

Two further limits are structural and no patch addresses them. The Creation Kit has no
conflict view: it loads a load order and shows the winning record with no way to see who
lost, which is why xEdit exists. And it has no merge of behaviour graphs, which is why every
animation mod needs Nemesis or Pandora to run after installation.

The Skyrim Behavior Editor README shows the animation side of the same story. It is a Qt
tool that converts packed hkx to XML through hkxcmd, graphs the behaviour files of a
character project, and exposes over seventy Havok and Bethesda behaviour classes for
editing, with merge of projects and behaviour files. It cannot read Skyrim SE files at all,
because hkxcmd predates the SE format, so SE authors edit LE files and convert. A behaviour
editor is therefore part of the editor scope, not an external tool.

## 3. Design

### 3.1 The Godot editor as host

Section 8 of the map picks Godot 4 for the runtime because it buys a renderer, physics,
audio and an editor. The editor is the larger of those gifts. Godot's editor is itself
extended through `EditorPlugin`, which can add docks (`add_control_to_dock`), a main screen
tab beside 2D, 3D and Script (`_has_main_screen`), inspector plugins for custom property
editors (`EditorInspectorPlugin`), 3D gizmos, and import plugins (`EditorImportPlugin`) that
turn a source file in the project into a Godot resource with options and presets. All of
these are plain Godot classes and are reachable from a Rust GDExtension through gdext, the
same crate `godot-host` already uses.

The rule from `godot-host.md` carries over: the Godot layer decides nothing. Every editor
operation is a function in a Rust crate over `soe-esm` records, tested without Godot, and
the Godot plugin is a view and a command dispatcher. That makes the editor logic reusable
by a command-line tool (batch navmesh regeneration for a whole modlist, for instance) and
keeps the Godot binary, which is not on the development machine, out of the test loop.

### 3.2 The document model: a load order, one active plugin

The editor opens what the game opens: a load order resolved by `soe-esm::LoadOrder` into
a form table, plus one active plugin that receives edits. This is the Creation Kit's model
and it is kept unchanged, for three reasons. Authors know it. Every existing tool (xEdit,
LOOT, Wrye Bash, Synthesis) assumes it. And the plugin file is the unit of distribution,
so anything the editor cannot express as records in one plugin is not a mod.

What changes is what the editor knows about the losers. `LoadOrder` keeps every version of
a form, not only the winner, so the editor can show the override chain for any record and
any field, the way xEdit does, inside the same window where the record is edited. Editing a
field on a form owned by another plugin creates the override in the active plugin, as the
Creation Kit does, and the chain view shows the new override on top.

Masters are editable directly. CKPE's `AllowSaveESM` and `AllowMasterESP` exist because the
Creation Kit forbids it. `soe-esm` writes any plugin, and the ESM flag is a header bit, so
the restriction is gone and the editor instead warns when the active file is a master that
other loaded plugins depend on.

### 3.3 Docks and screens

| Editor surface | Godot mechanism | Backed by |
|---|---|---|
| Forms dock: every form by type, searchable, with the override chain per record | dock (`add_control_to_dock`) | `soe-esm` form table |
| Record inspector: the fields of one record with the override chain per field, conditions grid, script properties | `EditorInspectorPlugin` over a `FormResource` wrapper | `soe-esm` typed views, raw subrecords for the rest |
| Cell view: worldspaces, cells, and references per cell | dock | `soe-esm` placement index |
| Render view: the loaded cell grid, selection, transform gizmos, drop to ground, snap | the 3D main screen, `CellStreamer` from `godot-host`, `EditorNode3DGizmoPlugin` | `godot-host` streaming, `soe-nif` meshes, `soe-physics` for drop to ground |
| Landscape: height, texture layer and colour painting | 3D gizmo plus Terrain3D's own brushes | `soe-esm` LAND encode, `godot-host` terrain |
| Navmesh: triangle editing, generation, cover, finalize | 3D gizmo and a dock | `soe-navmesh` (round 2) and the `NavMesh` view |
| Quest and dialogue: stages, aliases, scenes, topics, responses | main screen tab with a graph (`GraphEdit`) for branches and a table for responses | `soe-esm` QUST, DIAL, INFO, SCEN views (round 3) |
| Papyrus: source editing, compile, fragments | Godot's script editor with an LSP client to `papyrus-language-server` | `soe-papyrus` for the compiled output, Caprica or the open compiler for compilation |
| Behaviour graphs: state machines, clips, events, variables | main screen tab with `GraphEdit` | `soe-hkx` data model |
| Conflicts: who wins each record across the load order | dock, the same override chain view filtered to conflicts | `soe-esm` |
| Log and diagnostics | the output dock | every crate's typed errors |

### 3.4 Standard formats in, engine formats out

Authors work in Blender and image editors and should never hand-write a NIF or a DDS. The
editor uses Godot's import pipeline for the author-facing side and the engine crates for the
game-facing side.

- **Meshes.** An `EditorImportPlugin` for `.nif` reads through `soe-nif` for preview and
  placement. A glTF file in the project imports through Godot's own importer for preview,
  and an export step writes a NIF through `soe-nif`'s writer (round 3) with a material
  mapping from Godot's `StandardMaterial3D` to `BSLightingShaderProperty` flags and texture
  set slots. Collision comes from glTF extras or a convention (nodes named with a collision
  suffix) and becomes bhk shapes. The neutral mesh struct in `soe-nif` is the pivot for both
  directions.
- **Textures.** PNG and other Godot-imported images export to DDS through the `dds` crate
  with the format chosen from the slot (diffuse BC7 or BC1, normal BC5 or BC3 with the
  specular in alpha for Skyrim SE, and so on), with mips generated. The engine reads DDS,
  the author never sees one.
- **Animation.** Godot's animation player edits clips imported from glTF. Export to hkx clip
  data goes through `soe-hkx`'s writer once it exists. Behaviour graphs are edited as data
  in the graph tab and written as hkx, and the runtime reads them as data, so the merge that
  Nemesis and Pandora perform after installation becomes a load-time merge in the engine
  (see `design-system.md`).
- **Plugins.** The editor reads and writes plugin files through `soe-esm`. For version
  control it also reads and writes a text form of a plugin, one file per record, the way
  Spriggit does, so a mod's history is a git history and two authors can merge, which is
  the `VersionControlMergeWorkaround` problem solved by not having a version control system
  of its own.
- **Archives.** `soe-bsa` reads them. Packing a release archive is an export step.

### 3.5 Navmesh regeneration on save

Navmesh generation in the Creation Kit is a manual step that authors forget and that
produces broken meshes when references move. In the editor, saving a cell whose references
or landscape changed marks its navmesh stale, and the save step regenerates it through
`soe-navmesh` from the cell's collision and LAND heights, then reruns finalization for the
edge links to neighbours. Hand edits survive as overrides on triangles the author marked
locked. A stale navmesh that the author chooses not to regenerate is saved with a flag the
engine reads, so the runtime can generate it at load instead. This is the one place the
editor does something the Creation Kit does not: the workaround (running xEdit's navmesh
checks, or a generation pass in a separate session) becomes the default behaviour.

### 3.6 What the editor renders

The render view is the game renderer, because it is the same Godot scene the runtime
builds. Fog, land shadows and the frame cap that CKPE had to restore are not separate code.
A "play from here" button launches the runtime in the current cell with the active plugin
loaded, which the Creation Kit cannot do without saving and starting the game by hand.

### 3.7 Scale limits

The reference cap, the 32,000 animation cap, the response cap and the Papyrus editor size
limit are fixed arrays in the Creation Kit. The editor's tables are `soe-esm` vectors and
the form table already holds the 1,168,671 ids of the five base masters in 2.3 seconds of
parse time (`docs/esm.md`), against the minutes CKPE's load optimisation patch fights.
There is no ONAM ceiling because `soe-esm` writes ONAM from the record set. Text is UTF-8
throughout and is transcoded to the game's Windows-1252 only in string fields on write,
with a warning when a character does not fit.

## 4. What to keep from the Creation Kit, and what to drop

Keep, as workflow the scene knows by heart: the Object Window and Cell View split, the
render window's selection and transform hotkeys (CKPE's table is the default binding set),
form dialogs organised by record type with the same field names, the condition grid, the
quest stage and alias model, the response table with lip sync generation on save, and
drop-to-ground, snap and align.

Keep, as file contracts: plugin files as the only output, FaceGen data as NIF and DDS in
the paths the game expects, SEQ files for start-enabled quests, and the archive layout.

Drop: the proprietary version control, the fixed-size tables, the Win32 dialog toolkit, the
separate render code path, ANSI text, the "No Landscape" and multi-master prohibitions, and
manual navmesh regeneration as the default.

Add, because the workarounds prove the need: the conflict and override-chain view (xEdit),
behaviour graph editing and merge (Nemesis, Pandora, the behaviour editor), LOD generation
from the same form table once the mesh pipeline is stable (DynDOLOD, deferred per section 6
of the map, consuming DynDOLOD output stays the first-year policy), a text plugin form for
git (Spriggit), and play-from-here.

## 5. Risks specific to the editor

- **The condition grid and the quest dialogs are large.** They are the Creation Kit's real
  content, and no open project has rebuilt them. The typed views in `soe-esm` cover six
  record types today. Quest, dialogue, package and perk views are the bulk of round 3.
- **Godot's inspector is per-resource, not per-record-database.** Wrapping every form in a
  `Resource` is the natural fit and also a memory cost at a million forms. The forms dock
  must wrap lazily, on selection, and the inspector plugin must edit through the form table
  rather than through the wrapper's own state.
- **Undo.** Godot's `EditorUndoRedoManager` works on object properties. Record edits need
  their own undo log in the Rust layer with the Godot manager calling into it, or two undo
  stacks disagree.
- **FaceGen.** Generating face meshes and tint textures from head parts and morphs is a
  pipeline the Creation Kit runs on save and that crashes past sixteen NPCs. It needs the
  morph and tint code before the editor can save an NPC that the game will render.
- **No Godot binary on the development machine.** Every milestone below is testable in Rust,
  but the docks and gizmos cannot be seen until Godot is installed. Install it before
  milestone 2.

## 6. Work breakdown

Effort on the plan's scale: S weeks, M one to three months, L three to nine months, XL a
year or more or blocked.

1. **Editor core crate** (`crates/editor`, S). Open a load order into `soe-esm`, hold the
   active plugin, an edit log with undo, override creation on edit, save through the writer,
   and the text plugin form for git. Tested with synthetic plugins, no Godot.
2. **Forms dock and record inspector** (M). The `EditorPlugin`, the forms tree by type with
   search, lazy `FormResource` wrappers, an `EditorInspectorPlugin` that renders typed views
   as fields and raw subrecords as hex with names from xEdit's definitions, and the override
   chain per field. Needs the Godot binary installed.
3. **Cell view and render view** (M). Worldspace and cell tree, the 3D main screen driven
   by `CellStreamer`, selection, transform gizmos, drop to ground through `soe-physics`,
   snap and align, duplicate, and the CKPE hotkey table as defaults. Play from here.
4. **Import and export pipeline** (M). NIF and DDS import plugins for preview, glTF to NIF
   and PNG to DDS export with the material and slot mappings, archive packing. Depends on
   the `soe-nif` writer and the `dds` crate.
5. **Landscape editing** (M). Height, layer and colour painting through Terrain3D's brushes
   written back to LAND records, quadrant texture layers, cell border stitching.
6. **Navmesh editing and regeneration on save** (L). Triangle gizmos, generation from
   `soe-navmesh`, cover and finalization, locked triangles, the stale flag. The Creation
   Kit's navmesh tools are the most intricate of its 3D tools.
7. **Quest, dialogue and scene editing** (L). The typed views for QUST, DIAL, INFO, SCEN,
   PACK and the condition grid, the branch graph, the response table, SEQ generation, lip
   sync through an open generator. The largest single milestone because it is most of the
   Creation Kit's dialogs.
8. **Papyrus integration** (S). The language server in Godot's script editor, compile on
   save through an open compiler, fragment scripts attached to stages and responses, and
   property binding from the inspector.
9. **Behaviour graph editor** (L). `GraphEdit` over `soe-hkx`'s model with the seventy
   classes the Skyrim Behavior Editor exposes, clip and event tables, and the load-time
   merge that replaces Nemesis and Pandora. Blocked on the behaviour runtime design
   (`docs/hkx.md`) for what a merged graph must preserve.

10. **FaceGen and NPC editing** (L). Head part assembly, morphs, tint layers to DDS, the
    NPC dialog, and export in the paths the game expects. Needs morph support in the mesh
    pipeline.
11. **Conflict tooling and patchers** (M). The conflict dock, a patcher API over the form
    table so Synthesis-style patchers run inside the editor, and batch operations from the
    command line using the same crate.
12. **LOD generation** (XL, deferred). Object and terrain LOD from the form table and the
    mesh pipeline, replacing DynDOLOD and xLODGen output. Consuming their output is the
    policy until the mesh pipeline has been stable for a release cycle.

### Editor core implementation

`soe-editor` opens plugin paths in load-order order through `soe-esm`, keeps one active
plugin, and updates the resolved order when an edit is applied or undone. A subrecord edit
to a supported winning top-level `STAT` record or a QUST text edit creates an active-plugin
override. Undo restores the prior active plugin. Saving writes the active plugin bytes and
parses those bytes again before returning the saved plugin. The TES4 record count is updated
with edits.

Copying a winning record whose owner differs from the active plugin requires relocating
FormIDs in the record and its group path. The editor currently rejects nested records,
supports copied top-level `STAT` records, and supports QUST text edits only when source and
active master order align. The FMT-15 FormID renumbering and FMT-26 master relocation rows
remain open. The Spriggit output guide describes separate text files
per record, while the current editor writes plugin files only. No text plugin translation
or `.spriggit` configuration reader is implemented.

`EditorDocument::search_forms` filters the load order's winning forms by an optional record
signature and a case-insensitive substring of `EDID`. It yields `GlobalFormId` values, so a
caller can fetch a winning record when selected instead of building a Godot resource for
every form. The query walks `soe-esm::LoadOrder::winners`. The Godot editor's
`tutorials/plugins/editor/making_plugins.rst` and `inspector_plugins.rst` describe the dock
and inspector controls that can display those results. This row implements the Rust search
operation only, not the Godot dock or inspector controls.

### Quest stage and objective text edits, EDT-07

`EditorDocument::edit_quest_stage_log_entry_text` edits an existing QUST `CNAM` journal
entry, selected by the occurrence of its `INDX` stage and the occurrence of its `QSDT` log
entry. `edit_quest_objective_text` edits an existing `NNAM` display string, selected by
objective occurrence. Occurrences are used because stage and objective numbers may repeat.
Both operations preserve the remaining ordered QUST subrecords, including stage flags,
objective flags, targets, conditions and aliases. A winning top-level QUST becomes an
override in the active plugin, and both operations use the editor document's undo history.

The field grouping follows xEdit's `wbRecord(QUST)` definition in
`engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas` and UESP's `QUST.wikitext`: each `INDX`
starts a stage, each `QSDT` starts a log entry, each `QOBJ` starts an objective, and
`QSTA` targets and following conditions are retained. UESP notes that stage and objective
numbers are not unique. xEdit's DIAL definition and UESP's `DIAL.wikitext` identify QNAM as
the QUST owner link; DIAL editing is not part of this operation. No source implementation
was copied.

The plugin writer stores non-localized quest strings inline as NUL-terminated UTF-8. Editing
a localized QUST string returns `LocalizedQuestText`, because this writer does not write
the companion string tables. Creating an override from another plugin is allowed only when
its masters and the active plugin's master prefix match the load-order prefix. That keeps
the raw QUST links in their original FormID context. Nested records and QUST records with a
different master order still require FMT-15 and FMT-26 relocation support. The editor does
not add stages, log entries, objectives or targets, and it does not edit DIAL, INFO or SCEN.
`crates/editor/tests/edt_07_quest_stages.rs` covers saving and reparsing both text changes,
undo, duplicate stage and objective numbers, preserved targets and safe rejection of
localized text.

### Quest alias fill rules and match conditions, EDT-18

`EditorDocument::set_quest_alias_fill_rule` sets or removes an alias's optional four-byte
`ALFI` field, selected by the occurrence of its `ALST` marker. Setting a missing field
places it after the alias flags and before its fill type. `set_quest_alias_conditions` replaces
the alias's ordered `CTDA`, `CIS1` and `CIS2` subrecords. It validates the condition
sequence with the `soe-esm` condition reader and preserves every other alias subrecord in
its original order. Both edits create a top-level QUST override through the editor's
existing master-order check and participate in undo.

The layout follows xEdit's `wbRecord(QUST)` in
`engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas`: aliases start at `ALST`, contain the
optional fill rule and fill type, then match conditions, and end at `ALED`. The same
definition identifies DIAL's `QNAM` as its QUST link. This operation edits only reference
aliases with `ALID` and `FNAM`, and does not resolve alias FormIDs or evaluate conditions.
`crates/editor/tests/edt_18_quest_aliases.rs` covers override save and reload, retained
alias fields, undo, and rejection of an orphan condition string.
