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
must also remain present, and conflicting occupied slots are rejected.
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

The current path is ordinary melee at Adept difficulty. Its separate damage
to/from player settings are read on every contact, while NPC-to-NPC contacts
do not use those multipliers. It rejects all perked
actors, enchantments, item tempering/poison, weapon/armor templates, special
resistance flags and unsupported weapon skills/types. These are open execution
requirements, not silently omitted modifiers. Protected and essential targets
are rejected pending bleedout handling. Ghosts and dead actors are rejected.
An essential attacker can still attack an ordinary target.

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

- TES5Edit/Core/wbDefinitionsTES5.pas WEAP and ARMO definitions.
- uesp-modfileformat/WEAP.wikitext and ARMO.wikitext, including the distinction
  between animation type, governing skill and equip slot.
- ck-wiki/OnHit_-_ObjectReference.wikitext for event arguments and actor
  projectile behavior, and EquipItem/GetEquippedWeapon for equipment contracts.
- skymp TES5DamageFormula.cpp and TES5DamageFormulaTest.cpp. Their weapon,
  armor, block, sneak and difficulty TODOs prevent adopting that implementation
  as a complete Skyrim formula. Its GMST-based armor cap is a comparison point.

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

The combined branch passes tools/lint.sh with 1,163 Rust tests and 11 Python
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
Adept damage multipliers, absent engine defaults, rejected malformed settings,
record truncation, stale equipment, ordered hit/death delivery and deferred
killer attribution across engine save/load. Event argument values and hit
notifications to aliases or active effects lack dedicated hit-path assertions.
No test in this increment performs host collision detection or renders combat.

## Remaining execution work

Host attack input, timing/contact detection, hand-specific selection, ranged
weapons/ammunition, unarmed attacks, bashing, sneak and power attacks, blocking,
perk entry points, tempering, enchantments, difficulty selection, bleedout,
stagger, crime and combat AI remain unfinished. The API is an execution
increment and does not establish playable combat or completion of the broader
gameplay-foundations goal.

## Bleedout investigation, CMB-13 in progress

The threshold query reads the selected STATS template source through the
existing actor-template resolver. ACBS Bleedout Override supplies absolute
health. Otherwise CLAS DATA's Bleedout Default multiplies permanent health,
which includes the base and permanent modifier, without damage or temporary
effects. A missing class link uses fBleedoutDefault. An invalid class record
returns an error. Zero is a valid threshold. This is preparation for the
transition, and does not yet make protected or essential contacts executable.

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

The focused combat_bleedout tests cover winning class overrides, permanent
health scaling, inherited absolute overrides, live settings and malformed
inputs. The runtime evidence has not yet been checked through an interactive
vanilla comparison. Entry, automatic recovery, NoBleedoutRecovery behavior,
OnEnterBleedout and persistence are still required before CMB-13 is done.
