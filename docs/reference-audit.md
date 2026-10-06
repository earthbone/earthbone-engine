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
| `skse64/scripts/vanilla/LeveledSpell.psc`, modified `LeveledSpell.psc`, `PapyrusLeveledSpell.cpp` and `PapyrusLeveledItem.cpp`, `skse64/scripts/vanilla/Location.psc` | `game/src/natives.rs` registers each listed method and tests the registry inventory. `SetNthLevel` is implemented for all leveled-list classes with the SKSE 16-bit cap. `SetChanceNone` stores the supplied byte as SKSE does. The LeveledSpell-only count getter was removed. `Location.IsSameLocation` now follows the script's shared-ancestor and keyworded parent/child cases. Tests `scr_65_registry_inventory_matches_location_and_leveled_spell_contracts`, `scr_65_leveled_spell_setters_match_skse_contract`, `scr_65_same_location_matches_keyworded_parent_child_contract` | CK pages for these Location methods are absent locally. The shipped script is the source. `Location.IsLoaded`, `HasRefType`, `GetRefTypeAliveCount` and `GetRefTypeDeadCount` are quiet stubs assigned to SCR-35. |
| `skse64/scripts/vanilla/Utility.psc` and `ck-wiki/Utility_Script.wikitext` distinguish wait seconds, menu seconds and game hours | `papyrus/src/vm.rs`, `vm/state.rs`, `vm/hash.rs`, `game/src/lib.rs`. Tests `utility_wait_domains_survive_save_and_timescale_changes`, `version_one_clock_loads_with_legacy_defaults`, `game_tick_supplies_timescale_and_menu_pause_to_waits` | Menu UI must call `tick_menu_mode`. Version 1 saves cannot recover which wait function produced a legacy sleep. |
| CK `Quest_Script.wikitext`, `Alias_Script.wikitext`, `ReferenceAlias_Script.wikitext`, and the named `Clear_-_ReferenceAlias.wikitext` contract cover the Quest and reference-alias registry bindings | `NativeRegistry::method_bindings` and `global_bindings` expose sorted runtime binding inventories. `game/tests/scr_68_native_contracts.rs` compares each Quest, Alias and ReferenceAlias method and Quest global with the CK class declarations. Actor getters filter non-actors. `Clear` validates the QUST Optional flag, reports required aliases, clears optional aliases and reevaluates the held actor's package. `TES5Edit/Core/wbDefinitionsTES5.pas` and `uesp-modfileformat/QUST.wikitext` establish Optional as FNAM bit 1. | The corpus lacks most linked individual pages for these methods, so those comparisons use the class-page descriptions. Quest `IsStarting` and `IsStopping` are quiet false stubs despite CK describing transition state. SCR-43 owns those fixes. |
| `ck-wiki/DispelSpell_-_Actor.wikitext` and `DispelAllSpells_-_Actor.wikitext` require active-effect removal | `game/src/natives.rs` no longer destroys known or equipped spells. Test `dispelling_never_removes_known_or_equipped_spells` replaces the incorrect test oracle. Both calls are counted as stubs. | Active-effect ownership, expiry, removal and protected categories remain unimplemented. Cast recording is not magic execution. |
| SPID `SPID/src/LookupConfigs.h::ChanceComponentParser` accepts trailing `!`. `Defs.h::Chance` preserves deterministic mode. `LookupFilters.cpp` seeds per actor. | `moddata/src/spid.rs` preserves the percent and marker, rejects malformed values. `game/src/moddata.rs` rolls ordinary chance per matching base and reports unsupported deterministic fractional rules. Parser and runtime regressions cover `0!`, `50!`, `100!`. | Exact SPID per-actor lifecycle and seed, and KID's separate seed in `src/Data/BaseFilters.cpp`, remain work. |
| Shipped `skse64/scripts/vanilla/Quest.psc::GetStage` calls `GetCurrentStageID`. CK `Quest_Script.wikitext` describes the highest stage. | Both native getters return `highest`. Test `quest_getters_agree_after_lower_stage_and_reset` sets 20 then 10 and resets. | Wider quest lifecycle semantics still need original-runtime comparisons. |
| `godot-docs/classes/class_physicsserver3d.rst::body_add_shape` accepts a shape transform | `godot-host/src/godot_physics.rs` preserves native boxes, spheres, capsules and convex hulls, with dimensions in metres. Tests check translated spheres, both capsule endpoint orders and compound box corners. | The Bannered Mare Jolt contact smoke test passes, as recorded in `physics.md`. No performance improvement is claimed. Shell radius and unsupported bhk shapes remain listed in `physics.md`. |
| `godotwind/src/core/nif/nif_material_info.gd::get_cache_key` includes alpha and culling | `mesh::MaterialKey` and `godot_layer::MaterialMap` separate material state from path-keyed uploads. `shared_textures_keep_distinct_alpha_and_culling_in_both_orders` covers all supported states. | Glow, environment and other shader inputs remain unwired. This is not a new shading model. |

## Spell and Topic native audit, SCR-71

The registry inventory is method-only: `Spell.Cast`, `Spell.IsHostile`,
`Spell.RemoteCast`, `Spell.Preload`, `Spell.Unload` and `Topic.Add`. No Spell
or Topic globals are registered. The SCR-71 registry test checks every listed
binding and confirms `Topic.Remove` is absent. The local CK wiki corpus has no
named Spell or Topic native pages. The shipped
`skse64/scripts/vanilla/Spell.psc` and `Topic.psc` provide the native
signatures and behavior notes instead.

| Binding | Local contract source and comparison | Implementation and disposition |
|---|---|---|
| `Spell.Cast` method | `Spell.psc`: source reference, optional target reference, no return value. `Papyrus_Runtime_Errors.wikitext`: calling from an unloaded source cell warns and cannot cast. | Calls `World::cast_spell` with the explicit source reference and credits that source as caster. Supported direct casts execute. The unloaded-source warning is not reproduced because the cast API has no loaded-cell query. Assign that diagnostic to `MAG-02` with delivery integration. Unsupported delivery or effects are logged by the native and remain in the existing magic delivery rows. |
| `Spell.IsHostile` method | `Spell.psc`: zero arguments and a boolean result. The CK page is absent, so the record rule is inferred from the MGEF hostile flag used by the runtime. | Added in SCR-71. Resolves each attached effect in the spell's plugin context and returns true if any referenced MGEF has the hostile flag. Bad references log and return false. |
| `Spell.RemoteCast` method | `Spell.psc`: source reference, blame actor, optional target reference, no return value. `Papyrus_Runtime_Errors.wikitext` gives the same unloaded-source-cell warning as Cast. | Calls `World::cast_spell` with the explicit source and blame actor. The unloaded-source warning has the same `MAG-02` assignment as Cast. Unsupported delivery or effects are logged and remain in the existing magic delivery rows. |
| `Spell.Preload` method | `Spell.psc`: no arguments or return value. Preloads spell art and warns about misuse and memory use. | Quiet stub. Owning implementation row: `SCR-47`. |
| `Spell.Unload` method | `Spell.psc`: no arguments or return value. Unloads spell art previously preloaded and warns about misuse. | Quiet stub. Owning implementation row: `SCR-47`. |
| `Topic.Add` method | `Topic.psc`: no arguments or return value. Adds the topic to the list known by the player. | Quiet stub. Owning implementation row: `SCR-48`. |

