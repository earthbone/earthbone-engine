# CommonLib shim scope

Written 2026-09-06 by the round-2 shim fork. Section 7 of `../../research/skyrim-open-engine-map.md`
sets the frame: SKSE compatibility is source-level only. A plugin is recompiled against a shim
that presents the `RE::` and `SKSE::` surface of CommonLibSSE-NG over the new engine. Binary
plugins are out, since they call Skyrim's own function addresses through Address Library. This
document is the scope of that shim: which types the sample plugins actually touch, which of
them the engine has a concept for, and the counts that decide where the shim's effort goes.

## The two header sets

| Checkout | `include/RE` headers | `include/SKSE` headers | Last commit |
|---|---|---|---|
| CharmedBaryon/CommonLibSSE-NG (`engine-refs/CommonLibSSE-NG`) | 1,631 | 17 | 2024-09-03 |
| alandtse/CommonLibSSE-NG (`engine-refs/CommonLibSSE-NG-alandtse`) | 2,015 | 19 | 2026-09-03 |

The path diff is 394 entries: 390 headers exist only in the alandtse fork (113 under `RE/B`,
92 under `RE/C`, 52 under `RE/H`, mostly split-out classes and new reverse-engineered types)
and 4 exist only in CharmedBaryon (`RE/D/DBTraits.h`, `RE/H/SendHUDMessage.h`,
`RE/M/ModelProcessor.h`, `RE/Offsets.h`, the last one replaced by per-runtime ID tables in the
fork). Of the 349 names the plugins use, 250 resolve to a header of the same name in both sets,
2 only in the alandtse fork (`DragonCameraState`, `BSSimpleList`), and 97 are nested types,
enums, free functions or vtable ids that live inside another header. The shim targets the
alandtse set, as section 7 of the map decided.

## The plugins surveyed

Every `RE::Name` and `SKSE::Name` token in each plugin's `src` and `include` trees, counted
once per name. Hook sites are `write_call`, `write_branch` and `write_vfunc` uses, and
`RELOCATION_ID` or `REL::` references count address lookups.

| Plugin | Licence | RE names | SKSE names | Hook sites | Address lookups | Verdict |
|---|---|---|---|---|---|---|
| Spell Perk Item Distributor (SPID) | MIT | 64 | 18 | 1 | 48 | Shimmable. One hook on form loading becomes an engine event. |
| Base Object Swapper (BOS) | GPL-3.0 | 23 | 11 | 4 | 3 | Shimmable, and better as a native feature: its four vfunc hooks intercept reference loading, which the engine can expose as one event. |
| Papyrus Extender (PO3) | GPL-3.0 | 243 | 18 | 7 | 59 | Mostly shimmable. 55 of its 243 names are scene graph, Havok, UI or camera types with no target yet, so those natives return defaults until the runtime exists. |
| True Directional Movement (TDM) | GPL-3.0-or-later with modding exception | 129 | 24 | 65 | 184 | Not shimmable. It rewrites camera and movement through 19 vtables and 65 patch sites. Its features are engine features (see the design-system table) and its code is a specification, not a port. |

SKSE interfaces used: `MessagingInterface` (all four), `SerializationInterface` (SPID, PO3,
the cosave path), `TaskInterface` (SPID, PO3), `PapyrusInterface` (PO3, TDM),
`Trampoline` (SPID, TDM), `RegistrationSet` and `RegistrationMap` (PO3, event registration for
scripts), `CrosshairRefEvent` (TDM). The shim can provide all of them except the trampoline.

## The grouped surface

349 names, grouped by the engine concept they need. The last column is what the engine has for
the group today, so the shim's binding order follows it: forms and the VM first, references
and events next, everything below the line waits on a runtime that does not exist.

