# Host, rendering and audio dependency audit

Reviewed against master `4e03b82` on 2026-09-20. `BACKLOG.csv` remains the
task and progress register. This document explains its dependency edges,
including implementation ordering chosen to share existing mechanisms.
It does not claim that any implementation or runtime acceptance check passed.

## Host ownership and animation

`godot_layer::add_cell` batches surfaces from several references into one
instance. `add_instance` uses an identity transform because placement is baked
into vertices. Collision separately retains the owning reference. A swing or
animation cannot move an entire material batch without moving unrelated forms.
HST-01 and HST-10 therefore follow HST-04's reference-level rendering and
collision update path. This is an implementation ordering decision, not a
claim that each feature needs its own scene node.

`nif::blocks` retains controller block links but does not decode the transform
controllers. The local `nifxml/nif.xml` defines `NiTimeController` frequency,
phase, start/stop time, cycle flags and target, and the interpolator link in
`NiSingleInterpController`. RND-08 supplies that shared foundation. HST-01 and
HST-10 follow RND-08 to play authored reference animation rather than invent
a fixed swing angle or treat an event dispatch as completed animation.

`World::activate_with_unload` toggles ordinary door state but has no FURN
activation branch. `World::animation_event` records and dispatches events,
while `SkinnedActorNode` independently runs an HKX interpreter. HST-09 follows
HST-10 to connect a seated actor's host pose and completion to the shared game.
The local xEdit `wbRecord(FURN)` declares active sit/sleep markers and FNPR
entry animation types. HST-11 retains HST-09 as its marker ownership and
release prerequisite. This does not add a dependency on AI-20, whose Sit
procedure already depends on HST-09, and avoids that cycle.

HST-03 retains UI-08. The existing key check does not provide a lockpicking
interaction. UI-08 in turn depends on UI-01. A load door can exercise this
contract independently of swinging-door animation.

## Streaming and controller consumers

`PlayerNode::sync_game_pose` refreshes resources on a changed logical cell,
but ordinary movement only writes the player's position into the game.
`streaming::CellStreamer` already plans grid loads and hysteresis unloads.
HST-02 connects those decisions to walking. RND-03 follows HST-02 so distant
terrain residency and replacement use the live moving focus. RND-10 retains
RND-03 for the shared LOD residency and replacement mechanism. HST-08 retains
HST-02 so its per-tick budgets constrain that actual traversal path.

RND-13 and RND-14 follow RND-08 for the common controller target and clock
implementation. In local `nifxml/nif.xml`, shader float/color controllers and
`NiPSysEmitterCtlr` inherit through the same `NiSingleInterpController` and
`NiTimeController` base as transform controllers. Material tracks and emitter
lifetimes remain their own work. They do not require one another.

RND-07 retains RND-04 so the occlusion measurement uses the retained batching
or instancing path. An improvement against a discarded path would not justify
keeping the occluders.

## Audio boundaries

`game::sound::prepare` already resolves standard SNDR, SNCT and SOPM records,
but explicitly refuses CTDA conditions. AUD-09 follows AUD-07 so cell and
region ambience can evaluate descriptor eligibility without bypassing authored
conditions. Local xEdit `wbRecord(SNDR)` supplies the descriptor conditions,
and `wbRecord(REGN)` supplies region sound entries.

AUD-04 is independent of AUD-07. In local xEdit, SOUN is a sound marker with
an SDSC link to SNDR. Resolving that link does not require implementing every
conditional descriptor. AUD-05 is also independent of AUD-07: xEdit places
music conditions on MUST records and fade duration on MUSC. It must use the
game's condition evaluator, but does not consume the SNDR condition path.
No blanket dependency on AI-01 is added: that row covers one specific group
of condition functions, not the existing evaluator as a whole. A required
unsupported function found in a fixture must be linked to its actual AI row.

## Independent work and evidence requirements

The other reviewed rows retain no unfinished prerequisites. Existing terrain
geometry and texture loading support RND-01 and RND-02. RND-04 can compare
static repeated geometry without runtime reference mutation. Existing skinned
surfaces support RND-05 independently of HST-10. Glow and environment inputs
are separate material features. Grass can read LAND/LTEX/GRAS and exercise
explicit cell unloads without waiting for terrain texture rendering or a
walking benchmark. Marker filtering has its own installed-record survey.

HST-06 can measure cold preparation and its fade decision at the existing
door transition. HST-07 can prefetch authored door destinations independently
of exterior grid walking. Neither requires comparing positions from separate
interior coordinate systems. HST-05 requires a tracked windowed process and
a human observation of a named sound on the selected physical output. Mixer
capture, which already exists, cannot supply that observation.

The audio decoder tasks operate on separate payloads. AUD-06 caches generated
tracks from the existing viseme source and does not depend on AUD-01's LIP
grammar. AUD-10's hinted alignment comparison can run independently of caching.
AUD-08 concerns archive selection and VFS precedence, not a new decoder.
Controlled LIP generation, NX Opus input, a non-English voice fixture and
alignment ground truth must be available for their respective acceptance
checks. Their availability has not been established by this source audit.

## Validation scope

The audit checks dependency IDs, cycles and CSV constraints. It records three
delegated rows as work in progress on their assigned branches. No row becomes
done from a source inspection, a test that skipped its external data, or an
unapproved diff. The repository gate and each row's own acceptance check remain
required before reporting completion.
