# `save`: Skyrim Special Edition save games

The `soe-game` SOEW world save is version 37. It records the active Flee
package procedure using the actor's existing AI procedure field. Older world
save versions remain readable through the current version gate.

`crates/save` reads an `.ess` file into a typed structure and turns the part of
it that describes progress into scenario expectations the harness in
`crates/game` can run. The game crate also imports the saved global-variable
table and quest change forms using saved plugin names. Full world restoration
remains unfinished.

CELL has a typed decoder for its common form flags, cell flags, full name,
owner and xEdit-defined seen-data shape. Seen data precedes the full name and
owner. The interior/exterior choice validates the suffix against xEdit's
32-byte exterior and repeated 34-byte interior shapes. A truncated interior
that leaves 32 bytes is indistinguishable from exterior data. CELL flags 28 to 30 are rejected
because xEdit does not define their serialized fields. ECZN and LCTN decode
their common form flags and retain other flagged bytes as opaque payloads,
because the available schema marks those fields as placeholders. See
[saved spatial fields](save-spatial-fields.md) for the source boundary and
60-save corpus accounting. Live restoration remains unfinished.

The projectile and hazard decoder tasks share a missing subtype-layout
prerequisite. See [saved projectile layouts](save-projectile-layouts.md) for
the xEdit dispatch gap, runtime type boundaries and evidence needed by SAV-55.

The SAV-55 regression test pins the type indices for PMIS, PGRE, PBEA, PFLA,
PHZD, PBAR, PCON and PARW and checks the generic change-form header preserves
flags and payload bytes. SAV-11 exposes all 32 xEdit-defined PMIS, PGRE and
PBEA change flag bits through `saved_projectile_change`. SAV-23 exposes the
same flag positions for PFLA, PHZD and PBAR through `projectile_change`. Both
retain their inflated subtype payload as exact bytes. The payload fields
remain undecoded:
xEdit's subtype dispatch structures are empty and FallrimTools uses its raw
fallback. PCON and PARW expose typed envelopes containing their change flags,
version, compression length and exact stored bytes. Their
subtype payloads remain opaque because the inspected Skyrim xEdit structures
are empty and FallrimTools has no typed reader. SAV-25 tests envelope
truncation and reports PCON/PARW flag and byte coverage across the external
save corpus. See [saved projectile layouts](save-projectile-layouts.md) for
the source review and remaining evidence requirement.

The format was not reverse engineered. It is documented and it has a mature
open implementation, so both were read first.

## Sources, and what each gave

- `../engine-refs/uesp-savefile/` holds 37 wikitext pages fetched from the UESP
  wiki by `fetch_uesp_savefile.py` next to them. `Save_File_Format.wikitext` is
  the top level layout, `File_Format_Conventions.wikitext` defines `wstring`,
  `vsval` and `RefID`, and one page per change form and global data type covers
  the rest. Rerunning the script picks up pages added since.
- xEdit's `TES5Edit/Core/wbDefinitionsTES5Saves.pas` defines WOOP and MGEF
  change flags. Bit zero carries the six-byte form flags, bits one and two
  select payload-free sections, and bits three through thirty-one are unnamed.
  FallrimTools has no typed reader for either record and keeps them as default
  payload data, so their typed layout follows xEdit while retaining unnamed
  flag bits.
- `../engine-refs/FallrimTools/` is ReSaver, Apache 2.0, the save editor the
  modding community uses to repair broken saves. Its `resaver.ess` package is
  the authority used here for everything the wiki states loosely: which fields
  sit inside `headerSize`, that Special Edition is told from Legendary by the
  header `version` alone, that the third global data table count is short by
  one, and that Special Edition widens Papyrus string counts and element ids.
  The Papyrus second pass follows `papyrus/Papyrus.java`,
  `DefinedElement.java`, `ScriptInstance.java`, `Reference.java`,
  `ArrayInfo.java`, `ActiveScript.java`, `Variable.java` and `Type.java`.
  Execution data follows `StackFrame.java`, `OpcodeData.java`,
  `Parameter.java`, `FragmentTask.java`, `FunctionMessage.java`,
  `FunctionMessageData.java`, `SuspendedStack.java` and `QueuedUnbind.java`.
  These supply table order, optional fields, ID widths and value tags. The
  Java reader is not a Rust library, so this crate implements the documented
  layouts locally. No source code was copied.
  Runtime variable import uses `Script.getExtendedMembers` for parent-first
  slot order and CommonLibSSE-NG `src/RE/O/Object.cpp` for distinct inherited
  slots. The VM scope rules also follow Caprica's identifier resolution.
- The npm `@save-ward/save-parser-skyrim` was not used. It states that it does
  not support Special Edition, which is the only edition this crate targets.

## Where the two sources are wrong

A `vsval` packs its value into the bits above a two bit width tag, so one byte
holds at most `0x3F`. The UESP page says a value up to `0x40` fits in one byte
and ReSaver's encoder follows it, but `0x40` shifted left by two is `0x100` and
does not survive the cast to a byte, so that path writes zero. The same off by
one sits at `0x4000`.

The previous reader also incorrectly treated width tag two as three bytes.
xEdit's `Core/wbInterface.pas`, `ReadIntegerCounterSize` and
`ReadIntegerCounter`, define one, two and four bytes. The largest encoding
therefore holds thirty value bits, through `0x3FFFFFFF`. This crate follows
that reader, rejects the reserved tag three, and keeps the original width.
The encoder uses the corresponding bit widths rather than copying the
inconsistent reference encoders. Fixed-byte boundary tests check alignment
with the following field as well as truncation and re-encoding.

## The layout, and its two traps

A save is an uncompressed header block, then a body that Special Edition
compresses with LZ4 in block format.

The header block is the magic, a `uint32` size, the fields that size covers,
and then the screenshot. The screenshot is outside the declared size and
Special Edition writes four bytes per pixel. Ending the header in the wrong
place does not fail at the header: the body then starts four or more bytes off,
and the failure surfaces much later as a location table full of nonsense.

Inside the body the plugin lists sit behind a `uint32` byte length. It is easy
to read past, because the plugin count immediately after it looks like a
plausible count on its own. Skipping it shifts every later field by four bytes
with the same late, confusing failure.

Both are pinned by `tests/probe.rs`, since both cost a full parse of the corpus
when they were wrong and neither announces itself.

## Writing files

