# Runtime execution work

Requested on 2026-09-19 after the mechanical batch at `ca6239c`. The goal is
execution of dialogue, combat, magic and crafting, host door/footstep
integration, and full vanilla-save restoration. None of these is complete
merely because records parse or a synthetic test passes.

The user clarified the implementation order on 2026-09-19: first match Skyrim
well enough to play it, then improve transition presentation and core AI.
Portal doorways and flying across city worldspace boundaries are deferred
experiments. Host integration should first execute the existing door travel
contracts. Visibility and distance may reduce rendering, animation and AI
cost, but must preserve observable schedules, quest state and events.

## Milestones and acceptance evidence

| Area | Required behavior | Evidence needed before completion |
|---|---|---|
| Dialogue | Winning topic and INFO selection, conditions and owning quest eligibility, response order, shared responses, fragments, scenes and voice playback | Authored branching quests and installed dialogue execute through the VM and host, with correct flags, event order and voice routing |
| Combat | Race/NPC/template actor values, equipment and damage rules, melee/ranged attacks, blocking, death/essential/protected behavior and combat AI | Record-backed synthetic encounters plus installed actor/weapon cases, host input and hit detection, state continuation across save/load |
| Magic | Cast eligibility and costs, effect application and expiry, archetypes, resistance, concentration, dispel and effect scripts | Cast/expire/dispel sequences preserve learned/equipped spells, run effect events and continue after save/load, with installed spell coverage and explicit accounting of all unsupported effects |
| Crafting | Bench and recipe conditions, atomic ingredient/output changes, inventory events, tempering, alchemy, enchanting and host interaction | Ordinary and DLC recipes execute, insufficient resources cannot partially mutate inventory, item improvements preserve instances, progression/events persist, host menus invoke the same runtime path |
| Host doors and footsteps | Activation resolves game references, doorway arm/commit/settle lifecycle, renderer/physics synchronization, grounded movement drives record-backed sounds | Godot walkthrough crosses both ways with correct cell events and placement, verifies collision and audible footsteps, with process exit and runtime assertions recorded |
| Vanilla-save restoration | Full ESS world changes, plugin and FormID mapping, Papyrus objects/arrays/stacks/latent state, quest/alias state and continued execution | All supported vanilla/DLC save state is restored from the external corpus, independent state comparisons and resumed VM scenarios pass, host resumes in the saved location, unsupported fields are eliminated rather than silently dropped |

Shared condition evaluation, item instance state, event dispatch and persistence
must stay attached to the existing form-backed World and VM. They are enabling
work, not replacement definitions of the requested end state. Use local xEdit,
CK, UESP, FallrimTools and the reference implementations named in
[reference-audit.md](reference-audit.md), checking their documented limitations.

## Current work

The integrated foundation includes actor-value components and record-backed
actor initialization, COBJ transactions and inventory events, dialogue selection
and fragments, direct non-hostile magic applications, host doors and footsteps,
and two vanilla-save groundwork increments. These share one form table and VM.
SOEW version 9 retains dialogue, active effects and actor-value components
without replaying applied effects or dialogue begin fragments on restore.

The remaining boundaries are listed in [status.md](status.md). In particular,
weapon combat, complete magic/crafting/dialogue, their host interfaces and
full vanilla-save continuation remain unfinished. Save object import still
requires an explicit caller-provided mapping and does not resume saved stacks.
Each subsequent milestone needs its own source review, installed-data evidence
and full lint gate. New work should branch from the combined master baseline.
