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
these defaults. It rejects all perked
actors, enchantments, item tempering/poison, special
resistance flags and unsupported weapon skills/types. These are open execution
requirements, not silently omitted modifiers. Protected and essential targets
are rejected pending bleedout handling. Ghosts and dead actors are rejected.
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
initialization may still populate record-derived state. Accepted contacts queue
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
weapons/ammunition, bashing, sneak and power attacks, blocking,
perk entry points, tempering, enchantments, bleedout,
stagger, crime and combat AI remain unfinished. The API is an execution
increment and does not establish playable combat or completion of the broader
gameplay-foundations goal.

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
Instance quality, poison and enchantment continue to reject as described above.

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
