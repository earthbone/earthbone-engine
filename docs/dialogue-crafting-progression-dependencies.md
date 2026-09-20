# Dialogue, crafting and progression dependencies

Reviewed against `4e03b82` on 2026-09-20 in `dlg-crf-prg-dependencies`.
`BACKLOG.csv` remains the task and status registry. This document records
dependency decisions, not a second completion list. The audit changes no
gameplay behavior and establishes no original-runtime equivalence.

An edge names an unfinished capability needed by the consuming row. Shared
files and useful implementation order alone do not justify an edge. Existing
capabilities need no open prerequisite. Work in other branches is not assumed
to be available in the audited baseline.

## Changed dependencies

| Consumer | Required rows | Reason |
|---|---|---|
| DLG-08 | PRG-05 | Merchant container merchandise can contain LVLI entries. Resolve those before exposing eligible merchandise. INV-01 adds concrete reference inputs to AddItem, which vendor-data resolution does not call. |
| DLG-09 | FMT-04, FMT-18 | Scheduling consumes SCEN fields and its begin, end and phase fragment declarations. Scene state currently records only playing membership and OnBegin/OnEnd callbacks. |
| DLG-10 | UI-01, DLG-12 | Native controls and resolved topic/response text are required to display shipped conversations. A localized string ID is not the player's choice text. |
| DLG-13 | None | Speaker resolution consumes a filled alias. World::alias_ref and force_alias already supply and clear that identity. AI-12 adds a search-based filling method. |
| DLG-16 | PRG-01, PRG-07 | Training advances a skill and tracks purchases per character level. Skill experience and character level transitions are separate rows. |
| DLG-18 | DLG-09, DLG-13 | A SCEN dialogue action names an actor alias and a DIAL. It needs action lifetime scheduling and alias-backed speaker resolution. |
| CRF-01 | CRF-04, PRG-08 | Tempering mutates one selected instance and applies the Mod Tempering Health entry point. |
| CRF-02 | PRG-08 | Alchemy effectiveness is an authored perk entry point. MAG-11 applies a consumed potion's effects, which is independent of creating its ALCH definition and inventory item. |
| CRF-05 | PRG-14 | OnStoryCraftItem targets a quest selected and started by Story Manager. Existing inventory callback dispatch is not that scheduler. |
| CRF-06 | UI-01, DLG-12 | Recipe controls need localized item names. Existing Game::craft owns the ordinary COBJ transaction. |
| CRF-07 | CRF-03, CRF-04, PRG-08 | Creation needs learned enchantments, selected item mutation and the enchantment-power, soul-gem and enchantment-count entry points. |
| PRG-01 | PRG-08, AI-37 | Skill-use accumulation applies Mod Skill Use. Its advance-object and action predicates, CTDA 691 and 692, belong to AI-37. The curve also reads AVIF AVSK, not only GMST. |
| PRG-02 | None | Perk purchase consumes the existing saved World::perk_points balance and current actor values. Earning new skill XP is not required to spend already granted points. |
| PRG-04 | None | ECZN clamps the selection level used by existing actor-template selection. PRG-03 computes attributes after selection and does not supply the zone's level lock. |
| PRG-09 | PRG-15 | Skill initialization needs verified manual and automatic calculation rules. DNAM caches can already include racial bonuses. |
| PRG-12 | PRG-06, PRG-13, DLG-10 | Arrest needs bounty state, an executable jail outcome and the choice interface. Jail service can be checked before the guard conversation. |
| PRG-13 | PRG-06, CRF-04, PRG-01 | Jail uses faction bounty, transfers selected confiscated stacks and applies skill-progress penalties without losing instance metadata. |

DLG-16's read-first sources now name CLAS trainer data and CommonLib's
`skillTrainingsThisLevel`. FACT vendor fields alone do not define a trainer.
PRG-01's sources now include AVIF skill coefficients and the separate skill
experience/threshold fields in PlayerCharacter.

PRG-14 is the missing Story Manager prerequisite split out before CRF-05 is
claimed. It owns SMEN/SMBN/SMQN selection, quest start with event arguments,
selection/reset history and persistence. The crafting transaction hook stays
in CRF-05. PRG-14 depends on AI-69 for the shared GetEventData predicate
contract. Broadcasting to already running quests would not satisfy either
row. Other event producers and native bindings need their own dependency
review before they are claimed.

## Dependencies retained after review

DLG-01 uses the existing DLBR reader. DLG-03, DLG-04, DLG-05, DLG-06 and
DLG-15 extend the existing conversation selection and lifecycle. Their flags,
history and cursor already have an owning system. DLG-02 and DLG-21 can use
the current speaker/listener, quest priority and close APIs. DLG-07 consumes
current speech values without requiring new XP accrual. None needs a host
menu to check its game-side contract.