| Group | Types | SPID | BOS | PO3 | TDM | No header | Engine |
|---|---|---|---|---|---|---|---|
| Forms and record enums | 86 | 23 | 12 | 78 | 15 | 16 | Bound now. `soe-esm` carries every record as subrecords and six typed views, the rest of the typed views are per-record work. |
| References, actors, inventory and world singletons | 70 | 19 | 6 | 53 | 18 | 18 | Partial, round 2 `game` crate. SHM-12 binds inventory factory descriptors, inventory views and reference handle lookup. The rest remains planned. |
| Papyrus VM | 11 | 3 | 0 | 11 | 3 | 11 | Bound now. `soe-papyrus` has the native registry with the first-registered-wins rule, `StaticFunctionTag` and the stack and handle ids map onto its thread and object ids. |
| Conditions | 4 | 0 | 0 | 4 | 2 | 3 | Bound. `CONDITIONITEMOBJECT`, `CONDITION_ITEM_DATA`, `TESConditionItem` and `TESCondition` pass condition calls through the MIT `ConditionRuntime` trait. `soe-game` reads its live world and VM. Unsupported copy hooks return a typed error. |
| Event sources, sinks and story events | 26 | 7 | 0 | 23 | 6 | 4 | Planned as typed channels in the `game` crate. Each `TES*Event` becomes an enum variant, the sink trait keeps the continue or stop control. |
| Containers, strings, locks and memory | 23 | 7 | 1 | 19 | 6 | 9 | Shim-provided from Rust std. `BSFixedString` is an interned string, arrays are `Vec`, locks are `RwLock`. Nothing engine-side. |
| NetImmerse scene graph and materials | 35 | 1 | 3 | 30 | 8 | 7 | Different shape. The engine scene is a Godot tree, so `NiAVObject` and `NiNode` become opaque node handles with name lookup, the math types are shim-provided, shader material fields map to the `soe-nif` material struct where a field exists. |
| Havok animation and physics | 29 | 0 | 0 | 13 | 18 | 4 | Split. The `hkp*` raycast, shape and world queries map onto the round-2 `physics` crate over Jolt. `BSAnimationGraphManager`, `hkbClipGenerator` and graph variables need the behavior runtime, which does not exist. |
| Camera, input and player controls | 27 | 0 | 0 | 2 | 26 | 4 | Different shape and host-owned. Input events can be fed from Godot, camera states and the player-control handlers are engine internals a plugin cannot replace, see TDM below. |
| Scaleform UI and menus | 19 | 4 | 1 | 10 | 8 | 2 | Missing and replaced. No GFx runtime, so `GFxValue` and the menu classes have no target. `MenuOpenCloseEvent` can be emitted by the native UI layer. |
| Vtable hooks | 19 | 0 | 0 | 0 | 19 | 19 | Never. Vtable and call-site patching has no meaning against a Rust engine. Each hook site becomes a feature request for an engine event or setting. |

## What the shim exposes

`crates/commonlib-shim` (MIT, unlike the GPL core) compiles with no engine dependency. Its Rust
surface includes `FormId` with the master and light index rules, the `Form` and `DataHandler`
traits, reference and actor handles, the VM registry types with the collision rule, event
sources with continue-or-stop control, SKSE plugin interfaces as traits, NetImmerse math types,
world-state snapshots for worn inventory markers, game settings, placed hazards, actor combat
process state and resolved hit results, condition types that call a host-provided runtime trait,
and a `support` table that keeps the group counts aligned with this document. These snapshots are
source-level API types. They do not provide a C ABI or executable singleton access.

The event API includes typed payloads for `ShoutAttack`, `SkillIncrease`,
`SoulsTrapped`, `SpellsLearned` and `TESCellFullyLoadedEvent`. Their fields
carry resolved form ids, actor handles or actor values so a sink can resolve
the event against the same engine state used by the other shim interfaces.
The payload layouts follow alandtse CommonLibSSE-NG's
`include/RE/S/ShoutAttack.h`, `SkillIncrease.h`, `SoulsTrapped.h`,
`SpellsLearned.h` and `include/RE/T/TESCellFullyLoadedEvent.h`. The source-level
headers do not connect these channels to runtime producers. The game must
raise them when those mechanics are implemented.

`src/headers.rs` generates the corresponding supported C++ surface under `include/RE` and
`include/SKSE`. `RE/Skyrim.h` and `SKSE/SKSE.h` are the umbrella includes. The world view
binds settings, calendar hour, and active MGEF effects with the reanimation archetype to the
game runtime. Actor sit and sleep animation state is not represented, and returns an explicit
unsupported result. Binary address and vtable hooks also return an unsupported result. The
mapping follows the local CommonLibSSE-NG `ReanimateEffect.h`, `EffectArchetypes.h`,
`ActorState.h`, `Setting.h`, `Sky.h` and `TES.h` declarations. These declarations remain
source-level contracts without a C ABI. Regenerate the files
after changing the header mapping with `cargo run -p soe-commonlib-shim --bin generate-headers`.
The RE and SKSE interface declarations are source-level headers and do not provide engine
bindings. `SKSE/Plugin.h` supplies a separate C entry point for adapted shared libraries.
Condition methods delegate to `ConditionRuntime`. The game implementation evaluates against the
current world and VM. Native condition copying reports `UnsupportedHook`.
The mapping follows the Rust API while adapting Rust names and types to C++, including
`NativeRegistry::register`, which is emitted as `register_native` because `register` is a C++
keyword. `RE::TESQuestStartStopEvent` matches CommonLib's 8-byte form id, started flag, failed
flag and two-byte padding layout. The Rust shim event carries the resolved runtime form id and
the quest's failed state. `soe-game` emits it after successful start and stop transitions through
the MIT event sink API.

