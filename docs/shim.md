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
| References, actors, inventory and world singletons | 70 | 19 | 6 | 53 | 18 | 18 | Bound through the `game` crate's shared world. The runtime object model Papyrus sees is the same one these types describe. |
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
`SpellsLearned.h` and `include/RE/T/TESCellFullyLoadedEvent.h`. The MIT event
holder exposes one source per payload. The game emits shout
attacks after successful casts, player skill increases after a skill rank is
gained, soul traps after a soul gem is filled, learned player spells when
newly added, and fully loaded cells when streamed into the world. Binary hooks
remain unsupported. The SHM-20 game integration test streams all five live
payloads to the generated C++ event sources, whose sinks check the fields and
the unsupported hook result. This test transport does not add a C++ to Rust
ABI.

The UI-21 research in [ui-frameworks.md](ui-frameworks.md) scopes a possible
MIT menu API for source-adapted plugins. It identifies registration, callback
handles, menu state, input and pause requests as declarations the shim could
own. No ImGui renderer or SKSE Menu Framework implementation is part of the
shim today.

SHM-04 adds `ActorPtr` as an optional actor handle and `ActorState` as a
read-only snapshot of the states the runtime currently tracks. The
`ActorEquipManager` surface equips and unequips through the live world. The
`game` crate implements these traits over its shared `World`, including actor
values, race, death state, inventory and equipment. Handles contain runtime
FormIDs and resolve against the load order on access. The current adapter
supports Health, Magicka and Stamina actor values. Unsupported actor handles,
forms and equipment records return errors. Address patches, call detours,
vtable overrides and trampolines are explicit `HookError` variants with no
implementation path.

The declarations follow the names and responsibilities in the local
CommonLibSSE-NG-alandtse headers `RE/A/Actor.h`,
`RE/A/ActorEquipManager.h`, `RE/A/ActorState.h` and
`RE/B/BSPointerHandle.h`. The shim keeps its own safe Rust API and does not
copy those headers' implementation details.

`src/headers.rs` generates the corresponding supported C++ surface under `include/RE` and
`include/SKSE`. `RE/Skyrim.h` and `SKSE/SKSE.h` are the umbrella includes. The world view
binds settings, calendar hour, and active MGEF effects with the reanimation archetype to the
game runtime. Actor sit and sleep animation state is not represented, and returns an explicit
unsupported result. Binary address and vtable hooks also return an unsupported result. The
mapping follows the local CommonLibSSE-NG `ReanimateEffect.h`, `EffectArchetypes.h`,
`ActorState.h`, `Setting.h`, `Sky.h` and `TES.h` declarations. These declarations remain
source-level contracts without a C ABI. Regenerate the files
after changing the header mapping with `cargo run -p soe-commonlib-shim --bin generate-headers`.
The RE and SKSE interface declarations are source-level headers only. They do not provide engine
bindings or a C ABI. Selected contracts are implemented by adapters in the game crate. The
separate `SKSE/Plugin.h` entry point lets the loader validate and call adapted shared libraries,
but does not provide engine interfaces. It declares the `SKSEPlugin_Query`, `SKSEPlugin_Load` and
`SKSEPlugin_Version` exports used by CommonLibSSE-NG plugins. The loader checks the version data,
calls query and then load, and reports failures from either entry point. It supplies a non-null
runtime descriptor to both entry points, but its interface query callback returns no services and
does not bind to a running engine.
Condition methods delegate to `ConditionRuntime`. `ConditionRuntimeBridge` forwards condition
items and caller context over request/reply streams. The game evaluates each request against its
current world and VM. Native condition copying reports `UnsupportedHook`. The SHM-17 integration
fixture calls this bridge from compiled C++ and serves it from a live `Game` instance. The bridge
is a condition-call transport for integration and is not a general plugin loader interface.
The mapping follows the Rust API while adapting Rust names and types to C++, including
`NativeRegistry::register`, which is emitted as `register_native` because `register` is a C++
keyword. `RE::TESQuestStartStopEvent` matches CommonLib's 8-byte form id, started flag, failed
flag and two-byte padding layout. The Rust shim event carries the resolved runtime form id and
the quest's failed state. `soe-game` emits it after successful start and stop transitions through
the MIT event sink API.
The SHM-23 game integration test streams those emitted payloads to a compiled C++ event sink.
The C++ sink checks successful start and failed-stop fields, plus the unsupported trampoline
diagnostic. This test transport connects the live Rust world to the source-level C++ contract
without adding a C ABI.

