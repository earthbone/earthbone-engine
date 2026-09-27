# Physical melee execution

The combat-hits worktree builds on the frozen 63-file NPC and death-lifecycle
review. Game::melee_hit accepts a host-confirmed ordinary melee contact and
settles its scripts. World::melee_hit retains the queued events for save/load
or callers that control VM scheduling. Neither entry point performs collision
detection or selects a hand. The caller identifies the equipped weapon.

## Record and runtime inputs

Weapon DATA supplies base damage, and DNAM supplies animation, governing
skill, reach, speed and resistance/behavior flags. Governing skill is read
independently from animation, so a sword animation can still use another
supported weapon skill. Armor DNAM supplies the rating in hundredths and
BOD2/BODT supplies its occupied slots and armor skill. UESP documents the
runtime's low-16-bit armor rating. Transient views read the existing winning
record database. No parallel authoritative item store was added.

The weapon must be present in the attacker's inventory and equipped. Armor
must also remain present. Equipping armor replaces previously equipped armor
that overlaps one of the fourteen normal BOD2 biped slots. Other equipment and
unsupported BOD2 bits remain unchanged during equip resolution. Any overlap,
including unsupported bits, still rejects the combat contact.
Malformed armor or an armor type above clothing rejects the equip operation and
combat contact. A legacy BODT record without an armor type remains equippable,
then rejects the combat contact because no armor skill is available.
Clothing does not add armor rating or the hidden category bonus. Item stacks
retain their existing saved metadata, and unsupported instance modifiers reject
the contact. Existing actor values supply skills and the non-equipment
DamageResist contribution. Equipped armor ratings are computed for the hit,
not written over that actor value. Native GetActorValue(DamageResist) does not
yet include the computed equipment contribution.

Player and NPC skill scaling use their separate GMST minimum/maximum values.
Armor uses the player's or NPC's maximum armor-skill coefficient. Reduction
includes the hidden bonus for occupied head, body, hands, feet and shield
categories and the configured armor cap. Runtime SetGameSetting overrides
are read on every hit. Absent combat GMSTs use the documented engine defaults
through the shared game-setting lookup, so Papyrus sees the same defaults. Plugin records and
runtime overrides take precedence. Malformed, negative or non-finite settings
reject the hit.

The vanilla base-game unarmed attack form `0:0001F4` selects an unarmed
contact. It is still decoded as a live WEAP record, so a missing, deleted,
wrong-type, malformed or unsupported modifier override rejects the hit. The
form needs no inventory entry or equipped hand. Hand selection belongs to the
host contact bridge. Its base damage is the current UnarmedDamage actor value,
seeded from the winning RACE DATA through the actor's traits template source.
This preserves actor-value changes and saved state without a second combat
value store. Unarmed damage does not use weapon skill scaling or
`fDamageWeaponMult`. Missing or malformed race data rejects the hit before
health or events change.

AttackDamageMult scales the attacker's ordinary weapon or unarmed damage.
Its shared actor-value default is 1. Base, permanent, temporary and damage
components all contribute. An explicit zero remains zero through save/load.
The target's AttackDamageMult does not change incoming damage. Negative or
overflowing multipliers reject before health or events change. Armor,
difficulty and the existing weapon/equipment validation still apply.

The current path is ordinary melee at a selected difficulty. It defaults to
Adept and persists the selection in SOEW version 18. Its separate damage
to/from player settings are read on every contact, while NPC-to-NPC contacts
do not use those multipliers. Novice uses 2.0 damage by the player and 0.5 to
the player. Apprentice uses 1.5 and 0.75. Adept uses 1.0 and 1.0. Expert uses
0.75 and 1.5. Master uses 0.5 and 2.0. Legendary uses 0.25 and 3.0. Plugin
records and runtime overrides for the selected GMST pair take precedence over
these defaults. It rejects all perked actors, enchantments, item poison and
unsupported weapon skills/types. Tempered item quality scales damage instead
of rejecting, CMB-10. A weapon's resistance flags scale damage instead of
rejecting too, CMB-26: only a reserved DNAM Resist index still rejects. These
are open execution requirements, not silently omitted modifiers. Protected
and essential targets are rejected pending bleedout handling. Ghosts and dead
actors are rejected.
An essential attacker can still attack an ordinary target.

Weapon `CNAM` and armor `TNAM` template links resolve through their source
record's load-order context until a terminal template supplies the combat
fields. A child only needs a well-formed template or enchantment link. It may
omit DATA, DNAM and BOD2/BODT, so those fields are decoded only from the
terminal record. Each link remains a form lookup, so templates do not create a
parallel item store. Cycles, unresolved links, wrong record types and
enchantments anywhere in the chain reject the contact before it changes health
or queues an event.

TES5Edit and UESP identify the links and field layouts, while CommonLibSSE
exposes the template pointers. None of these local sources states a
field-by-field merge mask for WEAP or ARMO. This path therefore treats the
terminal referenced form as the combat source for CMB-11 instead of merging
child fields. The synthetic cross-plugin fixtures cover weapon damage, armor
rating, slots, skill and weapon behavior fields from that terminal record.
An EITM link on a child or terminal record still rejects the contact, pending
weapon enchantment execution.

## Application and events