`SKSE/Plugin.h` declares a small native module entry point for source adaptations. A module
exports `SOEPlugin_Manifest` with an ABI version, name and the names of unsupported hooks, plus
`SOEPlugin_Load`. The C++ loader at `crates/commonlib-shim/tools/plugin-loader.cpp` opens a
shared library, validates those exports, prints every unsupported hook and invokes the load
entry point. It rejects an unnamed hook, a missing export, an invalid manifest and a failed
entry point. The loader does not supply engine interfaces. Its ABI is separate from SKSE's
Windows DLL ABI.

The first event batch adds typed `DisarmedEvent`, `DragonSoulsGained`,
`LevelIncrease` and `LocationDiscovery` payloads, with one source for each on
`ScriptEventSourceHolder`. Payload form identities are the load-order file
index and object id packed into a `u64`. The engine owns the holder and sends
events after the corresponding gameplay operation succeeds. Sink delivery
keeps registration order and stops after a sink returns `Stop`. A requested
binary call, branch or vtable hook is reported by the module manifest without
installing a hook. These generated declarations remain source-level only and
do not connect a separately compiled plugin to a running engine by themselves.

The payload fields follow the local alandtse CommonLib headers
`RE/D/DisarmedEvent.h`, `RE/D/DragonSoulsGained.h`, `RE/L/LevelIncrease.h`,
`RE/L/LocationDiscovery.h` and `RE/S/ScriptEventSourceHolder.h`. Only their
published field meaning and event source names informed this API. The Rust
surface uses engine independent form identities and owns its own event source
implementation.

The event batch declares `TESContainerChangedEvent`, `TESDeathEvent`,
`TESEnterBleedoutEvent`, `TESFormDeleteEvent` and `TESFurnitureEvent` payloads. `EventState`
defines queries for current inventory count, death, bleedout, deletion and furniture occupancy
for an engine binding to implement against live world state. Missing objects return
`std::nullopt`. Vtable and call-site patch requests return the `UNSUPPORTED_VTABLE_HOOK`
description because those addresses have no meaning in this runtime.

The event API includes payloads for `TESGrabReleaseEvent`,
`TESHarvestedEvent::ItemHarvested`, `TESHitEvent`, `TESObjectLoadedEvent` and
`TESQuestStageEvent`. References and actors use the shim's `ObjectRefHandle`
and `ActorHandle`, form identities use `FormId`, hit flags retain their
CommonLib bit values, and the executable quest callback pointer is omitted.
`EventSource` sends each typed payload to registered sinks in registration
order and stops when a sink returns `Stop`. `request_hook` returns an explicit
`Unsupported` error for call-site, branch and vtable hooks. The SHM-22 C++
fixture routes all five payloads into stateful receiver methods and checks the
three hook results. It exercises the source-level API contract and does not
link a plugin to a running game process.

The payload field layout was checked against the alandtse
`TESGrabReleaseEvent.h`, `ItemHarvested.h`, `TESHitEvent.h`,
`TESObjectLoadedEvent.h` and `TESQuestStageEvent.h` headers. The Rust handles
and omission of executable callback state follow this shim's source-level
boundary rather than copying CommonLib's pointer representation.
SHM-11 adds `RE/WorldTypes.h`. Its `WorldTypes` interface groups the five planned concepts behind
one caller-owned adapter, while `GameSettingCollection` exposes a read-only setting query.
Unsupported executable hooks return a named `UnsupportedHookInfo` with a reason. C++ fixtures execute
against a contract fixture, and game tests exercise the Rust adapter against `World` state. A C++
plugin cannot directly call the Rust adapter until a native bridge is added.
The public alandtse CommonLib headers read for this batch were
`include/RE/E/ExtraWorn.h`, `include/RE/G/GameSettingCollection.h`,
`include/RE/H/Hazard.h`, `include/RE/H/HighProcessData.h` and
`include/RE/H/HitData.h`. They supplied the type names and fields used to choose
the narrow mappings. Executable methods and runtime layouts were not copied.