The SHM-15 `RuntimeStateCallbacks` table defines the C++ callback boundary for the supported
`TES` world view. `RE::CallbackTES` converts those callbacks into C++ `Setting`, `Sky` and
`ReanimateEffect` values. Unsupported sit or sleep animation state and binary hooks return named
diagnostics. The game fixture calls the C++ adapter, and its callback requests are answered by
the test host through a pipe using the live `World` on each request. The MIT shim remains free
of game crate dependencies. This callback table is not a Skyrim executable ABI.

The SHM-02 fixture models Papyrus Extender's startup shape cited from
`engine-refs/PapyrusExtenderSSE/src/main.cpp`: it defines the standard SKSE query, load and version
exports, compiles as a shared library against the shim headers, and runs through
`crates/commonlib-shim/tools/plugin-loader.cpp`. It prints unsupported trampoline, scene graph
and Havok hooks declared in its unsupported-hook metadata. The fixture validates the source-level
entry-point path on this host, not a build of the upstream Papyrus Extender source, the unavailable
engine services, or compatibility with Windows SKSE binaries.

`RE/World.h` exposes a `WorldState` contract for actor critical stages,
actor-value modifier components, the selected AI package and procedure,
authored `XAPR` activation parents, and active-effect applications. The game
adapter binds those calls to live `World` state. Engine-internal `AIProcess`
hooks that require character controllers or scene nodes return an explicit
unsupported-hook error. Critical-stage reads return a value or an explicit
unsupported-state error, so a missing actor cannot appear as the default stage.

`Actor::actor_ptr` requires each provider to return its optional handle. A
stale actor handle can therefore be represented as an empty pointer result.

## SHM-13 magic and equipment contracts

`CommonLibMagic` in `soe-game` implements the MIT `MagicWorldState` provider
over the current `World`. Favorite membership is a separate actor-owned set
and is saved in the named `SHM-13` actor section. Casting checks report an
active cast, exhausted power recovery and insufficient magicka where those
states are represented, and return a typed unsupported-state error when the
spell cost or record cannot be evaluated. Magic-target queries read the
target's active applications, and spell dispel uses the existing world magic
path.

Map-marker visibility and fast-travel flags read and write the placed
reference's `on_map` and `fast_travel` fields. `ShowAllHidden` has no runtime
field and is rejected. Equip requests use the shared inventory equipment
operation when parameters match the supported default. Extra-data, slot,
force, message, sound and count variants return an unsupported-state error.
The local alandtse CommonLib sources read for this mapping were
`include/RE/M/MagicFavorites.h`, `MagicSystem.h`, `MagicTarget.h`,
`include/RE/E/ExtraMapMarker.h` and
`include/RE/M/MiddleHighProcessData.h`. The MIT Rust provider is the engine
boundary. The generated C++ headers remain declarations and do not create a
C ABI or executable singleton access.

## SHM-05 world contracts

`RE/World.h` exposes `ActorValueList` reads and writes through an `ActorValueWorldState`
provider. Its name and index lookups use the game's vanilla actor-value name
table. Actor reads and writes map FormIDs to the existing component store.
`BGSDefaultObjectManager::get` reads winning DOBJ data through the
footstep record view and returns its resolved FormID. These calls delegate to
the provider on each call, so they do not keep a second copy of actor or
record state.

The API scope follows `CommonLibSSE-NG-alandtse/include/RE/A/ActorValues.h`,
`ActorValueList.h`, `ArrowProjectile.h`,
`CommonLibSSE-NG-alandtse/include/RE/B/BGSDefaultObjectManager.h`, and
`BGSLoadFormBuffer.h`. The game-side DOBJ adapter accepts the record key used
by the engine's DOBJ data rather than exposing CommonLib's numeric default
object enum.

ArrowProjectile save, load and impact hooks return
`UnsupportedHook::ArrowProjectile`. `BGSLoadFormBuffer` version and form reads
return `UnsupportedHook::LoadFormBuffer`. The projectile requires runtime
behavior and vtable callbacks that are not implemented. Skyrim's binary save
buffer is not the engine's `FormReader` and is not exposed as a shim buffer.
The C++ fixture compiles and executes the supported provider dispatch and
checks the unsupported hook identifiers.

