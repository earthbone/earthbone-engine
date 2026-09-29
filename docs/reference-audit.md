# Reference audit and implementation map

Reviewed 2026-09-19 against engine `75cfb55`, local `engine-refs` and
[tes4skyrim at 2977735](https://github.com/bryantmh/tes4skyrim/tree/29777351936fb5253f6cb2d1b14d11a71dd17aaf).
This is a targeted source audit and an inventory of 60 local directories,
not a line-by-line review of every reference. Reference paths below are
relative to the engine's parent directory. This document supersedes broad
"no source exists" claims in the older sibling research notes for terrain
LOD and TRI morph data.

## Contracts corrected in this change

| Contract and exact source | Implementation and regression | Remaining boundary |
|---|---|---|
| `skse64/scripts/vanilla/Utility.psc` and `ck-wiki/Utility_Script.wikitext` distinguish wait seconds, menu seconds and game hours | `papyrus/src/vm.rs`, `vm/state.rs`, `vm/hash.rs`, `game/src/lib.rs`. Tests `utility_wait_domains_survive_save_and_timescale_changes`, `version_one_clock_loads_with_legacy_defaults`, `game_tick_supplies_timescale_and_menu_pause_to_waits` | Menu UI must call `tick_menu_mode`. Version 1 saves cannot recover which wait function produced a legacy sleep. |
| CK `Quest_Script.wikitext`, `Alias_Script.wikitext`, `ReferenceAlias_Script.wikitext`, and the named `Clear_-_ReferenceAlias.wikitext` contract cover the Quest and reference-alias registry bindings | `NativeRegistry::method_bindings` and `global_bindings` expose sorted runtime binding inventories. `game/tests/scr_68_native_contracts.rs` compares each Quest, Alias and ReferenceAlias method and Quest global with the CK class declarations. It also verifies optional-only `ReferenceAlias.Clear` behavior. `TES5Edit/Core/wbDefinitionsTES5.pas` and `uesp-modfileformat/QUST.wikitext` establish QUST alias Optional as FNAM bit 1. | The corpus lacks most linked individual pages for these methods, so those comparisons use the class-page descriptions. Quest `IsStarting` and `IsStopping` are quiet false stubs despite CK describing transition state. SCR-43 owns those fixes. |
| `ck-wiki/DispelSpell_-_Actor.wikitext` and `DispelAllSpells_-_Actor.wikitext` require active-effect removal | `game/src/natives.rs` no longer destroys known or equipped spells. Test `dispelling_never_removes_known_or_equipped_spells` replaces the incorrect test oracle. Both calls are counted as stubs. | Active-effect ownership, expiry, removal and protected categories remain unimplemented. Cast recording is not magic execution. |
| SPID `SPID/src/LookupConfigs.h::ChanceComponentParser` accepts trailing `!`. `Defs.h::Chance` preserves deterministic mode. `LookupFilters.cpp` seeds per actor. | `moddata/src/spid.rs` preserves the percent and marker, rejects malformed values. `game/src/moddata.rs` rolls ordinary chance per matching base and reports unsupported deterministic fractional rules. Parser and runtime regressions cover `0!`, `50!`, `100!`. | Exact SPID per-actor lifecycle and seed, and KID's separate seed in `src/Data/BaseFilters.cpp`, remain work. |
| Shipped `skse64/scripts/vanilla/Quest.psc::GetStage` calls `GetCurrentStageID`. CK `Quest_Script.wikitext` describes the highest stage. | Both native getters return `highest`. Test `quest_getters_agree_after_lower_stage_and_reset` sets 20 then 10 and resets. | Wider quest lifecycle semantics still need original-runtime comparisons. |
| `godot-docs/classes/class_physicsserver3d.rst::body_add_shape` accepts a shape transform | `godot-host/src/godot_physics.rs` preserves native boxes, spheres, capsules and convex hulls, with dimensions in metres. Tests check translated spheres, both capsule endpoint orders and compound box corners. | The Bannered Mare Jolt contact smoke test passes, as recorded in `physics.md`. No performance improvement is claimed. Shell radius and unsupported bhk shapes remain listed in `physics.md`. |
| `godotwind/src/core/nif/nif_material_info.gd::get_cache_key` includes alpha and culling | `mesh::MaterialKey` and `godot_layer::MaterialMap` separate material state from path-keyed uploads. `shared_textures_keep_distinct_alpha_and_culling_in_both_orders` covers all supported states. | Glow, environment and other shader inputs remain unwired. This is not a new shading model. |

No new runtime dependency or upstream source file was imported. Core code is
GPL-3.0-or-later. No derived code belongs in the MIT shim. tes4skyrim's own
MIT license does not cover every bundled external component. PyNifly shares
HavokLib ancestry, so it is not an independent codec oracle. HKX2's missing
license must be resolved before code reuse. Do not execute bundled tools
merely because the repository includes them.

## Sources to use for current gaps and future systems

| System and current entry point | Existing sources to use | What tes4skyrim or its references add | Boundary |
|---|---|---|---|
| Papyrus VM and native semantics, `papyrus/src/vm.rs`, `game/src/natives.rs` | Shipped vanilla and modified PSC in `skse64/scripts`, CK function pages, skymp compatibility tests, Caprica and Champollion | Game Bridge for original-runtime measurements, russo-2025 compiler for another PEX implementation | A signature or native-call count does not prove semantics. Different compilers can share bugs or intentionally differ. |
| Dialogue and scenes, `game/src/natives.rs:2784`, quest/VMAD views | xEdit DIAL/INFO/DLBR/SCEN layouts, CommonLib runtime types, CK native contracts | `docs/reference/dialogue_engine_contracts.md`, generated subtype tables, dialogue conversion and emulator tools | Preserve original record semantics. Do not adopt converter policies that reassign quest ownership. Reproduce disassembly claims on an identified runtime. |
| PACK and CTDA, `game/src/package.rs`, `ai.rs` | xEdit `wbConditionFunctions` and parameter types, PACK wiki, PO3 condition parser, CommonLib procedure types | Package contracts, actor wiring, alias and ForceGreet examples | Our alias RunOn field matches xEdit's Parameter #3. That candidate was checked and is not a finding. Conversion heuristics are not runtime rules. |
| Combat, actor values, magic, inventory | skymp `GetBaseActorValues.cpp`, `EvaluateTemplate.h`, `LeveledListUtils.cpp`, `ActiveMagicEffectsMap.cpp`, `formulas/TES5DamageFormula.cpp` and corresponding unit tests. xEdit/UESP RACE, MGEF, SPEL, PERK, COBJ | Creature attack graph/event wiring and magic-effect conversion examples | skymp is a partial implementation with explicit TODOs and multiplayer policies. Its leveled-list code has limitations. Use as a comparison, not a drop-in specification. |
| Havok interpreter, `hkx/src/behavior.rs`, `interp.rs` | serde-hkx expression and modifier classes, activeragdoll/CommonLib structures, OAR clip handling, Nemesis and Pandora outputs | Smaller creature graphs, PyNifly codec and writer, HKX2 serializer, live readback | `transition_matches` currently accepts conditions/intervals without evaluating them, and selection is first match. Layout references do not establish transition priority or blend semantics. |
| HKX round trips and fixtures, `hkx/src/writer.rs` | serde-hkx, HavokLib, Nemesis cached graphs | PyNifly alignment corrections, HKX2 round trips | PyNifly's codec shares HavokLib ancestry with ours. Equal file size is not proof of equivalent graphs. Compare fields, links, events, and sampled poses. |
| NIF controllers, particles, skinning and LOD blocks | nifxml, nifly `Animation.cpp`, `Particles.cpp`, `Skin.cpp`, OpenMW NIF/NIFOSG controller and particle code | NIF sequence, particle, flipbook and furniture conversion paths | Current unknown/raw blocks preserve bytes but do not render behavior. Morrowind runtime rules are not necessarily Skyrim rules. |
| Materials, terrain, water and rendering | godotwind material keys and static renderer, Community Shaders `package/Shaders/Lighting.hlsl`, Terrain3D, Godot docs, OpenMW/vsgopenmw terrain paging | Terrain tile generation, texture composition and LOD settings | Community Shaders includes added features, not only vanilla shading. Keep Godot as host. The current StandardMaterial path only wires diffuse, normal, alpha and culling. |
| Runtime LOD and optional authoring | nifxml segmented/multibound blocks, nifly, OpenMW/vsgopenmw paging | `asset_convert/lod/terrain_lod.py`, `terrain_nif.py`, texture generation | Terrain source exists. Object generation still invokes LODGenx64. Reading a NIF prefix does not implement segment visibility, bounds or streaming policy. |
| Face animation, appearance and visemes | Existing `audio::viseme`, RACE PHTN/PHWT mapping, nifxml, TiltedEvolution face/tint integration | `asset_convert/character/facegen_tri.py`, PyNifly TRI tooling, EGT conversion | TRI deltas and morph names provide a concrete route to rendering existing visemes. TiltedEvolution calls the original engine for important operations. EGM/EGT conversion is not proof of a complete Skyrim FaceGen runtime. |
| Navmesh movement, `navmesh/src/graph.rs` | Recast's Detour `findStraightPath`, DetourCrowd path corridor, xEdit NAVM/NAVI definitions | Door/link validation cases and difficult generated topology | We already use Recast generation but still walk edge midpoints. Port/adapt established funnel behavior instead of inventing it. Keep native NAVM data and link semantics. |
| ESS restoration, `save/src/papyrus.rs` | FallrimTools `ActiveScript`, `StackFrame`, `ScriptInstance`, `ArrayInfo`, `Variable`, `SuspendedStack`, UESP save pages | No stronger replacement found | Current parsing ends early and preserves the tail. Deeper restoration is unfinished, not blocked on undocumented formats. Retain the user's saves outside the repository. |
| Mod-data execution, `moddata`, `game/src/moddata.rs` | SPID/KID/BOS own parsers and lifecycle code, OAR `Parsing.cpp`, `ActiveClip.cpp`, dargh | Converted assets can broaden a future compatibility corpus | OAR counting is not OAR runtime support. Missing filters, user overrides, exclusive groups and distribution lifecycles remain separate features. |
| Native UI, MCM and extension APIs | SkyUI `SKI_ConfigBase.psc`/`SKI_ConfigManager.psc`, SKSE modified scripts, JContainers `api_3`, PO3 functions | Dialogue menu and service-trigger examples | Keep the chosen native UI. Read interface contracts without assuming permission to copy SkyUI. Native JContainers compatibility also needs ownership and save semantics. |
| ESM, archives, profile resolution and editor | Mutagen, xEdit, esplugin, bsa, libloot, LOOT docs, Wrye Bash patchers, Spriggit, CKPE | Additional converted plugin and asset cases | Sorting and patch construction belong to tools. Runtime must respect the user's resolved order and winning records. Existing readers are already sourced. |
| DDS and audio containers | Existing bcdec/image_dds comparisons, Wabbajack, fuzmanager, Skyrim-NX-Toolkit, FaceFXWrapper framing, CKPE LipGen | Audio conversion and voice-file routing examples | No new native LIP curve decoder was established. Keep the existing audio-derived viseme route and Godot audio output. |

## New-reference shortlist

| Candidate | Priority and intended use |
|---|---|
| [tes4skyrim](https://github.com/bryantmh/tes4skyrim) | Add as a pinned research reference first. Dialogue contracts, live bridge, small behavior graphs, terrain generation and TRI codec are the highest-value paths. Pin the reviewed revision above when adding it to `engine-refs`. |
| [PyNifly](https://github.com/BadDogSkyrim/PyNifly) | Add for TRI and animation read/write comparisons. Compare upstream with tes4skyrim's marked local fixes. Shared HavokLib ancestry limits independence. |
| [HKX2-Enhanced-Library](https://github.com/Monitor221hz/HKX2-Enhanced-Library) | Research candidate for serializer and class coverage. Resolve the missing license before planning code reuse. Its upstream README does not promise the XML-to-HKX path used by tes4skyrim. |
| [russo-2025/papyrus-compiler](https://github.com/russo-2025/papyrus-compiler) | Secondary compiler/PEX comparison. Keep Caprica as the already-selected implementation reference. Document intentional compiler differences. |
| [hkxpack](https://github.com/Dexesttp/hkxpack) | Lower priority format research. Its stated Havok 2014 target differs from Skyrim's 2010 packfiles. |
| [Open Papyrus documentation](https://open-papyrus.github.io/docs/Papyrus_Language_Reference/index.html) | Additional language-reference candidate discovered through the compiler's references. Not independently audited here. |
| [Ormin ScriptConverter](https://github.com/Ormin/skyblivion-ScriptConverter) | Defer unless earlier-game script conversion enters scope. It does not answer the current Skyrim runtime gaps. |

## Follow-up work with acceptance checks

These are scoped future systems, not features claimed complete by this fix.
They supplement the substrate plan in `ENGINE-PLAN.md`.

| Item | Read first | First deliverable and acceptance check |
|---|---|---|
| `magic-effects` | CK Dispel pages, xEdit SPEL/SPIT and EFID/EFIT layouts, skymp `ActiveMagicEffectsMap` as a partial comparison | Store active effects by resolved source spell, effect, caster and target. Wire creation, expiry, save/load and removal. A cast then dispel removes effects while `HasSpell` and equipment stay unchanged. DispelAll preserves abilities, diseases, worn/constant enchantments and addictions. |
| `dialogue-contracts` | tes4skyrim `docs/reference/dialogue_engine_contracts.md`, `tools/dialog/dialog_emulator.py`, xEdit DIAL/INFO/DLBR/SCEN and CK semantics | Typed winning-record views and eligibility evaluation. Fixture with DATA then SNAM verifies parse-order subtype handling. Inactive owning QNAM quest suppresses its topic. INFO conditions, priority, responses and fragments run through the existing VM. Conversion-specific rewiring is not a runtime rule. |
| `gameplay-records` | xEdit COBJ, SPEL, MGEF, WEAP, GMST, race/NPC records and skymp stat formulas with their TODOs | Split combat, magic, crafting and progression into separate plan items. Start with race plus NPC actor-value composition, template chains and COBJ requirements. Each test compares winning overrides, missing forms and documented vanilla outcomes. Never cache a second authoritative form database. |
| `behavior-semantics` | serde-hkx `hkbExpressionCondition` payloads, generator/modifier classes, tes4skyrim small creature graphs | Preserve condition payloads, evaluate transition conditions/intervals and priority before expanding modifiers. Test false conditions, competing priorities, cycles and sampled poses. Equal output file sizes do not establish graph equivalence. |
| `face-morphs` | tes4skyrim `asset_convert/character/facegen_tri.py`, PyNifly TRI, nifxml, current audio visemes and RACE PHTN/PHWT | Apply authored TRI deltas by morph name and validate mesh indices. Bind existing visemes. Keep head construction, tint composition and complete FaceGen semantics separate. |
| `lod-runtime` | tes4skyrim `asset_convert/lod/terrain_lod.py` and `terrain_nif.py`, nifxml segmented blocks, OpenMW paging | Read and stream BTR/BTO with measured visibility and residency. Optional terrain authoring compares generated fields and seams. Do not claim an open replacement for the LODGen object-generation step. |
| `distributor-lifecycle` | SPID LookupConfigs/LookupFilters, KID BaseFilters, BOS Manager/SwapData, OAR Parsing/ActiveClip | Per-actor SPID events, exact deterministic seeds and filtered candidate selection. Compare same-base distinct actors and save/load across distribution events. Report unsupported rules. Add OAR evaluation separately from its current tree count. |
| `save-restore` | FallrimTools `ActiveScript`, `StackFrame`, `Variable`, `SuspendedStack`, UESP save pages | Decode preserved Papyrus tails and restore owned objects and latent state. Use the user's gated external saves, never copy them into fixtures. |

### Original-runtime probes

Use tes4skyrim's Game Bridge as a protocol reference, not a portable runtime
dependency. It requires Skyrim/SKSE and Windows named pipes. Begin with a
small explicit command set for quest stages/reset, wait start/finish,
active effects and animation input/output events. Keep transport outside the
engine. Capture request IDs, runtime and SKSE versions, plugin load order,
resolved form identities, timescale, pause state, elapsed clocks and results.
Store authored scripts and sanitized observations, not game assets or saves.

For each probe, run the same authored sequence through `soe-game` and compare
structured results. Establish tolerances for scheduler timing before running,
with repeated observations and both menu states. Treat a disagreement as an
open contract until the relevant source or a repeatable probe resolves it.
The current Rust tests establish engine regressions, not measured parity with
Skyrim. No bridge was executed during this change.

### Compatibility corpus

Keep authored edge cases in normal tests and real mod outputs behind an
explicit local data path. Every fixture records source tool, revision,
license, generation command and the contract under test. Prioritize paired
SPID ordinary/deterministic rules, KID multiple targets, OAR overrides,
Nemesis/Pandora transition graphs, TRI deltas and terrain seams. Compare
resolved fields and sampled behavior, not only whether parsing succeeds.
A source with shared ancestry is a cross-check, not an independent oracle.

## Validation of the reference fixes

The full `tools/lint.sh` gate passes with 835 reported tests and no ignored
tests. Gated tests return early when their environment variables are absent,
so this count is not a claim of 835 real-data cases. Every gate tool ran:
workspace formatting, all three clippy passes, comment-lint, prose-lint,
Taplo, markdownlint, fuzz formatting and workspace tests. Taplo and
markdownlint were installed under `/tmp` for this run. Existing Markdown
formatting errors were corrected without changing their recorded results.
Comment-lint used a task-local writable cache with the installed model.

Additional release checks with the installed Skyrim data: both
`soe-godot-host --test real_collision` tests passed. The
`soe-game --test real_files` test loaded all 14,026 scripts with zero failures,
started 423 quests and dispatched 1,388 native calls with zero unresolved
calls. Godot 4.7.2 with Jolt passed `godot/physics_test.gd`, settling the probe
at y 8.295 after 87 frames, with 281 bodies and 285 shapes. See
[physics](physics.md) for the triangle count and measurement boundaries.
The first parallel real-collision run received SIGTERM after compilation.
A serial rerun completed both tests. The Flatpak editor import loaded the
extension but aborted on shutdown. The subsequent game-mode test exited 0.

No original-Skyrim bridge probe, visual material comparison, active-effect
lifecycle test or full distributor lifecycle comparison was run. The safe
stub and unsupported-rule behavior above are limitations, not completed
magic or distributor implementations. No changes were made to the MIT shim,
the user's game files, save corpus or other active worktrees.

## Inventory disposition

All 60 local directories are accounted for below. Entries describe the intended use after this audit, not equal-depth source review. Detailed source comparisons and executable checks are identified above.

| Local directory | Disposition |
|---|---|
| `activeragdoll` | Existing Havok structure source. Ragdoll integration reference. |
| `BaseObjectSwapper` | Existing source used. Extend property language and reference-load behavior. |
| `bsa` | Additional archive implementation for differential tests. Existing parser is sourced from LMO. |
| `Caprica` | Existing source used. Primary planned PSC compiler reference. |
| `Champollion` | Existing source used. PEX decoding and differential comparison. |
| `ck-wiki` | Underused semantic source. 402 pages, including the contracts behind the corrections above. |
| `CommonLibSSE` | Existing source used. Runtime layouts and interfaces, not full algorithms. |
| `CommonLibSSE-NG` | Existing source used. Runtime types and shim interface comparison. |
| `CommonLibSSE-NG-alandtse` | Existing source used. Selected shim header surface. |
| `Creation-Kit-Platform-Extended` | Existing source used. Editor behavior and LipGen integration. |
| `dargh` | Existing legacy animation-format source. Compare runtime condition behavior. |
| `esplugin` | Additional Rust parser and plugin-validation comparison. |
| `FaceFXWrapper` | Existing source used. LIP framing only, not a proven curve decoder. |
| `FallrimTools` | Existing source used. Deeper save restoration remains available to port. |
| `fuzmanager` | Existing FUZ source. No native LIP curve decoder established. |
| `gdext` | Existing binding dependency/reference. Use exposed server APIs. |
| `godot-docs` | Underused API contract. Per-shape transforms replaced the assumed limitation. |
| `godotwind` | Partly used. Material keys informed the cache fix, renderer/controller code helps future work. |
| `HavokLib` | Existing source used. Spline codec and comparison ancestry. |
| `hkxcmd` | Existing editor/conversion reference. Its shipped SDK-linked binary is not an engine dependency. |
| `JContainers` | Underused future source. Papyrus collections, ownership, JSON and serialization API. |
| `JoltPhysics` | Backend reference. Use through Godot, not another linked physics engine. |
| `Keyword-Item-Distributor` | Existing source used. Correct deterministic seed and exclusive groups need follow-up. |
| `libloot` | Tool-side sorting and diagnostics. Do not reorder runtime winners implicitly. |
| `loot` | Profile/tool documentation and diagnostics. |
| `loot.github.io` | Supporting documentation for the loadout/tool layer. |
| `Mutagen` | Existing source used. Winning overrides, record models and tooling. |
| `nifly` | Existing skinning reference. Animation, particle and geometry implementations remain useful. |
| `nifxml` | Existing format basis. Add missing block payloads and consider selective generation. |
| `OpenAnimationReplacer` | Parser source used. Runtime conditions, clip replacement and user overrides unfinished. |
| `openmw` | Existing NIF, ESM and physics reference. Controller/particle code and engine patterns remain useful. |
| `OpenSkyrim` | Alternative architecture. Conversion to Luau is not a Papyrus compatibility authority. |
| `Pandora-Behaviour-Engine-Plus` | Future corpus and behavior patching reference. Not a Havok evaluator. |
| `PapyrusExtenderSSE` | Shim survey covers it. Native bodies and condition metadata are useful for game work. |
| `Project-New-Reign---Nemesis-Main` | Existing cached graph fixtures. Expand patched-output coverage, not runtime semantics. |
| `recastnavigation` | Generator reference used. Detour funnel/path-corridor code answers documented movement gap. |
| `saves` | User data. Existing gated corpus, not an implementation reference or redistributable fixture set. |
| `serde-hkx` | Existing layout source. Condition payloads and missing modifier classes remain unused. |
| `skse64` | Underused shipped PSC contracts. Direct evidence for quest aliases and wait clock domains. |
| `skymp` | VM source used. Actor stats, templates, effects, damage and leveled-list code are underused. |
| `Skyrim-Behavior-Editor-` | Existing editor reference. Graph editing and expression representation. |
| `skyrim-community-shaders` | Underused material semantics and rendering reference. Distinguish enhancements from vanilla. |
| `skyrim-hkx-fixtures` | Existing game-file corpus. Add semantic checks beyond successful loading. |
| `Skyrim-NX-Toolkit` | Existing audio source. Container and alternate codec routing. |
| `skyui` | Existing UI plan reference. MCM script API is useful, no blanket source-copy permission established. |
| `Spell-Perk-Item-Distributor` | Source used incompletely. Confirmed chance-suffix bug and lifecycle work. |
| `Spriggit` | Existing editor reference. Record serialization and interchange. |
| `Terrain3D` | Existing host/editor reference. Terrain LOD and foliage design. |
| `TES5Edit` | Primary layout source. Also generate condition parameter metadata from its tables. |
| `TES5Edit.github.io` | Supporting documentation. Not a separate implementation oracle. |
| `TiltedEvolution` | Future integration reference. Face/tint and animation networking call Skyrim internals. |
| `TrueDirectionalMovement` | Already surveyed in shim docs. Movement/camera feature behavior, not patch-site portability. |
| `uesp-modfileformat` | Existing record specification, 130 pages. Pair layouts with runtime behavior evidence. |
| `uesp-savefile` | Existing save specification, 37 pages. Compare ambiguities with ReSaver. |
| `uesp-skyrim-mod` | Five-page overview corpus. Discovery material rather than detailed format authority. |
| `vsgopenmw` | Future paging/terrain comparison. Host replacement remains outside this audit recommendation. |
| `VulkanSceneGraph` | Already considered host alternative. No reason found to replace Godot. |
| `wabbajack` | Existing texture/archive reference. Reproducible mod corpus acquisition belongs in tooling. |
| `wrye-bash` | Future patcher/editor reference, especially leveled-list merge behavior. |
| `wrye-bash.wiki` | Supporting patcher/user documentation, not runtime merge policy. |
