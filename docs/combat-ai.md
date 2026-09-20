# Combat AI source work

CMB-31 is in progress on `task-cmb-31`. Its prerequisite increment decodes
NPC AI data and initializes its actor values. It does not yet acquire a combat target, drive
combat movement, or disengage when that target dies. Those remain the original
CMB-17 acceptance check. The prerequisite rows in BACKLOG.csv separate shared
detection, CMB-31, from live host observations, CMB-32.

## NPC AI data

`npc::AiData` reads the serialized 20-byte AIDT layout. Its six initial bytes
are aggression, confidence, energy, morality, mood and assistance. The next
two bytes contain the aggro-radius switch and unused padding. Three unsigned
32-bit radii follow. Lengths and enum values are checked before construction.
The unused byte is ignored as xEdit specifies.

Actor initialization follows the independent AI_DATA template chain and reads
the winning override. Its six actor values enter the existing base-value
components, so ordinary modifiers and engine save/load retain their meaning.
Aggro radii stay in the NPC view until a consumer needs them. This does not add
a second actor store, a save-format version, dependencies, or host nodes.

The layout and enum bounds come from local
`TES5Edit/Core/wbDefinitionsTES5.pas`, `wbAIDT`, and
`TES5Edit/Core/wbDefinitionsCommon.pas`, the aggression, confidence, morality,
mood and assistance enums. `uesp-modfileformat/NPC.wikitext` establishes the
AI_DATA template group. The runtime layout in CommonLib is packed differently
and is not a plugin-byte layout.

## Detection contract

The local CK `IsDetectedBy_-_Actor.wikitext` defines an observer-specific
query. `HasLOS_-_Actor.wikitext` distinguishes visible geometry from detection,
including the player's camera clipping and three visibility picks.
`GetLightLevel_-_Actor.wikitext` describes the measured light input, and
`CreateDetectionEvent_-_ObjectReference.wikitext` supplies event ownership and
sound magnitude.