The previous inventory grouped `Topic` with `MusicType` and listed both Add
and Remove. The source declares no `Topic.Remove`. The docs now list only
`Topic.Add`. These sources contain no Spell or Topic global bindings.

## ShaderParticleGeometry and Sound native audit, SCR-70

`NativeRegistry::method_bindings` and `global_bindings` produce the registered
inventory. The local CK crawl has no `ShaderParticleGeometry` or `Sound`
function pages. Its `ATTEMPTS.md` records the archive crawl and unsuccessful
routes, so this audit uses the local shipped PSC and reference headers where
available rather than inferring undocumented CK descriptions.

| Binding | Contract source and comparison | Current behavior and owner |
|---|---|---|
| `ShaderParticleGeometry.Apply` method | CK page absent. CommonLib RTTI identifies a `void(float)` native shape. Local UESP `Raw_Function_Data.wikitext` lists the corresponding condition function with `Form, Float`. The method fade parameter remains based on the engine's registered API and implementation. | Reads and validates the winning SPGD record, adds or restarts its transient layer, and fades by seconds. `SCR-46` owns layer behavior and the missing host render path. |
| `ShaderParticleGeometry.Remove` method | Same missing CK page and `void(float)` RTTI evidence. | Removes or fades the matching layer. `SCR-46` owns layer behavior and the missing host render path. |
| `Sound.Play` method | Local shipped `skse64/scripts/vanilla/Sound.psc`: takes an `ObjectReference`, returns an integer instance ID. | Records source and sound, returns a fresh nonzero ID. Physical output verification is `HST-05`. |
| `Sound.StopInstance` global | `Sound.psc`: takes an integer instance ID and returns nothing. | Removes only the selected instance. |
| `Sound.SetInstanceVolume` global | `Sound.psc`: takes integer instance ID and float volume, clamps the volume to 0 through 1, returns nothing. | Updates the selected instance and clamps its volume. |

The same PSC declares `PlayAndWait(ObjectReference) -> Bool` and
`GetDescriptor() -> SoundDescriptor`. Neither is in the native registry. No
existing backlog row owns those missing bindings. They remain unassigned API
gaps because this audit cannot add or widen a backlog row. The exact registry
sets and registered method behavior are covered by the `scr_70_` tests listed
in `docs/game.md`.

## Leveled-list native contracts

The registry in `game/src/natives.rs::leveled` registers methods, not Papyrus
globals, for these classes. CK's `LeveledItem` record page describes list
fields but does not define Papyrus methods. No matching method pages exist in
the local `ck-wiki` dump. The Papyrus method contracts therefore compare the
CK record behavior with SKSE's shipped declarations in
`skse64/scripts/vanilla/LeveledItem.psc` and `LeveledActor.psc`, and its
extended declarations in `skse64/scripts/modified/LeveledItem.psc` and
`LeveledActor.psc`.

| Class and binding | Contract compared | Registry result |
|---|---|---|
| LeveledItem.AddForm | CK list entry. SKSE accepts `Form, level, count` | Registered with level and count |
| LeveledItem.Revert | SKSE removes script-added forms | Registered. Restores authored entries and chance-none |
| LeveledItem.GetNumForms | SKSE returns entry count | Registered |
| LeveledItem.GetNthForm | SKSE returns the indexed form or None | Registered |
| LeveledItem.GetNthLevel | SKSE returns indexed entry level or zero out of range | Registered |
| LeveledItem.SetNthLevel | SKSE sets an indexed level, caps it at 65535, and ignores an invalid index | Added. Caps to the stored 16-bit range and ignores invalid indices |
| LeveledItem.GetNthCount | SKSE returns indexed entry count or zero out of range | Registered |
| LeveledItem.SetNthCount | SKSE sets an indexed count, caps it at 65535, and ignores an invalid index | Added. Caps to the stored 16-bit range and ignores invalid indices |
| LeveledItem.GetChanceNone | SKSE returns the list chance-none value | Registered |
| LeveledItem.SetChanceNone | SKSE sets the list chance-none byte | Registered, stores the supplied value's low byte |
| LeveledItem.GetChanceGlobal | SKSE returns the list's chance-global form | Missing. Record-backed support belongs to PRG-05, the leveled-item selection owner |
| LeveledItem.SetChanceGlobal | SKSE stores the list's chance-global form | Missing. Same PRG-05 ownership, since it needs record override and saved-state handling |
| LeveledActor.AddForm | SKSE accepts `Form, level` | Registered. Count defaults to one |
| LeveledActor.Revert | SKSE removes script-added forms | Registered. Restores authored entries |
| LeveledActor.GetNumForms | SKSE returns entry count | Registered |
| LeveledActor.GetNthForm | SKSE returns the indexed form or None | Registered |
| LeveledActor.GetNthLevel | SKSE returns indexed entry level or zero out of range | Registered |
| LeveledActor.SetNthLevel | SKSE sets an indexed level, caps it at 65535, and ignores an invalid index | Added. Caps to the stored 16-bit range and ignores invalid indices |
| LeveledActor.GetNthCount | SKSE returns indexed entry count or zero out of range | Registered |
| LeveledActor.SetNthCount | SKSE sets an indexed count, caps it at 65535, and ignores an invalid index | Added. Caps to the stored 16-bit range and ignores invalid indices |

The `scr_64_leveled_actor_and_item_bindings_match_skse_contracts` test drives
the registered list methods on both classes, including 16-bit saturation and
invalid-index no-ops. Chance-global access remains unsupported and is assigned
to PRG-05 because resolving that field affects leveled-item selection as well
as the Papyrus API.

## SCR-66 LocationAlias and Message contracts