The calculation prepares a changed health component before emitting events.
A rejected hit does not apply damage or queue events. Lazy reference/actor
initialization may still populate record-derived state, and that state is
part of saved state: `World::actor_base` caches into the same
`actor_bases` map that natives such as `SetProtected` and `SetEssential`
write, and `World::save_state` serializes the whole map unconditionally
(`crates/game/src/world/save.rs`). A rejected first melee hit that reads
`protected`, `essential` or `invulnerable` through `actor_base` therefore
persists that actor's base into `save_state` even though the hit itself is
never applied, the same contract crafting already documents for rejected
transactions that still read records ("Crafting execution, first
increment" in `docs/game.md`). There is no separate rejected-only cache to
omit from persistence, and every other lazy World cache (globals, leveled
lists, factions, cells) follows the identical read-through, save-whole-map
pattern, so carving out actor bases alone would be an inconsistent special
case rather than a fix. Accepted contacts queue
OnHit to the actor, its running aliases and active-effect scripts. Ordinary
melee reports the selected weapon, a None projectile and four false special-hit
flags. A lethal hit then uses the existing death transition with the attacker
as killer. Deferred kills retain that attribution until EndDeferredKill.

The existing event queue and actor-value serialization retain pending hits and
applied damage in SOEW version 15. Loading and delivering an undelivered hit
does not apply damage again. No save-format change, dependency or host node was
introduced by this increment.

## Sources

Local sources read before implementation:

- TES5Edit/Core/wbDefinitionsTES5.pas WEAP, ARMO and RACE definitions.
- TES5Edit/Core/wbDefinitionsTES5.pas GMST definitions. The checkout defines
  record structure but does not include Skyrim's engine-default difficulty
  values.
- skymp5-client/src/services/services/ragdollService.ts, which names all six
  `fDiffMultHPToPC` difficulty GMST suffixes.
- CommonLibSSE `TESObjectWEAP::templateWeapon` and
  `TESObjectARMO::templateArmor`, which name the runtime pointers for `CNAM`
  and `TNAM`.

- TES5Edit/Core/wbDefinitionsTES5.pas WEAP and ARMO definitions.
- TES5Edit/Core/wbDefinitionsTES5.pas BOD2/BODT biped mask layout and armor type.
- CommonLibSSE include/RE/B/BipedObjects.h and BGSBipedObjectForm.h for the
  fourteen normal biped slots and light, heavy and clothing type values.
- uesp-modfileformat/WEAP.wikitext and ARMO.wikitext, including the distinction
  between animation type, governing skill and equip slot.
- ck-wiki/OnHit_-_ObjectReference.wikitext for event arguments and actor
  projectile behavior, and EquipItem/GetEquippedWeapon for equipment contracts.
- skymp TES5DamageFormula.cpp, TES5DamageFormulaTest.cpp and ActionListener.cpp.
  The unarmed source form resolves to the attacker's RACE unarmed damage and
  bypasses inventory ownership. Their weapon, armor, block, sneak and
  difficulty TODOs prevent adopting that implementation as a complete Skyrim
  formula. Its GMST-based armor cap is a comparison point.

Online sources were consulted after local sources lacked the skill formulas:

- [Difficulty mod author's setting examples](https://www.nexusmods.com/skyrim/mods/42352?tab=description)
  identifies the Adept damage-to/from-player settings and their effect.
- [SkyEdit author on absent GMSTs](https://www.gamesas.com/skyedit-alpha-t203912.html)
  documents engine defaults omitted from Skyrim.esm and confirms that a plugin
  GMST with the same editor ID overrides them.
- [Vigor author's measured NPC defaults](https://www.nexusmods.com/skyrimspecialedition/mods/38075?tab=posts)
  identifies NPC weapon skill minimum 1 and maximum 3.
- [Measured skill settings](https://www.gamesas.com/known-damage-formula-yet-t263175.html)
  records a mod author's tests with changed skill-min/max settings and separate
  player/NPC coefficients.
- [Armor settings](https://www.gamesas.com/quick-how-see-armor-t201770.html)
  supplies the armor-skill interpolation and reduction settings.
- [SkyTweak author's armor explanation](https://www.gamesas.com/global-increase-armor-rating-t332362.html)
  distinguishes the hidden category bonus from displayed armor rating.
- [Armor formula plugin author](https://www.nexusmods.com/skyrimspecialedition/mods/35296?tab=description)
  identifies the five worn armor categories. Its replacement reduction formula
  was not adopted.

These reports guide this implementation but are not an original-runtime
comparison performed by this project. Floating-point rounding and behavior
with unusual modded settings remain unverified against vanilla.

## Validation

The initial melee checkpoint passed tools/lint.sh with 1,163 Rust tests and 11 Python
tests. All optional gate tools ran, and no warnings were reported. Its separate
installed-data run passes 12 melee tests and 11 death-lifecycle tests.

The five-master survey decodes 3,262 winning weapon records and 3,675 armor
records. It executes 184 ordinary weapons against synthetic actors. It excludes
489 weapons for type or governing skill and 2,589 for templates or enchantments.
Real armor records are decoded but not exercised as equipped items by this
survey. Synthetic fixtures verify armor scaling, hidden categories and the cap.
The death survey initializes 6,361 actors, records 6,018 deaths and retains
343 essential actors, then verifies persistence.

Synthetic tests also cover live settings overrides, player/NPC coefficients,
Adept through Legendary damage multipliers, absent engine defaults, rejected malformed settings,
record truncation, stale equipment, ordered hit/death delivery and deferred
killer attribution across engine save/load. Event argument values and hit
notifications to aliases or active effects lack dedicated hit-path assertions.
No test in this increment performs host collision detection or renders combat.
The installed-data unarmed survey executes the canonical source form against
161 winning RACE records through synthetic actor references. The integration
regression combines unarmed damage, templated armor, Legendary difficulty and
engine save/load. Ordinary weapons retain their shared template resolver.

## Remaining execution work

Host attack input, timing/contact detection, hand-specific selection, ranged
weapons/ammunition, dual-wield and bash power
attacks, weapon-only blocking, perk entry points, enchantments, poison, bleedout,
stagger, crime and combat AI remain unfinished. The API is an execution
increment and does not establish playable combat or completion of the broader
gameplay-foundations goal.

## Sneak attack multipliers, CMB-06

An ordinary melee hit is a sneak attack when its attacker is currently
sneaking and the target's CMB-31 awareness of that attacker is not Combat.
No awareness entry means the target has not detected the attacker. Combat
awareness suppresses the multiplier even if the attacker remains in the
sneaking state. Bashes do not use this weapon multiplier.

The multiplier is read from the load order's GMST using the WEAP animation
type: hand-to-hand uses `fCombatSneakHandMult`, one-handed swords, daggers,
axes and maces use their corresponding `fCombatSneak1H...Mult` setting, and
two-handed swords and axes use `fCombatSneak2H...Mult`. The xEdit animation
enum in `TES5Edit/Core/wbDefinitionsTES5.pas` defines these WEAP categories.
Wrye Bash's `GmstTweak_Combat_StealthDamageBonus` identifies the dagger, bow
and crossbow sneak settings. The installed Skyrim.esm contains the six melee
settings used here. Missing or invalid selected settings reject the hit before
damage is committed. The multiplier scales base weapon damage before the
existing actor-value, quality, armor, block, resistance and difficulty terms.

The CK `OnHit_-_ObjectReference.wikitext` defines `abSneakAttack` as whether
the hit was caused by a sneak attack. `OnHit` now receives the same result as
`HitResult::sneak_attacked`. Ranged sneak multipliers remain outside this
melee-only path.

## Block damage reduction, CMB-05

Shield block reduces incoming ordinary melee damage when the target holds a
block stance and has armor occupying the shield BOD2 slot, bit 9. The stance
comes from `World::set_blocking`, a host-driven flag on `ActorState` set from
the animation graph, not from the plugin database. The shield itself, its
DNAM rating and the target's Block actor value are the record-backed inputs,
resolved through the same armor-record and template path CMB-01 already
established for equipped armor.

The reduction follows the shield term UESP's Block page and its talk archive
describe: `fShieldBaseFactor + fShieldScalingFactor / 100 * shieldRating *
(fBlockSkillMult - (100 - blockSkill) / 100)`, capped at `fBlockMax` and
clamped to `[0, 1]`, mirroring the read/resolve/scale/clamp shape CMB-01 uses
for armor. It stacks with armor reduction multiplicatively,
`(1 - armor_reduction) * (1 - block_reduction)`, rather than by addition.
Vanilla's exact ordering is not stated in any local or online source found,
so multiplicative stacking is this increment's design choice, chosen because
it cannot itself push total reduction past 100 percent and matches how
Skyrim layers other percentage-based reductions. `HitResult` reports both
`blocked` and `block_reduction` for callers that need the breakdown. The
`OnHit` event's `abHitBlocked` argument, previously hardcoded false, now
carries the same flag.

The Block actor value and the equipped shield already round-trip through the
existing generic actor-value store and equipment set, so no SOEW change was
needed for the record-backed multipliers to persist. The block stance flag
itself is intentionally not part of saved state. It is analogous to the
animation-driven `alerted`/`sneaking`/`weapon_drawn` flags that are, but
adding it to the fixed-position, version-gated `write_actor_state`/
`read_actor_state` sequence would need a SOEW version bump this increment
did not attempt, and blocking, unlike those flags, has no meaning unless the
host actively re-asserts it after load from the live animation graph anyway.

Open gaps, not attempted by this increment:

- Blocking with a weapon and no shield. UESP's talk archive names
  `fBlockWeaponBase`/`fBlockWeaponScaling` but flags the formula as
  possibly reflecting an engine bug (it appears to use the attacker's
  weapon damage rather than the defender's), so it was not ported.
- The Shield Wall perk family and other block-related perks. Perked actors
  are already globally rejected by `validate_combat_actor`, ahead of and
  independent from this row.
- Fortify Block enchantments and potions.
- `fBlockPowerAttackMult`. `MeleeHit.is_power_attack` exists now (CMB-21),
  but `combat_block`'s reduction formula still does not read it, so no
  attenuation term applies it yet.
- Bash attacks (`abBashAttack`), which read BIDS/BAMT rather than the
  block-reduction path and are unrelated to this row's done check.
- `fBlockMax`'s value. UESP's talk archive gives 0.7 as the documented
  engine default, and this increment uses that value, but its own testers
  report an observed cap closer to 85 percent with no written explanation
  for the gap. The documented default is used here rather than the
  observed number, pending an original-engine measurement.

Sources read for this increment, beyond the CMB-01 sources this section
builds on:

- [UESP Skyrim:Block](https://en.uesp.net/wiki/Skyrim:Block) for the shield
  base and scaling factors and the general shape of the formula.
- [UESP Skyrim talk:Block, Archive 3](https://en.uesp.net/wiki/Skyrim_talk:Block/Archive_3)
  for the full game-setting default values and the community-derived
  formula, including the weapon-block ambiguity and the `fBlockMax`
  discrepancy noted above.
- `uesp-modfileformat/ARMO.wikitext` lines 64 to 68 confirm ETYP, BIDS and
  BAMT are shield-only fields. BIDS and BAMT govern bash impact sound and
  material, not the damage-reduction magnitude this row implements, so they
  were not added to the `Armor` record view.
- `CommonLibSSE-NG/include/RE/B/BGSBipedObjectForm.h` names the shield biped
  slot as bit 9 (`kShield`), matching the bit `combat_armor` already reads
  into its hidden category bonus.
- `ck-wiki/OnHit_-_ObjectReference.wikitext` for the `abHitBlocked` event
  argument position.

## Authored contact bridge, CMB-03

`World::equip_hand_weapon` assigns ordinary weapons to explicit left or right
slots in shared actor state. WEAP ETYP resolves EQUP parent choices, with
left/right roots read from DOBJ LHEQ/RHEQ entries in load order. EQUP DATA
selects all parents or one parent. Animation type does not determine hand
occupancy. Both-hand selections occupy the right slot and clear the left.
Equipping one base form in both hands requires two inventory copies.
Malformed or cyclic slot links reject before changing equipment.

SOEW version 18 appends difficulty and then a sorted hand-selection table after
record overrides. The world hash includes both. Versions 4 to 15 load at Adept
with unassigned hands. Version 16 retains its difficulty and has unassigned
hands. Version 17 retains its hands and defaults to Adept. Version 18 retains
both fields. The integration checks all four extension layouts and rejects
invalid difficulty bytes before replacing world state.
This does not infer item-instance selection from a form-only equipment set.
Poison and enchantment continue to reject as described above. Tempered item
quality reads instead of rejecting, CMB-10, but the same form-only equipment
set still cannot name which physical stack a hit swung: when an item's
stacks disagree on quality, `check_combat_stack` rejects rather than guess.

Hand selection and armor slot replacement use the same template resolution
as hit calculation. Terminal weapon ETYP supplies the slot choices. The child
item identity remains equipped and is the contact source. Integration tests
exercise a fieldless weapon through hand selection and contact, and a fieldless
armor through overlapping-slot replacement.

The host converts decoded HitFrame, preHitFrame, weaponSwing and attackStop
annotations to the existing graph trigger representation. Authored triggers
are retained, exact duplicates are omitted, and the clip's disable-annotation
flag is honored. SkinnedActorNode emits named graph events after installing
their corresponding pose.

`MeleeContact` captures the selected hand weapon at swing start. HitFrame
consumes its one contact opportunity, including on a miss or rejected hit.
Earlier markers cannot apply damage. Equipment changes reject stale swings,
and attackStop cancels pending contact. A successful query calls the shared
Game::melee_hit, retaining its validation and event delivery.

The Godot adapter queries a supplied world-space weapon segment through
PhysicsDirectSpaceState3D. It excludes registered attacker bodies and maps
the first body RID to an actor FormID. An unmapped body obstructs the swing.
PlayerNode exposes registration, start, cancellation and animation-contact
entry points. Cell replacement clears registrations and cancels the swing.
Collision owners must unregister bodies before freeing them. No dependency or
per-reference node is added.

The CMB-03 tests exercise the decoded annotation through the behavior
interpreter into Game and assert the exact frame, hand and single damage
application. They also cover misses, cancellation, equipment changes, rejected
damage, slot-parent cycles, animation-independent hand occupancy and save/load.
These tests use synthetic records and query results. A live Godot swing and
installed attack-animation corpus have not been exercised. CMB-28 owns the
scene wiring that supplies weapon segments and registered bodies. CMB-04 owns
device input and depends on both CMB-03 and CMB-28.

Additional local sources read for this increment:

- TES5Edit/Core/wbDefinitionsTES5.pas EQUP and DOBJ definitions.
- uesp-modfileformat/WEAP.wikitext, EQUP.wikitext and DOBJ.wikitext for hand
  occupancy, parent choices and sparse default-object overrides.
- ck-wiki/GetEquippedWeapon_-_Actor.wikitext for hand identity.
- Project-New-Reign---Nemesis-Main animation event tables for separate
  weaponSwing, preHitFrame, HitFrame and attackStop timestamps.
- godot-docs/classes/class_physicsdirectspacestate3d.rst for first-contact
  ray results, RID identity and exclusions.

### Live contact prerequisites

CMB-28 requires a central owner for actor collision RIDs and pose-derived
weapon segments. The current SkinnedActorNode is a locomotion demo without a
shared actor identity. Loaded actor state alone does not prove cell residency.
The live contact owner must derive residency and placement from the shared Game,
advance an authored attack in the physics step, and cancel on location changes.

CMB-29 exposes and validates character controller metadata first. The local
serde-hkx hkbCharacterDataCharacterControllerInfo declaration places height,
radius and filter information at offsets 0, 4 and 8 within character data's
controller field. Nemesis default male and female XML contain dimensions 1.7
and 0.4. Those declarations do not establish host units, cap-height semantics
or Skyrim's active hit hull. The row requires that evidence before creating
contact shapes. CommonLib weapon attachment names identify a transform rather
than a blade endpoint. Weapon segment construction also needs a verified axis
and the record-defined reach, or a declared supported authored collision shape.

CMB-30 installs obstruction bodies with each loaded location. PlayerNode
currently creates SimpleWorld movement collision. CellStreamer's Jolt static
installation has a separate entry point that normal startup and location
replacement do not call. Actor registration alone would therefore permit
contact through walls. Static and actor query resources must follow the same
location lifecycle, with owned RIDs released on replacement.

## Bleedout entry and recovery, CMB-13

The threshold query reads the selected STATS template source through the
existing actor-template resolver. ACBS Bleedout Override supplies absolute
health. Otherwise CLAS DATA's Bleedout Default multiplies permanent health,
which includes the base and permanent modifier, without damage or temporary
effects. A missing class link uses fBleedoutDefault. An invalid class record
returns an error. Zero is a valid threshold.

The local xEdit NPC_ and CLAS definitions supply the record layouts.
CommonLibSSE-NG ActorValueOwner.h identifies the permanent-health virtual call,
TESClass.h identifies bleedoutDefault at class offset 0x68, and ActorState.h
distinguishes EssentialDown from Bleedout. These declarations alone do not
describe the calculation. Read-only llvm-objdump inspection of the installed
SkyrimSE.exe supplied the following additional evidence. The executable is
37,910,440 bytes, with SHA-256
`846efccf0c1374d71f892907f46549560f2fcb0a75cb87a3eed438baa0f1402f`.
Addresses below are virtual addresses for that executable only.

- At 0x14041dc50, the threshold calculation tests ACBS bit 18 and returns
  the sign-extended two-byte override. Otherwise it reads class offset 0x68,
  or fBleedoutDefault when the class pointer is null, and multiplies the
  result by ActorValueOwner virtual slot 2 for Health (24).
- fBleedoutDefault's engine Setting value at 0x1420a13a8 is 0.2. The installed
  Skyrim.esm GMST overrides it with 0.15. Setting.h supplies the name and
  value layout used to identify it. Plugin and runtime settings retain
  precedence through World::game_setting.
- At 0x1406c4a58, the regeneration path recognizes both down states. It reads
  NoBleedoutRecovery from actor flag bit 5 and uses a cached bleedout rate,
  initializing that rate from the threshold divided by fBleedoutRate.
  Other branches apply combat and actor-value multipliers. This observation
  does not yet establish the complete recovery transition.
- AIProcess.h names essentialDownTimer at offset 0x90. The getter at
  0x1406e9690 and setter at 0x1406e96a0 read and write that field. At
  0x14067a5eb the actor update checks NoBleedoutRecovery before decrementing
  a positive timer for EssentialDown. Crossing zero calls Actor virtual slot
  0xE5, which CommonLib identifies as StopCombat, then returns an actor still
  in EssentialDown to life state zero. The following health restoration call
  needs its own modifier interpretation before it can be reproduced.
- At 0x1406c68f9 a positive health change reaching at least 1 can leave
  EssentialDown. A positive essentialDownTimer is cleared first, with a
  StopCombat call. Bleedout instead calls 0x1406cf330, which compares the
  changed health with its cached threshold and checks NoBleedoutRecovery.
  These are distinct recovery paths, not one unconditional timeout.

The record survey found 159 winning classes, including 16 with a zero
threshold. It found 391 NPC records carrying the override flag, including
384 zero overrides and seven overrides of 50. Of 496 NPC records directly
flagged essential or protected, 98 inherit their STATS group. These counts
are authored record counts, not a survey of live actors entering bleedout.
Run the survey with SKYRIM_DATA set and
`cargo run -p soe-game --example bleedout_survey`.

Entry and the health-driven recovery path are implemented. `melee_hit` no
longer rejects a protected or essential target outright. It clamps the
target's health at `actor_bleedout_threshold` instead of letting a hit carry
it lower, matching the comparison at 0x1406cf330, sets `unconscious`, stops
combat and clears the target's `combat_target`, and dispatches
`OnEnterBleedout` to the same recipients `OnHit` reaches. The hit is never
lethal for a bleedout target: `melee_hit` never calls `kill_actor` on one, so
a scripted `Kill()` remains the only way to finish off a protected-but-not-
essential actor, matching the unchanged `request_actor_death` guard. Both
`melee_hit` targets and attackers are also rejected by
`validate_combat_actor` while `unconscious`, so a repeat hit on a downed
actor is out of scope here rather than silently re-entering bleedout.

Recovery runs through the same choke point every health change already goes
through. `World::change_actor_value` calls the new
`World::resolve_bleedout_recovery` right after `resolve_health_death` on
every named-health change. It clears `unconscious` once current health is
carried back above the cached threshold by a restore, a heal spell or a
potion, unless `SetNoBleedoutRecovery` holds the actor down.
`unconscious` and `no_bleedout_recovery` already round-tripped through save
and the determinism hash before this row, so no save-format change was
needed.

Three items stay out of scope for this row, each with a concrete next step:

- The `essentialDownTimer` path at 0x1406e9690, 0x1406e96a0 and 0x14067a5eb,
  which brings an unhealed actor back up on its own after a duration with no
  health change involved, is not modeled. `game` has no per-actor tick or
  timer scheduler to hang a countdown on yet. Next step: a follow-up row once
  such a facility exists elsewhere in the crate, so this one is not the first
  caller of a new scheduling primitive built to fit it.
- Only `melee_hit`'s health-damage path applies the clamp. The scripted
  `DamageActorValue` native and any future active-effect magnitude
  application reach `change_actor_value` too, and so still call
  `resolve_bleedout_recovery` on the way up, but nothing on the way down
  clamps them at the threshold: a large enough magic hit still kills a
  protected or essential actor outright. Next step: thread the same
  threshold clamp through the magic-damage call sites once CMB's magic-damage
  rows land, reusing `resolve_bleedout_recovery`'s threshold read rather than
  duplicating it.

The focused combat_bleedout tests cover winning class overrides, permanent
health scaling, inherited absolute overrides, live settings and malformed
inputs. The runtime evidence has not yet been checked through an interactive
vanilla comparison.

## Player-only kill exception for protected actors, CMB-34

`melee_hit` now lets the player kill a merely protected, non-essential
actor instead of always clamping them into bleedout. `World::melee_hit`
computes `player_exception` from `self.player == Some(hit.attacker)` and
the target's `protected`/`essential` flags, the same attacker check the
difficulty and skill-scale branches above already use, and folds it into
`bleedout_target`. With the exception set, the hit skips the threshold
clamp entirely and falls through to the ordinary lethal/`kill_actor` path,
so a lethal player hit kills the actor outright and a non-lethal one just
damages them normally. An NPC attacker still always sees the clamp and
enters the target into bleedout, since `player_exception` only ever holds
for `self.player == Some(hit.attacker)`.

`protected`/`essential` are read from the target's actor base fresh on
every hit, the same live lookup the invulnerable check just below already
uses, rather than from the value `ActorState` cached at its own first
build. `ActorBase.SetProtected` and `ActorBase.SetEssential`
(`crates/game/src/natives.rs`) can run before or after that build, so a
cached copy would let a script's call land on one and miss the other. The
World::actor build path also derives `protected` through the same
`actor_bases` override `essential` already used, so the two stay
consistent for a target that is never touched again after the script call.
`World::set_actor_base_protected` (`crates/game/src/world/death.rs`)
mirrors `set_actor_base_essential` and additionally propagates the change
to every already-built `ActorState` sharing that base, keeping
`Actor.IsProtected` and saved state in step even though `melee_hit`
itself no longer depends on that propagation.

Per the ck-wiki `ActorBase_Script` page, `SetProtected` and `SetEssential`
are mutually exclusive, so `target.essential` alone is enough to rule out
the exception without a separate check. Essential actors stay gated for
every attacker including the player, matching the unchanged
`request_actor_death` guard: no local source grounds a player exception
for essential actors the way the protected case is grounded.

CMB-13 flagged a "once combat with them has ended" qualifier from the
original playtesting note, but no local CK wiki page pins that detail.
Rather than track a formal combat-ended transition with no ground truth,
the exception here keys on the attacker alone, like every other
player/NPC branch already in `melee_hit`. If a source surfaces the
combat-ended condition later, narrowing `player_exception` to it is a
small, local change confined to this one field.

## Magic and scripted damage bleedout clamp, CMB-35

CMB-13 flagged the gap this row closes: only `melee_hit`'s health-damage path
clamped at the bleedout threshold, so a scripted `DamageActorValue` call or a
magic effect's health damage still killed a protected or essential actor
outright, or in `change_actor_value`'s case left an essential actor's health
at or below zero and fully conscious, since `request_actor_death` already
refuses to kill an essential actor and nothing set `unconscious` either.

`World::clamp_health_at_bleedout` (`crates/game/src/world/bleedout.rs`) is the
new shared entry point. It takes the actor, the caller's own eligibility
decision and a post-damage health value, reads
`World::actor_bleedout_threshold` and returns the health to commit plus
whether the actor is entering bleedout. An unreadable threshold is skipped
rather than propagated, the same way `resolve_bleedout_recovery` already
skips: many callers reach actors without a full NPC record backing them
(tests, synthetic actors), and CMB-13's own next-step note recommended
reusing that read rather than duplicating it. This is a deliberate deviation
from `melee_hit`'s `.transpose()?`, which does propagate the error, since
`melee_hit` targets are validated combat actors by the time it runs and
`change_actor_value`/`apply_effect_values` are not.

`World::change_actor_value` (`crates/game/src/world/actors.rs`) now clones
the actor's `Values`, applies the operation to the clone, and for a health
change checks whether the result is a decrease for a protected or essential
actor before writing back, matching `melee_hit`'s clone-then-clamp order so
the clamp always sees the change before it reaches the live actor. Every
`Values` setter already validates or rolls back internally without changing
state, so the clone costs nothing a direct call did not already risk: a
setter that fails now leaves the live actor completely untouched instead of
possibly partially applied, which the original direct-mutation code did not
guarantee. Entering bleedout sets `unconscious`, clears `in_combat` and
`combat_target` the same way `melee_hit` does, and skips
`resolve_health_death` for that change, so `kill_actor`'s own essential
guard is not what stops the death, the clamp is. `DamageActorValue`
(`crates/game/src/natives.rs`) and the five other actor-value natives all
share this through `change_actor_value`, per the ck-wiki `DamageActorValue`
page's own note that the native "modifies the current value" and converts a
negative argument to positive, i.e. it is always a `Values::damage` call
through the existing operation dispatch.

`World::apply_effect_values` (`crates/game/src/world/magic.rs`) follows the
same clone-then-clamp shape and now takes an `Alloc<'_>` to dispatch
`OnEnterBleedout` to `actor_event_recipients`, mirroring `melee_hit`'s event
dispatch exactly. Its three call sites (`apply_prepared_effects`,
`dispel_effect`, `tick_magic`) already carry `alloc` in scope, so no new
plumbing was needed above them.

The one real difference between the two paths, and the only difference from
melee, is player-exception scope, per this row's task pack: a scripted
`DamageActorValue` call carries no caster identity at all, so
`change_actor_value` never grants the merely-protected exception CMB-34 gave
`melee_hit`, a script can down a protected actor but never kill them
outright. `apply_effect_values` does have a caster on `ActiveEffect`, so it
grants the same exception `melee_hit` does, keyed on `effect.caster ==
self.player`, with essential actors staying gated for every caster exactly
as CMB-34 already established for melee.

`validate_combat_actor` rejects an already-unconscious actor as a melee
attacker or target, but neither `change_actor_value` nor
`apply_effect_values` carries an equivalent gate, so a script or a ticking
effect can still reach an already-downed actor. No new gate was added: the
clamp is idempotent, re-damaging an actor already sitting at the threshold
computes the same threshold and the same clamped health, so a repeat hit is
a no-op on health and never flips `unconscious` a second time. A dispelled
temporary effect's own reversal (`dispel_effect`'s `apply_effect_values(...,
-1.0, ...)` call) only ever raises health back toward its pre-effect value,
so it can never itself trigger the clamp, which only fires on a measured
decrease.

Reusing `resolve_bleedout_recovery` for both new call sites means the same
existing focused tests and the `combat_bleedout_recovery*` suites already
exercise the recovery half of this row unchanged. The writer added no tests,
the verifier and breaker suites named below cover entry. The existing `health_damage_respects_essential_and_deferred_kill_flags`
test in `crates/game/tests/actor_values.rs` still passes unchanged: its
synthetic actor has no backing NPC record, so `actor_bleedout_threshold`
fails and the clamp silently skips, leaving the test's own assertions
untouched.

### Round 1 fix: the clamp commit must not assume `Values::damage`

The first round's clamp commit in both `change_actor_value` and
`apply_effect_values` called `Values::restore("health", clamped - after)`
(`crates/game/src/actor_values.rs`). `restore` only ever raises the `damage`
component toward zero, capped at its floor. That correctly undoes a decrease
that itself went through `Values::damage`, the path `DamageActorValue` and a
non-RECOVER detrimental magic effect both take, so `cmb_35_magic_scripted_bleedout.rs`
and the reference cases in the adversarial suites passed from the start. It
is a silent no-op for any decrease that lands somewhere else: `SetActorValue`
writes `base`, `ModActorValue` and `ForceActorValue` write `permanent`,
`ModTemporaryActorValue` writes `temporary`, and a RECOVER-flagged magic
effect writes `temporary` through the same `modify_temporary` branch
`change_values` (`crates/game/src/world/magic.rs`) takes for it. For all of
these, `damage` was already at its floor of 0, so `restore` changed nothing
and the actor's committed health stayed at whatever the underlying operation
produced, tens to hundreds of points below the threshold. The three adversarial
test files under `crates/game/tests/` (`cmb_35_script_damage_bleedout_adversarial.rs`,
`cmb_35_magic_recover_bleedout_adversarial.rs`) drove each of these paths and
recorded the exact miss.

Round 1 made both commits call `Values::force("health", clamped)`. `force`
solves for `permanent` so that `current()` equals its argument directly,
independent of which component the preceding operation touched, so it lands
on the threshold for every path above the same way it already did for
`Values::damage`. This also restores the idempotency this section already
claimed: forcing current health to the same threshold on a second call is
a true no-op on the committed value, not just on the component `restore`
happened to touch, so the no-new-gate reasoning above holds for every
actor-value native, not only `DamageActorValue`.

`apply_effect_values`'s `dies` check had a second, independent defect: it
read `values.get("health").current() <= 0.0` after the clamp commit instead
of checking `entering_bleedout` directly. With the `restore` no-op above, a
RECOVER-flagged detrimental effect left `current()` deeply negative even
though `entering_bleedout` was `true`, so `dies` came out `true` and
`kill_actor` ran anyway, killing a protected or essential actor the clamp
had just flagged as spared and leaving `unconscious` and `dead` set at the
same time. `dies` is now `touches_health && !entering_bleedout && ...`, so a
successful bleedout entry can never also be read as lethal, regardless of
what the health reading is.

### Round 2 fix: keep the maximum when damage can carry the clamp

The round 1 `force` commit landed on the threshold but moved the wrong
component on the path that already worked. After `DamageActorValue` for 500
on a 100 health actor, `damage` sits at -500 and `force("health", 20)` set
`permanent` to 420, so `maximum()` read 520 and a later full
`RestoreActorValue` healed the actor to 520. `melee_hit` never had this
problem because it corrects `damage` through `restore`.

Both commits now go through `commit_bleedout_health`
(`crates/game/src/world/bleedout.rs`). When the threshold is at or under the
current maximum, it raises `damage` with `restore`, the melee correction, so
the maximum is unchanged. Only when the operation pulled the maximum itself
under the threshold (a large negative `SetActorValue`, `ModActorValue`,
`ForceActorValue`, `ModTemporaryActorValue` or RECOVER effect) does it clear
`damage` and call `force`, since `damage` cannot be positive. Both branches
commit exactly the threshold, so the idempotency argument above still holds.
Two unit tests in `bleedout.rs` pin the maximum on the damage path and the
threshold on the maximum path.

Seven tests in `cmb_35_script_damage_bleedout_adversarial.rs` and
`cmb_35_magic_recover_bleedout_adversarial.rs` still assert the pre-fix
values (committed health of -400, -500 or -700, and a protected target that
dies to a non-player caster). Their own comments name the threshold, 20, and
bleedout without death as the documented contract, which the implementation
now meets, so they fail against it and are left for review unchanged.

## Combat actor-value modifiers, CMB-15 in progress

The intrinsic AttackDamageMult milestone uses the shared actor-value store,
including its default, native reads and mutations, and serialization. There
is no combat-private multiplier cache and no save-format change. An older
save without the entry receives the documented default. Saved explicit
values are retained. The value is applied after weapon skill scaling and
before armor and difficulty reduction. Its exact floating-point rounding
relative to the original runtime remains unmeasured.

Local CommonLibSSE and skymp actor-value enums identify AttackDamageMult as
154. The [Skills-based Damage and Magnitude Multipliers author](https://www.nexusmods.com/skyrim/mods/21828)
documents its native default of 1 and its damage-multiplier role. That public
mod description supplies the behavior missing from the local enum declarations.
The four `cmb_15` combat tests cover shared components, armor and difficulty
composition, explicit zero, unarmed damage, save/load, overflow and rejection
without health or event changes.

CMB-15 remains unfinished because the skill Mod and PowerMod values are
PERK inputs, not intrinsic percentage multipliers. The installed five-master
survey finds PerkSkillBoosts at 0:0CF788 and AlchemySkillBoosts at 0:0A725C.
Both use function 14, multiply by one plus actor value times coefficient.
OneHanded, TwoHanded and Marksman modifier entries use ModAttackDamage with
coefficient 0.01. LightArmor and HeavyArmor modifier entries instead use
ModIncomingDamage with coefficient -0.01. Their conditions and priorities
come from the winning PERK records. Hardcoding those effects into armor
rating would ignore record edits and apply the wrong term. The
`combat_modifier_survey` example prints the relevant subrecords using the
existing plugin reader, without copying installed assets.

CMB-15 now depends on CMB-09 so those modifiers will execute through the perk
path. They continue to reject pending that work. `docs/combat-perks.md`
records the dispatch prerequisites and the ordering evidence still needed.

## Stagger from hit magnitude, CMB-14

`melee_hit` now sets a target's stagger window from the same post-armor,
post-block health damage that already feeds bleedout and death. The
magnitude is that health damage scaled by the weapon's own DNAM Stagger
fraction, `health_damage * weapon.stagger`. A hit above `fStaggerThreshold`
sets `ActorState::stagger_end_time` to `World::clock.real_seconds` plus
`fStaggerDuration`, the same real-time clock `ai::wait_until` uses for its
own deadlines, not the calendar `game_days` clock. `World::is_staggered(id)`
reads that deadline lazily against the current clock, mirroring the
`is_player` and `is_reference` query methods rather than adding an active
per-tick clear. `HitResult::staggered` reports whether this particular hit
crossed the threshold.

The local `TES5Edit/Core/wbDefinitionsTES5.pas` WEAP definition places the
Stagger float as DNAM's last field, the four bytes at offset 96 to 100 of
the 100-byte struct, right after an unparsed four-byte resist gap. The task
pack's own citation names this field under CRIT DATA. That is not what the
local pas file or a direct read of the record layout shows: CRIT DATA holds
the critical-hit damage and multiplier, DNAM holds Stagger. `Weapon::stagger`
now parses it, 0.0 to 1.0 in vanilla data, matching the 0.0 to 1.0
`afStagger` convention `ProcessTrapHit` documents in the CK wiki natives
survey. Unarmed contacts carry no WEAP record, so `melee_hit` gives them a
stagger fraction of 0.0 and they never stagger. No corpus source states what
an unarmed stagger contribution should be instead.

No local or UESP source states a vanilla `fStaggerThreshold` or
`fStaggerDuration` value. Both are engineering estimates in
`game_settings.rs`, documented there and here rather than left silent:
3.5 for the threshold, so a low-Stagger dagger needs a heavy hit while a
high-Stagger warhammer staggers on an ordinary one, and 1.2 seconds for the
duration, the midpoint of the 0.5 to 2.0 second animation lengths the task
pack observed. Both stay overridable through the existing plugin and
live-setting GMST precedence `World::combat_setting` already gives every
other engine default here.

No vanilla Papyrus event announces a stagger. A search of the local
`ck-wiki` corpus, both `HTML_FALLBACK.txt` and `FAILED.txt`, found no
stagger-related event anywhere. Host and render code reads
`ActorState::stagger_end_time` or calls `World::is_staggered` directly
instead of waiting on a fabricated event.

`validate_combat_actor` already rejects any actor carrying a non-empty
`perks` set before `melee_hit` reaches the stagger computation, so a perked
actor's stagger immunity or resistance is out of scope the same way every
other perk-gated combat modifier is, pending CMB-09's perk dispatch path.

`ActorState::stagger_end_time` is a new `Option<f64>` field, so SOEW moved
from version 28 to 29. `write_actor_state` writes it unconditionally and
`read_actor_state` only reads it past version 29, so older saves load with
no stagger window rather than a version error. `save_tests.rs`'s and the
shared `tests/common/legacy.rs`'s `without_exclusive_branches` fixtures both
had to learn to strip the new field's bytes alongside the exclusive dialogue
branch they already stripped, since the writer is unconditional and the
fixtures simulate a pre-version-29 stream by cutting real bytes back out.

Left out of this row, each a concrete next step rather than a silent gap:

- Physical recoil and knockback, an actual push on the target's collision
  body, belongs to the physics crate once it drives combat contacts.
  `ActorState::stagger_end_time` only gates animation and input on the
  `game` side, it moves nothing.
- Vanilla's Light, Medium and Heavy stagger tiers are not modeled. Every
  qualifying hit uses the same `fStaggerDuration`, not a duration keyed to
  hit strength.
- Power attacks apply their damage and stamina multipliers as of CMB-21, but
  stagger still uses the weapon's own DNAM Stagger fraction unchanged. UESP's
  per-attack-type table gives a single-weapon power attack's stagger
  multiplier as x1, the identity, so this happens to match for the one entry
  CMB-21 reads. Dual-wield and bash power attacks, whose table entries are
  not x1, are not selected by `PowerAttack::from_race` and so never reach
  this path.
- No live or installed-data comparison against `SKYRIM_DATA` has been run
  for stagger specifically, since no source pins the vanilla threshold or
  duration to compare against.

## Bash attacks, CMB-20

A bash is a variant `MeleeHit` where `is_bash` is true and `weapon` names the
attacker's equipped shield ARMO instead of a WEAP record, the row summary's
own "shield becomes the weapon" reading. `melee_hit` resolves that ARMO
through the existing `combat_armor_record` template chain, the same one
`combat_armor` and `combat_block` already use, and rejects a bash source
that does not occupy the shield slot (BOD2 bit 9) with
`Error::Unsupported("bash source is not a shield")`. The attacker must have
the shield equipped and counted in inventory, checked the same way an
ordinary weapon swing checks its WEAP source.

BIDS and BAMT, the shield's impact data set and alternate material, are now
on `Armor` rather than `Weapon`. The CMB-05 section above already found that
UESP's ARMO layout (lines 64 to 68) ties both fields to shields, and deferred
adding them pending this row. `TES5Edit`'s WEAP layout (lines 10444-10445)
places the same two subrecords on WEAP for hand-to-hand bash sources, out of
scope here since this row restricts bashing to shields, open question 3.
`Armor::from_sources` reads both through `inherited`, following the TNAM
template chain the way every other physical ARMO field already does, using
the same `link()` helper `template`/`enchantment` use. No engine code reads
`bash_impact`/`bash_material` yet, an audio and impact-effect gap this row
does not close.

Stamina cost is `fBashStaminaCost` from game settings, queried through the
existing `combat_setting` lookup that validates it finite and non-negative.
No local or UESP source pins a vanilla bash stamina cost, so 25.0 is an
engineering estimate placed inside the general power-attack stamina range
Skyrim wikis document, roughly 10 to 30. The check happens before any of the
hit's later fallible steps (armor, block, difficulty, health and stagger),
but the deduction itself is deferred to the same late commit block that
writes the target's health and stagger state, so a bash that errors out
anywhere after the stamina check never spends stamina, and one that succeeds
spends it exactly once. Insufficient stamina returns
`Error::Unsupported("insufficient stamina for bash")`, favoring the row
summary's own prose over the task pack's Tests-to-write section, which named
an Equipment error for the same case. `Error::Equipment` already means
missing or unequipped gear here and in every other branch of `melee_hit`,
not an exhausted resource, so `Unsupported` is the closer fit and this
deviation is called out explicitly rather than left implicit.

Bash damage reuses the shield's DNAM `rating` as the base input, then flows
through the same post-armor, post-block, post-difficulty pipeline every
other hit uses, mirroring how `unarmed_damage` already injects a base value
into that shared pipeline instead of a parallel one. This is the open
question the task pack itself flagged as the highest design uncertainty:
no source states whether vanilla scales bash damage from the shield's
rating, a separate GMST, or the attacker's other-hand weapon. Shield rating
was chosen because it is already loaded, already scaled by the same armor
math a shield contributes defensively, and needs no new unverified GMST.
Stagger fraction for a bash is a fixed 1.0, full pass-through of
post-reduction damage into the stagger magnitude check the same way CMB-14
already computes it for a weapon swing. A shield's DNAM carries no Stagger
field of its own to read instead, and a shield bash's core purpose is
interruption, so a bash that lands at all is meant to threaten stagger the
way a high-Stagger weapon does. Both choices are engineering estimates, not
sourced defaults, and belong alongside `fStaggerThreshold` and
`fStaggerDuration` as the kind of value a five-master survey or live
`SKYRIM_DATA` comparison should refine once bash records are enumerated.

OnHit's sixth boolean argument, `abBashAttack`, now carries `hit.is_bash`
instead of a hardcoded false. The CK wiki's OnHit page (lines 8, 22, 41)
documents the event's seven-argument signature and notes `akSource` can be
None for a bash from a light object such as a torch, but a shield bash
always has one, the shield itself. `HitResult::bashed` reports the same flag
back to callers that do not want to re-inspect the original `MeleeHit`.

`crates/godot-host/src/combat.rs`'s `MeleeContact::event` now sets
`is_bash: false` on every contact it delivers, a mechanical fix for the new
required field, not new behavior. That bridge has no shield-bash gesture
detection yet, so every host-driven contact remains an ordinary weapon swing
until CMB-03's own future work adds one, exactly as the task pack's own
"Lines of evidence" section anticipated.

Left out of this row, each a concrete next step rather than a silent gap:

- Host-side bash gesture detection. `MeleeContact` has no shield-bash
  trigger, so `is_bash` is only reachable today through a direct
  `melee_hit` call, not through the animation bridge.
- Power bash (higher damage and stagger on a held or charged bash) is out
  of scope pending power-attack infrastructure, open question 3.
- Physical knockback belongs to the physics crate and CMB-28's collision
  work, the same boundary CMB-14 draws for stagger versus recoil.
- Bash interaction with an incoming weapon-shield block is not evaluated,
  open question 5.
- No SOEW version bump: stamina is an existing persisted actor value and
  the bash path writes it through the same `Values::damage` every other
  actor-value mutation already uses, so no new field needed a schema
  change here.
- No live or installed-data comparison against `SKYRIM_DATA` has been run
  for bash specifically, so `fBashStaminaCost` and the bash damage and
  stagger-fraction choices above are unverified against vanilla play.

Sources read for this increment: `TES5Edit/Core/wbDefinitionsTES5.pas`
(lines 10431+, 10444-10445), `uesp-modfileformat/WEAP.wikitext` (lines
12-14), `uesp-modfileformat/ARMO.wikitext` (lines 64-68),
`CommonLibSSE/include/RE/B/BGSBlockBashData.h` (lines 21-22), and
`ck-wiki/OnHit_-_ObjectReference.wikitext` (lines 8, 22, 41), all cited in
the CMB-20 task pack. This section's stagger-fraction and damage-formula
choices build on the CMB-14 section above and the CMB-05 section's own
documented deferral of BIDS/BAMT to this row.

## Power attack damage and stamina, CMB-21

`MeleeHit` carries a new `is_power_attack: bool` field, host-confirmed by the
attacker's animation graph the same way `World::set_blocking` already
confirms the target's block stance. `melee_hit` does not derive it from an
actor value such as `IsPowerAttacking`.

When set, and the swing is not unarmed, `World::combat_power_attack` reads
the attacker's `ActorState::race` (already cached at actor-build time by
`World::actor`) and re-reads the winning RACE record fresh through
`self.record`, matching the "never an engine-private copy" rule. The new
`PowerAttack::from_race` (`crates/game/src/combat/records.rs`) walks the
RACE's uncounted `ATKD`/`ATKE` Attacks array and returns the first entry
whose Attack Flags has Power Attack set and Bash, Left and Rotating clear,
the default single-weapon forward power attack. Each `ATKD` is a fixed 44
byte struct (xEdit `wbAttackData`, `wbDefinitionsTES5.pas`, confirmed
against the source directly). Only Damage Mult and Stamina Mult are read,
every other field including the per-entry Stagger fraction is skipped.

The multiplier is applied to `weapon_damage` before the existing armor,
block and difficulty scaling, so it composes with every reduction CMB-01 and
CMB-05 already established rather than bypassing them.

Stamina cost has no named GMST on either UESP `Skyrim:Combat` or UESP
`Skyrim:Damage`, both pages state the same formula in prose:
`power attack stamina cost = (40 + weapon weight * 2) * attack cost multiplier * (1 - perk effect)`. `POWER_ATTACK_STAMINA_BASE` (40.0) and
`POWER_ATTACK_STAMINA_PER_WEIGHT` (2.0) in `world/combat.rs` are that
formula's two plain numeric constants, cited at their declaration rather
than invented GMST names. `validate_combat_actor` already rejects an
attacker carrying any perk entry ahead of this row, so the perk term is
always 1 here and is not itself computed. "Attack cost multiplier" is the
selected `ATKD` entry's Stamina Mult, read as `PowerAttack::stamina_mult`.

The cost is deducted from the attacker's Stamina actor value through the
same unclamped `Values::damage` path `health_damage` already uses on the
target, prepared alongside the target's health change so a malformed
attacker Stamina entry cannot leave a partially-applied hit, and committed
into `self.actors` right after the target's own commit. The done check
("a power attack applies its damage multiplier and consumes the recorded
stamina once") is met: both happen exactly once per accepted power attack
hit, and not at all when the hit is rejected earlier (invulnerable target,
non-melee weapon, missing equipment, and so on).

Sources read: `engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas` (`ATKD`
layout, Attack Flags bit values), UESP `Skyrim:Combat` and `Skyrim:Damage`
(stamina cost formula, per-attack-type damage/stagger/stamina multiplier
table). `engine-refs/activeragdoll/src/main.cpp` and
`engine-refs/skymp/skymp5-client/.../sweetTaffyPlayerCombatService.ts` were
also checked and confirmed no named-GMST stamina formula exists in either:
activeragdoll calls an opaque `ActorValueOwner_GetStaminaCostForAttackData`
and applies it directly to the Stamina actor value.

Open gaps, not attempted by this increment:

- Unarmed power attacks. UESP's Combat and Damage pages give conflicting
  unarmed multipliers (damage x2/stagger x0.5 versus damage x1/stagger x0.5),
  so `melee_hit` rejects an unarmed power attack with
  `Error::Unsupported("unarmed power attack")` rather than guess between them.
- Dual-wield, bash, off-hand and other non-default `ATKD` entries.
  `PowerAttack::from_race` only selects the default single-weapon entry.
  Bash power attacks in particular read BIDS/BAMT, not this path, matching
  the same scoping CMB-05's block section already applies to ordinary bash.
- An `NPC_` record's own override Attacks array. Only the RACE's array is
  read, an NPC-level override (when present) is not consulted.
- `fBlockPowerAttackMult`, the block-side attenuation for a power attack
  landing on a blocking target. See the CMB-05 section's gap list.
- No stamina floor or insufficient-stamina fallback. Vanilla is understood
  to fall back to a regular attack when the attacker lacks enough stamina to
  power attack, `melee_hit` never rechecks stamina before applying the
  swing, since `is_power_attack` is host-confirmed input the host is assumed
  to have already gated on available stamina.
- No live or installed-data comparison against `SKYRIM_DATA` for the
  selected RACE Attacks entry or the stamina formula's constants.

## Tempered item quality, CMB-10

`check_combat_stack` used to reject any weapon stack carrying an extra of
kind 37 (CommonLib `ExtraHealth`, poison 62 and enchantment 155) outright.
It now separates the three: poison and enchantment still reject the contact
unconditionally, unchanged from CMB-03. Quality is read instead, and
`melee_hit` applies it as a multiplier on `weapon_damage` right after
AttackDamageMult (CMB-01) and before armor and shield block reduction
(CMB-05). A weapon with no quality extra on any of its stacks leaves damage
unchanged, matching CMB-01's existing untempered path exactly.

`ExtraHealth` carries a single float, `health`, which the CK's
`SetItemHealthPercent`/`GetItemHealthPercent` pages document as the item's
"health (tempering state)" as a fraction: 1.0 is the untempered baseline
and values above it are a tempered percentage. `damage * health` is what
this row ports, the same direct scaling the task pack specifies and the
one CK page in the local corpus that names the field's semantics supports.
No local or UESP source enumerates the five tempering tier multipliers
(Fine, Superior, Exquisite, Flawless, Legendary in the perked tempering
chain) as fixed numbers, those come from the smithing perks and the
tempering recipe's quality slider at craft time, not from a table this row
could port. `check_combat_stack` reads whatever value smithing wrote, it
does not compute or validate against a tier table.

CMB-03 already established that the equipped set is form-only,
`BTreeSet<GlobalFormId>`, and cannot name which physical `ExtraDataList`
instance a hit swung when an item has more than one stack. This row cannot
add that tracking without changing the equip model CMB-03 shipped, which is
out of scope here. Instead, `check_combat_stack` reads quality from every
stack of the equipped item: no stack carrying kind 37 returns `None`
(no adjustment), every stack that carries it agreeing on one finite,
non-negative value returns that value, and stacks that disagree return
`Error::Unsupported` rather than pick one arbitrarily. A tempered stack
next to a plain, unmodified stack of the same item is unambiguous under
this rule, since only one candidate value exists. Two stacks of the same
item tempered to different levels are not resolvable from the equipped set
alone and reject the contact, a concrete gap the equip model would need to
close, not a silent one.

Quality bits are read with the same `finite()` helper CMB-01 already uses
for every other combat float, rejecting a nonfinite or negative value the
same way a malformed game setting rejects. `Inventory::update_selected`
(CRF-04) already validates finite, non-negative quality before writing it,
so this only matters for stacks assembled outside that path, such as
synthetic fixtures or saved data the crafting API never touched.

`melee_hit` runs the quality read once more, read-only, before it reads the
target's actor base. That read may initialize the base lazily, and
`save_state` persists the entry (CMB-36). Validating an equipped weapon's
quality first means a malformed, nonfinite or ambiguous quality rejects the
contact with the saved state unchanged, not only with no damage applied.
The early pass skips unarmed contacts, unequipped weapons and attackers
without a reference, so their existing errors keep their order.

Sources read: local CommonLib `ExtraHealth.h` (single `float health`
member, `ExtraDataType::kHealth`) and `ExtraEnchantment.h` (a distinct
`EnchantmentItem*` field, confirming quality and enchantment are unrelated
per-instance extras), the CK wiki's `SetItemHealthPercent_-_ObjectReference`
and `GetItemHealthPercent_-_ObjectReference` pages for the health-as-
tempering-percentage semantics, and this repository's `docs/game.md` CRF-04
section for the extra-data field layout and validation already in place.
No new dependency or record view was needed, the extra was already decoded
by the inventory model CRF-04 built.

Left out of this row:

- The fixed vanilla tempering tier multipliers (Fine through Legendary) are
  not modeled as a table, since no corpus source states them as constants
  independent of the smithing skill and perk formula that produces them.
  `docs/game.md`'s crafting sections own that formula when it lands.
- Two or more stacks of the same equipped item carrying different quality
  values reject the contact instead of resolving to one, see above. Fixing
  this needs instance-level equip tracking beyond CMB-03's form-only set.
- Unarmed contacts hold no WEAP record or inventory stack, so quality never
  applies to them, matching the existing unarmed-modifier rejection CMB-01
  already documents.
- Enchanted-and-tempered weapons still reject on the enchantment extra
  alone, so quality's interaction with an active enchantment is untested
  and unimplemented pending the enchantment row.
- No live or installed-data comparison against `SKYRIM_DATA` has been run,
  since no vanilla save in the local corpus was checked for its equipped
  weapons' quality extras specifically.

## Weapon resistance flags, CMB-26

`melee_hit` used to reject any weapon whose DNAM Resist field was not -1
outright, folded into the same check as the non-hostile flags. It now honors
the field instead: a resist index of 39 DamageResist through 44 MagicResist
(`combat/records.rs`'s `Weapon::resist`) names a further target actor value
that reduces this weapon's damage, read the same way `combat_armor` already
reads DamageResist and applied as its own multiplicative term alongside armor
reduction and shield block, before the difficulty multiplier. DNAM flags bit
0x01, "Ignores Normal Weapon Resistance", suppresses the term instead of
rejecting the weapon, matching its documented meaning. -1, the value every
original vanilla record carries, keeps this row a no-op for ordinary weapons.
An index outside -1 and 39 through 44 is reserved and still rejects the
contact, now with its own `"unsupported weapon resistance value"` error
instead of the old combined message. The unarmed attack form (CMB-01) carries
its own DNAM the same way, so `unarmed_damage` reads its resist index and
ignoring flag and applies them identically, an unarmed contact is not a
special case.

Sources read: `uesp-modfileformat/WEAP.wikitext` line 58 for the ignoring
flag and line 123 for the Resist field's documented range, and
`TES5Edit/Core/wbDefinitionsTES5.pas` lines 3344 to 3349 for the six actor
value names at indices 39 through 44 (already present in
`actor_values.rs`'s `ACTOR_VALUE_NAMES`, confirmed matching rather than
re-derived) and line 10532 for the field's own `wbActorValueEnum` typing.
Neither source states an exact reduction formula. This row treats the actor
value the same way `combat_armor` treats DamageResist, a percentage divided
by 100 and clamped to 0.0 to 1.0, rather than invent an unverified formula
shaped differently from the one already established in this file for the
same actor-value family.

No persisted state changed: the resist index and ignoring flag are read from
the WEAP record on every hit, not cached on `ActorState` or written to saved
data, so SOEW's version did not move for this row.

Left out of this row:

- The exact vanilla reduction formula and its ordering against armor and
  shield block are unverified. This mirrors `combat_armor`'s own DamageResist
  treatment rather than adding a second, differently sourced guess.
- No live or installed-data comparison against `SKYRIM_DATA` has been run,
  since no local source names a weapon record carrying a non-default Resist
  value to check against, matching the task pack's note that all original
  files use -1.
- Perked resistance modifiers (Mod Target Damage Resistance, actor value 37)
  are out of scope pending CMB-09's perk dispatch path, the same gap
  CMB-15's armor-side perk modifiers document above.

## Weapon poison consumption, CMB-25

An ordinary weapon swing now reads the equipped weapon's stacks for a
CommonLib `ExtraPoison` extra (kind 62, `[Field::Form(Some(poison_item)),
Field::Word(count)]`) and, when exactly one stack carries one, applies the
poison's magic effects to the target once and clears that stack's coating.
Quality (CMB-10) and poison are two separate extras on the same stack model,
so `check_combat_weapon` now reads both together after `check_no_enchant`
rejects an enchanted stack, and `combat_stack_poison` mirrors
`combat_stack_quality`'s read shape while applying its own stricter
ambiguity rule below.

The equipped set is form-only (CMB-03), so a hit cannot name which physical
stack instance the attacker swung. When more than one stack of the equipped
weapon carries a kind-62 extra, `combat_stack_poison` rejects with
`Error::Unsupported("ambiguous poisoned item instances")` rather than guess,
the same shape as quality's ambiguity rule with one difference: two stacks
that agree on the same poison form and count still reject here, unlike
quality's agreement exception. Quality only needs a value to multiply
damage by, so an agreed value is unambiguous even split across stacks.
Poison needs a physical stack index to clear after application, and
agreement on the form does not tell `melee_hit` which index that is. A
kind-62 extra whose fields are not exactly `[Field::Form(Some(_)),
Field::Word(count)]`, or whose count is 0, is "malformed weapon poison
extra", the same fail-closed treatment CMB-10 gives a malformed quality
extra.

Poison is read twice, both read-only. `melee_hit`'s early validation pass
calls `combat_stack_poison` right after `combat_stack_quality`, ahead of the
target's lazy actor-base initialization CMB-36 documents, so a malformed or
ambiguous poisoned stack rejects with the saved state untouched, not only
with no damage applied. The ordinary-weapon branch then calls
`check_combat_weapon` again to bind the selected stack's index and poison
item for the deferred application step. A bash never reaches either call
since `check_no_poison_or_enchant` rejects a poisoned or enchanted equipped
item unconditionally before the branch, the same as CMB-03's original bash
rule, and `HitResult::poisoned` is `false` for every bash and unarmed
contact.

Application happens last, after every other part of the hit has already
committed, the same deferred placement `bash_stamina_cost` and
`attacker_stamina` already use in this function: the physical hit's damage,
armor, block and resistance terms commit first, then the poison's magic
effects apply through the same `apply_prepared_effects` pipeline
`drink_ingestible` and `cast_spell` share, then the stack's kind-62 extra
clears through `Inventory::clear_stack_extra`. A poison error partway
through can leave the physical hit committed with the coating still on the
weapon, the same accepted partial-commit risk those two already carry.

No corpus source pins an exact vanilla dose or reapplication count for a
poisoned weapon: local CommonLib `ExtraPoison.h` stores only
`AlchemyItem* poison; uint32_t count;` with no decrement semantics
documented alongside it, and `PoisonedWeapon.h`'s helpers only read and
clear the extra, they do not describe a per-hit dose curve. This row applies
the poison's full effect list once per hit against the done check's literal
wording, "applies its poison once and clears the consumed coating," rather
than invent an unverified decrementing-dose formula. `apply_poison` does not
itself check the ALCH record's `INGESTIBLE_POISON` ENIT flag (bit 17): a
kind-62 extra already only ever points at a poison item by construction (the
crafting path that writes it, CRF-04, only accepts a poison ALCH), so the
flag read would be redundant here the same way `Inventory::update_selected`
already validates the extra before it reaches combat.

Sources read: local CommonLib `ExtraDataTypes.h` for the kind-62
`ExtraPoison` tag, `ExtraPoison.h` and `ExtraPoison.cpp` for the
`AlchemyItem*`/`uint32_t` field pair, and `PoisonedWeapon.h` for the
read-and-clear helper shape this row's deferred clear step mirrors.

Left out of this row:

- No vanilla dose or reapplication count is sourced. A single full
  application per hit is this row's reading of the done check's own
  wording, not a measured vanilla behavior.
- No corpus source confirms whether a real poisoned-weapon animation event
  gates the consumption the way `OnHit`'s `abBashAttack` gates CMB-20's bash
  reporting. `melee_hit` applies and clears on every successful ordinary
  swing that carries a matching stack, host-side animation timing is out of
  scope here the same way it already is for the rest of this function.
- Perked poison modifiers (Deadly Poisons and similar) are out of scope
  pending CMB-09's perk dispatch path, the same gap CMB-26 leaves open for
  weapon resistance.