The [CK detection reference](https://ck.uesp.net/wiki/Category:Detection)
describes score terms for sound, visibility and skills, then Normal, Alert,
Combat and Lost states. The score text repeats attenuation in both the total
and component formulas. It is not sufficient evidence to choose whether that
factor is applied once or twice. It also does not define every skill term.
No approximate score calculation has been added from that text.

[Stealth Points](https://ck.uesp.net/w/index.php?title=Stealth_Points&mobileaction=toggle_view_desktop)
states that an actor's pool follows the highest detection value among
aggressive observers. Combat holds that pool at zero. Alert and lost timers
delay recovery, and sufficiently high detection bypasses the pool. The pool
only delays aggressive detection. Theft witnessing must use the separate
observer result instead of waiting for pool depletion.

Local `CommonLibSSE-NG/include/RE/A/ActorKnowledge.h` and
`RE/D/DetectionState.h` retain observer/target identity and detection history.
`RE/C/CombatGroup.h` stores target detection, stealth points, locations and
timestamps. These structures support the state distinction but do not contain
the arithmetic that updates it.

The [RAID author's GMST table](https://www.nexusmods.com/skyrimspecialedition/mods/2345)
provides measured defaults and names for the drain, recovery, delay and
threshold settings. That table does not establish their exact update formula.
Its lighting notes also distinguish detection lighting from final screen
brightness. CMB-32 must produce physical observations and record-derived
lighting, not a caller's precomputed detection result.

## Source boundary

[The governing intent](intent.md#sources-licensing-and-user-data) requires
published formats, public source and observed behavior for semantics. Executable
disassembly is not an implementation source for this project. The shared
score, observer aggregation and timer implementation remains pending until
those contracts have sufficient permitted evidence.

The CK pages establish the distinction between observer detection, a shared
target pool and engagement state. They do not establish the complete arithmetic
or every timestamp producer. The RAID author's settings table supplies named
measurements, not an independent specification of the updater. A complete
implementation must also evaluate applicable detection perks or reject them
explicitly until the shared PRG-08 dispatcher handles them.

## Permitted observation protocol

The MIT-licensed [Detection Meter source](https://github.com/max-su-2019/MaxsuDetectionMeter/tree/fcc5ef75d6cdabc63db0b214bc61fc272a5b22cf)
provides a concrete observation route. `Hook.cpp` requests the observer's
numeric detection level. `DataHandler.cpp` reads the target's stealth points
from its combat group. Its meter rescales and clamps both numbers for display,
so a measurement probe must log the original numbers before that conversion.
The source observes values and does not implement their calculation.

SKSE's [published console command list](https://skse.silverlock.org/vanilla_commands.html)
also documents `ToggleDetectionStats`, `tds`. Whether that command produces
usable output in the installed retail version needs a live check. A prepared
experiment is not a recorded observation.

Use an isolated test profile and record the runtime version, plugin hashes,
actor identities, real time, frame duration, positions, headings, equipment,
current actor values, game settings, light input and geometric visibility.
Capture each observer's raw score and each group's target identity and raw
pool. Account for detection-query wakeup behavior documented by `IsDetectedBy`.
Do not save experimental GMST or actor-value edits into a normal playthrough.

Before measurement, require repeated stationary score samples to agree exactly.
Require fitted score rules to predict the held-out integer scores exactly.
Pool predictions must agree within 0.01 points, and observed state transitions
within one captured simulation update. Repeat timing cases at 30 and 60 frames
per second. Record failures instead of changing tolerances after the run.

| Experiment | Evidence it must distinguish |
|---|---|
| Stationary actor at several distances, with sound and visual inputs isolated | The number of attenuation applications and behavior beyond maximum distance |
| Independently vary observer and target Sneak, including the player | Skill scaling, standing versus sneaking, bounds and player-specific settings |
| Change only the light multiplier or additive setting | Operation order, minimum light, interior and exterior inputs |
| Walk, run and stand with several equipped weights and noise modifiers | Movement sound, action sound and visual motion terms |
| One hostile observer, then two with different raw scores | Maximum versus summed drain, observer removal and group sharing |
| Lose visibility, then emit a sound or attack during recovery | Wait starts, wait resets, drain, regeneration and clamps |
| Approach the combat threshold from both sides | Equality behavior, pool bypass and raw detection versus aggro |
| Repeat a non-hostile detection query while pool is nonzero | Witness detection remains independent of hostile pool depletion |

The [Dynamic Muffle Scaling author's measurements](https://www.nexusmods.com/skyrimspecialedition/mods/16899?tab=posts)
identify a default `MovementNoiseMult` of 1.0 for player and NPC actors, reduced
by Muffle effects. The shared actor-value store currently leaves this value
unseeded. Verify and seed that base in the shared store before implementing
movement sound, preserving modifiers through save/load.

## Target selection

Local `TiltedEvolution/Code/client/Services/Debug/Views/CombatView.cpp`,
`CalculateTargetScore`, is the concrete target-scoring reference. It combines
recent detection, the current attacker, ranged-weapon agreement, distance,
other attackers, movement reachability, death and fleeing. Several helpers
still call addresses in Skyrim's executable. Their contracts need confirmation
before this code can be used as a complete selector.

The present package driver stores paths in `AiState`, while combat identity
already belongs to `ActorState`. Integration must use that shared actor state
and the existing vanilla navmesh. `loaded_actors` means initialized actor state,
not live host residency, and the package planner can fall back to a straight
line. Neither behavior alone establishes combat visibility or a reachable
combat route.

## Validation

The focused `combat_ai` tests cover malformed AIDT, template-group selection,
winning overrides across a two-link template chain, independent child stats,
and modifier persistence through engine save/load. The installed five-master
survey decodes all 6,361 winning NPCs and all 6,361 authored AIDT blocks.
These are parser and initialization checks. They do not execute detection or
the CMB-17 engagement check.

The AIDT milestone passed the full gate on the CMB-31 branch from integrated
master `617687c`, with 1,195 Rust test results and 11 Python tests. All optional
tools ran. The gate excludes external game data and save-corpus runs.

Before transfer from `combat-ai`, the installed restoration survey checked
13,362 NPC records across 60 saves, including authored AIDT bytes and record
persistence. The synthetic save tests separately check the six live AI actor
values. CMB-31 remains in progress because its scoring and shared-pool
acceptance check has not been implemented.