DLG-11 retains DLG-10 for host response lifecycle routing. DLG-14 retains
AI-28 for the ForceGreet procedure. DLG-17 retains AI-22 for Follow ownership.
DLG-19 retains DLG-09 and AI-02 for scheduled package/timer execution.
DLG-20 retains DLG-11 and ANM-13 for audio timing and facial morph mapping.
DLG-12 is independent. It builds on ESM localized text identities and the VFS.

CRF-03 retains CRF-04 for destructive selection of the enchanted instance.
CRF-04 needs no new storage prerequisite: the canonical Inventory already
retains stack extra data through engine saves. CRF-08 retains PRG-01 for
recording skill use after the existing ordinary crafting transaction. It does
not depend on Story Manager dispatch.

PRG-03 derives NPC attributes from records and remains independent of race
skill bonuses. PRG-05 and PRG-11 have separate LVLI and LVSP consumers. They
can reuse the existing LVLN selection reference without waiting for each
other. PRG-06 owns witnessed crime state. PRG-07 retains PRG-01 for skill
advancement input. PRG-08 evaluates already owned ranks and conditions, so
buying ranks is not a prerequisite. PRG-09 requires PRG-15 to establish the
skill calculation before applying initialization once.
PRG-10 retains PRG-04 for clearing a saved zone lock during reset.

## Evidence inspected

Local corpus sources under `engine-refs`:

- `TES5Edit/Core/wbDefinitionsTES5.pas`: FACT VENC/VEND, CLAS trainer fields,
  AVIF AVSK, ECZN DATA, SCEN phases/actions and aliases, SMEN/SMBN/SMQN,
  COBJ outputs and perk entry points 22, 66, 76, 77, 79 and 80.
- `CommonLibSSE-NG/include/RE/B/BGSEntryPoint.h`: named skill-use,
  tempering, alchemy and enchanting entry-point identities.
- `CommonLibSSE-NG/include/RE/P/PlayerCharacter.h`: separate skill
  progress/threshold state, jail sentence and training count per level.
- `CommonLibSSE-NG/include/RE/T/TESClass.h`: training skill and maximum.
- `ck-wiki/Quest_Script.wikitext`: OnStoryCraftItem fires when Story Manager
  starts a quest, with bench, location and created-item arguments.
- `ck-wiki/Game_Script.wikitext`: skill experience, training and jail APIs.

Code inspection covered the dialogue and crafting execution boundaries,
scene playing state, inventory callbacks, actor initialization, alias state,
perk points, and native stubs. `vmad.rs` retains SCEN fragment tails as raw
bytes. `quest/state.rs` retains saved story arguments without dispatching
them. Serena confirmed the alias lookup and mutation definitions.

## Validation

CSV validation checks all 541 rows. It checks unique IDs, required fields,
title/source/crate bounds, canonical quoting, valid status/evidence/date
fields, retained original IDs, existing non-dropped prerequisites and cycle
freedom across the whole graph. All checks pass. The selected scope now has
44 rows, including Story Manager and live NPC skill calculation. Nineteen existing rows
change, including two completed implementations and one active implementation claim.

These edges describe implementation requirements. They do not claim that
the named external AI, format, UI or animation work is complete. The full
repository gate is required before a commit. Changes remain unstaged until
the user approves their concrete diff summary.

## Race skill scope correction

The PRG-09 installed-data survey found 678 of 820 fixed-level, level-one
auto-calculated NPC sources whose DNAM skill arrays already equal 15 plus
their resolved race boosts. Unconditionally adding the boosts would apply
them twice to this population. The other 142 records are not classified by
that equality check. It is not a comparison against a running vanilla game.

A second survey found nine manual level-one sources whose arrays also match
15 plus race boosts. Neither survey proves how the original runtime interprets
manual DNAM values. xEdit and CommonLib expose skill values and offsets without
a manual-mode distinction. The local NPC format documentation qualifies the
health, magicka and stamina calculation, but does not settle skill semantics.
The proposed manual-only addition therefore remains unapproved.

PRG-15 owns source-grounded skill calculation for manual and automatic modes,
including offsets, class weights, resolved level, winning race bonuses,
rounding and caps. PRG-09 retains its original initialization contract and
depends on PRG-15 so bonuses apply once across spawn and reload. Cached DNAM
bytes do not identify the inputs used to calculate them. Reusing those bytes
alone cannot honor independent RACE overrides or separate STATS and TRAITS
template sources. PRG-03 continues to own health, magicka and stamina.