The world API binds `BGSSaveLoadGame` and `BGSSaveLoadManager` to the
world's engine save bytes, and `CALCED_OBJECT` form, count and supported COED
metadata to record-backed LVLI selection. The `BeamProjectile::AddImpact` and
`bhkCharacterController::flags` hooks return named unsupported-hook results
because aimed projectile delivery and per-character Havok state have no
runtime path yet. `CHARACTER_FLAGS` preserves the source enum names and bit
values. Change-map queries and filename-based save catalog operations also
return named unsupported-hook results. The generated headers and fixtures check this
source-level API. The C++ save fixture also changes live payload state, loads
the saved bytes, and verifies the payload is restored. C++ declarations alone
do not call into the Rust engine. The SHM-15 callback table supplies the
selected world view.
Other generated C++ declarations remain source-level contracts. The condition
request/reply bridge is the current C++ to Rust runtime call path used by
integration tests.

## SHM-07 world contracts

The world API includes `Calendar`, `Character`, `DEFAULT_OBJECTS`,
`ExtraActivateRefChildren` and `ExtraAliasInstanceArray` contracts. The game
adapts calendar values, loaded actor state, resolved DOBJ entries and attached
quest aliases from its existing world tables. Alias results are sorted by
quest and alias id. Default-object lookup preserves malformed-record errors.
The runtime has no activate-children extra state, so that call returns an
explicit unsupported-hook result. These contracts do not expose CommonLib
memory layouts or executable hooks.

## Extra-data stack views (SHM-08)

`RE/ExtraData.h` exposes read-only views for `ExtraDataList`, the selected
`ExtraDataType` values, `ExtraEnchantment`, `ExtraHotkey` and
`ExtraLeveledCreature`. The game crate adapts its live inventory `Stack` and
the active load order to this MIT API. Enchantment identity and charge,
hotkey slot, and original/template actor-base identities are read from that
stack. `remove_on_unequip` is optional because the engine stack save model
does not retain CommonLib's runtime flag. Invalid field layouts return no
typed view. Hotkey values outside slots 0 to 7 are represented as unbound.

Adding, removing or changing enchantment extras remains unsupported because
those operations mutate engine-owned stack state. Their C++ declarations carry
explicit unsupported reasons. The API does not copy or retain stack state.
The declarations follow `CommonLibSSE-NG-alandtse/include/RE/E/ExtraDataList.h`,
`ExtraDataTypes.h`, `ExtraEnchantment.h`, `ExtraHotkey.h` and
`ExtraLeveledCreature.h`. The CommonLib implementation confirms enchantment
equality includes its runtime removal flag and hotkey's unbound value is -1.
The reference-extra API names `ExtraLeveledItemBase`, `ExtraLightData`,
`ExtraLinkedRefChildren`, `ExtraMapMarker` and `ExtraMissingLinkedRefIDs` with
read-only value views. `ReferenceExtraData` is the MIT boundary implemented by
the game adapter. Missing optional values mean the reference has no such
extra. The game test dispatches through this trait object after constructing a
view from live `World` records and state. The C++ fixture checks the matching
source-level declarations and unsupported-hook metadata. `EXTRA_DATA_HOOKS`
reports that executable vtable hooks are not supported by this engine.
`World::shim_reference_extras` supplies the game adapter for those five views.
It borrows the world, reads XLIB, XLIG and map-marker fields through current
records, and includes live marker names and visibility flags. Incoming links
are derived from winning XLKR records and loaded reference state, including
authored children before their state is materialized. Missing links are targets
absent from the form table, rather than merely unloaded references. Derived
link arrays are sorted and retained only for the lifetime of the world borrow.
This does not import additional ESS reference extras or expose mutation hooks.
Field identities follow the local CommonLibSSE-NG-alandtse headers
`include/RE/E/ExtraLeveledItemBase.h`, `ExtraLightData.h`,
`ExtraLinkedRefChildren.h`, `ExtraMapMarker.h` and `ExtraMissingLinkedRefIDs.h`.
Record layouts follow local xEdit `Core/wbDefinitionsTES5.pas` REFR definitions.
The existing record table supplies these fields without another parser.
`RE/ExtraData.h` also declares the SHM-10 views for `ExtraMissingRefIDs`,
`ExtraNorthRotation`, `ExtraOutfitItem`, `ExtraPoison` and `ExtraTeleport`.
The value field names and types were checked against the alandtse CommonLib
headers `include/RE/E/ExtraMissingRefIDs.h`,
`include/RE/E/ExtraNorthRotation.h`, `include/RE/E/ExtraOutfitItem.h`,
`include/RE/E/ExtraPoison.h` and `include/RE/E/ExtraTeleport.h`.
The `ReferenceExtraQuery` interface distinguishes unsupported kinds from an
absent value. The game adapter reads outfit item, poison and door teleport
values from shared reference and inventory state. Missing reference IDs are
read from the reference's resolved XAPR activation-parent records, and invalid
XAPR targets that are not REFR or ACHR records are rejected. North
rotation reads the interior CELL default of zero, matching CommonLib when no
extra is present. Exterior worldspace north rotation is not represented by the
current runtime records and returns no value. Binary hook requests return a
diagnostic through `HookRegistry::reject`. Poison counts retain CommonLib's
unsigned 32-bit range through the Rust and C++ views.
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