### Live inventory and reference views (SHM-12)

`RE::IFormFactory` describes a loaded record signature and name. `Create` reports the
`FormCreation` unsupported hook because forms are supplied by the immutable load order.
`InventoryChanges` and `InventoryEntryData` project current reference inventory totals,
distinct stack quantities, extra-data kind ids and worn state from `World`. Inventory mutation
reports `InventoryMutation` so writes continue through validated game transactions. `ItemList`
is a menu-neutral copy of those rows. Selection and update report `ItemSelection` and
`ItemListUpdate` because the runtime does not provide Scaleform menu state.
`LookupReferenceByHandle` resolves a source-level handle represented by the reference's runtime
FormID and reports whether the reference is an actor.

The C++ fixture compiles and calls these declarations, including all unsupported results. The
Rust game fixture exercises the same views against loaded records and live `World` state. The
CommonLibSSE-NG `IFormFactory.h`, `InventoryChanges.h`, `InventoryEntryData.h`, `ItemList.h`,
`Misc.h` and `Misc.cpp` supplied the source-level names and signatures. The shim declarations
are original MIT code and do not copy upstream implementation code. These views are not binary
compatible with Skyrim's objects or handle representation.

The SHM-13 `RE/Magic.h` projects interfaces for favorite forms, cast rejection reasons, active
magic target queries and dispel, map-marker visibility and travel flags, and the supported subset
of equip parameters. Binary-address calls and vtable patches have explicit unsupported-hook
values. Equip options outside the supported plain single-item operation are reported through
`UnsupportedEquipOption`. The C++ fixture checks these declarations with in-process test doubles.
They do not call the Rust `World` or prove a live engine binding. `game` currently owns actor
spell hands, active effects, reference map flags and inventory equipment, but this source-level
shim has no bridge from C++ into that state.

## Live world views, SHM-16

`soe-game` implements the MIT data-handler and reference traits over its winning-form table
and instantiated reference and actor state. The form view carries resolved runtime FormIDs, record
signatures, editor IDs and associated keywords. `TESFile` views expose load-order names and
regular or light compile indices. `TESObjectREFRPtr` is a nullable runtime FormID handle.
`WEAPON_STATE` exposes the six CommonLib numeric values. Since the game currently stores only
whether an actor's weapon is drawn, live actor queries return `kSheathed` or `kDrawn`. The four
animation transition states are available in the contract but not produced by the runtime.

The runtime does not implement `TESObjectREFR::SetBaseObject` for an existing reference. Its
MIT trait returns an `Unsupported` value naming that operation, leaving the current base intact.
The C++ fixture compiles and calls the exported contracts, while game tests query the live
`World` adapter. These are source-level contracts, not a compiled SKSE plugin or binary ABI.
The declarations were checked against alandtse CommonLib's `TESDataHandler.h`, `TESFile.h`,
`TESObjectREFR.h` and `ActorState.h`.
The existing `SKSE::TRAMPOLINE` declaration continues to identify machine-code patching as
unsupported, and the C++ contract fixture checks that diagnostic.

## Untested

The SHM-01 test compiles the generated umbrella headers with a C++20 compiler and checks C++
signatures for the Rust methods they project. The SHM-02 test compiles and loads a small C++
module through the new entry point and checks its unsupported hook report. SHM-20 compiles the
five typed event payload contracts and checks sink dispatch against synthetic state fixtures.
SHM-22 checks the additional payload contracts, live-state receiver calls and unsupported hook
results in Rust and C++ fixtures.
SHM-23 compiles and runs the quest event payload and verifies the explicit trampoline rejection
marker. A game integration test checks that live quest start and failed-stop transitions reach
the MIT sink with current state. SHM-11 C++ fixtures check the world type contract, and game tests
exercise its Rust adapter against `World`. SHM-13 checks magic contracts with test doubles, not
live game state. SHM-14 exercises player and process-list reads against synthetic `World` state.
No surveyed plugin has been compiled against the headers or loaded, and the generated declarations
are not linked to engine behavior or a C++ to Rust ABI. The counts are token counts.

