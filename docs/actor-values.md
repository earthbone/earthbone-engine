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
- CommonLibSSE-NG `ActorValues.h` and xEdit
  `TES5Edit/Core/wbDefinitionsTES5.pas` define the full actor-value index
  range from 0 through 163. The runtime resolves every index to its
  case-insensitive Papyrus name, including the effect and skill-progress
  values above ResistDisease at 45. It retains the existing `marksman` and
  `speechcraft` Papyrus spellings for the editor's Archery and Speech labels.
- xEdit `TES5Edit/Core/wbDefinitionsTES5.pas`, NPC DNAM and RACE DATA,
  with Mutagen's NPC PlayerSkills definition and UESP NPC/RACE descriptions.
  NPC health starts at byte 36, magicka at 38, stamina at 40, far-away model
  distance at 44 and geared-up weapons at 48. The old reader and synthetic
  builder agreed with each other at incorrect offsets, hiding the error.
- skymp GetBaseActorValues and EvaluateTemplate supplied comparison points
  for race starting attributes plus ACBS offsets and template groups.
  Its unfinished auto-calculation was not adopted as a runtime formula.
- xEdit `wbDefinitionsTES5.pas` CLAS DATA and RACE Skill Boosts, UESP
  `CLAS.wikitext` and CommonLibSSE-NG `TESClass.h` supplied the class skill
  and attribute weight layout, the race skill boost pairs and the Weight
  Calculations formula an auto-calculated actor's skills, health, magicka
  and stamina now use. The attribute half of the formula adds a flat
  `5*(Level-1)` to health on top of its own weighted share and, unlike
  skills, has no upper clamp, matching the wikitext note that health,
  magicka and stamina carry no cap once the weighted formula runs.

The new race view only reads the DATA attributes needed here. Stats and
traits follow independent NPC template chains, with cycle detection and
winning-file link resolution. Manual attributes use race starts plus ACBS
offsets. Auto-calculated attributes with a resolvable class and a non-zero
attribute weight sum use the CLAS.wikitext Weight Calculations formula
(class-weighted racial start plus the ACBS offset), the same as skills. An
auto-calculated actor with no class, an unresolvable class, or a class whose
attribute weights sum to zero keeps DNAM's authored cache without adding the
offsets again. A `PC_LEVEL_MULT` actor also keeps the DNAM cache, matching
skills, since this store does not track a live player level.
The existing ESM Race view was checked, but it reads FaceFX PHTN/PHWT rather
than DATA attributes. The game view therefore adds the missing prefix only.

`World::actor` seeds a spawned actor's values from the resolved stats and
traits sources once. Manual NPC skills preserve the DNAM cache because the
available sources do not establish whether manual values already contain race
boosts. Fixed-level auto-calculated skills use the class weight formula with
the resolved race boost. The resulting actor values are saved in SOEW, so
loading an existing actor restores its initialized values without running the
seed calculation again. `prg_09_race_skill_initialization.rs` checks cache
preservation and reload behavior with a synthetic spawn and an installed-data
sample. Manual-mode race bonus initialization remains pending PRG-15 evidence.

The store rejects non-finite inputs and overflowing components or totals
before mutation. Resurrection clears health damage and death identity while
retaining modifiers. Essential actors reject death. Deferred kills retain the first lethal request
until EndDeferredKill. The lifecycle below delivers OnDying and OnDeath.
Bleedout, protected-actor attacker rules and invulnerability remain unfinished.

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

### Live run (2026-09-24)

The class attribute weight formula (health, magicka, stamina) reran the same
survey after landing: 161 races, 6,361 NPC bases, 4,718 resolved sources,
4,265 cached auto-calculated values, 1,643 LVLN templates, no other source
errors. Player, Hulda and MQ101Alduin still resolve to 100/100/100, 75/60/60
and 2355/50/80, unchanged from the 2026-09-20 run, so none of the three
examples carries a resolvable class with a non-zero attribute weight sum.
`prg_15_actor_skills.rs`'s installed survey, which now also exercises this
path, still finds every attribute-eligible auto-calculated skill value inside
0 to 100 across the full 6,361 base population. These values have not been
checked against a running vanilla game.

## Remaining gameplay work

Leveled templates now retain a selection per reference, as detailed below.
Encounter-zone levels and zone reset are described in [game.md](game.md).
Manual skills retain DNAM's cache until race-bonus semantics are established.
Health, magicka and stamina follow RACE starts for a manual actor's base, and
CLAS.wikitext's Weight Calculations for a
fixed-level auto-calculated actor with a resolvable class and a non-zero
weight sum. A `PC_LEVEL_MULT` actor keeps the DNAM cache for both, since this
store does not track a live player level. Missing RACE
DATA retains the previous synthetic-record fallback. Runtime flags, inventory, spells, perks, factions, outfits and packages now
resolve their owning template groups. Script, keyword, AI-data and attack-data
inheritance still need the corresponding runtime consumers.

No melee contact, weapon damage formula, armor/block/perk evaluation,
stagger, attack animation or host attack input was added.
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
zone reset and actor respawn were not connected in this increment. Therefore it
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
base's fields when inheritance reaches a leveled list. The temporary-base
follow-up below adds a distinct identity for per-reference ActorBase queries.

The installed five-master survey initializes every one of the 6,361 NPC bases
at selection levels 1, 20 and 50, for 19,083 actor initializations. Each pass
has zero suppressed spawns and zero source errors. The full population saves
and reloads to the same world hash. The acceptance threshold, more than 6,000
successful actors and zero errors per level, was written before the first run.
This exercises authored inputs and runtime initialization, not comparison with
a running vanilla executable or rendered NPC spawning.

## Restoring saved template choices, work in progress