## SHM-22 event contracts

`ScriptEventSourceHolder` also exposes the `TESGrabReleaseEvent`,
`TESHarvestedEvent::ItemHarvested`, `TESHitEvent`, `TESObjectLoadedEvent` and
`TESQuestStageEvent` channels. Payload fields follow the local alandtse
CommonLib headers `RE/T/TESGrabReleaseEvent.h`, `RE/I/ItemHarvested.h`,
`RE/T/TESHitEvent.h`, `RE/T/TESObjectLoadedEvent.h`,
`RE/T/TESQuestStageEvent.h` and `RE/S/ScriptEventSourceHolder.h`. Runtime
references and actors use resolved runtime handles. Hit flags retain the four
published low bits. The executable callback pointer in the quest stage event
is not exposed.

The game emits grab and release events when its held-reference state changes,
harvest events after produce has been added to an actor inventory, and hit
events after a melee contact commits. Reference loaded events follow 3D
resource residency changes. Quest stage events follow successful defined
stage changes and use item index zero because this runtime does not execute
individual quest log callbacks. These channels have in-process Rust producers
and generated source-level C++ declarations. They do not install executable
hooks or connect an independently loaded binary plugin to the game.

The event batch declares `TESContainerChangedEvent`, `TESDeathEvent`,
`TESEnterBleedoutEvent`, `TESFormDeleteEvent` and `TESFurnitureEvent` payloads. `EventState`
defines queries for current inventory count, death, bleedout, deletion and furniture occupancy.
The game implements these queries on `World`: inventory reads an instantiated reference's item
count, actor reads use current actor state, deletion reads reference flags and winning-record
deletion flags, and furniture occupancy reads the actor's reserved seat. Unknown or uninstantiated
objects return `std::nullopt`. The adapter reports an occupied seat as an Enter event. Vtable and
call-site patch requests return the `UNSUPPORTED_VTABLE_HOOK` description because those addresses
have no meaning in this runtime.

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
Unsupported executable hooks return a named `UnsupportedHookInfo` with a reason. The C++ fixture
checks the source-level contract using a fixture adapter. Game tests exercise the Rust adapter
against live `World` settings, inventory markers, placed hazards, actor combat state and resolved
melee results. There is no C++ to Rust bridge, so separately compiled C++ plugins cannot call these
live adapters. The API is source-level and carries no C++ runtime layout.
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

The C++ fixture compiles and calls these declarations, including all unsupported results. A
second C++ fixture exchanges factory, inventory and handle requests with the Rust game test
process, which answers through `commonlib` using live `World` state. The Rust game fixture also
exercises the views directly. The
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
The C++ fixture compiles and calls the source-level contracts. The game test samples populated
views from `World` and passes those values to a compiled C++ consumer, which checks the contract
results and unsupported-hook values. The C++ contract objects remain fixture implementations,
and this does not provide a C++ to Rust ABI or a compiled SKSE plugin.
The declarations were checked against alandtse CommonLib's `TESDataHandler.h`, `TESFile.h`,
`TESObjectREFR.h` and `ActorState.h`.
The existing `SKSE::TRAMPOLINE` declaration continues to identify machine-code patching as
unsupported, and the C++ contract fixture checks that diagnostic.

## Untested