SHM-14 adds `RE/World.h` and the matching MIT Rust boundary in `world.rs`.
`PlayerCharacter` reads player identity, position and actor values from the
world provider. `ProcessLists` lists actor forms whose runtime actor state is
built. The game crate implements that provider against its `World` form table.
`ObjectRefHandle` and `ProjectileHandle` retain their 32-bit CommonLib-shaped
representation. The current interface addresses live references by `FormKey`
so file and object identity remain distinct. Projectile runtime access returns
the typed `ProjectileRuntime` unsupported-hook result. Binary patch hooks are
also identified as unsupported. These declarations remain a source-level API,
with no C++ to Rust ABI or binary hook mechanism.

The contracts follow the local alandtse CommonLibSSE-NG declarations in
`include/RE/B/BSPointerHandle.h`, `include/RE/P/PlayerCharacter.h`,
`include/RE/P/ProcessLists.h` and `include/RE/P/Projectile.h`. They preserve
the handle width, player read shape, process actor collection and projectile
type while adapting reads to engine-owned state.

## Untested SHM-14 contracts and survey limits

The SHM-01 and SHM-14 tests compile generated umbrella headers with a C++20 compiler and check
C++ signatures for the Rust methods they project. SHM-14 also exercises player and process-list
reads against a synthetic live `World`. No sample plugin has been compiled against the headers,
and the declarations have no C++ to Rust ABI. The counts are token counts,
so a name mentioned only in a comment is counted and a type reached only through `auto` is not.
Four plugins are a sample, not the population: the map's 70 to 90 percent open-source estimate
is from a six-plugin sample and the shimmable share here is 3 of 4 by plugin, far less by hook
site because TDM dominates. The alandtse fork's 390 added headers were counted, not read.

## Event binding batch SHM-18

`RE::BSEventNotifyControl`, `BSTEventSink` and `BSTEventSource` project to
`NotifyControl`, `EventSink` and `EventSource`. A source delivers sinks in
registration order and stops after the first `Stop`. `ActorKill::Event`
contains killer and victim runtime identities. `CriticalHit::Event` contains
aggressor and weapon runtime identities plus the sneak-hit flag. The runtime
identities are 64-bit keys composed from the engine's load-order file index
and object id. These source-level payloads do not use Skyrim pointer layout.

`World::actor_kill_events` reports a non-silent death with a valid actor
killer after the victim enters dead state. Silent kills and deaths without a
valid actor killer do not raise this channel. `World::report_critical_hit`
validates the aggressor and winning WEAP form before reporting the caller's
confirmed outcome. It does not calculate critical chance or damage. Binary
call, branch and vtable hooks remain unsupported through
`SKSE::TRAMPOLINE`, which exposes the reason as data.

The event type declarations follow CommonLibSSE-NG-alandtse
`include/RE/A/ActorKill.h`, `include/RE/C/CriticalHit.h` and
`include/RE/B/BSTEvent.h`. The C++ contract fixture checks payload fields,
sizes, notify-control values, dispatch and the unsupported hook result.

## Appendix: the 349 names by group

**Forms and record enums** (86): `AlchemyItem`, `BGSArtObject`, `BGSAssociationType`, `BGSBaseAlias`, `BGSBipedObjectForm`, `BGSBodyPart`, `BGSBodyPartData`, `BGSBodyPartDefs`, `BGSColorForm`, `BGSConstructibleObject`, `BGSDebris`, `BGSFootstepSet`, `BGSHazard`, `BGSHazardData`, `BGSHeadPart`, `BGSIdleCollection`, `BGSImpactDataSet`, `BGSKeyword`, `BGSKeywordForm`, `BGSLightingTemplate`, `BGSListForm`, `BGSLocation`, `BGSMaterialObject`, `BGSMessage`, `BGSMovementType`, `BGSOutfit`, `BGSPerk`, `BGSProjectile`, `BGSRefAlias`, `BGSReferenceEffect`, `BGSRelationship`, `BGSScene`, `BGSSoundDescriptorForm`, `BGSTextureSet`, `BIPED_MODEL`, `BIPED_OBJECT`, `COL_LAYER`, `Effect`, `EffectSetting`, `EnchantmentItem`, `FACTION_RANK`, `FormID`, `FormType`, `MATERIAL_ID`, `MagicItem`, `OBJECT_TYPE`, `OBJ_BOOK`, `PackageNS`, `QUEST_DATA`, `RACE_SIZE`, `SEX`, `ScrollItem`, `SpellItem`, `TESAmmo`, `TESBoundObject`, `TESDescription`, `TESEffectShader`, `TESFaction`, `TESForm`, `TESFurniture`, `TESGlobal`, `TESIdleForm`, `TESImageSpaceModifier`, `TESKey`, `TESLevCharacter`, `TESLevItem`, `TESLevSpell`, `TESLeveledList`, `TESNPC`, `TESObjectARMA`, `TESObjectARMO`, `TESObjectBOOK`, `TESObjectCELL`, `TESObjectLIGH`, `TESObjectWEAP`, `TESPackage`, `TESQuest`, `TESRace`, `TESRegion`, `TESRegionList`, `TESShout`, `TESSound`, `TESTopicInfo`, `TESWeather`, `TESWorldSpace`, `TES_LIGHT_FLAGS`