The native registry tests derive the inventory by filtering contract
candidates against registered method and global names, then assert the exact
sets. The CK wiki corpus contains `Alias` and
`GetOwningQuest` pages, but does not contain the named `LocationAlias` or
`Message` function pages. The CK-derived signatures and descriptions were
cross-checked with the Skyrim Papyrus Index, which links back to those CK
function pages.

| Registry binding | CK contract comparison | Result and owner for remaining work |
|---|---|---|
| `LocationAlias.GetLocation` method ([CK-derived reference](https://papyrus.bellcube.dev/skyrimse/script/locationalias/function/getlocation/)) | Returns the Location held by this alias, or None when empty. | Reads the alias's current form through the shared quest alias state. Covered by `scr_66_location_alias_holds_a_location`. |
| `LocationAlias.ForceLocationTo` method ([CK-derived reference](https://papyrus.bellcube.dev/skyrimse/script/locationalias/function/forcelocationto/)) | Takes a Location and assigns it to this alias. | Resolves the form argument and writes shared quest alias state. Covered by `scr_66_location_alias_holds_a_location`. |
| `Message.Show` method ([CK-derived reference](https://papyrus.bellcube.dev/skyrimse/script/message/function/show/)) | Takes nine optional float substitutions. Returns the selected button for a message box, and -1 when it is not a message box or display fails. | The registry binding queues record text and immediately returns 0. Numeric formatting and blocking selection belong to `UI-01`, the host menu lifecycle. |
| `Message.ShowAsHelpMessage` method ([CK-derived reference](https://papyrus.bellcube.dev/skyrimse/script/message/function/showashelpmessage/)) | Takes an input event, duration, interval and maximum display count. The event suppresses later displays until reset or triggered. | The binding only queues a notification. Help prompt rendering and display lifetime belong to `UI-06`, the HUD. |
| `Message.ResetHelpMessage` global ([CK-derived reference](https://papyrus.bellcube.dev/skyrimse/script/message/function/resethelpmessage/)) | Takes an input event and clears the event's prior help-message status. | It remains a quiet stub under the explicit native work in `SCR-36`. The incorrect `Message.ResetHelpMessage` method registration was removed. |

The inventory test also checks that `GetID` remains registered on `Alias`,
where `LocationAlias` inherits it, and that the invalid instance form of
`Message.ResetHelpMessage` is absent. The CK comparison covers every direct
registry binding for both classes and the Message global.

## SCR-57 Debug and EffectShader contracts

The inventory below follows registrations in `game/src/natives.rs`, including
quiet stubs. `Debug_Script.wikitext` provides the class-level Papyrus
signatures and summaries. The CK corpus has an individual page only for
`CenterOnCell`, which specifies its `None` return and warns that it can freeze
the application. Individual CK pages for the other registered names are not
present locally. `skse64/scripts/vanilla/EffectShader.psc` supplies the two
EffectShader signatures and the optional duration default because there is no
EffectShader CK page in the corpus.

| Registry binding | CK or PSC contract | Current behavior and resolution |
|---|---|---|
| `Debug.Trace` global | String, Int severity, no result. Write to Papyrus log and debug text page. | Logs the stringified first argument. SCR-07 covers form-id conversion. Severity and debug text page output have no owning open backlog row. |
| `Debug.TraceUser` global | String log name, String text, Int severity, Bool result. Write to named user log and debug text page. | Writes to Papyrus log and returns `None`. User log behavior and Bool result belong with SCR-20's user-log system. |
| `Debug.TraceConditional` global | String, Bool condition, no result. Log only when true. | Logs only when true. Matches the documented condition and result. |
| `Debug.TraceStack` global | String, Int severity, no result. Write message and full call stack. | Logs the stringified message without a call stack. Native calls do not expose stack frames yet, and no open backlog row owns this gap. |
| `Debug.Notification` global | String, no result. Display an in-game notification. | Adds the text to `World::notifications`. The host display path is separate. Matches the runtime queue contract. |
| `Debug.MessageBox` global | String, no result. Display an OK message box. | Adds the text to `World::message_boxes`. The host display path is separate. Matches the runtime queue contract. |
| `Debug.SendAnimationEvent` global | ObjectReference, String, no result. Send the event to the reference. | Queues the event when its target resolves and returns `None`, matching the Papyrus result shape. |
| `Debug.OpenUserLog` global | String, Bool result. Open a user log and fail if already open. | Returns `true` without creating a log. SCR-20 owns user-log behavior. |
| `Debug.QuitGame` global | No arguments or result. Quit the game. | Quiet stub. SCR-20 owns the debug host action. |
| `Debug.CenterOnCell` global | String cell name, no result. Teleport the player. The individual CK page warns it can freeze. | Quiet stub. SCR-20 owns the debug host action. |
| `Debug.CenterOnCellAndWait` global | String cell name, Float result. Teleport and wait until arrival. | Returns `0.0` with the declared Float shape, without teleporting. SCR-20 owns the debug host action. |
| `Debug.CloseUserLog` global | String log name, no result. Close the named log. | Quiet stub. SCR-20 owns user-log behavior. |
| `Debug.DumpAliasData` global | Quest, no result. Dump alias fill information. | Quiet stub. SCR-21 owns the debug host action. |
| `Debug.DebugChannelNotify` global | No local CK contract found. | Quiet stub. SCR-21 owns this registry-only debug binding until a local contract is established. |
| `Debug.SetGodMode` global | Bool, no result. Set god mode. | Quiet stub. SCR-21 owns the debug host action. |
| `Debug.ToggleAI` global | No arguments or result. Toggle AI. | Quiet stub. SCR-21 owns the debug host action. |
| `Debug.ToggleCollisions` global | No arguments or result. Toggle collision. | Quiet stub. SCR-21 owns the debug host action. |
| `Debug.StartScriptProfiling` global | String script name, no result. Start script profiling. | Quiet stub. SCR-22 owns profiling behavior. |
| `Debug.StopScriptProfiling` global | String script name, no result. Stop script profiling. | Quiet stub. SCR-22 owns profiling behavior. |
| `Debug.StartStackProfiling` global | No arguments or result. Start profiling the calling stack. | Quiet stub. SCR-22 owns profiling behavior. |
| `Debug.StopStackProfiling` global | No arguments or result. Stop profiling the calling stack. | Quiet stub. SCR-22 owns profiling behavior. |
| `Debug.TakeScreenshot` global | No arguments or result. Take a screenshot. | Quiet stub. SCR-22 owns the debug host action. |
| `Debug.SetFootIK` global | Bool, no result. Enable or disable foot IK. | Quiet stub. SCR-23 owns the host animation control. |
| `Debug.ShowRefPosition` global | No local CK contract found. | Quiet stub. SCR-23 owns this registry-only debug binding until a local contract is established. |
| `Debug.PlayerMoveToAndWait` global | String destination reference, Float result. Teleport and wait until arrival. | Returns `0.0` with the declared Float shape, without teleporting. SCR-23 owns the debug host action. |
| `Debug.MoveToCell` global | No local CK contract found. | Quiet stub. SCR-23 owns this registry-only debug binding until a local contract is established. |
| `EffectShader.Play` method | PSC: ObjectReference and optional Float duration defaulting to -1 seconds. Negative duration means infinite. | Quiet stub. SCR-24 owns Papyrus behavior. MAG-14 owns host shader lifetime and rendering. |
| `EffectShader.Stop` method | PSC: ObjectReference. Stop the shader on that target. | Quiet stub. SCR-24 owns Papyrus behavior. MAG-14 owns host shader lifetime and rendering. |

`scr_57_registry_contains_every_audited_binding` checks the 26 Debug globals
and two EffectShader methods against the actual native registry. The audit
does not claim that queued notifications or message boxes are drawn by a host,
that debug-only host controls execute, or that effect shaders render.

No new runtime dependency or upstream source file was imported. Core code is
GPL-3.0-or-later. No derived code belongs in the MIT shim. tes4skyrim's own
MIT license does not cover every bundled external component. PyNifly shares
HavokLib ancestry, so it is not an independent codec oracle. HKX2's missing
license must be resolved before code reuse. Do not execute bundled tools
merely because the repository includes them.

## Native contract audit, SCR-67

The registry inventory comes from `game/src/natives.rs::register`, including
implemented methods and quiet stubs. `scr_67_object_reference_method_inventory_matches_registry`
compares the full sorted set returned by `NativeRegistry::method_bindings`
against the ObjectReference inventory below and checks the corresponding
global set is empty. `scr_67_music_type_bindings_are_methods_and_not_globals`
does the same for MusicType.Add and MusicType.Remove. Neither class has a
direct global binding. Inherited `Form` methods are outside this inventory.

For each listed `ObjectReference` method, the corresponding
`ck-wiki/<Function>_-_ObjectReference.wikitext` page was compared for its
Papyrus signature, parameter order and defaults, return type and stated
behavior. The class declaration is `ck-wiki/ObjectReference_Script.wikitext`.
`MusicType.Add` and `MusicType.Remove` have no named function or class page in
the local CK corpus. Their no-argument, no-return signatures were compared
with `skymp/skyrim-platform/src/platform_se/codegen/convert-files/FunctionsDump.txt`.
They validate a live MUSC receiver and mutate the world's transient music-type
list, as described in `game.md`.

| Registry method | CK contract comparison |
|---|---|
| Enable, EnableNoWait, Disable, DisableNoWait, IsEnabled, IsDisabled | The enable pages distinguish waiting and no-wait variants. The state queries return booleans. |
| Delete, IsDeleted, Is3DLoaded | Delete and IsDeleted match the documented calls. Is3DLoaded reads reference cell residency. Publishing exact host 3D residency is assigned to HST-13. |
| GetBaseObject, GetParentCell, GetWorldSpace, IsInInterior | Form returns and interior-cell query match their CK pages. |
| GetPositionX, GetPositionY, GetPositionZ, GetAngleX, GetAngleY, GetAngleZ | The six scalar getters match the axis and degree contracts. |
| SetPosition, SetAngle | Both take three floats. SetAngle uses degrees at the Papyrus boundary. |
| MoveTo, MoveToMyEditorLocation, GetDistance | MoveTo takes target, three offsets and an optional rotation flag. SCR-67 now applies the default-true rotation flag. |
| Activate, BlockActivation, IsActivationBlocked | Activate returns success and default-only processing suppresses events while bypassing the block. |
| GetLinkedRef, GetNthLinkedRef | Both return references selected by their optional keyword or index contracts. |
| GetItemCount, AddItem, RemoveItem, RemoveAllItems | Item queries and mutation calls return the documented integer or None. RemoveItem reads the optional recipient at parameter four. RemoveAllItems honors `abKeepOwnership`. Quest-item retention remains unsupported because stacks do not classify quest objects. |
| GetScale, SetScale, GetDisplayName, SetDisplayName | Scalar, string and setter return types match the named pages. |
| PlayAnimation, PlayAnimationAndWait, PlayGamebryoAnimation, WaitForAnimationEvent | PlayAnimation queues an event and returns a boolean. PlayAnimationAndWait and WaitForAnimationEvent defer until an event. PlayGamebryoAnimation only accepts a non-empty name. Authored legacy animation playback remains unsupported under SCR-39. |
| GetOpenState, SetOpen, Lock, IsLocked, GetKey, GetLockLevel, SetLockLevel | State and key queries match the class contract. Lock and setters return None. |
| PlaceAtMe, PlaceActorAtMe | The placement calls return the created reference and honor the optional count/disabled inputs supported by their implementation. |
| AddToMap, EnableFastTravel | Both are void methods with optional fast-travel input as documented. |
| SetDestroyed, ClearDestruction, GetCurrentDestructionStage | The state setters and query match the documented types. ClearDestruction resets supported destruction state. GetCurrentDestructionStage returns the stage or -1 when none is set. |
| IgnoreFriendlyHits, IsIgnoringFriendlyHits | Setter and query match the documented boolean behavior. |
| GetCurrentLocation, GetEditorLocation, IsInLocation | Reference location queries match their return types and child-location behavior. |
| IsInDialogueWithPlayer | The named page takes no arguments and returns whether this actor or talking activator is talking to the player. The native reads the current dialogue target and returns false for other references. |
| GetActorOwner, GetFactionOwner, SetActorOwner, SetFactionOwner | Owner methods match their typed form parameters and None returns. |
| AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters | These are void methods on references. Their matching alias declarations are separately registered on `ReferenceAlias`. |
| SetAnimationVariableFloat, SetAnimationVariableInt, SetAnimationVariableBool | Each setter takes its typed value and returns None. |
| GetAnimationVariableFloat, GetAnimationVariableInt, GetAnimationVariableBool | Each getter returns the matching scalar type. |
| GetCurrentScene | Takes no arguments and returns the active scene for this reference, or None. It reads the world's active scene set. |
| IsFurnitureInUse, IsFurnitureMarkerInUse | Signatures and defaults match the CK pages. The runtime reads HST-09 marker reservations. It currently returns false whenever `abIgnoreReserved` is true because the saved actor seat state does not distinguish a reservation from actual use. HST-09 owns that state distinction. |
| GetTriggerObjectCount | Takes no arguments and returns the count of tracked trigger occupants. A non-trigger logs an error and returns zero. This uses HST-14 trigger tracking. |
| SetMotionType, ApplyHavokImpulse | SetMotionType takes the motion type and optional activation flag. ApplyHavokImpulse takes a direction and magnitude. Both validate the request and queue host physics work. Host support is bounded to HST-12's movable references. |
| SetNoFavorAllowed, DamageObject | Their argument and result shapes match their pages. SetNoFavorAllowed updates the shared flag. DamageObject advances supported destruction state. |
| KnockAreaEffect, PushActorAway | Their signatures match the CK pages. Both queue shared actor impulses through CMB-38. KnockAreaEffect uses the documented magnitude and radius with a documented linear attenuation approximation. |
| InterruptCast, ForceAddRagdollToWorld, ForceRemoveRagdollFromWorld, TetherToHorse | InterruptCast clears the shared pending cast. Ragdoll methods set a saved actor override through CMB-38. TetherToHorse moves the horse to an approximate cart-relative position. Host constraint work belongs to HST-15. |
| CreateDetectionEvent | Takes an owner Actor and sound level, then records a shared detection event. |
| TranslateTo, SplineTranslateTo, StopTranslation, Reset | Translation methods queue linear or spline motion and dispatch completion/failure events through HST-16. Reset restores the reference's supported authored state. |
| MusicType.Add, MusicType.Remove | Both are methods with no arguments and no return. They validate the receiver as a live MUSC record and add or remove it from the world's transient active music list. |

The audit found that `MoveTo` ignored `abMatchRotation`. SCR-67 fixes it, and
`scr_67_move_to_respects_match_rotation_and_default` checks the default true
case and the false case. The named `IsInDialogueWithPlayer` page was also
missing from the earlier hand-kept inventory. The registry-derived check now
includes it and the docs record its argument and result contract.

`RemoveAllItems` honors `abKeepOwnership` when transferring stacks. The
`abRemoveQuestItems` policy remains unsupported because the game's stack model
does not classify quest objects for this call. No existing open backlog row
owns that Papyrus removal policy. A dedicated inventory prerequisite row is
needed. This audit does not edit `BACKLOG.csv` under the task boundary.
This remains an explicit implementation gap.

## Native contract audit, SCR-73

`crates/game/tests/scr_73_native_contract_audit.rs` derives the audited
Weapon and Weather entries from the registry by checking each class binding
registered by `natives.rs`. The class-specific inventory has two methods and
no globals: `Weapon.Fire` and `Weather.SetActive`. Both are quiet stubs.

The local `ck-wiki/Function_Reference.wikitext` and named-page corpus contain
no Weapon or Weather Papyrus contracts. The vanilla
`skse64/scripts/vanilla/Weapon.psc` declares `Fire(ObjectReference akSource,
Ammo akAmmo = None)` as an instance method. The registry has the matching
instance binding. `Weather.psc` declares `SetActive(bool abOverride = false,
bool abAccelerate = false)` as an instance method, also matching the registry.
There is no signature or method/global discrepancy for either registered
binding. The PSC comments state that Fire uses a source and optional ammo,
while SetActive sets the active sky weather.

The Weather PSC also declares eight natives absent from the registry:
instance methods `ForceActive` and `GetClassification`, and globals
`ReleaseOverride`, `FindWeather`, `GetCurrentWeather`, `GetOutgoingWeather`,
`GetCurrentWeatherTransition` and `GetSkyMode`. SCR-54 owns adding these
bindings with weather-system behavior. SCR-53 owns replacing the quiet
`Weapon.Fire` stub with weapon-system behavior. The audit does not claim that
either current stub implements the vanilla contract.

## ActorBase and Alias native audit, SCR-55

The inventory below is checked against the registrations made by
`natives::register` in `scr_55_actorbase_alias_inventory_matches_registry`.
`ActorBase` and `Alias` have no registered global bindings. The CK class pages
also say both classes have no global functions.

| Registry binding | CK contract comparison | Disposition |
|---|---|---|
| `ActorBase.GetSkin` | Returns the actor base skin armor. | Reads the record-backed skin. |
| `ActorBase.SetSkin` | Sets the actor base skin armor. | Writes the record-backed skin override. |
| `ActorBase.GetRace` | Returns the actor base race. | Reads its race form. |
| `ActorBase.GetClass` | Returns the actor base class. | Reads its class form. |
| `ActorBase.GetSex` | Returns the actor base sex as an integer. | Reads the female flag as 1 or 0. |
| `ActorBase.GetDeadCount` | Returns the number of dead actors using this base. | Reads the current dead count field. |
| `ActorBase.IsEssential` | Reports whether the base is essential. | Reads the base flag. |
| `ActorBase.IsProtected` | Reports whether the base is protected. | Reads the base flag. |
| `ActorBase.IsInvulnerable` | Reports whether the base is invulnerable. | Reads the base flag. |
| `ActorBase.IsUnique` | Reports whether the base is unique. | Reads the base flag. |
| `ActorBase.SetEssential` | Sets the essential flag. A base cannot be both essential and protected. | Current setter updates the base and initialized actors from that base. Mutual exclusion is not enforced. SCR-55 assigns the correction to SCR-18. |
| `ActorBase.SetProtected` | Sets the protected flag. A base cannot be both protected and essential. | Current setter updates the base and initialized actors from that base. Mutual exclusion is not enforced. SCR-55 assigns the correction to SCR-18. |
| `ActorBase.SetInvulnerable` | Sets the invulnerable flag. | Updates the base flag. |
| `ActorBase.SetOutfit` | Sets the default or sleep outfit according to its boolean argument. | Updates the corresponding base outfit field. |
| `ActorBase.GetGiftFilter` | Returns the actor base gift-filter FormList. | Quiet stub. Implement record-backed value access under SCR-18. |
| `Alias.GetOwningQuest` | Returns the quest owning the alias. | Returns the quest in the alias handle. |
| `Alias.GetID` | SKSE addition. Returns the alias ID. | Returns the alias handle ID. |
| `Alias.GetName` | SKSE addition. Returns the alias's unique name, not its display name. | Reads the alias definition name. |
| `Alias.GetReference` | Returns the ObjectReference held by the alias. | Returns the current alias target. |
| `Alias.GetRef` | Shorthand for `GetReference`. | Returns the same current alias target. |
| `Alias.GetActorReference` | Returns the held reference as an Actor. | Returns the target only when its resolved runtime class is `Actor`, otherwise returns `None`. |
| `Alias.GetActorRef` | Shorthand for `GetActorReference`. | Uses the same actor-type check and return value as `GetActorReference`. |
| `Alias.ForceRefTo` | Forces the specified ObjectReference into the alias. | Stores the provided form in alias state without checking its record type. Assign reference-type validation to SCR-59. |
| `Alias.ForceRefIfEmpty` | Sets the alias only when empty and reports whether it succeeded. | Preserves an occupied target and returns false, or stores and returns true without checking its record type. Assign reference-type validation to SCR-59. |
| `Alias.Clear` | Clears an optional reference alias, logs an error for a required alias, and causes the held actor to reevaluate its package. | Required aliases retain their target and emit a trace diagnostic. Optional aliases are cleared, then the held actor's package stack is reevaluated. |
| `Form.GetFormID` inherited by `ActorBase` | Returns the form's runtime ID. | Reads the resolved form's runtime ID. |
| `Form.GetFormID` inherited by `Alias` | Returns the form's runtime ID. | Registry contains the shared Form method. Alias runtime objects have no resolved form ID. |
| `Form.GetName` inherited by `ActorBase` | Returns the form name. | Reads the winning form name. |
| `Form.GetName` inherited by `Alias` | Returns the form name. Alias declares its own SKSE `GetName`, which shadows this binding. | Alias-specific binding returns the alias definition name. |
| `Form.GetType` inherited by `ActorBase` | Returns the form type code. | Reads the resolved record type. |
| `Form.GetType` inherited by `Alias` | Returns the form type code. | Alias runtime objects have no resolved form type. |
| `Form.HasKeyword` inherited by `ActorBase` | Reports whether the form has the specified keyword. | Queries the form-backed keyword list. |
| `Form.HasKeyword` inherited by `Alias` | Reports whether the form has the specified keyword. | Alias runtime objects do not resolve to forms, so the native returns false. |
| `Form.GetKeywords` inherited by `ActorBase` | Returns the form's keyword array. | Builds an array from the form-backed keywords. |
| `Form.GetKeywords` inherited by `Alias` | Returns the form's keyword array. | Alias runtime objects do not resolve to forms, so the native returns an empty array. |
| `Form.RegisterForSingleUpdate` inherited by `ActorBase` | Registers one update after the given seconds. | The VM object receives one real-time update. |
| `Form.RegisterForSingleUpdate` inherited by `Alias` | Registers one update after the given seconds. | The alias VM object receives one real-time update. |
| `Form.RegisterForUpdate` inherited by `ActorBase` | Registers or changes repeating update seconds. | The VM object receives repeating real-time updates. |
| `Form.RegisterForUpdate` inherited by `Alias` | Registers or changes repeating update seconds. | The alias VM object receives repeating real-time updates. |
| `Form.RegisterForSingleUpdateGameTime` inherited by `ActorBase` | Registers one update after the given game hours. | The VM object receives one game-time update. |
| `Form.RegisterForSingleUpdateGameTime` inherited by `Alias` | Registers one update after the given game hours. | The alias VM object receives one game-time update. |
| `Form.RegisterForUpdateGameTime` inherited by `ActorBase` | Registers or changes repeating update game hours. | The VM object receives repeating game-time updates. |
| `Form.RegisterForUpdateGameTime` inherited by `Alias` | Registers or changes repeating update game hours. | The alias VM object receives repeating game-time updates. |
| `Form.UnregisterForUpdate` inherited by `ActorBase` | Cancels repeating and single real-time updates. | Removes real-time update registration. |
| `Form.UnregisterForUpdate` inherited by `Alias` | Cancels repeating and single real-time updates. | Removes the alias's real-time update registration. |
| `Form.UnregisterForUpdateGameTime` inherited by `ActorBase` | Cancels game-time updates. | Removes game-time update registration. |
| `Form.UnregisterForUpdateGameTime` inherited by `Alias` | Cancels game-time updates. | Removes the alias's game-time update registration. |
| `Form.RegisterForSleep` inherited by `ActorBase` | Registers for player sleep start and stop events. | Adds the VM object to the sleep listener set. |
| `Form.RegisterForSleep` inherited by `Alias` | Registers for player sleep start and stop events. | Adds the alias VM object to the sleep listener set. |
| `Form.UnregisterForSleep` inherited by `ActorBase` | Unregisters from player sleep events. | Removes the VM object from the sleep listener set. |
| `Form.UnregisterForSleep` inherited by `Alias` | Unregisters from player sleep events. | Removes the alias VM object from the sleep listener set. |
| `Form.RegisterForTrackedStatsEvent` inherited by `ActorBase` | Registers for tracked-stat updates. | Adds the VM object to the tracked-stat listener set. |
| `Form.RegisterForTrackedStatsEvent` inherited by `Alias` | Registers for tracked-stat updates. | Adds the alias VM object to the tracked-stat listener set. |
| `Form.UnregisterForTrackedStatsEvent` inherited by `ActorBase` | Unregisters from tracked-stat updates. | Removes the VM object from the listener set. |
| `Form.UnregisterForTrackedStatsEvent` inherited by `Alias` | Unregisters from tracked-stat updates. | Removes the alias VM object from the listener set. |
| `Form.RegisterForAnimationEvent` inherited by `ActorBase` | Registers for a named animation event on a sender. | Adds a listener for the sender and event name. |
| `Form.RegisterForAnimationEvent` inherited by `Alias` | Registers for a named animation event on a sender. | Adds the alias VM object as listener. |
| `Form.UnregisterForAnimationEvent` inherited by `ActorBase` | Removes a named animation event registration. | Removes matching listener entries. |
| `Form.UnregisterForAnimationEvent` inherited by `Alias` | Removes a named animation event registration. | Removes matching alias listener entries. |
| `Form.RegisterForLOS` inherited by `ActorBase` | Registers for gain and lost line-of-sight events for a viewer and target. | Adds a persistent viewer-target listener. |
| `Form.RegisterForLOS` inherited by `Alias` | Registers for gain and lost line-of-sight events for a viewer and target. | Adds a persistent alias listener. |
| `Form.RegisterForSingleLOSGain` inherited by `ActorBase` | Registers for one line-of-sight gain event. | Adds a single-gain listener. |
| `Form.RegisterForSingleLOSGain` inherited by `Alias` | Registers for one line-of-sight gain event. | Adds a single-gain alias listener. |
| `Form.RegisterForSingleLOSLost` inherited by `ActorBase` | Registers for one line-of-sight lost event. | Adds a single-lost listener. |
| `Form.RegisterForSingleLOSLost` inherited by `Alias` | Registers for one line-of-sight lost event. | Adds a single-lost alias listener. |
| `Form.UnregisterForLOS` inherited by `ActorBase` | Unregisters matching line-of-sight events. | Removes matching viewer-target listener entries. |
| `Form.UnregisterForLOS` inherited by `Alias` | Unregisters matching line-of-sight events. | Removes matching alias listener entries. |

Contract sources are `ck-wiki/ActorBase_Script.wikitext`,
`ck-wiki/Alias_Script.wikitext`, `ck-wiki/ReferenceAlias_Script.wikitext`,
`ck-wiki/GetClass_-_ActorBase.wikitext`,
`ck-wiki/GetOwningQuest_-_Alias.wikitext`, and
`ck-wiki/Clear_-_ReferenceAlias.wikitext`. The class pages define the remaining
methods listed there. Alias reference getter and setter variants are compared
against the ReferenceAlias class page, which includes their return types and
shorthand relationships. The current check is synthetic because these methods
do not require installed game records.

## Cell and Clear native contracts, SCR-56

The registry-derived inventory is tested by
`scr_56_registry_inventory_matches_ck_contracts`. It includes every registered
method whose class is `Cell` and every method or global whose name is exactly
`Clear`. There are no matching global bindings. Contract statements below
come from `engine-refs/ck-wiki` and the shipped
`skse64/scripts/vanilla` declarations.

| Binding | CK contract comparison | Runtime result or owning row |
|---|---|---|
| `Cell.GetActorOwner` | `GetActorOwner_-_Cell.wikitext` returns the cell's actor base owner and states that inherited ownership is separate. | Returns only an `NPC_` owner from the cell's `XOWN` state. |
| `Cell.GetFactionOwner` | `Cell_Script.wikitext` says it returns the owning faction. | Returns only a `FACT` owner from the cell's `XOWN` state. |
| `Cell.IsInterior` | `Cell_Script.wikitext` asks whether the cell is an interior. | Uses absence of a worldspace in the record placement. |
| `Cell.IsAttached` | `Cell_Script.wikitext` and `skse64/scripts/vanilla/Cell.psc` declare a boolean attached-state query. | Quiet true stub does not track attachment. SCR-19 owns the live attachment query. |
| `Cell.Reset` | `Cell_Script.wikitext` says it flags the cell for reset on next load. | Quiet no-op stub. SCR-19 owns cell reset behavior. |
| `Cell.SetActorOwner` | `SetActorOwner_-_Cell.wikitext` and `Cell.psc` take an `ActorBase`. `None` clears the owner. | Stores only an `NPC_` record or clears on `None`. Other types are ignored. |
| `Cell.SetFactionOwner` | `Cell_Script.wikitext` and `skse64/scripts/vanilla/Cell.psc` declare a `Faction` argument. | Stores only a `FACT` record or clears on `None`. Other types are ignored. |
| `Cell.SetPublic` | `Cell_Script.wikitext` declares a boolean public/private setting. `Cell.psc` gives the argument a default of true. | Writes the supplied boolean, defaulting a missing argument to true. |
| `Alias.Clear`, `ReferenceAlias.Clear` | `Clear_-_ReferenceAlias.wikitext` clears the target, errors for a non-optional alias, and reevaluates packages. | Registered on both classes because alias method inheritance is not resolved for ReferenceAlias dispatch. Validates Optional, clears the runtime alias override and reevaluates the former target package. |

The Cell method comparison has eight bindings. The exact-name `Clear` comparison
has two method bindings and no global binding. The setter type discrepancy is
corrected by checking resolved record tags before changing `XOWN`.

## Faction and ForceRefIfEmpty native contracts, SCR-58

The registry inventory is checked against `natives::register` by
`scr_58_faction_registry_matches_the_contract_inventory` and
`scr_58_force_ref_if_empty_is_scoped_to_reference_alias`. CK contracts below
come from `ck-wiki/Faction_Script.wikitext` and
`ck-wiki/ReferenceAlias_Script.wikitext`. The function pages add no different
semantics beyond their declared signatures and summaries.

| Registry binding | CK contract comparison | Runtime result or owning row |
|---|---|---|
| `Faction.SetAlly` | Takes a faction and optional boolean, making the other faction an ally when true. | Updates the relation entry for that faction. |
| `Faction.SetEnemy` | Takes a faction and optional boolean, making the other faction an enemy when true. | Updates the relation entry for that faction. |
| `Faction.GetReaction` | Takes a faction and returns the relation code. | Reads the stored reaction, defaulting to neutral. |
| `Faction.SetReaction` | Takes a faction and integer relation code. | Stores the relation code. |
| `Faction.GetCrimeGold` | Returns total crime gold owed to this faction. | Sums violent and nonviolent bounty. |
| `Faction.GetCrimeGoldViolent` | Returns violent crime gold owed. | Reads the violent bounty field. |
| `Faction.GetCrimeGoldNonViolent` | Returns nonviolent crime gold owed. | Reads the nonviolent bounty field. |
| `Faction.SetCrimeGold` | Sets total crime gold owed. | Updates the total bounty value. |
| `Faction.SetCrimeGoldViolent` | Sets violent crime gold owed. | Updates the violent bounty field. |
| `Faction.ModCrimeGold` | Modifies crime gold by an amount and optional violent flag. | Applies the amount to the selected bounty. |
| `Faction.GetInfamy` | Returns total infamy for the player in this faction. | Sums violent and nonviolent infamy. |
| `Faction.GetInfamyViolent` | Returns violent infamy. | Reads the violent infamy field. |
| `Faction.GetInfamyNonViolent` | Returns nonviolent infamy. | Reads the nonviolent infamy field. |
| `Faction.CanPayCrimeGold` | Reports whether the player can pay this faction's crime gold. | Checks player gold against the recorded bounty. |
| `Faction.PlayerPayCrimeGold` | Pays crime gold, with optional stolen-item removal and jail transfer. | Updates bounty and queues the supported payment effects. |
| `Faction.IsPlayerExpelled` | Reports whether the player is expelled from this faction. | Reads the faction state. |
| `Faction.SetPlayerExpelled` | Sets whether the player is expelled. | Updates the faction state. |
| `Faction.SetPlayerEnemy` | Sets whether the player is an enemy of this faction. | Updates the faction state. |
| `Faction.IsFactionInCrimeGroup` | Takes another faction and reports whether it belongs to this faction's crime group. | Resolves `CRGR` to its linked `FLST` and checks membership. |
| `Faction.SendAssaultAlarm` | Finds a nearby NPC in this faction and has them behave as if assaulted. | Selects the nearest built faction actor and records a player assault through the crime system. |
| `Faction.SendPlayerToJail` | Sends the player to this faction's jail, with inventory and real-jail options. | Serves the faction's bounty and marks the player arrested. Jail travel and belongings storage are not implemented. |
| `Faction.ModReaction` | Modifies this faction's reaction toward another faction. | Saturating-adds the signed amount in the requested direction. |
| `ReferenceAlias.ForceRefIfEmpty` | Takes an object reference and fills the alias only if it is empty, returning whether it succeeded. | Preserves an occupied alias. An empty alias accepts the form. SCR-59 owns reference-type validation. |

The inventory tests cover 22 implemented `Faction` methods and the single
`ReferenceAlias.ForceRefIfEmpty` method. There are no
global bindings for these names. No per-reference package reevaluation is
claimed for alias assignment.
`scr_56_cell_owner_setters_require_the_ck_record_type` covers both accepted
record tags, cross-type rejection, reference rejection and `None`. The
remaining gaps are already assigned to their feature rows above.

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
| [SkyCraft](https://github.com/chasmlol/SkyCraft) | MIT. An SKSE plugin on CommonLibSSE-NG that renders a hidden Minecraft server inside Skyrim, reviewed at `bfcaf17` (2026-10-01). Gameplay ideas only: the terrain material lookup, stagger direction, light flicker and weapon skill use were ported on 2026-10-03, each checked against corpus sources first. Its Address Library IDs and hit pipeline offsets come from the binary, so none of that reaches the engine. |

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

## SCR-65 binding contract inventory

The integration test builds candidates from the script contract inventory,
queries the native registry for each method and checks that no candidate is
registered as a global. Every binding below is a method. Vanilla SKSE scripts
are the available declaration source for LeveledSpell and Location. The
modified LeveledSpell script supplies the extra getters and setter.

| Binding | Contract comparison |
|---|---|
| `LeveledItem.AddForm` | Vanilla SKSE takes a form, level and count. The native appends one entry with those values. |
| `LeveledItem.Revert` | Vanilla SKSE removes script-added forms. The native truncates to the base entry count and resets chance none. |
| `LeveledItem.GetNumForms` | Modified SKSE returns the entry count. The native returns the list length. |
| `LeveledItem.GetNthForm` | Modified SKSE returns the entry at the requested index or None. The native does the same. |
| `LeveledItem.GetNthLevel` | Modified SKSE returns the entry level or zero for an invalid index. The native does the same. |
| `LeveledItem.SetNthLevel` | Modified SKSE updates an existing entry and caps the level at 65535. The native does the same. |
| `LeveledItem.GetNthCount` | Modified SKSE returns the entry count or zero for an invalid index. The native does the same. |
| `LeveledItem.GetChanceNone` | Modified SKSE returns the list chance-none byte. The native returns that field. |
| `LeveledItem.SetChanceNone` | Modified SKSE writes the supplied byte to the list chance-none field. The native stores its low byte. |
| `LeveledActor.AddForm` | Vanilla SKSE takes an actor base, level and count. The native appends one entry with those values. |
| `LeveledActor.Revert` | Vanilla SKSE removes script-added forms. The native truncates to the base entry count and resets chance none. |
| `LeveledActor.GetNumForms` | Modified SKSE returns the entry count. The native returns the list length. |
| `LeveledActor.GetNthForm` | Modified SKSE returns the entry at the requested index or None. The native does the same. |
| `LeveledActor.GetNthLevel` | Modified SKSE returns the entry level or zero for an invalid index. The native does the same. |
| `LeveledActor.SetNthLevel` | Modified SKSE updates an existing entry and caps the level at 65535. The native does the same. |
| `LeveledActor.GetNthCount` | Modified SKSE returns the entry count or zero for an invalid index. The native does the same. |
| `LeveledActor.GetChanceNone` | Modified SKSE returns the list chance-none byte. The native returns that field. |
| `LeveledActor.SetChanceNone` | Modified SKSE writes the supplied byte to the list chance-none field. The native stores its low byte. |
| `LeveledSpell.AddForm` | Vanilla SKSE takes a form and level only. The native appends one spell entry with count one. |
| `LeveledSpell.Revert` | Vanilla SKSE removes script-added forms. The native truncates to the base entry count and resets chance none. |
| `LeveledSpell.GetNumForms` | Modified SKSE returns the entry count. The native returns the list length. |
| `LeveledSpell.GetNthForm` | Modified SKSE returns the entry at the requested index or None. The native does the same. |
| `LeveledSpell.GetNthLevel` | Modified SKSE returns the entry level or zero for an invalid index. The native does the same. |
| `LeveledSpell.SetNthLevel` | Modified SKSE updates an existing entry and caps the level at 65535. The native does the same. |
| `LeveledSpell.GetChanceNone` | Modified SKSE returns the list chance-none byte. The native returns that field. |
| `LeveledSpell.SetChanceNone` | Modified SKSE writes the supplied byte to the list chance-none field. The native stores its low byte. |
| `Location.GetKeywordData` | Shipped `Location.psc` returns a keyword's float data. The native returns zero when no value is stored. |
| `Location.SetKeywordData` | Shipped `Location.psc` stores a keyword float. The native updates the location's value. |
| `Location.IsCleared` | Shipped `Location.psc` returns the cleared flag. The native returns it. |
| `Location.SetCleared` | Shipped `Location.psc` sets the flag and defaults the argument to true. The native does the same. |
| `Location.IsChild` | Shipped `Location.psc` checks whether the argument is a descendant of self. The native does so. |
| `Location.HasCommonParent` | Shipped `Location.psc` checks shared ancestors and optionally filters by keyword. The native does so. |
| `Location.IsSameLocation` | Shipped wrapper checks identity, filtered shared ancestors and a keyworded parent/child in either direction. The native now does so. |
| `Location.IsLoaded` | Shipped `Location.psc` asks whether the location is loaded. This remains a false quiet stub, assigned to SCR-35. |
| `Location.HasRefType` | Shipped `Location.psc` asks whether the location has the reference type. This remains a false quiet stub, assigned to SCR-35. |
| `Location.GetRefTypeAliveCount` | Shipped `Location.psc` returns the alive count for a reference type. This remains a zero quiet stub, assigned to SCR-35. |
| `Location.GetRefTypeDeadCount` | Shipped `Location.psc` returns the dead count for a reference type. This remains a zero quiet stub, assigned to SCR-35. |

`LeveledSpell.GetNthCount` is not part of either shipped LeveledSpell script,
so it is not registered. This corrected an accidental binding shared from
LeveledItem and LeveledActor.

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