The SHM-01 test compiles the generated umbrella headers with a C++20 compiler and checks C++
signatures for the Rust methods they project. The SHM-02 test compiles a Papyrus Extender startup
adaptation against the headers, checks PluginVersionData's layout, loads it through the standard
SKSE export names and checks its unsupported-hook metadata. It does not compile the surveyed
upstream plugin, whose game interfaces and trampoline hooks have no engine implementation here.
The SHM-03 test compiles the generated headers, and its game-side test calls the adapter over live
actor and AI state. SHM-10 compiles an extra-data C++ fixture and checks live game reads in Rust.
SHM-11 C++ fixtures check the world type contract, and game tests exercise its Rust adapter against
`World`. SHM-13 checks magic contracts with test doubles, not live game state. SHM-14 exercises
player and process-list reads against synthetic `World` state and through C++ callbacks that query
a live Rust `World`. SHM-20 compiles the five typed event payload contracts and checks sink dispatch
against synthetic state fixtures, as well as streaming events from live game producers to generated C++
event sources. SHM-21 implements five event-state queries on `World` and checks live inventory, actor,
deletion and furniture state plus unsupported-hook errors. SHM-22 checks the
additional payload contracts, live-state receiver calls and unsupported hook results in Rust and
C++ fixtures. SHM-23 compiles and runs the quest event payload and verifies the explicit trampoline
rejection marker. A game integration test streams live quest start and failed-stop events through the
MIT sink into a compiled C++ consumer, which checks their payload fields. This test transport does
not add a C++ to Rust ABI. No surveyed plugin has been compiled against the headers or loaded, and
the generated declarations are not generally linked to engine behavior. The SHM-14 callback table
is a source-level provider path, not a binary-compatible CommonLib ABI for existing Skyrim plugins.
The counts are token counts,
so a name mentioned only in a comment is counted and a type reached only through `auto` is not.
Four plugins are a sample, not the population: the map's 70 to 90 percent open-source estimate
is from a six-plugin sample and the shimmable share here is 3 of 4 by plugin, far less by hook
site because TDM dominates. The alandtse fork's 390 added headers were counted, not read.

SHM-14 adds `RE/World.h` and the matching MIT Rust boundary in `world.rs`.
`PlayerCharacter` reads player identity, position and actor values from the
world provider. `ProcessLists` lists actor forms whose runtime actor state is
built. The game crate implements that provider against its `World` form table.
`ObjectRefHandle` and `ProjectileHandle` retain their 32-bit CommonLib-shaped
representation. C++ consumers can use `CallbackPlayerWorldState`, a callback-table
adapter over the MIT `PlayerWorldState` contract. The game's Rust adapter resolves
player handles to runtime FormIDs and reads player state and actor lists from
the existing `World`. Its C++ integration fixture sends callback queries over
private pipes to the live Rust world service. Projectile runtime access returns
the typed `ProjectileRuntime` unsupported-hook result. Binary patch hooks are
also identified as unsupported. The callback table is an explicit source-level
bridge and does not make existing Skyrim binary plugins compatible.

The contracts follow the local alandtse CommonLibSSE-NG declarations in
`include/RE/B/BSPointerHandle.h`, `include/RE/P/PlayerCharacter.h`,
`include/RE/P/ProcessLists.h` and `include/RE/P/Projectile.h`. They preserve
the handle width, player read shape, process actor collection and projectile
type while adapting reads to engine-owned state.

## Untested SHM-14 contracts and survey limits

The SHM-01 and SHM-14 tests compile generated umbrella headers with a C++20 compiler and check
C++ signatures for the Rust methods they project. SHM-14's game test sends callback requests from
C++ to a Rust service borrowing the live `World`. It checks reference handles, player state, process
lists and the unsupported projectile result. The bridge is exercised in the integration fixture,
not yet in the plugin loader. No sample plugin has been compiled against the headers. The counts are token counts,
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
The game integration fixture forwards events raised by its live `World` to a
compiled C++ sink process and checks the payloads and sink results returned by
that process. The pipe is test transport, not a plugin ABI. The shim still has
no C ABI, and C++ declarations do not expose a running engine to a separately
compiled plugin.

## Event binding batch SHM-19

The MIT event API also projects `DisarmedEvent`, `DragonSoulsGained`,
`LevelIncrease` and `LocationDiscovery` through `ScriptEventSourceHolder`.
Their source-level payloads carry actor identities, awarded soul count,
player identity and level, and map-marker identity with an optional
worldspace identity. The C++ contract fixture checks these source methods,
constructs and dispatches each payload, and verifies that hook installation
returns an unsupported result with the requested hook name.

The payloads follow CommonLibSSE-NG-alandtse `include/RE/D/DisarmedEvent.h`,
`include/RE/D/DragonSoulsGained.h`, `include/RE/L/LevelIncrease.h`,
`include/RE/L/LocationDiscovery.h` and
`include/RE/S/ScriptEventSourceHolder.h`. The source-level declarations do
not reproduce CommonLib pointer layouts or connect directly to a runtime.
The SHM-19 game fixture subscribes a compiled C++ sink to each generated
source, forwards payloads emitted by the live `World` over a test pipe, and
checks the values received by those sinks. It also checks that vtable hook
requests return an explicit unsupported result. The pipe is test transport,
not a C++ to Rust ABI.

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