**References, actors, inventory and world singletons** (70): `ACTOR_CRITICAL_STAGE`, `ACTOR_VALUE_MODIFIER`, `AIProcess`, `ActivateParentID`, `ActiveEffect`, `Actor`, `ActorEquipManager`, `ActorHandle`, `ActorPtr`, `ActorState`, `ActorValue`, `ActorValueList`, `ArrowProjectile`, `BGSDefaultObjectManager`, `BGSLoadFormBuffer`, `BGSSaveLoadGame`, `BGSSaveLoadManager`, `BeamProjectile`, `CALCED_OBJECT`, `CHARACTER_FLAGS`, `Calendar`, `Character`, `DEFAULT_OBJECTS`, `ExtraActivateRefChildren`, `ExtraAliasInstanceArray`, `ExtraDataList`, `ExtraDataType`, `ExtraEnchantment`, `ExtraHotkey`, `ExtraLeveledCreature`, `ExtraLeveledItemBase`, `ExtraLightData`, `ExtraLinkedRefChildren`, `ExtraMapMarker`, `ExtraMissingLinkedRefIDs`, `ExtraMissingRefIDs`, `ExtraNorthRotation`, `ExtraOutfitItem`, `ExtraPoison`, `ExtraTeleport`, `ExtraWorn`, `GameSettingCollection`, `Hazard`, `HighProcessData`, `HitData`, `IFormFactory`, `InventoryChanges`, `InventoryEntryData`, `ItemList`, `LookupReferenceByHandle`, `MagicFavorites`, `MagicSystem`, `MagicTarget`, `MapMarkerData`, `ObjectEquipParams`, `ObjectRefHandle`, `PlayerCharacter`, `ProcessLists`, `Projectile`, `ProjectileHandle`, `ReanimateEffect`, `SIT_SLEEP_STATE`, `Setting`, `Sky`, `TES`, `TESDataHandler`, `TESFile`, `TESObjectREFR`, `TESObjectREFRPtr`, `WEAPON_STATE`

**Papyrus VM** (11): `BSScript`, `EffectArchetypeToString`, `FUNCTION_DATA`, `FormTypeToString`, `MaterialIDToString`, `SCRIPT_PARAM_TYPE`, `StaticFunctionTag`, `StringToFormType`, `VMHandle`, `VMStackID`, `VMTypeID`

**Conditions** (4): `CONDITIONITEMOBJECT`, `CONDITION_ITEM_DATA`, `TESCondition`, `TESConditionItem`

**Event sources, sinks and story events** (26): `ActorKill`, `BSEventNotifyControl`, `BSTEventSink`, `BSTEventSource`, `CriticalHit`, `DisarmedEvent`, `DragonSoulsGained`, `LevelIncrease`, `LocationDiscovery`, `ScriptEventSourceHolder`, `ShoutAttack`, `SkillIncrease`, `SoulsTrapped`, `SpellsLearned`, `TESCellFullyLoadedEvent`, `TESContainerChangedEvent`, `TESDeathEvent`, `TESEnterBleedoutEvent`, `TESFormDeleteEvent`, `TESFurnitureEvent`, `TESGrabReleaseEvent`, `TESHarvestedEvent`, `TESHitEvent`, `TESObjectLoadedEvent`, `TESQuestStageEvent`, `TESQuestStartStopEvent`