`Save::write_file` serializes an ESS container, validates compressed change-form
payloads and checks exact re-encoding before replacing the destination. It uses
a synchronized sibling staging file and keeps a prior link across replacement.
The [file protocol](save-resilience.md#sav-62-file-replacement) documents error
paths, cooperating-writer locking and the tested filesystem model. Validation
does not claim runtime support for opaque payloads.

## What is decoded

- The header, every field, including the screenshot kept as raw pixels.
- The plugin list and the light plugin list.
- The file location table, with the third table count corrected.
- All three global data tables, as typed blocks with raw payloads.
- Type 3, the global variable table, as references and floats.
- Every change form header. `ChangeForm::quest_change` decodes documented QUST
  flags, stages, objectives, runtime aliases/events and quest instances.
- `ChangeForm::static_object_change` decodes the named form flags for CONT,
  MISC and FURN. The xEdit value and full-name fields have zero-byte layouts.
  Unnamed flags and suffix bytes remain available for exact payload re-encoding.
- PCON and PARW change-form envelopes, with original stored payload bytes
  retained without subtype interpretation. Their shared initial placement
  prefixes decode into the normal reference reconstruction path. The xEdit
  subtype structs are empty, so any remaining bytes stay opaque.
- `ChangeForm::info_change`, `story_manager_quest_node_change` and
  `scene_change` decode the defined INFO, SMQN and SCEN flag sections and state
  markers.
- `ChangeForm::npc_change` decodes complete NPC_ payloads, including base data,
  factions, spells, AI data, names, skills, class, race, face, gender and outfits.
- `ChangeForm::class_change`, `faction_change` and `relationship_change` decode
  CLAS, FACT and RELA change sections on demand.
- `ChangeForm::word_of_power_change` and `magic_effect_change` decode WOOP and
  MGEF form flags, the two payload-free base-object change markers, and retain
  unnamed flag bits. Only the form-flags marker contributes payload bytes.
  The game crate applies these fields as persisted record metadata. It leaves
  source subrecords in place because the two base-object markers contain no
  replacement values or text.
- `ChangeForm::activator_change` decodes ACTI, TACT and DOOR form flags and
  their value, full-name and TACT speaker change markers. The markers consume
  no payload bytes, as defined by xEdit. Unnamed xEdit flag bits remain in the
  typed value without assigned meaning.
- `ChangeForm::ingredient_change`, `alchemy_change` and
  `enchantment_change` decode INGR, ALCH and ENCH change markers. Unnamed flag
  bits remain available, marker sections add no payload bytes, and form flags
  retain their six bytes.
- `ChangeForm::item_change` decodes the shared ARMO, WEAP and AMMO form-flags
  section and the value/name change markers. Unnamed bits and any remaining
  bytes stay available in the typed value.
- `ChangeForm::pack_change` and `navmesh_change` decode PACK and NAVM sections.
  PACK retains optional form flags plus waiting and never-run markers. NAVM
  retains optional form flags plus base-object value and full-name markers.
  The four markers after the form flags have no payload bytes in xEdit's
  definitions, so typed output re-encodes to the exact source payload.
- `ChangeForm::word_of_power_change` and `magic_effect_change` decode WOOP and
  MGEF form flags, the two payload-free base-object change markers, and retain
  unnamed flag bits. Only the form-flags marker contributes payload bytes.

- `ChangeForm::cell_change` decodes defined CELL fields, and the ECZN/LCTN
  decoders retain their placeholder payload bytes with the common form flags.
- Type 107, quest static data, including its two event arrays, three reference
  arrays, reference-keyed values and final byte.
- Type 1001, the Papyrus block: the string table, the script definitions with
  their member descriptors, script instances that bind scripts to forms,
  reference and array headers, runtime ID and active-script headers. The
  second pass decodes instance and reference variables and array elements,
  then active-script bodies, saved function bodies and execution positions,
  queued messages, both suspended-stack tables, unknown ID lists and queued
  unbinds. Decoding ends after the Skyrim save-file version field.

Papyrus values include null, object and struct references, strings, integers,
floats, booleans, nested variants and array references. String indices are
checked, counts are bounded by remaining bytes, and variant nesting is limited
to 128 levels. Each second-pass ID must occur exactly once in its header table.
Payloads retain file order, which can differ from header order. Null and boolean
payloads and float bit patterns survive re-encoding. Unresolved object and
array references remain IDs, since a dangling reference is readable save data.

Saved frames retain their parameters, locals, instructions, owner, variable
slots and instruction pointer. Instructions share the opcode table in the
existing `soe-papyrus` crate. This adds no third-party dependencies, since its
only runtime dependency is the existing `soe-trace` crate. Saved operands have
32-bit string references and byte booleans, so the PEX operand reader cannot
read them directly. Opcode values outside the Skyrim set return an error.

Active bodies must match their headers by ID exactly once, independently of
file order. Suspended IDs must be unique within each of the two tables, but
may occur in both. Unresolved message, object and unbind IDs remain readable.
Fragment tasks retain their discriminator, payload and optional variable.
Function-message flags retain the signed-byte interpretation in FallrimTools.
Unknown fields are retained without assigning them runtime behavior.

## What is left raw

Change form payloads stay as original bytes and typed readers decode supported
types on demand. Unsupported types remain opaque. The Papyrus auxiliary data
after the save-file version stays raw, as does the original payload used when
writing the save body.

PFLA, PHZD and PBAR expose their shared xEdit change flag word as typed flag
positions, including unnamed bits. Their subtype payload fields remain raw
because the inspected sources do not establish subtype field layouts. The
decoder accepts every flag word without dropping bits and preserves the exact
inflated payload bytes. It does not infer that a set change flag corresponds
to a specific serialized payload field.

Most decoded state remains inspection data. The game crate can apply globals,
quest state, Papyrus instances and active script frames. Queued messages,
suspended stack tables, latent native waits and other world state are not
resumed by this path. The explicit instance-import API below can apply decoded
variables to a prepared VM, but this is not a complete save loader.

## CONT, MISC and FURN payloads

`ChangeForm::static_object_change` reads the optional six-byte form-flags
prefix and exposes the value and full-name markers. The xEdit definitions
resolve both of those marked fields to zero bytes. Other flag bits and bytes
after the defined prefix stay in the decoded value, and its `data_bytes`
method rebuilds the original payload. A short form-flags prefix returns a
truncation error.

The implementation follows xEdit's `wbDefinitionsTES5Saves.pas`. UESP's save
format page establishes the type indices but does not document these payload
fields. FallrimTools `ChangeFormDefault` retains payload bytes without a typed
layout, so it remains useful as a raw preservation reference.

`World::restore_vanilla_static_objects` maps these changes through saved plugin
names and applies the form-flags inversion mask to winning CONT, MISC and FURN
records. It retains the value and full-name markers and opaque suffix bytes.
The markers have zero-byte layouts in xEdit, so neither marker replaces a
record subrecord. Restore prepares every record before installing any override.
SOEW version 37 stores the decoded fields and reapplies the mask when loading
engine state.

## Runtime instance import

`Papyrus::restore_instances` joins parent-first member descriptors to saved
slots by ID. Its low-level API takes an explicit map from each saved instance
or reference ID to a VM object. `Game::restore_vanilla_papyrus` builds that map
from the ESS plugin list, FormID array and RefIDs. Script instances share the
VM object for their resolved live form. Reference-only objects use their saved
runtime type. An instance whose signed `unknown` field is `-1` ignores its
RefID and gets a standalone VM object. A missing plugin, invalid bound RefID or
form absent from the current world is an error. Saved form/script pairs are
not used as unique identities, since the corpus contains repeated pairs and
unbound instances.

Arrays are allocated once per saved ID, then filled and shared by every
variable naming that ID. Scalar, object and array references become VM values.
Both the script-instance and reference tables carry variable state. In the
corpus, `critterFish` reference entries include 95 slots and the state
`schooling`, despite FallrimTools naming that payload field `TYPE`.

Each variable retains its declaring script, including private parent and
child variables with the same name. The VM imports the saved state without
calling `OnInit`, `OnEndState` or `OnBeginState`. It validates declarations
before replacing any live instance. State names with no matching PEX body
are retained, as the VM's existing state transition path allows.

The returned instance and array ID maps are available for later stack import.
The save crate now depends on the existing `soe-papyrus` crate, whose only
runtime dependency is the existing `soe-trace` crate. `soe-bsa` is a test
dependency for reading installed PEX files. No new third-party package is
required.

The import refuses mismatched slots or declarations, duplicate target
instances, inheritance cycles, missing identities, invalid array elements
and unsupported variant/struct runtime values. Arrays use the existing VM
allocation ceiling. It also refuses to replace instances beneath existing
threads. It does not invent a form or alias binding from an opaque saved ID.

`Papyrus::restore_execution` converts active script frames after instance
import. It resolves frame owners through the supplied saved-object mapping,
checks saved instruction bodies against loaded PEX, and validates frame
positions and local slots before scheduling any thread. A nested call returns
through the destination operand in the saved caller instruction. Invalid
frame references leave the VM thread list unchanged. Queued messages and
suspended stack tables remain decoded data.

## Quest change and static data

`ChangeForm::quest_change` reads the documented QUST payload in full, including
the instance section omitted by FallrimTools `ChangeFormQust.java`. The older
`quest_stages` method remains a prefix reader for inspection. Optional sections
retain the distinction between an unset header bit and a present empty array.
Unknown change bits, invalid event parameter tags, impossible counts, truncated
fields and trailing bytes return errors. Counts are checked against remaining
bytes before allocation. Compressed change forms use the existing zlib path.

Saved objectives retain their ID and status word. Runtime reference aliases
carry one RefID for kind zero and five for nonzero kinds. xEdit names kind one's
five references as new reference, bound object, reference location, starting
location and location-ref type. Location aliases have their own array. Quest
instances retain separate alias text references and snapshots of global values.
The scalar fields without established save semantics remain explicitly unknown.

Story events retain their ID, four-byte code and typed parameter payloads.
xEdit identifies the code where UESP and FallrimTools describe an unknown float.
Type zero is a reference in xEdit, although the other two sources omit it.
The real QUST corpus contains 2,850 type-zero parameters. Reference types one,
two and four and integer type three are also represented. No event is executed
by decoding it. The script-change header bit has no payload in xEdit's schema.

`Save::quest_static_data` decodes global data type 107. Its event layout is the
same one used by QUST runtime data. The meaning of its outer arrays is not
established by the sources, so their order and fields are retained without
assigning scheduler behavior. Duplicate type-107 blocks return an error.

`QuestChange::data_bytes` and `QuestStaticData::data_bytes` independently rebuild
payloads from their typed fields. They preserve variable-count widths and float
bits, and reject inconsistent edits to count or presence fields. These are
diagnostic encoders. `Save::write` continues to use its original raw payloads.

Sources read before implementation were local UESP `QUST_Changeform.wikitext`
and `Quest_Static_Data.wikitext`, xEdit `wbDefinitionsTES5Saves.pas`, and
FallrimTools `ChangeFormQust.java` and `ChangeFlagConstantsQust.java`.
CommonLibSSE-NG `TESQuest.h` and `QuestObjectiveStates.h` establish runtime flag
and objective-state names, but do not establish how to resume all saved state.
The existing Rust cursor and compression paths are reused. No new runtime
dependency is introduced. Tests use the existing `soe-esm` crate to compare
saved IDs with installed quest definitions, adding no workspace packages.

The 2026-09-20 survey re-encoded all 32,256 QUST payloads across 60 saves,
1,987,156 bytes exactly. Those contain 92,077 stages, 6,442 objectives, 116,015
runtime alias entries, 1,735 story events, 9,978 quest instances and 9,308
instance global values. The type-107 blocks reproduce all 84,021 bytes.
Their event arrays are empty in this corpus and have synthetic coverage only.
Synthetic tests cover all 512 section-presence combinations, wide counts,
every truncated fixture prefix, compressed payloads and rejected mutations.

The game application path below uses the decoded quest state. Story-event
execution remains unimplemented. Transitional QUST flags restore a pending
start or stop promotion task, and those task states survive SOEW save/load.
Full ESS restoration remains unfinished.

## Applying saved quests

`World::restore_vanilla_quests` validates all QUST change forms before replacing
any live quest state. It reads winning definitions without lazily changing the
world. Invalid definitions, stage/objective/alias IDs, duplicate entries and
missing journal selectors fail atomically. Unlisted quests remain unchanged.
Present empty runtime data clears alias fill results. Omitted sections use
fresh quest defaults, including authored forced references.

Done stages drive GetStage, GetCurrentStageID and stage queries. The vanilla
Quest.psc describes both stage getters as the highest completed stage. Saved
stages do not establish the order of SetStage calls, so the internal last-stage
field starts at that highest stage. CommonLib QuestObjectiveStates supplies all
six objective statuses. The import applies enabled, completed, failed and active
bits separately. Remaining flags, change-form flags, script delay, already-run,
alias creation metadata and story-event context persist without invented events.

Journal entries store alias text forms and global snapshots under mapped form
identities. Current and historical entries share one canonical collection.
UpdateCurrentInstanceGlobal changes only the selected instance. The instance
ID, current selector, journal stage and journal-item interpretation follow the
matching CommonLib TESQuest/BGSQuestInstanceText structures. This field mapping
is an inference supported by all nonempty instance lists in the corpus, whose
selector always names an entry. xEdit and UESP leave those scalar names unknown.

SOEW version 11 preserves the journal and runtime extension, including unknown
bytes and float bits. Versions 4 through 10 remain readable. Their single global
snapshot map becomes instance zero when nonempty. The version-11 legacy prefix
and selected instance globals must agree bit for bit. Quest/global tables and
quest alias maps are sorted during serialization. Hashes include the extension.

No OnInit, startup stage, fragment, alias-fill or story event is dispatched by
restoration. The report lists identities missing from both current records and
resident references, and quests carrying stage-wait. Created references retain
their identity even before world change forms allocate them. QUST's transitional
stop/start flag word restores a pending promotion task. IsStarting and
IsStopping report its direction, and IsRunning remains false until the task
completes. SOEW version 37 persists pending quest promotion tasks in the
`SAV-10` world section. Older world states and version-37 saves without that
section load without a pending task.

The first 60-save application survey restores 32,256 quests and preserves every
engine-save byte and the world hash after reloading. It reports 6,816 unresolved
world identities across saves and one stage-wait quest. Synthetic checks cover
plugin reordering, native queries, all objective statuses, created aliases,
current versus historical globals, metadata persistence, old saves, lifecycle
task resumption and atomic failure. Runtime reference alias kind one has
synthetic coverage only. The corpus contains kind zero. No game has resumed
its quests from an ESS yet.

Sources were local Quest.psc, UESP QUST_Changeform, xEdit's QUST save definitions,
CommonLib TESQuest.h, TESQuest.cpp and QuestObjectiveStates.h. CommonLib's
reserved stop/start flag and the CK Quest_Script descriptions define the
restored lifecycle task direction. Existing form mapping, World, VM native and
binary state paths are reused. No dependency was added. Type-107 quest-static
state remains decoded inspection data. The remaining work includes
created/world forms, live Papyrus object bindings, scheduler state,
saved stacks and latent calls, atomic whole-game loading and host resumption.

## Saved forms and live globals

`soe_game::vanilla_save::SavedFormMap` resolves RefIDs through the save's own
FormID array, then matches regular and light plugin names to the current load
order without regard to ASCII case. It preserves the local object ID and
created-form namespace. It does not treat a saved runtime slot as a current
slot, allocate created forms or infer Papyrus object identity from a form.

Missing plugins, duplicate names, changed regular/light classification and
invalid references return errors. These are strict import checks, not a
missing-mod recovery policy. Created forms use the internal `u16::MAX`
namespace, separate from plugin positions, while their runtime IDs retain
the `FF` prefix. This mapping accepts all 254 regular and 4096 light plugin
slots, including a plugin at position 255. See [game.md](game.md#save-and-load)
for SOEW version 19 and the migration limits of older engine saves.

`World::restore_vanilla_globals` requires one global table and validates all
targets before applying any value. Each target must be a non-deleted winning
GLOB record, and duplicate targets fail. Unlisted globals retain their current
values. Float bits, including signed zero and NaN payloads, are preserved for ordinary
globals. Calendar globals require finite, valid date and clock values. Their
import initializes elapsed game days and timescale before execution, with
date and hour retained as separate globals. See
[calendar restoration](game.md#vanilla-calendar-restoration).
The global reader checks that the declared count exactly fits the payload
before allocating, including when the count uses a wider encoding.

The first corpus run exposed the engine-defined `PlayCredits` global, form
`00000063`, absent from Skyrim.esm. The local xEdit file
`Core/Hardcoded/Skyrim.esp` defines it as a short global with initial value zero.
The load order now supplies that record when the first file is Skyrim.esm and
does not define it. Authored records and later overrides retain precedence.
The source plugin on disk is not changed. This does not implement the credits
screen or supply the rest of Skyrim's hardcoded records.

Identity rules were read in local FallrimTools `RefID.java` and
`PluginInfo.java`, with table layout from UESP `Global_Variables.wikitext`.
The existing Rust save reader is reused. `soe-game` now depends on `soe-save`,
whose existing dependencies include encoding_rs, flate2, lz4_flex, thiserror,
soe-papyrus and soe-trace. No package was added to the workspace lockfile.

Validation on 2026-09-20 restored all 933 globals in each of the 60 external
saves, 55,980 entries total, into the five-master live form table. Every value
retained its float bits after an engine save/load cycle, with no events queued.
Synthetic checks cover reordered regular and light plugins, authored builtin
overrides, Papyrus GetValue, duplicate and malformed input, and unchanged world
state on failure. The corpus has no light plugins, so that path has synthetic
coverage only.

This API applies one component of save state. It does not synchronize calendar
or scheduler clocks, restore quest or actor state, apply change forms, bind
Papyrus instances or resume execution. Full ESS restoration still needs all
of those paths and an atomic whole-game load operation.

## Reference placements

`ChangeForm::reference_header` reads the REFR/ACHR placement prefix. It
decodes moved, created and promoted layouts, opaque Havok bytes, the two ACHR
header words, form masks, a replacement base and scale. It reports the exact
unconsumed suffix length. Scale is an integer percentage only in form version
55. Special Edition form version 78 uses a float.

`Save::player_location` reads the independent type-1 player location block
and next created-object ID. Duplicate blocks, truncated prefixes and impossible
Havok lengths fail. Typed diagnostic encoders preserve float bits and count
widths. They do not encode or validate the unparsed reference suffix.

Sources read were local xEdit `Core/wbDefinitionsTES5Saves.pas`, UESP
`REFR_Changeform.wikitext`, `ACHR_Changeform.wikitext` and
`Player_Location.wikitext`, and FallrimTools `ChangeFormInitialData.java`,
`ChangeFormRefr.java` and `ChangeFormACHR.java`. CommonLibSSE-NG
`TESObjectREFR.cpp` supplies the integer scale factor. The existing cursor,
compression, form mapping and cell index are reused. No dependency is added.
The secondary cell field has conflicting names in these sources, so it keeps
a neutral name and is not used to override the primary placement.

The decoder survey reproduced all 1,154,526 reference prefixes in 60 saves.
Of those, 23,571 are created references and 873,419 retain a nonempty suffix.
Every player primary location and position matches the independent type-1
block. No decoded transform in this corpus is non-finite.

The installed-data restoration survey applied all 1,154,526 placements and
verified their positions, base identities and cells through engine save/load.
World hashes match after reload and no events are queued. Across the saves,
the reports contain zero missing bases, 142 missing cell lookups and 1,538
references with no resolved cell. Missing or null locations remain unresolved.
Synthetic tests cover reordered regular/light plugins, negative cell grids,
allocation beyond saved IDs and unchanged whole-game bytes after late errors.
Running placements, globals and quests in that order restores all 32,256 quest
states with zero unresolved quest identities across the 60 saves, down from
6,816 when restoring quests without the reference pass. The combined world
hashes survive engine save/load. This does not resume quest or VM execution.

`World::restore_vanilla_reference_placements` builds a fresh reference table
atomically. Authored records supply unchanged placement properties, and saved
headers supply positions, rotations, scales and replacement bases. Saved
plugin names resolve bases and identities after load-order changes. Exterior
worldspace positions resolve through the existing 4096-unit cell grid, using
floor for negative coordinates. Missing bases and cells remain explicit in the
report. The player keeps its engine identity, and later dynamic allocation
reserves IDs from the saved counter, all change forms and the FormID array.

The pass refuses an already populated reference or actor table. It queues no
events and makes no actor-template selections. Restored references have no
inventory until a later inventory pass applies base contents and saved deltas.
Actor values, saved enable/delete masks, ownership and other extra data, Havok,
animation and script bindings are also still unapplied. Created NPC base forms
need their own restoration pass. The report retains counts and flags for these
requirements, while the original Save retains the payload bytes. An engine
save of this intermediate world does not retain the unparsed vanilla payloads.
This API is reconstruction groundwork, not a complete game-load entry point.

## Round trip

Everything not decoded is kept as raw bytes, so the body can be written back
exactly. `Save::split` gives the decompressed body and `Save::body_bytes` gives
the body this reader would write, and the two are compared byte for byte over
the whole corpus.

That raw-body check alone cannot validate decoded Papyrus values. The corpus
also re-encodes the typed second pass with `Papyrus::value_data_bytes` and
compares it with the original slice from `data_start` to `execution_start`.
`Execution::to_bytes` independently re-encodes execution data and compares it
with `execution_start..decoded_len`. These are diagnostic encoders for typed
data, not a save editor or a VM scheduler.

The file as a whole is deliberately not compared. Rebuilding it re-runs the
compressor and nothing promises an encoder reproduces the game's exact output,
so a mismatch there would say nothing about the parse.

## Expectations

`Save::to_expectations` renders quest stages and globals in the grammar
`validation.md` documents, as `expect stage-done <quest> <n> true` and
`expect global <name> <op> <value>`.

The save stores FormIDs and the harness resolves editor ids, so the caller
supplies the mapping. It comes from a loaded load order, which this crate does
not depend on: a save reader should not need the game's plugins to parse, only
to name what it found. A form the mapping does not cover becomes a comment
carrying its FormID rather than being dropped, so the output accounts for
everything in the save.

## Live run (2026-09-19)

Sixty real Special Edition saves, supplied by the user and held outside this
repository under `../engine-refs/saves/`. Saves are user data: none is
committed, and the tests skip cleanly when the corpus is absent.
`SOE_TEST_SAVE` names one file, `SOE_TEST_SAVES` names a directory.

Two sets. "Clean Saves" is nineteen files named after the main quest stage each
follows. "Main Quest + Dawnguard(Vampire) + Dragonborn" is forty one files
numbered in play order across the main quest, the Dawnguard vampire path and
Dragonborn.

| Measure | Result |
|---|---|
| Saves parsed | 60 of 60 |
| Bodies round tripped byte for byte | 60 of 60 |
| Typed Papyrus value payloads round tripped byte for byte | 60 of 60 |
| Papyrus value slots decoded, including array elements | 15,900,669 |
| Papyrus arrays decoded | 19,194 |
| Saves whose instance variables imported against installed PEX | 60 of 60 |
| Script-instance and reference entries imported | 2,649,854 |
| Shared arrays imported | 19,194 |
| Typed second-pass bytes re-encoded | 211,771,719 |
| Typed Papyrus execution payloads round tripped byte for byte | 60 of 60 |
| Active threads and stack frames decoded | 170 threads, 375 frames |
| Saved instructions decoded | 4,900 |
| Queued messages and suspended stacks decoded | 9 messages, 3 stacks |
| Typed execution bytes re-encoded | 118,439 |
| Change forms read | 1,344,172 |
| Quest stage entries read | 92,077 |
| Globals per save | 933, the same in every save |
| Papyrus block | 4.87 MB at character creation, 6.16 MB at Miraak defeated |
| Change forms per save | 10,000 at character creation, 33,463 at Miraak defeated |

Every count rises with story progress, which is the shape a real playthrough
should have.

### Progression

The numbered set is in play order, so a stage marked done in one save must
still be done in every later one. Over the 41 step sequence, 29,572 carry overs
held and 14 cleared.

All 14 are stage 200 or 255, the conventional shutdown stages, on eight forms.
That is `ResetQuest` clearing a radiant quest so it can run again for a new
target, which is the game doing what it is supposed to do. The test reports
them and fails only if the cleared share passes five percent, since a decode
error would clear most carry overs rather than a handful.

This is the check a single file cannot give. It tests the decode against the
shape of a real playthrough instead of against itself.

### Not covered

The player level reads 1 in all sixty saves. These are prepared saves driven
through the story with console commands rather than played, so that is the
data rather than a misread, and the header size check passing on all sixty
rules out a misaligned field.

No save with a mod load order was available, so the light plugin list is
exercised only as an empty list, and `RefID` resolution through the form id
array is exercised but not against a case where a mod supplies the form.

Execution decoding is checked with synthetic reordered thread bodies,
fragment variants, optional messages, malformed instructions and every
truncated prefix of the execution fixture. The real corpus has few queued
messages and suspended stacks. It does not establish how to resume latent
native calls, reconstruct scheduler timing or apply change forms to the world.

The instance-import survey requires both `SKYRIM_DATA` and `SOE_TEST_SAVES`.
It uses distinct standalone VM objects for saved IDs, not live game forms.
It validates every saved instance and reference against installed PEX
declarations and requires zero scheduled instructions after import.
Synthetic tests execute functions against restored state, distinguish parent
and child values, preserve shared arrays through VM save/load, and check that
late errors do not replace existing state. The external corpus is read in
place and never copied.

Game-world object bindings, alias and active-effect identities, change-form
application, saved stack conversion, pending native calls and scheduler clocks
remain required for full vanilla-save restoration.

## Reference contents and inventory, work in progress

`ChangeForm::reference_contents` reads the extra-data and inventory sections
following a placement header. Inventory nesting follows xEdit: entries carry
a signed count delta and a list of stacks, each with its own extra-data list.
Typed fields retain scalar widths, reference IDs, count widths, string bytes,
float bits and nested arrays. Diagnostic encoding reconstructs the section
exactly. Remaining actor or animation bytes remain explicit.

Sources are UESP `Extra_data.wikitext`, REFR/ACHR change-form pages, xEdit
`wbDefinitionsTES5Saves.pas`, and FallrimTools `ChangeFormExtraDataData.java`
and `ChangeFormNPC.java`. xEdit supplies signed rank width and inline NPC
sections where the other readers disagree. Array counts are bounded before
allocation. Unknown extra layouts stop decoding instead of searching for a
plausible next item. The nonempty PackageData payload is still unsupported.
Its absence from these public schemas remains a format gap.

The game-side application maps references in every decoded extra field and
retains exact non-reference scalar data. CommonLib `ExtraDataList::GetCount`
supplies the implicit count of one and `ExtraCount.h` its signed interpretation.
CommonLib `TESObjectREFR::GetInventory` combines base counts and deltas and
excludes authored counts for entries carrying ExtraLeveledItem. It also keeps
entry totals distinct from the extra lists, which is required by the corpus.
See game.md for the current mutation, persistence and execution limits.

The current 60-save decoder survey reproduces 1,154,357 reference-content
sections exactly, including 174,550 inventory sections, 865,169 item entries
and 844,285 stack lists. It still fails on 169 PackageData kind-18 payloads.
The corpus test continues to require zero failures. No failing record is
skipped to pass it. The separate non-actor application survey is evidence for
that narrower path, not full inventory restoration. Quest and reference-header
live tests pass all 17 checks after the count-width correction.

## Saved actor choices

`ChangeForm::leveled_creature` reads the prefix through the declared
ExtraLeveledCreature entry and reports where it stopped. Unknown earlier
layouts and a missing declared entry fail. Later extras, inventory and actor
execution remain outside this prefix's validation scope. The original and
selected RefIDs and the named `NpcChange` fields are retained. Both the prefix
reader and complete extra-data reader use the same NPC decoder. The complete
reader exposes a field-tree view that retains RefIDs for inventory mapping.
The full decoder's 169 PackageData failures remain required unfinished work.

The runtime restores all 156,169 template choices across the external saves
without rerolling and creates a private ActorBase for each. It applies the
4,930 inline NPC changes to those bases. A separate diagnostic applies the
114,696 actor inventories whose
complete contents decode, preserving 921,081 item entries and 678,486 stack
lists through engine save/load. It excludes 126 inventory-bearing actors with
unsupported PackageData and accepts no other decoding failures. There are no
missing item definitions in the applied subset. Forty-one entries have a zero
item total while retaining worn metadata, so the importer preserves both
without turning the metadata into an extra item. This diagnostic does not
prove complete actor inventory restoration or vanilla item-selection behavior.

## NPC change records

`ChangeForm::npc_change` decodes an entire NPC_ change payload and rejects
trailing bytes. `NpcChange::read_prefix` exposes the same field sequence for
embedded NPC changes and returns the exact number of bytes it consumed.
Neither API applies the changes to the live world. ACBS, AIDT and DNAM retain
their fixed record layouts. Factions have signed ranks, and spells, leveled
spells, shouts, class, race, outfits and face links retain saved RefIDs until
the world maps them through the saved plugin list. Face morph options retain
float bits, including NaNs and negative zero. Names retain their string bytes.

The sources were xEdit `Core/wbDefinitionsTES5Saves.pas`, specifically
`wbChangeFlags009`, `wbChangedActorBase` and `wbChangedNPC`, and FallrimTools
`ChangeFormNPC.java`. The existing Rust cursor and change-form decompressor
are reused. The Pascal and Java implementations are not libraries this Rust
crate can call. No dependency was added and no source code was copied.
Variable-length counts use the corrected one-, two- and four-byte reader from
the pending inventory work. That prerequisite is identical in both worktrees.

xEdit puts head parts and morph data outside the optional head-data block.
FallrimTools puts them inside it. This reader follows xEdit. The corpus has no
NPC face change with an absent head block, so the empty-head distinction is
covered by synthetic fixtures, not established by the corpus. Attribute bit 2
has no serialized payload in xEdit. Unknown bits are rejected. Presence flags,
retained list counts and remaining-byte bounds are checked before re-encoding.

Before the first corpus run, the acceptance check required at least 50 saves,
more than 100 fully decoded NPC changes, one decoded player base per save and
zero failures. All 13,362 NPC changes across 60 saves decode and re-encode
exactly from named fields. This includes 60 player bases and no created NPC
base identities. Synthetic tests cover every optional field alone, the full
field order, every truncation point, compressed payloads, reserved and
oversized counts, wide zero counts, invalid presence edits and complete versus
embedded sequence boundaries.

`World::restore_vanilla_npcs` now applies standalone NPC changes to shared
runtime record overrides before reference and actor initialization. See
[game.md](game.md#restoring-npc-records) for identity mapping, engine-save
persistence and verification, including temporary per-actor bases and inline
NPC application. This reader does not restore actor value modifiers
or saved AI execution, and nonempty PackageData remains open.

The embedded-NPC acceptance check requires at least 50 saves, more than
100,000 leveled entries and more than 4,000 nonempty NPC changes, with exact
re-encoding of every embedded payload. The 60-save run passes all 156,169
entries. Of the 4,930 changed entries, four contain only ACBS and 4,926 contain
ACBS, AIDT and DNAM. All saved ACBS blocks have the copied-template bit named
by CommonLib's `ACTOR_BASE_DATA::TEMPLATE_USE_FLAG::kCopiedTemplate`.

CK's GetActorBase and GetLeveledActorBase pages distinguish the authored base
from the temporary per-actor base. Applying these payloads to the shared
selected NPC would change unrelated actors. The runtime now creates private
records with inherited components and applies inline changes to them. Known
links retain their source namespace through relocation tokens. These bases
persist in SOEW version 14. Saved Papyrus references to vanilla temporary
base identities still need rebinding before complete ESS resumption.

## Empty package-state follow-up

The decoder accepts the observed empty kind-18 PackageData variant while
retaining explicit errors for named procedure state and other unknown layouts.
The earlier 169-failure reference survey above predates this increment. The
isolated package survey reduced that count to 81 and recovered 73 additional
inventory sections. See [saved package runtime state](save-package-data.md) for
source evidence, boundaries and integration validation. Runtime package
restoration and complete ESS resumption remain unfinished.

The nine observed kind-18 `Stacked` payloads remain rejected. The local public
save readers name the variant but do not define its fields or termination, so
the decoder does not guess where following extra data begins. The SAV-21
regression test keeps this unsupported case explicit.

## Reference mask semantics, SAV-15

The runtime retains both `CHANGE_FORM_FLAGS` words in the restore report. For
the observed Skyrim SE tuple of ESS header version 12, form version 78 and
change-form version 78, the placement pass applies the first word to the
runtime disabled and persistent properties. Other tuples with a mask fail
before reference state is inserted. Deleted winning records remain disabled
regardless of the saved mask.

Local xEdit `Core/wbDefinitionsTES5Saves.pas`, at
`wbUnionCHANGE_FORM_FLAGS`, names a 32-bit Mask Invert Flags followed by a
16-bit Mask Set unk010. The implementation interprets the first word as an
XOR mask over authored record flags, limited to `INITIALLY_DISABLED` and
`PERSISTENT`, whose runtime bits are identified by CommonLibSSE-NG
`TESForm.h` and checked by `TESObjectREFR.cpp`. The second word remains
preserved and does not affect these properties. FallrimTools
`src/main/java/resaver/ess/ChangeFormFlags.java` reads and writes the words
as FLAGS and UNKNOWN without applying them. UESP's local
`REFR_Changeform.wikitext` links a ChangeFlags page absent from the corpus.
The online page and MediaWiki API could not be retrieved during this audit.

The supported tuple is bounded by the local 60-save corpus, which contains
the observed masks only with those three version values. xEdit supplies the
operation label, not an independent version table. Synthetic tests exercise
all authored combinations of the two relevant flags for REFR and ACHR and
check that the second word does not change the result. A gated comparison
uses a reference's authored flags from installed Skyrim.esm as its controlled
input. This validates the engine's application against real vanilla record
bits, but does not claim a game-generated before-and-after save comparison.

The existing `all_external_reference_headers_reencode_and_compare_player_locations`
test now reports mask counts after re-encoding the complete typed prefix.
All seven reference-header tests pass with the 60-save corpus supplied.
All 1,154,526 prefixes reproduce their bytes exactly. Of 328,337 masks,
276,805 belong to REFR and 51,532 to ACHR. Every observed mask has save-header
version 12, form version 78 and change-form version 78.

| Observed bits in the first word | Entries |
|---|---|
| Neither bit 10 nor bit 11 | 69,708 |
| Bit 10 alone | 61,999 |
| Bit 11 alone | 147,495 |
| Both | 49,135 |

There are 826,189 references without a form-mask field. The second word is
nonzero in 68,235 masks, with observed values 0x0001, 0x0004, 0x0008,
0x0010, 0x0040, 0x0048 and 0x0050. These observations establish layout
coverage. The first word is applied only to the two runtime properties
described above. The second word remains opaque.

The runtime consumer audit found that `World::load_ref_record` derives enable
and persistence state from the winning plugin, while `World::unload_cell` and
`World::is_persistent_child` consume those values. Restored mask results flow
through those existing properties and are retained by engine save/load.

The SAV-54 follow-up on 2026-09-27 checked the same xEdit definition,
FallrimTools `ChangeFormFlags.java` and CommonLibSSE-NG
`TESObjectREFR.cpp`. xEdit's `ChangedFlag00Decider` selects whether the mask
payload is present. It does not select an application operation or a version
specific layout for the two words. FallrimTools reads and writes the words
without applying them. CommonLib checks authored runtime flags for disabled
and persistent state, but its public code does not connect those checks to the
saved words.

The recorded corpus survey has masks only in saves with header version 12,
form version 78 and change-form version 78. It records word values and byte
round trips, not game-generated before-and-after reference state, and does not
cover other save versions. SAV-54 remains open for those broader source and
runtime observations. SAV-15 implements the bounded interpretation above.

## ARMO, WEAP and AMMO change coverage

SAV-33 decodes change types 12, 23 and 24 with the shared base-object fields
defined in xEdit `Core/wbDefinitionsTES5Saves.pas`. For all three types, bit 0
selects a 32-bit mask and a 16-bit word. Bits 1 and 2 select the object-value
and full-name fields. Both xEdit unions use `wbNull` in their changed and
unchanged alternatives. The local UESP format conventions define null as a
zero-byte field, so these bits are typed markers and add no payload bytes.
Bits 3 through 31 are unnamed by xEdit and remain in the change flag word.
Any bytes after the defined fields are preserved without interpretation.

Local FallrimTools `ChangeForm.java::getData` dispatches none of these three
types to a typed reader. Its `ChangeFormDefault` fallback stores the raw bytes.
The UESP save pages identify the three change-type indices but do not describe
their payload fields. xEdit plus the UESP null-field convention establish the
shared fields, while the FallrimTools fallback supports retaining unclassified
bytes. No in-memory plugin record layout is used for the save payload.

The gated `sav_33_corpus_reports_item_change_field_coverage` survey records
save version, change version, record type, flags, payload size and count in
sorted order. Its threshold is at least 50 saves. The 2026-09-20 corpus run
read all 60 external saves and found zero ARMO, WEAP and AMMO change forms.
Each subsequently observed item payload is decoded and re-encoded byte for
byte by the survey. External saves are read in place and are not copied into
the repository.

Synthetic `sav_33_` tests cover all eight combinations of the three named
flags across all three item types, preservation of unnamed bits and remaining
bytes, and truncation at every byte boundary of the six-byte form-flags
section. An installed-data test checks that Skyrim.esm contains all three
vanilla record types. Decoding does not assign meaning to unnamed flag bits or
remaining bytes, and the external corpus currently has no item change forms.

## Projectile payload source audit

SAV-11 remains blocked. The PMIS, PGRE and PBEA payloads need an established
subtype layout before a typed decoder can satisfy the original completion
check. The following local sources were read before considering a parser.
Paths are relative to `../engine-refs/`.

| Source | Evidence and limit |
|---|---|
| `TES5Edit/Core/wbDefinitionsTES5Saves.pas`, `wbChangeFlags002` through `004` | Names the reference change flags for all three types. Flag names alone do not define their payloads. |
| The same file, `InitialDataTypeDecider` | Explicitly includes change types 2, 3 and 4 in the reference placement-prefix selection. |
| The same file, `wbChangedProjectile` | Declares a common projectile structure with scalar fields, references, optional inventory, an impact array and a versioned tail. It is never used by a dispatch entry. |
| The same file, `wbChangedFormData` | PMIS, PGRE and PBEA dispatch entries are empty. They do not bind the common structure or supply subtype fields. |
| The same file, `ChangedFormProjectileHasInventoryDecider` | Searches for a container named `Change Projectile`, while the unused structure is named `Changed Projectile`. This is another unresolved detail in that candidate definition. |
| `FallrimTools/src/main/java/resaver/ess/ChangeForm.java`, `getData` | None of these three types has a reader case. They reach `ChangeFormDefault` with a raw body when best-effort parsing is requested. |
| `uesp-savefile/Save_File_Format.wikitext` | Identifies change types 2, 3 and 4, without defining their payload fields. |
| `CommonLibSSE-NG/include/RE/P/Projectile.h` and the Missile, Grenade and Beam projectile headers | Declare per-class `SaveGame` and `LoadGame` overrides. The inspected sources contain no serialization implementations. Runtime member offsets are not saved field layouts. |

The xEdit snapshot has SHA-256
`229cc3df9a8daacf68f0f1da511acff02a96e3803ba158726aac1748a9b7fdec`.
The FallrimTools `ChangeForm.java` snapshot has SHA-256
`5e77598928fe3d21017e9df7d199133b6c181753f86806cbcd96f9c5bcd45319`.
No third-party code, parser dependency or executable disassembly was used.

The external census in `tests/projectile_coverage.rs` visits every change form
and reports each projectile type, flag word and file/form/change version. It
checks payload decompression through the existing reader. It does not claim
typed decoding or typed re-encoding. Before its first run, it required at
least 50 saves and more than 100,000 inspected change forms. The 60-save run
inspected 1,344,172 changes and found:

SAV-11 regression tests round-trip the three record indices, header flag words
and opaque payload bytes, and reject every truncated prefix of each synthetic
change form. The corpus test requires PMIS and PGRE records and reports the
PBEA count, including zero. These checks cover the generic envelope only.

| Type | Change flags | Count | Decompressed payload bytes |
|---|---|---:|---:|
| PMIS | `00000002` | 25 | 4,375 |
| PMIS | `10000002` | 15 | 6,479 |
| PGRE | `00000001` | 148 | 21,756 |
| PGRE | `10000000` | 21 | 7,413 |
| PBEA | None observed | 0 | 0 |

All observed projectile entries use file/form/change versions 12/78/78. The
census passed, but it is not SAV-11's completion check. There is no PBEA corpus
coverage, and the four observed flag combinations cannot stand in for every
source-defined combination. No save was copied or changed.

Unblocking the decoder requires a public format definition or reader/writer
implementation that binds the common fields and each subtype extension to
PMIS, PGRE and PBEA, including flag and version conditions. Controlled runtime
observations could establish those missing boundaries. The typed encoder must
then reproduce the payloads exactly and reject truncated fields across the
source-defined combinations. Wiring an unused candidate structure into all
three dispatch cases without that evidence would not establish compatibility.
SAV-01's live-record application remains dependent on the completed decoder.

## CLAS, FACT and RELA changes

`ChangeForm::class_change` retains the optional six-byte form flags and the
tag-skills change marker. The local xEdit Skyrim save definition names that
marker but supplies no payload layout, so the marker has no decoded data.

`ChangeForm::faction_change` reads the optional form flags, a variable-count
reaction array, the faction flag word, and four crime words. Each reaction
retains its faction RefID, signed modifier and unsigned reaction word. The
count keeps its original width. The reaction and crime field boundaries follow
the external Special Edition corpus and the corresponding definitions in
local xEdit FO3/FNV sources. Skyrim xEdit names both sections but leaves their
payload definitions empty. The corpus places reactions before the faction flag
word when both bits are set, despite xEdit listing the flag word first. Crime
words remain uninterpreted.

`ChangeForm::relationship_change` reads three RefIDs for a created form,
regardless of change flags, then an optional signed rank under bit one. This
ordering follows local FallrimTools `ChangeFormRela.java`. Noncreated forms do
not carry the three identities. The optional six-byte form flag section follows
xEdit's shared change flag definition.

All three readers reject unknown change bits, truncated known fields and
trailing bytes. Their typed encoders reconstruct the change flags and payload,
including empty versus absent reaction arrays. `Save::write` still uses the
original change-form bytes. The decoder does not apply these changes to live
records. The local UESP save format page supplies the three change type
indices. FallrimTools handles CLAS and FACT as opaque forms.
The game crate's `World::restore_vanilla_record_changes` validates CLAS
changes and applies FACT and RELA fields to winning record overrides. FACT
reactions update warm faction runtime state in the same transaction. xEdit's
CLAS tag-skills marker has no payload and therefore changes no live record
field. The shared six-byte form-flags section XORs its mask into authored
`Record::flags`, and its unknown final word is retained in SOEW version 37.
The 60-save external check reproduced all 345,703 payload bytes from 4,702
FACT and 547 RELA changes. It found no CLAS changes, so CLAS field coverage is
synthetic only.

## INGR, ALCH and ENCH changes

`ChangeForm::ingredient_change` reads INGR's optional six-byte form flags and
retains the ingredient-use marker. `alchemy_change` and `enchantment_change`
read the same form flags and retain the base-object value and fullname markers.
These marker unions have no payload in xEdit's `wbDefinitionsTES5Saves.pas`.
All remaining flag bits are named unnamed by xEdit and remain in the typed
value without assigned meaning. The typed encoders reproduce the markers and
the form-flag bytes, and the readers reject truncated form flags or trailing
payload bytes.

The local UESP save format page and FallrimTools `ChangeForm.Type` establish
the INGR, ALCH and ENCH type indices. FallrimTools has no typed item change
reader, and UESP has no dedicated pages for these three records. The local
xEdit save definitions establish their flags and payload layout. The
`sav_37_external_corpus_accounts_for_every_item_change` survey accounts for
all matching changes, flag combinations and payload bytes in the supplied
save corpus. The `sav_37_installed_skyrim_records_use_expected_record_types`
test checks that the installed Skyrim.esm contains all three corresponding
record types.

## PACK and NAVM changes

`ChangeForm::pack_change` reads the optional six-byte form flags and records
the waiting and never-run bits as typed zero-byte markers. `navmesh_change`
reads the same form-flags section and records the base-object value and
full-name markers. Both readers reject unknown bits, incomplete form flags,
and payload suffixes. Their encoders reproduce the change flag word and
payload exactly.

The field order and bit names follow xEdit's
`Core/wbDefinitionsTES5Saves.pas`. That file defines PACK as form flags,
waiting and never-run, and NAVM as form flags, base-object value and full name.
Its `wbNull` marker definitions consume no bytes. The UESP save format table
and FallrimTools `ChangeForm.Type` confirm change type indices 32 and 33.
FallrimTools does not decode these type payloads, so its reader is used only
to cross-check the type indices. Synthetic tests cover all eight combinations
for each type and each truncated form-flags prefix. The external corpus test
accounts for all matching PACK and NAVM payloads and records their flag counts.
`World::restore_vanilla_pack_navmesh` in the game crate applies decoded fields
to the shared record table only after validating the complete batch. The
record override retains marker state that has no plugin subrecord, and SOEW
version 37 carries it through engine save/load. Package and navmesh queries
continue reading their records from the same world table.

## INFO, SMQN and SCEN changes

`ChangeForm::info_change`, `story_manager_quest_node_change` and `scene_change`
decode the optional six-byte form flags and the bit 31 state marker. Their
typed values distinguish an absent flag section from one whose bytes are zero.
The INFO marker is said once and has no payload. SMQN's time-last-run marker
carries a 32-bit count followed by that many float bit patterns. Its decoder
checks the count against the remaining bytes and preserves NaN and signed-zero
bits. SCEN's active marker carries variable scene data. The decoder retains
that data as opaque bytes because its internal field boundaries are unknown.
The typed values reconstruct their flags and payload bytes. `Save::write`
still keeps the original compressed or raw change-form data.
The scene encoder rejects active data when its active marker is absent.

The field order and flag positions follow local TES5Edit
`Core/wbDefinitionsTES5Saves.pas`. UESP `Save_File_Format.wikitext` supplies
the INFO, SMQN and SCEN change type indices. FallrimTools `ChangeForm.java`
dispatches these types to its opaque default reader, so it supplies no field
layout to reuse. The 60-save external corpus supplies the observed SMQN count
and float-array layout. xEdit lists both SMQN and SCEN bit 31 as null payloads,
which conflicts with that corpus. Unknown change bits, truncated form flags,
invalid SMQN counts and trailing bytes outside SCEN's active data return errors.
SCEN active data cannot yet be checked for interior truncation.
The external survey read 60 saves in place and re-encoded all 32,438 INFO,
3,054 SMQN and 807 SCEN changes exactly, covering 197,758 payload bytes.
Every observed change used bit 31 alone. The other three defined flag
combinations have synthetic coverage.

## Form-list and leveled-list changes

`ChangeForm::form_list_change` decodes FLST additions.
`ChangeForm::leveled_list_change` decodes LVLN, LVLI and LVSP additions.
Both retain the optional shared form mask and undocumented word, list order,
duplicates and null identities. A missing list section remains distinct from
a present empty list. Saved identities remain RefIDs until runtime mapping.
The typed encoders reconstruct payloads and expose matching change flags.
`Save::write` continues to use the original raw change-form payload.

Sources read were local FallrimTools `ChangeFormFLST.java`,
`ChangeFormLeveled.java` and `LeveledEntry.java`, plus xEdit
`Core/wbDefinitionsTES5Saves.pas` change types 43 through 46. FallrimTools
supplies a 32-bit FLST count and a byte-sized leveled-list count. Each leveled
entry is a RefID, level byte, count word and chance byte. These fields retain
all wire bits without assigning runtime selection behavior. The Java reader
cannot be called as a Rust library, so the existing Rust cursor and zlib path
implement those declared layouts. No source code was copied and no dependency
was added.

FallrimTools checks bit 1 for FLST form flags. xEdit names bit 0 for that
section on all four list types. The decoder follows xEdit. A fixed fixture
asserts the six flag bytes and rejects bit 1 as unknown. The corpus contains
only the added-list flag and cannot independently resolve that discrepancy.
xEdit uses the same leveled-list section for LVLN, LVLI and LVSP. That shared
layout supplies LVSP coverage even though FallrimTools does not dispatch LVSP.

Counts are checked against the bytes remaining before allocation. Unknown
change bits, impossible counts, truncated sections and trailing bytes return
typed errors. FLST does not inherit FallrimTools' arbitrary 16,383-entry limit.
The leveled encoder rejects more than 255 entries instead of truncating its
count. Level, count and chance fields retain high bits, avoiding Java signed
byte conversion at the wire boundary.

The seven `list_change` tests cover every combination of the two declared
flags for all four types, independent field values, every truncated prefix,
trailing bytes, compression, unknown bits, omitted and empty sections, count
bounds and encoder overflow. Before the corpus run, acceptance required at
least 50 saves, at least one list change, zero decoder failures and exact
re-encoding of every matching change.

The 2026-09-20 run read 60 external saves in place and reproduced all 42,696
payload bytes from 1,401 FLST, 1,620 LVLI and 60 LVLN changes. File form version
and change version were both 78. No LVSP change occurred in the corpus, so
LVSP and shared form-mask combinations have synthetic coverage only. No save
bytes were copied into the repository. `World::restore_vanilla_lists` in the
game crate applies decoded changes to live record overrides.

## WOOP and MGEF change application, SAV-50

`World::restore_vanilla_base_objects` maps all WOOP and MGEF changes before
installing their record overrides. It retains the optional six-byte form flag
pair, both zero-byte change markers and unnamed flag bits. It validates every
change before installing overrides. The optional form-flag mask is XORed with
the winning record flags, following xEdit's `CHANGE_FORM_FLAGS` definition as
"Mask Invert Flags". The base-object value and full-name markers do not include
replacement data, so saved records keep their authored subrecords. MGEF runtime
views read through the override-aware record lookup. SOEW version 37 stores
the retained metadata and reapplies the mask after engine save/load. Earlier
versions still load without that extension.

## Reference ownership and locks, SAV-16, blocked

The partial `World::restore_vanilla_reference_extras` pass inspects every
REFR/ACHR top-level extra list, including general game-only and promoted lists.
It requires resident placements and resolves links through saved plugin names.
It validates the entire pass before mutation. Undecodable lists, invalid targets,
duplicate entries and missing or mistyped links fail without events or changes.
`ChangeForm::reference_extra_data` reuses the existing decoder without traversing
inventory stacks or later actor fields.

ExtraOwnership replaces the shared owner used by GetActorOwner and
GetFactionOwner. ExtraRank and ExtraGlobal retain their signed rank and mapped
global identity. ExtraLock replaces the shared level and locked bit, overrides
the authored key even when null, and retains its other flag bits and two words.
A declared ownership or lock change without the corresponding extra clears
that state. The lock bit applies only to REFR. ACHR bit 12 names merchant
container changes and must not clear a lock. Unlisted fields remain unchanged.
The removal interpretation follows the extra-list model and has synthetic
coverage, without comparison against a running vanilla executable.

Sources read were local UESP REFR_Changeform and Extra_data, FallrimTools
ChangeFormExtraDataData, xEdit wbChangedExtraData and ACHR change flags,
CommonLibSSE-NG ExtraLock.h and ExtraLock.cpp, and CK SetActorOwner and
SetFactionOwner. xEdit establishes the signed rank where UESP and FallrimTools
describe a reference. CommonLib defines the locked and leveled flag bits.
The saved level and flags byte ordering is an inference checked against the
corpus, since the public save schemas leave those bytes unnamed. The two
trailing words keep their bits without assigned behavior.

The initial filtered survey found 10,404 lock extras in 60 saves. Of 9,143
with authored XLOC data, 8,902 matched the proposed level byte, above the
90 percent threshold written before running. The flags byte contained only
zero, locked, leveled or locked plus leveled. The filtered application restored
12,557 references with 2,194 ownership changes and 10,404 locks, including
662 leveled locks. Those records survived engine save/load exactly.
These results do not establish the full SAV-16 contract.

Independent enumeration found 34 additional ExtraOwnership entries under
ACHR general flags 0x88000422. It also found 81 undecodable top-level lists,
72 carrying nonempty Sequence PackageData and nine carrying Stacked. Their
unknown boundaries prevent ruling out ownership or lock extras later in each
list. SAV-16 is blocked on SAV-12 and SAV-21. The importer now covers general
flags and rejects such undecodable lists atomically. The full-corpus acceptance
tests remain failing, without exclusions or a narrowed success claim.

The SOEW version 20 extension retains reference ownership and lock
metadata. Versions 4 through 19 remain readable and use their authored keys.
Load rejects missing or repeated reference targets, invalid key/global links,
conflicting locked bits, out-of-range levels and truncation before replacing
live state. Sorted serialization and hashing include every retained field.
Synthetic checks cover generic extra lists, the ACHR merchant flag, mapped
native owners and keys, activation, null and removed extras, corruption,
legacy versions and late unsupported data.

Leveled-lock scaling, ownership crime rules, rank/global ownership conditions
and lockpicking remain separate mechanics. Leveled locks are reported.
No dependency or host node was added. Full ESS loading and saved execution
are outside this pass. This is a blocked implementation checkpoint.

The checkpoint is reconciled onto completed SAV-07 with SOEW version 20.
The version 19 namespace boundary and difficulty/weapon-hand state remain intact.
The full gate passes with 1,215 emitted Rust test passes and 11 Python tests,
including gated tests that return early without external data. All optional
tools ran with no warnings. The game regression run passes 433 tests. A combined
world fixture preserves reference metadata alongside created identities, AI,
quests, modifiers and combat selections, and reads version 19 without metadata.
The host combat compatibility fixture adds only the empty metadata suffix.

The installed ownership suite passes seven synthetic checks and fails all three
strict corpus checks. Independent enumeration again reports 34 unflagged owner
entries and 81 undecodable lists, 72 Sequence and nine Stacked. The application
and lock-schema checks fail at the first unsupported Sequence payload. No saves
were excluded, and no corpus restoration success is claimed by this checkpoint.

## Actor-value restoration evidence

The SAV-06 engine-save check verifies that current and permanent actor-value
components remain separate through SOEW save/load. Vanilla ACHR modifier
restoration remains blocked on SAV-64's missing serialized component layout.
The 60-save ACHR survey accounts for 243,595 records, all without the five
documented actor-modifier bits. Existing prefix decoding fails on 81 named
package states and leaves actor data unparsed. These observations do not
establish zero modifiers or their field order. See
[saved actor-value modifiers](save-actor-values.md) for pinned sources,
coverage counts, dependency boundaries and the required completion evidence.

## Created Objects decoder checkpoint

`Save::created_objects` reads all four type-4 magic-item tables using the UESP
and skymp field definitions, retaining exact count encodings and float bits.
SAV-65 remains blocked because all 60 external saves have empty tables and the
predeclared populated-corpus requirement fails. Synthetic fields and malformed
input are covered, but no real nonempty entry is validated. See
[Created Objects tables](save-created-objects.md) for sources, full accounting,
the required fixtures and SAV-17's separate live-record restoration boundary.

## Base-object value and full-name field evidence

`ChangeForm::base_object_fields` decodes the shared prefix used by ACTI, TACT,
ARMO, BOOK, CONT, DOOR, LIGH, MISC, APPA, STAT, MSTT, FURN, WEAP, AMMO, KEYM,
ALCH, IDLM, NAVM, WOOP, MGEF and ENCH. xEdit defines optional six-byte form
flags followed by `CHANGE_BASE_OBJECT_VALUE` and
`CHANGE_BASE_OBJECT_FULLNAME`. Their masks are `0x2` and `0x4`. The xEdit
`wbNull` definition maps to zero bytes in `TwbByteArrayDef.GetSize`, so these
flags do not consume payload. The decoder returns the byte boundary and
remaining suffix length for type-specific readers. It does not restore a
value or name. See [saved base-object fields](save-base-fields.md) for the
source audit and scope.

`ChangeForm::light_static_change` fully decodes LIGH, STAT and MSTT. It accepts
the three defined flag bits, retains the optional form flag words, and treats
the value and full-name markers as zero-byte fields. It rejects other flag
bits, incomplete form flags and trailing payload bytes. The typed fields
rebuild the same flags and payload bytes. SAV-39 tests all eight defined flag
combinations for each record type and reports record counts, distinct flag
sets and payload bytes from `SOE_TEST_SAVES` when that corpus is configured.

`World::restore_vanilla_base_objects` applies these fields to shared record
overrides after mapping saved identities through plugin names. It validates
all LIGH, STAT and MSTT changes before changing the world. The live records
remain available through `World::record`, while
`World::restored_base_object_extra` returns the saved form-flag words and the
two zero-byte change markers. Since the markers contain no replacement value
or name, record-backed value and name queries continue to use the winning
plugin record. SOEW version 37 retains this metadata through engine save/load
in the SAV-53 world section.

`ChangeForm::apparatus_change` and `ChangeForm::idle_marker_change` decode
APPA and IDLM payloads from the xEdit `Change APPA Data` and `Change IDLM Data`
definitions. Both have an optional six-byte form-flags section. The value and
full-name change flags consume no payload bytes. The complete change-flags
word, including unnamed bits, is retained. Typed payloads re-encode the known
fields, and trailing or truncated form-flags data returns an error. FallrimTools
keeps both types raw. The gated SAV-43 survey accounts for both types across
the external save corpus and checks exact typed re-encoding. The 60-save corpus
contains no APPA or IDLM changes, so payload coverage comes from synthetic
fixtures. UESP's local mod-file format pages describe the APPA and IDLM records,
not their save payloads. The game applies their form-flag inversion masks to
winning records and keeps the decoded fields in the SAV-44 world section.
This section leaves older version-37 saves unchanged when absent.

## BOOK, NOTE and KEYM saved fields

`ChangeForm::book_change`, `key_change` and `note_change` decode the xEdit
source-defined fields for these three record types. Each has an optional
six-byte form-flags section. BOOK also reports the value, full-name, teaches
skill and read markers. KEYM reports the value and full-name markers. NOTE
reports its read marker. Those named marker fields consume no payload bytes in
xEdit's `wbNull` schema. The decoders reject reserved flag bits and truncated
form flags. Bytes after the defined fields remain an opaque suffix and are
written back unchanged. This retains corpus data such as the byte `08` on a
BOOK carrying only the read marker, whose meaning is not defined by xEdit.
FallrimTools keeps these payloads raw, and UESP documents the outer
change-form framing rather than these record layouts. The corpus test accounts
for every BOOK, NOTE and KEYM change in the external save set and checks exact
payload re-encoding.
