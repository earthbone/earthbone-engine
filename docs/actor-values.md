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

SOEW version 9 writes sorted names and all four components. Case-insensitive
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

Leveled templates need a persistent selection per spawned reference.
Class/level calculations and race skill bonuses are missing. Missing RACE
DATA retains the previous synthetic-record fallback. Other NPC properties,
including flags, inventory, spells and factions, still use the older template
view and need their own group and namespace corrections.

No melee contact, weapon damage formula, armor/block/perk evaluation,
death-event sequence, stagger, attack animation or host attack input was added.
The separate magic branch must use these modifier domains for active effects.
The full gameplay-foundations objective remains open.