**Containers, strings, locks and memory** (23): `BSAudioManager`, `BSContainer`, `BSFixedString`, `BSReadLockGuard`, `BSReadWriteLock`, `BSScrapArray`, `BSSimpleList`, `BSSoundHandle`, `BSSpinLockGuard`, `BSString`, `BSTArray`, `BSTSmartPointer`, `BSTimer`, `BSWriteLockGuard`, `CFilter`, `FixedStrings`, `GPtr`, `MemoryManager`, `NiPointer`, `TaskQueueInterface`, `free`, `literals`, `reference_array`

**NetImmerse scene graph and materials** (35): `BSFaceGenManager`, `BSGeometry`, `BSGraphics`, `BSLightingShaderMaterialBase`, `BSLightingShaderMaterialFacegenTint`, `BSLightingShaderProperty`, `BSMaterialObject`, `BSShaderMaterial`, `BSShaderProperty`, `BSShaderTextureSet`, `BSTextureSet`, `BSVisit`, `Color`, `ColorUtil`, `EffectShaderData`, `ModelReferenceEffect`, `NI_INFINITY`, `NI_TWO_PI`, `NiAVObject`, `NiBooleanExtraData`, `NiColor`, `NiColorA`, `NiExtraData`, `NiIntegerExtraData`, `NiMatrix3`, `NiNode`, `NiPoint2`, `NiPoint3`, `NiQuaternion`, `NiRect`, `NiStringsExtraData`, `NiTransform`, `NiUpdateData`, `ShaderReferenceEffect`, `deg_to_rad`

**Havok animation and physics** (29): `ATTACK_STATE_ENUM`, `AnimVariableCacheInfo`, `BSAnimationGraphEvent`, `BSAnimationGraphManager`, `BSAnimationGraphManagerPtr`, `BShkbAnimationGraph`, `IAnimationGraphManagerHolder`, `bhkCompressedMeshShape`, `bhkNiCollisionObject`, `bhkWorldObject`, `hkVector4`, `hkbClipGenerator`, `hkpBoxShape`, `hkpCapsuleShape`, `hkpCdBody`, `hkpCdPointCollector`, `hkpCharacterStateType`, `hkpCollidable`, `hkpEntity`, `hkpLinearCastInput`, `hkpMoppBvTreeShape`, `hkpRayHitCollector`, `hkpRigidBody`, `hkpShapeRayCastCollectorOutput`, `hkpShapeType`, `hkpSphereShape`, `hkpWorld`, `hkpWorldObject`, `hkpWorldRayCastInput`

**Camera, input and player controls** (27): `BGSCameraPath`, `BSInputDeviceManager`, `BSWin32GamepadDevice`, `ButtonEvent`, `CameraState`, `CameraStates`, `ControlMap`, `DragonCameraState`, `FirstPersonState`, `HorseCameraState`, `INPUT_DEVICE`, `INPUT_EVENT_TYPE`, `InputEvent`, `LookHandler`, `MouseMoveEvent`, `MovementHandler`, `NiCamera`, `PlayerCamera`, `PlayerCameraTransitionState`, `PlayerControls`, `PlayerControlsData`, `TESCamera`, `TESCameraState`, `ThirdPersonState`, `ThumbstickEvent`, `TogglePOVHandler`, `UserEvents`

**Scaleform UI and menus** (19): `BookMenu`, `ConsoleLog`, `ContainerMenu`, `FastTravelConfirmCallback`, `GFxMovieView`, `GFxValue`, `GRectF`, `HUDData`, `HUDMenu`, `MapMenu`, `MenuOpenCloseEvent`, `MenuTopicManager`, `RaceSexMenu`, `SleepWaitMenu`, `TESLoadScreen`, `TutorialMenu`, `UI`, `UIMessageQueue`, `UI_MESSAGE_TYPE`

**Vtable hooks** (19): `VTABLE_ArrowProjectile`, `VTABLE_BeamProjectile`, `VTABLE_Character`, `VTABLE_DragonCameraState`, `VTABLE_EnemyHealth`, `VTABLE_FirstPersonState`, `VTABLE_HorseCameraState`, `VTABLE_LookHandler`, `VTABLE_MissileProjectile`, `VTABLE_MovementHandler`, `VTABLE_MovementHandlerAgentPlayerControls`, `VTABLE_PlayerCameraTransitionState`, `VTABLE_PlayerCharacter`, `VTABLE_PlayerControls`, `VTABLE_Projectile`, `VTABLE_ThirdPersonState`, `VTABLE_TogglePOVHandler`, `VTABLE_TweenMenuCameraState`, `VTABLE_VATSCameraState`
