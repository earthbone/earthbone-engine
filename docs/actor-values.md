# Actor values and initial combat attributes

The combat execution branch separates base, permanent, temporary and damage
components in the shared actor state. Current and maximum values are computed,
not stored independently. This is a prerequisite for combat and magic.
Weapon hits, armor, blocking, perks and combat AI are still unfinished.

## Contracts and sources

The local corpus supplied these contracts:

- CK SetActorValue, ModActorValue, ForceActorValue, DamageActorValue,
  RestoreActorValue, GetActorValuePercentage and GetActorValueMax pages.
  Set preserves modifiers, Mod changes maximum, Force changes permanent,
  and Damage/Restore use the absolute amount. Restore cannot erase a
  permanent debuff. Percentages return 1 for zero maximum and CarryWeight.
- CommonLibSSE-NG ActorValues and ActorValueOwner headers distinguish the
  permanent, temporary and damage domains.
- xEdit `TES5Edit/Core/wbDefinitionsTES5.pas`, NPC DNAM and RACE DATA,
  with Mutagen's NPC PlayerSkills definition and UESP NPC/RACE descriptions.
  NPC health starts at byte 36, magicka at 38, stamina at 40, far-away model
  distance at 44 and geared-up weapons at 48. The old reader and synthetic
  builder agreed with each other at incorrect offsets, hiding the error.
- skymp GetBaseActorValues and EvaluateTemplate supplied comparison points
  for race starting attributes plus ACBS offsets and template groups.
  Its unfinished auto-calculation was not adopted as a runtime formula.

The new race view only reads the DATA attributes needed here. Stats and
traits follow independent NPC template chains, with cycle detection and
winning-file link resolution. Manual attributes use race starts plus ACBS
offsets. Auto-calculated attributes use DNAM's authored cache without adding
the offsets again. This does not implement level-dependent recalculation.
The existing ESM Race view was checked, but it reads FaceFX PHTN/PHWT rather
than DATA attributes. The game view therefore adds the missing prefix only.

The store rejects non-finite inputs and overflowing components or totals
before mutation. Resurrection clears health damage and death identity while
retaining modifiers. Essential and deferred-kill flags suppress the current
death flag transition. Bleedout, protected-actor attacker rules, invulnerability
and OnDying/OnDeath delivery require the combat lifecycle.

## Persistence

SOEW version 10 writes sorted names and all four components. Case-insensitive
duplicate names, non-finite values, positive damage components and truncated
input are rejected. State hashes include every component.

Versions 4 to 6 only restore actor values whose current and base totals agree.
Old saves with different totals lack the information needed to distinguish
damage from permanent or temporary modifiers and return an ambiguity error.
Version 7 restores with empty dialogue. Version 6 also restores its dialogue
state when actor values are unambiguous. Version 5 restores active effects with the same legacy-value restriction.
Version 8 restores dialogue and components with no active effects.
This is the engine's own save format, not vanilla ESS restoration.

## Verification

Synthetic tests cover Papyrus dispatch, Set/Mod/Force/Damage/Restore
interactions, temporary-effect removal, death suppression, resurrection,
component-preserving saves, malformed state and independent NPC field bytes.
Template fixtures include multiple inheritance steps and a winning override
that introduces a race in another plugin's namespace.

The installed-data survey reads Skyrim, Update, Dawnguard, HearthFires and
Dragonborn. Thresholds were set before the first run: more than 100 races,
more than 1,000 NPC bases and more than 1,000 resolved attribute sources.
The 2026-09-20 run found 161 races, all with 164-byte DATA, and 6,361 NPC bases.
It resolved 4,718 sources, of which 4,265 use cached auto-calculated values.
The remaining 1,643 lead to LVLN templates. No other source errors occurred.
Player resolved to 100/100/100, Hulda to 75/60/60 and MQ101Alduin to 2355/50/80.
These values have not been checked against a running vanilla game.

## Remaining gameplay work

Leveled templates now retain a selection per reference, as detailed below.
Encounter-zone difficulty and reset integration, class/level calculations and
race skill bonuses are missing. Missing RACE
DATA retains the previous synthetic-record fallback. Runtime flags, inventory, spells, perks, factions, outfits and packages now
resolve their owning template groups. Script, keyword, AI-data and attack-data
inheritance still need the corresponding runtime consumers.

No melee contact, weapon damage formula, armor/block/perk evaluation,
death-event sequence, stagger, attack animation or host attack input was added.
The integrated magic runtime uses these modifier domains for active effects.
The full gameplay-foundations objective remains open.

## Per-reference actor templates, 2026-09-20

The actor-templates increment resolves NPC TPLT chains through LVLN records.
Each reference retains one selection across stats, traits, inventory, spells,
perks, factions, outfits, basic AI packages, sex conditions and footstep skin
selection. Raw links are resolved in the file supplying each winning source.
Stats inheritance no longer copies the configuration flags belonging to base
data, which could remove a child's essential or protected status.

LVLF selects either all entries up to the supplied level or only entries at
the highest eligible level. Duplicate entries retain their weight. Nested
lists reuse that level. LVLD and a live LVLG global supply chance-none, with
the global taking precedence. An empty result persists and disables the
logical reference instead of being rolled again by each getter. Script list
changes affect future references. Existing selections retain their NPC chain.
Unsupported flags, counts other than one, malformed entries, deleted records
and cycles return errors. Failed selection restores the RNG, though lazy
list/global reads can still materialize state. Selection does not retry on an
empty result and does not treat a missing NPC as a default 100-health actor.

The current lazy-reference and PlaceAtMe paths supply player level. The public
prepare_actor_template entry point accepts a caller-resolved level before the
reference is materialized. ECZN level locking, XLCM difficulty multipliers,
zone resets and actor respawn are not connected. Therefore this increment
removes unresolved actor initialization without establishing vanilla encounter
selection. A single actor has one result, so LVLF's per-count flag adds no
extra selection in this path. Multi-actor count expansion remains unfinished.

Source evidence is local xEdit LVLN, LVLO and NPC definitions, UESP LVLN/NPC
notes, CommonLibSSE-NG TESLeveledList::GetChanceNone, and skymp NPC_.h and
EvaluateTemplate.h for group ownership. OpenMW's mwmechanics/levelledlist.cpp
provides the highest-eligible/all-level comparison. Its Morrowind flag positions
and missing-form behavior were not copied. skymp LeveledListUtils.cpp was also
read, but its selection omits the highest-level restriction and treats a chance
global as 100 percent, so those policies were not reused. The engine's existing
leveled-list state and saved RNG are reused. No dependency was added.

SOEW version 10 appends sorted per-reference choices and their selection
levels. Versions 4 to 9 remain readable with the earlier actor-value limits
and an empty selection table. Tests cover a reference saved before actor
initialization, stable inventory and random state after reload, retained empty
spawns, old-format loading, corrupt choices and atomic load rejection.
Base-form queries do not choose a result for a spawn. They retain the editor
base's fields when inheritance reaches a leveled list. Generated temporary
ActorBase identity and complete GetLeveledActorBase semantics remain pending.

The installed five-master survey initializes every one of the 6,361 NPC bases
at selection levels 1, 20 and 50, for 19,083 actor initializations. Each pass
has zero suppressed spawns and zero source errors. The full population saves
and reloads to the same world hash. The acceptance threshold, more than 6,000
successful actors and zero errors per level, was written before the first run.
This exercises authored inputs and runtime initialization, not comparison with
a running vanilla executable or rendered NPC spawning.
