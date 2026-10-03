# Combat perk dispatch prerequisites

CMB-09 consumes PRG-08's dispatcher for the ModAttackDamage and
ModIncomingDamage handlers. It applies supported numeric operations in
winning-record order, then ascending PRKE priority within a PERK, preserving
source order for equal priorities. Cross-perk interleaving remains in
winning-record order. The following research was completed on 2026-09-20.
Exact vanilla priority direction, equal-priority order and cross-perk
interleaving remain unverified. This handler places outgoing
and incoming modifiers before armor and other damage reductions, a local
execution choice that still needs a reference-runtime trace. CMB-15 depends on
CMB-09 because skill modifier actor values are inputs to the winning
skill-boost PERK records.

## Established format contract

TES5Edit `Core/wbDefinitionsTES5.pas` defines ModAttackDamage as entry 35 and
ModIncomingDamage as entry 36. Its PERK definition declares PRKE effect rank
and priority, DATA entry/function/tab count, PRKC condition tabs, CTDA
conditions and the EPFT/EPFD payload. `wbDefinitionsCommon.pas` displays the
stored effect rank plus one. A stored rank of zero means rank one.

Local `uesp-modfileformat/PERK.wikitext` gives the condition contexts:

| Entry point | Tab 0 | Tab 1 | Tab 2 |
|---|---|---|---|
| ModAttackDamage | Perk owner | Weapon | Target |
| ModIncomingDamage | Perk owner | Attacker | Attacker weapon |

Top-level PERK conditions govern acquisition. Entry conditions are separate.
An actor-value operand is its numeric ID encoded as an f32 followed by an
f32 coefficient, confirmed by TES5Edit's actor-value conversion and Mutagen's
`Mutagen.Bethesda.Skyrim/Records/Major Records/Perk.cs` decoder.

The numeric functions include set, add, multiply, random addition, absolute
and negative absolute value, and owner-actor-value operations. Function 14
multiplies by one plus the actor value times its coefficient. This is the
function used by the installed PerkSkillBoosts and AlchemySkillBoosts combat
entries. The format declaration does not establish random distribution or
negative-damage execution behavior.

## Missing execution evidence

UESP explicitly describes priority ordering as assumed. The local CK corpus
has no perk-editor reference and the live CK pages were inaccessible during
this research. A [mod author's reproduced stacking issue](https://forums.nexusmods.com/topic/7753268-le-new-damage-perk-seems-to-cause-stacking-problem-wrong-damage-output/)
reports that equal-priority addition and multiplication varied with perk
acquisition order, while assigning distinct priorities removed the conflict.
The engine chooses source order for ties to make results deterministic, but
that choice needs a vanilla capture. Ascending priority is also an engine
choice pending that capture. The `PRG-08 priority-and-interleave capture`
should start at damage 10 with Add 3
and Multiply 2, vary their priorities, then test equal-priority sections in
both source orders. Repeat across perk acquisition order, remove and re-add,
and save and reload. Record each operation sequence and resulting damage.
This settles priority direction and ties within one PERK. Test the same
noncommuting pair across two perks and vary acquisition order to establish
whether and how the runtime interleaves perks. Sorting ties by FormID would
invent behavior. The public
[Perk Entry Point Extender implementation](https://github.com/NoahBoddie/perk-entry-point-extender/blob/main/src/EntryPoint.h)
supports reversing an existing runtime entry array but delegates ordinary
evaluation to Skyrim. It does not establish the complete ordering algorithm.

TES5Edit, Mutagen's `Npc.xml` and CommonLibSSE `BGSPerkRankArray.h` retain the
NPC PRKR rank byte. UESP's NPC page calls that byte no longer in use. The
dispatcher reads the rank from an NPC actor base's PRKR entry and uses the
highest owned position in the NNAM chain for other actors. A synthetic fixture
checks that NPC rank two admits a rank two entry. Exact versus cumulative
selection and NPC versus player differences still need a vanilla capture.
NNAM links alone do not establish automatic replacement of earlier ranks.

The current engine stores runtime perk membership in a BTreeSet. It retains
neither acquisition rank nor acquisition order. NPC decoding keeps the PRKR
rank byte available to entry-point dispatch, while other actor ranks are
derived from NNAM chains. Those state and save contracts need to follow the
verified execution rules.
The generic condition evaluator lacks several actor-value and weapon
predicates used by perks. Its record owner must remain the PERK for resolved
FormIDs, independently from each condition tab's subject. CTDA Run On Target
must not be assumed to mean the third perk argument.

## Reference-runtime checks needed

Use a small authored plugin with noncommuting operations and record the
runtime version, plugin load order, initial state and observed health deltas.

1. At base damage 10, Add 3 and Multiply 2 distinguish 26 from 23. Vary
   priorities, equal-priority entry order within one PERK, acquisition order
   across PERKs, remove/re-add and save/reload.
2. Put distinct effects at stored ranks zero and one. Compare player
   acquisition, NPC PRKR rank values, duplicate AddPerk, removal, NNAM chains
   and save/reload. Determine exact versus cumulative selection.
3. Isolate each condition tab and CTDA run-on mode, including weapon forms,
   failed and empty tabs, explicit references and cross-plugin links.
4. At base damage 100 and 50 percent armor reduction, incoming Add -10
   distinguishes 45 before armor from 40 afterward. Repeat with difficulty
   and noncommuting outgoing and incoming functions.

Calculation must remain transactional: decode, evaluate and validate every
applicable operation before changing health or emitting events. The existing
perked-actor rejection remains until the execution contract is implemented.
