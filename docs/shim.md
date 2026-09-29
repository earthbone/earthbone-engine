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
| References, actors, inventory and world singletons | 70 | 19 | 6 | 53 | 18 | 18 | Planned, round 2 `game` crate. The runtime object model Papyrus sees is the same one these types describe. |
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
traits, reference and actor handles, condition types that call a host-provided
runtime trait, the VM registry types with the collision rule, event sources
with continue-or-stop control, SKSE plugin interfaces as traits, NetImmerse
math types, and a `support` table that keeps the group counts aligned with this
document.

`src/headers.rs` generates the corresponding supported C++ surface under `include/RE` and
`include/SKSE`. `RE/Skyrim.h` and `SKSE/SKSE.h` are the umbrella includes. Regenerate the files
after changing the header mapping with `cargo run -p soe-commonlib-shim --bin generate-headers`.
The declarations are source-level headers only. They do not provide a C ABI or engine bindings.
Condition methods delegate to `ConditionRuntime`; the game implementation evaluates against the
current world and VM. Native condition copying reports `UnsupportedHook`.
The mapping follows the Rust API while adapting Rust names and types to C++, including
`NativeRegistry::register`, which is emitted as `register_native` because `register` is a C++
keyword.

## Untested

The SHM-01 test compiles the generated umbrella headers with a C++20 compiler and checks C++
signatures for the Rust methods they project. No sample plugin has been compiled against the
headers, and the declarations are not linked to engine behavior. The counts are token counts,
so a name mentioned only in a comment is counted and a type reached only through `auto` is not.
Four plugins are a sample, not the population: the map's 70 to 90 percent open-source estimate
is from a six-plugin sample and the shimmable share here is 3 of 4 by plugin, far less by hook
site because TDM dominates. The alandtse fork's 390 added headers were counted, not read.

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