`World::restore_vanilla_actor_templates` runs after saved reference placement
and before actor initialization. It validates the original base against the
saved reference, finds the selected NPC through the original template's nested
LVLN graph, and retains the subsequent static template chain. It does not roll
chance-none or apply current level eligibility to an already-made choice.
Current live leveled-list edits, when present, participate in membership checks.
Missing choices, mismatched bases, duplicate references and incomplete chains
fail before any choices are installed. More than one unresolved leveled link
in a single NPC chain is still unsupported.

The saved leveled-creature entry identifies original and selected bases but
does not identify the encounter level that originally chose them. Imported
choices therefore have no selection level. SOEW version 12 encodes that as
zero and reads it back as an absent level. Versions 10 and 11 still reject
zero. New engine spawns continue to require and record a positive selection
level. No player-level substitute is invented during restoration.

The prefix decoder stops after ExtraLeveledCreature. This lets the template
pass read its own state without interpreting later unsupported package data.
The complete reference-contents decoder still requires every extra entry and
retains its failing PackageData acceptance check.

Public sources were CommonLibSSE-NG `ExtraLeveledCreature.h`, CK
`GetActorBase`/`GetLeveledActorBase`, xEdit's save definitions and UESP extra-data
notes. The first 156,154 fully decoded entries all named the reference's
original NPC first and a different selected NPC second. The prefix reader also
reaches fifteen leveled entries whose later package data is not decoded yet.
No NPC choice is inferred from current RNG or current player level.

Across 60 saves, all 156,169 recorded actor choices restore and survive engine
save/load, with unchanged RNG and no queued events. Synthetic tests cover
late failures, current chance-none and level differences, inherited inventory,
unknown selection levels and old-format rejection. The current pass creates
156,169 private NPC records and applies all 4,930 inline NPC changes, including
saved ACBS, AIDT and DNAM. Actor initialization reads the resulting stats.
GetActorBase retains the authored identity, while GetLeveledActorBase returns
the private record. SOEW version 14 persists the records and their owning
references. See [game.md](game.md#temporary-actor-bases) for inheritance,
namespace handling and validation.

Saved actor-value modifiers, AI execution and combat state remain separate
restoration work. Temporary-base cleanup on reset, deletion and respawn, and
saved Papyrus bindings to vanilla temporary identities are not implemented.

## Death lifecycle increment, unmerged

The combat-lifecycle worktree builds on the frozen inventory/NPC restoration
review. Kill, KillSilent, lethal actor-value changes and supported lethal magic
share the same death transition. Death records the killer, clears combat state
and queues OnDying followed by OnDeath. Repeated kills do not replace the killer
or queue duplicate notifications. Healing alone does not revive a dead actor.
Resurrect clears health damage and any held deferred-death request.

StartDeferredKill keeps the actor alive through lethal damage or Kill.
EndDeferredKill resolves the first held request, preserving its killer and
silent flag even if the actor healed in between. Held requests are persistent
state and do not keep the VM pump busy. ActorBase.SetEssential updates already
initialized actors that inherit that base-data source, and later actors read
the same runtime flag. Private temporary actor bases keep their own flag.

The event recipient snapshot contains the actor, its running quest aliases
and its active effect objects. Alias order is sorted, effect order follows
stable effect IDs. Clearing an alias or expiring an effect before delivery does
not redirect an already queued notification. Instant lethal magic registers
its effect before capturing recipients. This includes the lethal effect's own
script in death notifications.

SOEW version 15 persists held requests and undelivered death notifications,
including recipient handles. They also contribute to the state hash. Loading
rejects inconsistent held requests, duplicate held actors, empty recipient
lists and duplicate recipients before replacing world state. Version 14 and
older saves retain their existing payload shape and use nonpositive health
when EndDeferredKill has no saved request. No dependency or host node was added.

Sources are local CK StartDeferredKill, EndDeferredKill and KillSilent pages,
and skse64/scripts/vanilla/Actor.psc, including KillEssential, plus the death
event declarations in Actor.psc, ReferenceAlias.psc and ActiveMagicEffect.psc. Existing actor-value components, the pending queue and
engine-save IO were reused. The local skymp TES5DamageFormula was inspected,
but its unfinished armor, perks and block calculations were not adopted.

The tests cover native calls, essential changes, duplicate kills, resurrection,
healing during deferral, alias snapshots, active-effect events, engine saves,
old-version fallback and atomic rejection of invalid queues. They do not prove
original-runtime parity. KillSilent currently suppresses both Papyrus death
events, interpreting the CK phrase about no kill event. Whether that phrase
also describes a separate story event remains unverified. The healed deferred
request behavior follows the CK wording and also needs a vanilla comparison.

Both events enter the VM during one pump. There is no death-animation completion
signal or ragdoll timing yet. Essential alias flags, bleedout, protection by
attacker identity, invulnerability, weapon contacts, damage formulas and combat
AI remain unfinished. Full vanilla ESS restoration is a separate open scope.

Validation on 2026-09-20 passed the combined tools/lint.sh gate with 1,151 Rust
and 11 Python tests, no warnings and no skipped optional tools. The installed
five-master survey initializes 6,361 actors, kills 6,018 and retains 343
essential actors. Death state, killer attribution, queued notifications and
save bytes survive reload. It exercises immediate and deferred kills with
thresholds fixed before its first run. The installed run passes all 11
lifecycle tests, 42 game-side save tests and nine footstep tests.

The save surveys retain their earlier limits: 126 unsupported actor inventory
records are excluded, and full reference decoding still has 169 unsupported
PackageData payloads in this tree. The separate empty-package decoder increment
has not been copied here. Neither these surveys nor the death tests establish
full ESS resumption or a completed combat system.
