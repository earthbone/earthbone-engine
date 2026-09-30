# soe-hkx: Skyrim SE animation files and the behavior runtime design

The `hkx-rename` step renamed the former `soe-havok` package and `crates/havok`
directory to `soe-hkx` and `crates/hkx`. Consumers import `soe_hkx` and the
fuzz target is `fuzz-hkx`. The parser, writer and interpreter are unchanged.
Existing local fuzz inputs can be moved from `fuzz/corpus/fuzz-havok` to
`fuzz/corpus/fuzz-hkx` before using the renamed target. The name describes the
format this crate reads. It does not link the Havok SDK.

Crate `crates/hkx` reads the Havok 2010.2 packfiles Skyrim Special Edition
ships: skeletons, spline-compressed animation clips, character and project
descriptions, and behavior graphs, and runs the behavior graphs through the
interpreter in `interp.rs` (section 6). This document records what the crate
reads, where each layout came from, the design for the interpreter that the
map's weeks 9 to 13 ask for, and what the first implementation of it does.
No Havok SDK code was consulted or is required. The sources are the class descriptions serde-hkx
extracted from Skyrim's own files (`engine-refs/serde-hkx/assets/classes`),
the reverse-engineered runtime layouts in activeragdoll's
`include/RE/havok_behavior.h`, and HavokLib's spline decompressor for the
control point packings.

## 1. What the crate reads

| File kind | Root class | Skyrim path | Module |
| --- | --- | --- | --- |
| Rig | `hkaAnimationContainer` with one or two `hkaSkeleton` | `character assets/skeleton.hkx` | `skeleton` |
| Ragdoll | `hkaRagdollInstance` | `character assets/skeleton.hkx` | `ragdoll` |
| Clip | `hkaAnimationContainer` with one `hkaSplineCompressedAnimation` and its `hkaAnimationBinding` | `animations/*.hkx` | `skeleton`, `animation`, `spline` |
| Character | `hkbCharacterData` | `characters/*.hkx` | `character` |
| Project | `hkbProjectData` | `*project.hkx` | `character` |
| Behavior | `hkbBehaviorGraph` | `behaviors/*.hkx` | `behavior` |

The container format (`packfile`) is the memory-image layout: 64-byte header,
three section headers, a class name table, then the `__data__` section with
objects at their in-memory offsets, followed by three fixup tables. Local
fixups resolve array contents and strings, global fixups resolve object
pointers, virtual fixups name each object's class. Every read is bounds
checked and returns `Error` on malformed input. The reader takes the file's
pointer size from the header. Typed readers select explicit x86/x64 field
pairs through `Obj::layout_offset`, including embedded struct strides and
`hkUlong` width. The existing skeleton, animation, binding, behavior,
character and project models read both Skyrim LE and SE files.

Checked against real files on 2026-09-06 with the gated tests in
`tests/real_files.rs` (fixtures extracted from `Skyrim - Animations.bsa`
into `engine-refs/skyrim-hkx-fixtures`, set `SOE_HKX_DIR` to point elsewhere):

| File | Result |
| --- | --- |
| `character assets/skeleton.hkx` | 2 skeletons, the first with 99 bones and 8 float slots, `NPC Root [Root]` first, head above 100 units in model space |
| `animations/mt_jump.hkx` | 1.733 s, 53 frames in one block, 99 transform tracks, 4 float tracks, 99 annotation tracks, every sampled rotation a unit quaternion |
| `behaviors/0_master.hkx` | 787 nodes (103 state machines, 279 clips, no unknown classes), 1,217 events, 230 variables, 12 referenced behavior files, `iState` among the variables |
| `characters/defaultmale.hkx` | rig and ragdoll `Character Assets\skeleton.HKX`, behavior `Behaviors\0_Master.hkx`, 1,656 animation names, 39 character properties |
| Nemesis SSE cached behaviors | 20 behavior files and the 2 character files in `cached_behaviors/SSE` load |

## 2. Layouts used

Offsets are bytes from the object start in the 64-bit layout. Only the
fields the crate reads are listed. `hkReferencedObject` occupies the first
16 bytes of every reference-counted class (vtable, memSizeAndFlags,
referenceCount).

- `hkRootLevelContainer`: `namedVariants` array at 0, elements 24 bytes
  (name 0, className 8, variant 16).
- `hkaAnimationContainer`: skeletons 16, animations 32, bindings 48.
- `hkaSkeleton`: name 16, parentIndices 24, bones 40 (16-byte `hkaBone`:
  name 0, lockTranslation 8), referencePose 56, referenceFloats 72,
  floatSlots 88.
- `hkaAnimationBinding`: originalSkeletonName 16, animation 24,
  transformTrackToBoneIndices 32, floatTrackToFloatSlotIndices 48,
  blendHint 64. An empty index array means track `i` drives bone `i`.
- `hkaSplineCompressedAnimation`: type 16, duration 20,
  numberOfTransformTracks 24, numberOfFloatTracks 28, annotationTracks 40,
  numFrames 56, numBlocks 60, maxFramesPerBlock 64, blockDuration 72,
  frameDuration 80, blockOffsets 88, data 152.
- `hkbNode` (every graph node): variableBindingSet 16, userData 48, name 56.
  `hkbModifier` adds enable at 72.
- `hkbBehaviorGraph`: variableMode 72, rootGenerator 128, data 136.
- `hkbBehaviorGraphData`: attributeDefaults 16, variableInfos 32 (6-byte
  elements: role 0, roleFlags 2, type 4), characterPropertyInfos 48,
  eventInfos 64 (4-byte flags), wordMinVariableValues 80,
  wordMaxVariableValues 96, variableInitialValues 112, stringData 120.
- `hkbBehaviorGraphStringData`: eventNames 16, attributeNames 32,
  variableNames 48, characterPropertyNames 64.
- `hkbStateMachine`: eventToSendWhenStateOrTransitionChanges 72,
  startStateChooser 96, startStateId 104, returnToPreviousStateEventId 108,
  randomTransitionEventId 112, transitionToNextHigherStateEventId 116,
  transitionToNextLowerStateEventId 120, syncVariableIndex 124,
  wrapAroundStateId 132, maxSimultaneousTransitions 133, startStateMode 134,
  selfTransitionMode 135, states 144, wildcardTransitions 160.
- `hkbStateMachineStateInfo`: enterNotifyEvents 64, exitNotifyEvents 72,
  transitions 80, generator 88, name 96, stateId 104, probability 108,
  enable 112.
- `hkbStateMachineTransitionInfo` (72 bytes, by value in the array):
  triggerInterval 0, initiateInterval 16, transition 32, condition 40,
  eventId 48, toStateId 52, fromNestedStateId 56, toNestedStateId 60,
  priority 64, flags 66.
- `hkbClipGenerator`: animationName 72, triggers 80,
  cropStartAmountLocalTime 88, cropEndAmountLocalTime 92, startTime 96,
  playbackSpeed 100, enforcedDuration 104, userControlledTimeFraction 108,
  animationBindingIndex 112, mode 114, flags 115. `hkbClipTrigger` is 32
  bytes: localTime 0, event 8, relativeToEndOfClip 24, acyclic 25,
  isAnnotation 26.
- `hkbTimerModifier`: enable 72, alarmTimeSeconds 80, alarmEvent 88;
  secondsElapsed at 104 is `SERIALIZE_IGNORED`.
- `hkbBlenderGenerator`: referencePoseWeightThreshold 72, blendParameter 76,
  minCyclicBlendParameter 80, maxCyclicBlendParameter 84,
  indexOfSyncMasterChild 88, flags 90, subtractLastChild 92, children 96.
  `hkbBlenderGeneratorChild`: generator 48, boneWeights 56, weight 64,
  worldFromModelWeight 68.
- `hkbBlendingTransitionEffect`: selfTransitionMode 72, eventMode 73,
  duration 80, toGeneratorStartTimeFraction 84, flags 88, endMode 90,
  blendCurve 91.
- Skyrim's own: `BSiStateTaggingGenerator` (pDefaultGenerator 80,
  iStateToSetAs 88, iPriority 92), `BSSynchronizedClipGenerator`
  (pClipGenerator 80, SyncAnimPrefix 88, ...), `BSCyclicBlendTransitionGenerator`
  (pBlenderGenerator 80, events 88 and 104, fBlendParameter 120,
  fTransitionDuration 124), `BSOffsetAnimationGenerator`
  (pDefaultGenerator 80, pOffsetClipGenerator 96), `BSBoneSwitchGenerator`
  (pDefaultGenerator 80, ChildrenA 88).

The serde-hkx `BSBoneSwitchGenerator` declaration gives `ChildrenA` as an
array of `BSBoneSwitchGeneratorBoneData` pointers. Its child declaration gives
`pGenerator` at 48 and `spBoneWeight` at 56. The `hkbBoneWeightArray`
declaration stores the ordered real-valued mask. The loader resolves those
generator pointers into graph nodes and reads the optional mask using the
same checked array reader used for blender masks.

Where activeragdoll's runtime header and serde-hkx's class table both cover
a class (`hkbNode`, `hkbStateMachine`, `hkbBlendingTransitionEffect`,
`hkbBehaviorGraph`) the offsets agree, which is the cross-check that the
serialized layout is the runtime layout for these files.

## 3. Spline animation

A clip is `numFrames` frames at `frameDuration` seconds, cut into blocks of
`maxFramesPerBlock`. A block starts with a four-byte mask per transform
track and one byte per float track, padded to four bytes. The mask packs
the quantization kinds (position in bits 0 to 1, rotation in bits 2 to 5
plus two, scale in bits 6 to 7) and per-axis flags for position, rotation
and scale: bit `n` static, bit `n + 4` spline. A spline sub-track is a
`u16` item count, a `u8` degree, `items + degree + 2` `u8` knots in frame
units, then per spline axis a `f32` min and max, then `items + 1` control
points quantized to 8 or 16 bits across that range, point-major across the
axes. Rotation splines carry whole quaternions in the 32, 40 or 48-bit
packings, the 48-bit one being what Skyrim uses: three 15-bit components
around 0.5, the largest component dropped and its index carried in the
top bits of the first two words, its sign in the third.

Evaluation is de Boor on the block's local frame (`animation::locate`
turns a time into a block and a frame within it). Rotations are
renormalized after blending control points, which is what Havok does too.
Float tracks (4 in `mt_jump`, the weapon and shield slots) decode as
identity zero, a static value or a scalar B-spline with 8-bit or 16-bit control
points. `SplineCompressedAnimation::sample_float` samples a decoded track.
`Clip::sample_floats_into` uses the binding's separate float-slot map and
preserves unbound reference values. Transform-track mapping is unchanged.

The float layout was checked against the local Nemesis tree's Havok 2010.2
`Animation/Animation/Animation/SplineCompressed/hkaSplineCompressedAnimation.h`
and `.inl`: scalar quantization occupies mask bits 1 and 2, static uses bit 0,
spline uses bit 4, and `floatBlockOffsets` is relative to each block. The
implementation reuses this crate's spline evaluator and checked reader.
HavokLib's `source/hka_spline_decompressor.cpp` skips float masks but does not
decode float payloads, so its decoder could not supply this path. The
serde-hkx `hkaSplineCompressedAnimation` and `hkaAnimationBinding` class
layouts supply the offset arrays and slot map. No Havok source was copied.

ANM-12 rejects unsupported masks, non-finite values, malformed knots, missing
float block offsets and payload reads across a block boundary. Its synthetic
checks cover both quantizations, endpoints, midpoints, static and identity
values, negative slot indices, reordered named slots and block-relative
padding. The installed survey checks all 7,699 packfiles and 6,126 animations,
including 10,137 float tracks, with zero decode failures and finite samples.
This survey does not compare sampled values against an original Havok runtime.
Review also corrected time lookup to account for the shared boundary frame
between spline blocks, as described by HavokLib
`source/packfile/hka_animation_spline.cpp` and the local Havok inline header.
The cross-block test samples at and between shared frames. Review also fixed
container binding selection to match the chosen spline animation index,
rather than taking an unrelated first binding. A reordered-container regression
checks both bone and float-slot maps. Invalid timing metadata is rejected
before decoding, and the public time lookup has a defensive guard.
Float slots are exposed by the clip sampler. Behavior output blending and
host application of those slots are separate runtime work.

Float tracks (4 in `mt_jump`, the weapon and shield slots) are located but
not decoded yet. ANM-05 decodes the optional extracted-motion frame through
`motion::ReferenceFrame`, as described below.

## 4. The behavior graph, as data

`behavior::BehaviorGraph` is an arena of nodes reached from the root
generator. Every node keeps its class, name, user data and variable
bindings, and the classes an interpreter needs are decoded into
`NodeKind` variants: state machines with their states and transitions,
clip generators with their triggers, blenders with their weighted
children, manual selectors, modifier generators and lists, behavior
references, blending and generator transition effects, the Skyrim `BS*`
generators listed above, and event-driven, timer and expression modifiers. Any other
class is kept as `NodeKind::Unknown` (or `NodeKind::Modifier` with its
enable flag when the class name ends in `Modifier`), so a vanilla or
Nemesis-patched graph never fails to load because of an unmodelled node.
The `0_master.hkx` test prints which classes are still unknown, which is
the worklist for widening the model.

The graph's symbol tables come with it: event names, variable names with
types and roles, attribute defaults, character property names, and the
initial variable values. Ids in nodes index these tables directly.

## 5. Interpreter design

Skyrim drives every actor through one `hkbBehaviorGraph` per
`BShkbAnimationGraph`, updated once per frame with the frame's delta time.
The runtime in `havok_behavior.h` shows the shape: the graph keeps an
array of `hkbNodeInfo` for the active nodes, each with its template node,
its clone, its parent and child indices, and per-node flags
(`isStateMachine`, `syncToParent`, `ignoreEvents`). Events go through an
`hkbSymbolIdMap` from external (project) ids to internal (behavior file)
ids. Variables live in an `hkbVariableValueSet`: word values, quad values,
variant values. Generators write into an `hkbGeneratorOutput`, which is a
set of typed tracks (pose, world-from-model, extracted motion, float slots,
ragdoll controls, foot and hand IK controls, character controller
controls) each with an `onFraction` weight, and blenders combine outputs
track by track. That is the model to reproduce.

### 5.1 Event model

- The character's event table is the project's `eventNames` (about 2,000
  for the player). Each behavior file has its own `eventNames`, and the
  graph is linked by building the internal-to-external map by name at
  load, which is exactly what `hkbSymbolIdMap` holds at runtime.
- An event is an id plus an optional payload object plus a sender. Events
  are raised from outside (`SendAnimationEvent`, the game's AI and input
  code), from clip triggers (`hkbClipTrigger` at a local time, relative to
  the clip's end when flagged), from state enter and exit notify lists,
  from modifiers (timer, events-from-range, event-driven), and from the
  state machine itself (`eventToSendWhenStateOrTransitionChanges`).
- Delivery is a queue drained during the graph update. State machines
  consume events by matching `TransitionInfo::event_id` against the queue
  in priority order, with wildcard transitions checked after the current
  state's own. An event with `FLAG_SYNC_POINT` in `EventInfo` is a sync
  point for blenders. Transition effects can swallow events from one side
  by their `event_mode`. Events nobody handles are dropped at the end of
  the update.
- Annotations in clips (`hkaAnnotationTrack`, `FootLeft` and friends) are
  converted to triggers at load unless the clip's flag bit 16 says not to,
  which is how Skyrim gets `FootLeft` and `SoundPlay` events from vanilla
  animations.

### 5.2 Variable model

- Storage is one `VariableValueSet` per graph instance, initialized from
  `variable_initial_values`, with words for bool, int8, int16, int32,
  real and pointer and quads for vectors and quaternions. Character
  properties are a second table owned by the character and shared across
  its graphs.
- `VariableBindingSet` entries copy a variable into a node member
  (`member_path`, a dotted path with array indices, for example
  `playbackSpeed`, `blendParameter`, `children:2/weight`) before the
  node updates, and copy it back when the binding is an output
  (`hasOutputBinding`). Bindings with `binding_type` 1 read character
  properties instead. The interpreter resolves member paths to typed
  setters at load time, once per node class, and reports paths it cannot
  bind rather than ignoring them.
- `variable_mode` decides whether variables reset when the graph goes
  inactive. Skyrim graphs use maintain (1).

### 5.3 Update

Per frame, for one graph instance:

1. Apply input bindings for every active node.
2. Activate the root if inactive. Activation walks generators top down,
   creating an active-node record per node, entering the start state of
   each state machine (mode default, random, sync from the sync variable,
   or a chooser), and starting clips at their `start_time`.
3. Drain the event queue into the state machines, outermost first. A
   matching transition with a satisfied interval and condition starts a
   transition effect: a blending effect crossfades the two generator
   outputs over `duration` with the given curve, a generator effect plays
   a third generator in between. `max_simultaneous_transitions` bounds
   nesting. Exit notify events fire for the state left, enter notify
   events for the state entered.
4. Update generators bottom up with the frame's delta time. Clips
   advance local time by `playback_speed`, loop or stop by `mode`, fire
   triggers whose local time was crossed, and sample their animation into
   a pose track through the binding's track-to-bone map. Blenders weight
   children by `weight`, sync children when flagged, and normalize.
   Modifiers post-process the output of the generator they wrap.
5. Apply output bindings, then hand the root's output to the character:
   pose to the skeleton, extracted motion to the character controller,
   float slots to the game.

### 5.4 Replay harness

The gate for this work is a replay test, not a visual one. A capture is a
sequence of frames, each with delta time, the events sent that frame, the
variable values written that frame, and the observed result: the current
state id of every state machine, the clips active with their local times,
and the root pose. The harness feeds the inputs to the interpreter and
diffs the observables. Captures come from two places:

- Synthetic graphs built with `writer::PackfileBuilder`, where the
  expected sequence is written by hand (the `behavior.rs` test graph is
  the seed for these).
- Skyrim itself, through an SKSE plugin that logs `hkbEvent` ids,
  variable writes and the active state ids per frame for one actor (the
  activeragdoll header names the structures to read: `hkbBehaviorGraph`
  at `BShkbAnimationGraph + 0x208`, the active nodes array at `+0x98`
  of the graph, `hkbStateMachine::currentStateId` at `+0x80`). That
  capture format is the contract between the plugin and the harness, and
  it is what lets the interpreter be checked against the real thing
  without running the real thing here.

The first milestone on that road is the map's: an idle and a walk cycle
on the player skeleton. In graph terms that is `0_master` entering
`MT_BehaviorReference`, `mt_behavior` reaching its idle state, then a
`Speed` write and a transition into the walk blend. Everything that test
needs is loaded by this crate today except the extracted motion frame.
Scalar transition conditions are evaluated as described below.

## Live run (2026-09-14)

The gated test parses every packfile inside `Skyrim - Animations.bsa` and decodes
every spline compressed animation it finds. 7699 packfiles parsed in 0.3 seconds, 0
parse failures, every root a `hkRootLevelContainer`. 6126 spline animations turned
up and all 6126 decoded, 466793 tracks sampled, 0 decode failures. No code change
was needed.

## 6. Interpreter (2026-09-18)

`interp::Interpreter` is one graph instance: a `BehaviorGraph`, a
`ClipLibrary` (clips decoded once, keyed by the name the clip generator
uses, case and slash insensitive), the skeleton's reference pose, and a
run-time record per node. `update(dt)` runs the order the SDK headers
publish: apply variable bindings, activate the root if it is not, deliver
the queued external events outermost first, generate children before
parents, then deliver the events nodes raised during the frame (bounded
to eight rounds). The result is `Output { pose, weight }`, a local-space
pose with one transform per bone and weight zero when nothing had a pose
to give. `active_states`, `active_clips`, `take_changes` and
`take_events` expose the observables the replay harness in 5.4 diffs.

What each node does:

- `hkbStateMachine`: enters the start state on activation (mode 0 the
  start state, 1 the sync variable, 2 the most probable enabled state as a
  deterministic stand-in for random, 3 falls back to 0 since choosers are
  not decoded). An event chooses the highest-priority eligible transition in the current
  state's list and wildcards, whose event id it carries, whose flags
  are not disabled, whose target state is enabled and whose scalar condition
  passes. Eventless guards are polled once per active machine per frame.
  Trigger intervals gate admission and initiate intervals defer accepted requests.
  The return-to-previous-state event goes back one state.
  A blending transition effect with a duration crossfades the old and new
  generators with `blend_curve` (0 smooth, 1 linear, 2 linear to smooth, 3
  smooth to linear) and keeps the old one active until it ends. Without
  an effect the switch is a cut. Exit and enter notify events and the
  state-change event are raised.
- `hkbClipGenerator`: local time advances by `playbackSpeed` times `dt`.
  Mode 0 stops at the end, 1 loops, 2 follows `userControlledTimeFraction`,
  3 ping pongs. Triggers fire when the playhead crosses their time, end
  relative ones measured from the clip's duration, acyclic ones only on
  the first pass. The pose is the reference pose with every track written
  into the bone the binding maps it to.
- `hkbBlenderGenerator`: children are weighted by their `weight` field or
  its binding and normalized. With the parametric flag each weight is a
  position on the blend parameter's axis and the two neighbours of the
  parameter share the blend linearly. With the sync flag every direct clip
  child is parked at the master child's phase before it advances. Children
  at weight zero deactivate unless the keep flag is set. Two transforms
  blend by lerp of translation and scale and nlerp of rotation on the
  short arc.
- `hkbManualSelectorGenerator`: plays the selected child, switching when
  the index changes.
- `BSOffsetAnimationGenerator`: samples its default and offset generators
  each frame. `fOffsetVariable`, from its input binding when present, maps
  through `fOffsetRangeStart` and `fOffsetRangeEnd` to a clamped 0 to 1
  amount. The offset pose adds scaled translation, composes a scaled
  rotation, and multiplies a scaled scale into the default pose.
- `BSBoneSwitchGenerator`: starts its default generator and each authored
  bone child. A child with a mask overlays bones with positive entries in its
  `hkbBoneWeightArray`, blending partial mask values and leaving other bones
  at the default output. Missing mask entries contribute zero. A child with
  no mask overlays every bone.
- `hkbModifierGenerator` and Skyrim's `BS*` wrappers (`BSiStateTagging`,
  `BSSynchronizedClip`, `BSCyclicBlendTransition`) pass their generator
  child through.
  `hkbModifierGenerator` also gives an enabled modifier child activation and
  deactivation lifecycle.
- `hkbEvaluateExpressionModifier`: compiles its expression data at graph
  construction and evaluates active entries in serialized order each update.
  A nonnegative assignment variable index writes the finite scalar result to
  a bool, integer or real graph variable. Unsupported syntax, assignment
  targets, event indices and event modes are reported in
  `Unsupported::expression_modifiers`. The four event modes follow
  `hkbExpressionData`'s `ExpressionEventMode` values: send once after the first
  true result, send on every true frame, send on a false-to-true edge, or keep
  sending every frame after the first true result. The source field layout and
  enum names come from serde-hkx's `assets/classes/hkbEvaluateExpressionModifier.json`,
  `hkbExpressionDataArray.json` and `hkbExpressionData.json`. The runtime
  evaluator uses this crate's scalar `Expression` parser because serde-hkx
  describes serialized fields but does not execute them.
- `hkbRotateCharacterModifier` accumulates `degreesPerSecond` times
  `speedMultiplier` and elapsed frame time while enabled, then applies the
  resulting angle around `axisOfRotation` to the generated root orientation.
  Its serialized fields and offsets follow
  `serde-hkx/assets/classes/hkbRotateCharacterModifier.json`.
- `hkbEventsFromRangeModifier` reads its bound `inputValue` and ordered
  `eventRanges`. The serde-hkx declarations provide the class layout, each
  upper bound and event, and the two policies: event mode 0 sends on entry,
  while mode 1 sends on every update in range. This interpreter uses half-open
  intervals `[lower, upper)`, with each range's lower bound taken from the
  modifier or the previous range's upper bound. A shared boundary belongs to
  one range. Entry state is tracked per range. Non-finite or non-increasing
  bounds and unknown event modes reject graph loading.
- `hkbBehaviorReferenceGenerator` and unmodelled classes yield the
  reference pose at weight zero and are listed in `Unsupported`.

Bindings resolve at construction to typed setters: `playbackSpeed`,
`userControlledTimeFraction`, `blendParameter`, `weight` on a blender
child (also `children:N/weight`), `selectedGeneratorIndex`,
`fOffsetVariable` on `BSOffsetAnimationGenerator`, and `enable`. Any
`inputValue` on an events-from-range modifier is a real input. Any other
member path is reported in `Unsupported::unbound`. Reals live in the word
table as their bit pattern, which is how the file stores them.

The `BSOffsetAnimationGenerator` fields follow serde-hkx's generated
`BSOffsetAnimationGenerator` declaration and the local hkxcmd class table.
Those sources establish the serialized members and offsets. The interpreter
uses the input binding and authored range to scale the sampled offset pose.

### Checked against the sampler (2026-09-18)

`tests/interp.rs` drives a hand-built graph over two one-track ramp clips
so every expected pose is a number: the idle clip's acyclic trigger raises
the walk event, the 0.25 s linear blend lands halfway at the midpoint and
ends on the walk, the wildcard cuts back to a restarted idle, a bound
`Speed` doubles the walk's advance, a looping clip fires its end trigger
on every wrap and its acyclic trigger once, both blender modes and the
selector follow their bound variables, a missing clip is reported.

`tests/real_files.rs` runs the same graph over the vanilla human skeleton
(`character assets/skeleton.hkx`, 99 bones) and the clips `mt_behavior`
names, `Animations\male\MT_Idle.HKX` (9.133 s, 99 tracks) and
`Animations\male\MT_WalkForward.hkx` (1.133 s, 99 tracks), for 180 frames
at 1/60 s and compares every frame's pose with the spline sampler at the
interpreter's own clip times, the blend recomputed from a frame count. The
worst difference over 180 frames was 3.8e-6 (tolerance 1e-4), the
transition began at frame 18 when the trigger fired, 15 blend frames
followed, the head sits at z 120.3 in model space, and one update costs
2.8 us. One correction came out of it: the last frame of a blend returns
the target's own output, since a blend at alpha one negates the
quaternions the short arc flipped.

The same test then loads the vanilla `behaviors/mt_behavior.hkx` and links
every graph it references. That is the step that makes it animate: a
reference serializes only a file name, so before linking the graph reaches
`MT_Default_Behavior = MT_Standing_State` and stands there at **weight
zero**, because the standing idle lives behind `Behaviors\IdleBehavior.hkx`.

Linking pulls in four references over two rounds, `Shout_Behavior`,
`SprintBehavior` twice and `IdleBehavior`, with none missing and none
refused. The graph grows from 1,595 nodes, 931 events and 67 variables to
2,223 nodes, 974 events and 87 variables, and from 730 clip names to 788 of
which 787 are in the archive (`IdleValericaWalkForward` is still not).

After linking, two seconds of idle gives **weight one** with `MT_Idle.HKX`
playing, reached through the spliced states `Behavior00 = MT_Idle_State`,
`MT_Idle_3rdPersonBehavior = InitialIdleState` and `Behavior02 =
MT_Idle.HKX`. Setting `Speed` to 1 and sending `moveStart` still moves it to
`MT_LocomotionType_Behavior = MT_Locomotion_State` with `MT_WalkForwardSlow`
at weight one. Nothing on either path is an unknown class, a missing clip or
an unresolved reference.

Bindings went from 32 resolved and 92 unbound to **194 resolved and 5
unbound**. The five left are classes with no evaluation yet, not paths the
resolver cannot parse: `hkbBlenderGenerator.boneWeights`,
`BSEventOnFalseToTrueModifier.bVariableToTest1`,
`hkbRotateCharacterModifier.degreesPerSecond` and
`hkbEventsFromRangeModifier.inputValue` twice.

An update costs 9.9 us idle and 8.6 us walking. Idle rose from 0.9 us
because it now samples a 99 bone clip every frame instead of returning the
reference pose.

### Ported layouts (2026-09-18)

The classes came from `../engine-refs/serde-hkx/`, dual MIT and Apache, whose
`assets/classes/*.json` carry every Havok class with its signature, size per
pointer width and per member the ctype and both byte offsets. Read and
extracted into this crate's own types, never depended on, since it is one
maintainer and the layouts are what we need rather than the code.

- `hkbBehaviorReferenceGenerator`: `behaviorName` at 72, and a `behavior`
  pointer marked `SERIALIZE_IGNORED`, which is the proof that a file cannot
  say what sits under a reference.
- `hkbBehaviorGraph`: `eventIdMap`, `variableIdMap` and
  `characterPropertyIdMap` are all `SERIALIZE_IGNORED` too, so the engine
  builds them at load. This confirms 5.1 and corrects how section 7 phrased
  it: `hkbSymbolIdMap` is a runtime structure with no serialized class to
  port, so the map is built by name here exactly as 5.1 describes.
  `hkbLinkedSymbolInfo` is the only symbol class in the file format and it
  carries name lists alone.
- `BSIsActiveModifier`: five `bIsActiveN` flags interleaved with five
  `bInvertActiveN` from offset 80.
- `hkbVariableBindingSetBinding`: `hasOutputBinding` on the set is
  `SERIALIZE_IGNORED`, so a file never says whether a binding reads or
  writes. The member decides, see below.

### Linking, and why symbols map by name

`link::BehaviorGraph::link_reference` splices a referenced graph in under its
reference node, and `link_all` walks the chain with a round bound so a graph
that reaches itself terminates. Symbols map by name because a name is the
only thing two files share: a sub-graph event or variable the parent already
knows keeps the parent's id, one the parent lacks is appended to the parent's
tables. Then every id inside the sub-graph is rewritten, its node ids shift
past the parent's, and the reference points at the sub-graph's root.

Character properties use a separate name map. Their names and typed infos are
merged too, and binding type 1 is remapped through that property map.

### Input and output bindings

The file does not mark a binding's direction, so the member does. Two members
are node state with no setter behind them, so they are written back into
their variable after the update rather than read from it before:
`hkbStateMachine.isActive`, and `BSIsActiveModifier.bIsActiveN` which writes
whether the modifier is active, inverted when that slot's invert bit is set.
That is how a Skyrim graph lets script see that a branch is running. The rest
read as before, now including `hkbStateMachine.startStateId` (which beats the
stored start state in every mode), `hkbBlendingTransitionEffect.duration` and
`BSCyclicBlendTransitionGenerator.fBlendParameter`.

### Scalar transition conditions, ANM-01

`hkbExpressionCondition.expression` and legacy
`hkbStringCondition.conditionString` are string pointers at offset 8 on LE and
16 on SE, as `serde-hkx` declares them.
Conditions have no `hkbNode` header. The parser preserves their source in
`TransitionInfo.condition`, including an explicit unsupported class variant.
`Interpreter::new` compiles the source after reference linking. Named graph
variables then bind to the merged table, so a subgraph's original variable
index cannot accidentally select a parent variable with a different name.
Character property names resolve against the merged property table, including
properties declared only by a referenced graph. The linker retains each
expression's originating variable and property namespaces. Thus a parent
variable cannot capture a child property with the same name, or the reverse.
Those scopes survive further linking, and identical source text with different
scopes compiles separately.

Supported scalar syntax includes parentheses, decimal and exponent literals,
`true` and `false`, unary `!`, `+`, `-`, arithmetic `+`, `-`, `*`, `/`, `%`,
comparisons `==`, `!=`, `<`, `<=`, `>`, `>=`, short-circuit `&&`, `||`, and
function calls. The function names map to the operator inventory in
`hkbCompiledExpressionSetToken`: `rand01`, `sin`, `cos`, `asin`, `acos`,
`sqrt`, `fabs`, `ceil`, `floor`, `sqrtinv`, `pow`, `max`, `min`, `randrange`,
`atan2approx`, `clamp`, `mod`, `deg2rad`, `rad2deg`, `cosd`, `sind`, `acosd`,
`asind`, `atan2approxd`, `sign`, `lerp`, `clerp`, and `cond`. `rand01()` takes
no arguments, unary math functions take one, binary functions take two, and
`clamp`, `lerp`, `clerp` and `cond` take three. Random functions consume a
deterministic per-interpreter stream initialized from a fixed seed. Invalid
arity, unknown functions and non-finite results keep the guard closed.
Bool, signed 8-bit, signed 16-bit, signed 32-bit and real values use their
stored types. Numeric expression operands use `hkReal` precision. Pointer,
vector and quaternion operands, unknown symbols, malformed source and
non-finite arithmetic return expression errors. Their guards stay closed,
with deduplicated entries in `Unsupported.conditions`.

`DISABLE_CONDITION` bypasses a guard. `DISABLED` still disables the whole
transition. An event guard must match its event as well as its expression.
An event ID of -1 with a condition is polled once per node each frame, before
sampling, with no synthetic event added to the event queue. After a condition-only transition, the new
state waits until the next frame for another condition-only transition. An
external event enters its target before condition polling, so that target's
guard can run in the same frame.
Missing guards keep the existing event-driven behavior.

Sources read locally before implementation:

- `serde-hkx/assets/classes/hkbExpressionCondition.json` and
  `hkbStringCondition.json` supply the serialized string offsets.
- `serde-hkx/assets/classes/hkbCompiledExpressionSetToken.json` supplies the
  operator inventory and `hkReal` operand representation.
- `serde-hkx/assets/classes/hkbStateMachineTransitionInfo.json` supplies
  the condition pointer and disable-condition flag.
- `Skyrim-Behavior-Editor-/src/hkxclasses/behavior/hkbexpressioncondition.cpp`
  and `hkbstringcondition.cpp` preserve authored guard strings.
- `Skyrim-Behavior-Editor-/src/hkxclasses/behavior/hkbexpressiondataarray.cpp`
  preserves authored expression strings and assignments, but does not
  evaluate them.
- Nemesis local `test environment/mod` examples confirm named variables,
  ordinary scalar comparisons and logical operators in authored conditions.

Neither the class tables nor the editor implement expression evaluation, so
this crate uses its own bounded scalar parser instead of adding a dependency
on a serializer or a Qt editor. The operator inventory supplies names and
operand data types, not runtime formulas. ANM-49 implements the listed scalar
functions and deterministic random evaluation. Tests compare the supported
math functions with their scalar results and replay random guards from the
same interpreter seed. No Havok runtime capture was used and timing
arbitration outside these guards remains separate work.

`tests/conditions.rs` covers binary decoding, bool/int/real changes, false
guards, condition-only polling, cycles, disabled conditions, malformed or
unknown input, parent/subgraph symbol reordering, child-only properties,
variable/property name collisions, repeated linking and property binding IDs,
math-function guards and seeded random guard replay. Expression unit tests
cover precedence, short circuiting, typed storage, character properties,
operator function results, random replay and malformed-input limits.

The installed-data survey visits 428 behavior graphs and 814 guarded
transitions. It checks every serialized expression/string condition appears
in the decoded graph. Two draugr conditions refer to the undeclared variable
`SpeedSampled`, and one troll guard contains `staggerMagnitude > .3 , < .7`.
These three inputs stay closed and are asserted as exact diagnostics. ANM-50
requires runtime evidence for the comma syntax. The initial survey guessed
more than 1,000 guards and failed that count check. The final coverage check
compares serialized conditions directly instead of using that guessed count.

The existing installed-data checks still parse all 7,699 packfiles, decode
all 6,126 spline animations and sample 466,793 tracks without failures. The
linked human idle/walk graph still reaches both clips with weight one.

### Unequal transition priorities and blend capacity (2026-09-20)

ANM-02 selects the highest-priority enabled transition whose condition passes.
A lower-priority transition cannot interrupt a higher-priority active effect.
The playing-uninterruptible flag prevents interruptions while its effect runs.
Interrupted blends remain inputs to their successors. At capacity the oldest
effect ends to admit the new effect. Completing a later effect removes its
predecessors. Direct state generators retained by several effects advance once per update.
Nonpositive capacity permits state changes without retaining a blend.

Layout sources are local `serde-hkx/assets/classes/hkbStateMachine.json`,
`hkbStateMachineTransitionInfo.json` and
`activeragdoll/include/RE/havok_behavior.h`. They do not explain arbitration.
The [Havok state-machine declaration](https://github.com/Bewolf2/projectanarchy/blob/master/Source/Behavior/Behavior/StateMachine/hkbStateMachine.h)
documents retained interrupted blends, oldest eviction and predecessor cleanup.
Its active-priority comment does not resolve equality. No proprietary
implementation code was ported.

Equal-priority interruptions retain the previous permissive behavior. Candidate
ties retain authored order, with local entries preceding wildcard entries.
These choices have not been verified against the original runtime. ANM-54
tracks that comparison and the tied delayed-transition contract. ANM-02 is
limited to unequal-priority arbitration and blend capacity. Shared descendants
below distinct wrappers still need lifecycle and update ownership, tracked by
ANM-55. ANM-11 adds trigger and initiate intervals and delayed requests.

The `transition_priority` suite checks numeric chained poses, capacity eviction,
priority and guard selection, uninterruptible effects, shared-generator clocks,
instant transitions and cleanup when a later effect completes first.

### 6.5 Transition intervals

ANM-11 follows the local serde-hkx `hkbStateMachineTimeInterval` layout and
hkxcmd transition flags. The Havok declaration linked above supplies the
interval contract. Trigger bounds gate request admission. Initiate bounds
hold an admitted request. Fixed bounds use state-entry time, with zero exit
meaning no upper bound. Either event ID selects event mode for both bounds.
The implementation is original Rust, with no proprietary implementation port.

The interpreter uses half-open time bounds. Queued events see the previous
frame's state time. Delayed requests and condition-only guards are checked
before generation. Generation advances state, clip and effect clocks together,
so a time window reached there is observed on the next transition poll.
The raised-event phase also polls after generation. A request started there
keeps its target and effect clocks at zero until the next generation.
Event bounds update before transition selection, with closing
winning if both bounds name the same event. A missing enter event initializes
an event interval open. State entry resets local and wildcard interval flags,
and clears pending requests. Deactivation also clears requests. These ordering
choices are provisional and covered by tests, not original-runtime captures.

Delayed requests retain their trigger event and admission condition result.
Higher-priority requests can replace them unless the delayed request is
uninterruptible. The current equal-priority fallback retains one request in
authored order, with new equal-priority requests replacing the pending one.
ANM-54 owns the tied-candidate contract. Requests wait indefinitely if their
initiate window never opens. The declaration mentions an excessive-delay
fallback without defining its threshold. ANM-56 owns that threshold and the
boundary, reset and condition-recheck comparison against the original runtime.
Invalid fixed bounds fail closed. Event mode ignores fixed-time fields.

`transition_intervals` checks admission separately from start, exact boundaries,
event windows, condition-only transitions, state reentry, wildcard reset,
deactivation, priority protection and accepted requests outliving trigger windows.

### Installed passthrough modifier inventory (2026-09-20)

The installed `Skyrim - Animations.bsa` contains 428 behavior graphs. The
gated `installed_behavior_passthrough_modifier_inventory` test parses every
`hkbBehaviorGraph` object and fails if any graph cannot decode. It counts every
modifier path that the interpreter does not execute: generic `NodeKind::Modifier`,
`ModifierGenerator`, `ModifierList`, `EventDrivenModifier` and `TimerModifier`.
It found 4,148 remaining nodes across 28 classes after implemented modifiers
were excluded from the inventory. `BSIsActiveModifier` is excluded because
its bound activity slots already write their output variables. The test freezes
the sorted names and counts, and checks that every class has a bounded backlog
row whose title names that class.

| Class | Nodes | Follow-up |
| --- | ---: | --- |
| `BSDirectAtModifier` | 23 | [ANM-25](../BACKLOG.csv) |
| `BSEventEveryNEventsModifier` | 85 | [ANM-26](../BACKLOG.csv) |
| `BSEventOnDeactivateModifier` | 98 | [ANM-27](../BACKLOG.csv) |
| `BSIStateManagerModifier` | 3 | [ANM-29](../BACKLOG.csv) |
| `BSInterpValueModifier` | 36 | [ANM-30](../BACKLOG.csv) |
| `BSLimbIKModifier` | 1 | [ANM-31](../BACKLOG.csv) |
| `BSLookAtModifier` | 128 | [ANM-32](../BACKLOG.csv) |
| `BSModifyOnceModifier` | 26 | [ANM-33](../BACKLOG.csv) |
| `BSPassByTargetTriggerModifier` | 1 | [ANM-34](../BACKLOG.csv) |
| `BSRagdollContactListenerModifier` | 50 | [ANM-35](../BACKLOG.csv) |
| `BSSpeedSamplerModifier` | 42 | [ANM-36](../BACKLOG.csv) |
| `BSTimerModifier` | 1 | [ANM-37](../BACKLOG.csv) |
| `BSTweenerModifier` | 5 | [ANM-38](../BACKLOG.csv) |
| `hkbDampingModifier` | 41 | [ANM-39](../BACKLOG.csv) |
| `hkbEvaluateExpressionModifier` | 438 | [ANM-40](../BACKLOG.csv) |
| `hkbEventDrivenModifier` | 158 | [ANM-17](../BACKLOG.csv) |
| `hkbFootIkControlsModifier` | 25 | [ANM-41](../BACKLOG.csv) |
| `hkbFootIkModifier` | 2 | [ANM-42](../BACKLOG.csv) |
| `hkbGetUpModifier` | 46 | [ANM-43](../BACKLOG.csv) |
| `hkbKeyframeBonesModifier` | 91 | [ANM-44](../BACKLOG.csv) |
| `hkbModifierGenerator` | 1,909 | [ANM-52](../BACKLOG.csv) |
| `hkbModifierList` | 708 | [ANM-51](../BACKLOG.csv) |
| `hkbPoweredRagdollControlsModifier` | 93 | [ANM-45](../BACKLOG.csv) |
| `hkbRigidBodyRagdollControlsModifier` | 55 | [ANM-46](../BACKLOG.csv) |
| `hkbRotateCharacterModifier` | 6 | [ANM-14](../BACKLOG.csv) |
| `hkbTimerModifier` | 59 | [ANM-16](../BACKLOG.csv) |
| `hkbTransformVectorModifier` | 1 | [ANM-47](../BACKLOG.csv) |
| `hkbTwistModifier` | 17 | [ANM-48](../BACKLOG.csv) |

`hkbModifierList` traverses each listed modifier while decoding, and
`hkbModifierGenerator` traverses both its modifier and generator children.
The inventory therefore includes those wrappers even though the interpreter
currently activates and generates only the generator child.

### Modifier generator lifecycle (2026-09-20)

`hkbModifierGenerator` has a modifier pointer at 72 and a generator pointer
at 80 in the 64-bit layout, from
`engine-refs/serde-hkx/assets/classes/hkbModifierGenerator.json`. The
interpreter activates an enabled modifier child before its generator child,
deactivates a child when its `enable` binding changes to false, and deactivates
both through the node child walk. This makes the existing `BSIsActiveModifier`
output binding report the authored active state without changing the generated
pose. The synthetic test covers enabled and disabled children, an enabled to
disabled to enabled transition, explicit deactivation and reactivation, and a
sampled non-reference pose.

### Timer modifier (ANM-16)

`hkbTimerModifier` inherits `hkbModifier.enable`, then stores
`alarmTimeSeconds` and `alarmEvent`. The offsets and the ignored
`secondsElapsed` runtime field are declared in
`engine-refs/serde-hkx/assets/classes/hkbTimerModifier.json` and
`engine-refs/hkxcmd/compat/hkbTimerModifier_1.cpp`. The interpreter tracks
elapsed time separately for each graph node. An active timer adds the
nonnegative update interval once per graph update. It raises its event once
elapsed time reaches the authored alarm time and stays quiet until its next
activation. Activation
resets elapsed time and the one-shot flag. The event queue carries the event
ID and does not carry the authored payload. Synthetic tests cover field
decoding, the alarm boundary, one-shot delivery, and reset after an
enable-controlled deactivation and activation.

### Modifier lists (2026-09-20)

`hkbModifierList` inherits `hkbModifier.enable` and serializes an ordered
`hkArray<hkbModifier*>` at offset 80 in the 64-bit layout, from
`engine-refs/serde-hkx/assets/classes/hkbModifierList.json`. The interpreter
uses its authored enable value unless a binding overrides it, then activates
the child modifiers in that serialized order. Nested lists follow the same
rule. Disabling a list deactivates its full modifier subtree, so every
`BSIsActiveModifier` output changes to inactive while the wrapping generator
continues to supply its pose. Modifier output bindings publish in that same
order, including disabled children, so a later serialized modifier wins when
two bindings write one variable. Traversal is iterative, rejects cycles and
stops at 64 nested lists. It deactivates the rejected subtree and records the
malformed list. The
synthetic tests cover two direct outputs, an arena order different from the
serialized order, a shared output with invert, a nested output, list and child
enable transitions, authored disable, cleanup, malformed cycles and deep
lists, and pose preservation.

### Event-driven modifier (2026-09-28)

`hkbEventDrivenModifier` inherits the modifier enable flag and wraps one
modifier. The local `engine-refs/serde-hkx/assets/classes/hkbEventDrivenModifier.json`
declares `activateEventId`, `deactivateEventId` and `activeByDefault`, and marks
its serialized `isActive` field as ignored. The interpreter initializes a
per-instance interval from `activeByDefault` when the wrapper activates. Its
activate and deactivate events open and close the interval. The child modifier
is active only while the wrapper is enabled and the interval is open. Other
events reach the child while it is active. Deactivating the wrapper also
deactivates its child.

`tests/event_driven_modifier.rs` checks activation, deactivation, default
activity, wrapper disable, ignored serialized state and decoded event and child
fields through a synthetic packfile. These tests establish the declared layout
and the interpreter policy. They do not compare event ordering with a Havok
runtime capture.

### False-to-true event slots (2026-09-20)

ANM-03 decodes all three `BSEventOnFalseToTrueModifier` slots. Local
`serde-hkx/assets/classes/BSEventOnFalseToTrueModifier.json` and
`hkxcmd/compat/BSEventOnFalseToTrueModifier_1.h` supply the layout. Each slot
has an enable flag, tested boolean and event property. The Skyrim Behavior
Editor class and UI confirm both booleans can bind to variables or character
properties. Its code edits fields and does not implement runtime evaluation.

The interpreter evaluates the slots during modifier-list updates. An enabled
slot whose tested value changes from false to true raises its event once.
Steady true and falling edges are quiet. Slot activity is the conjunction of
its enable flag and tested value. Disabling a slot clears that activity.
Activation resets history to false, so an initially true enabled slot emits
on its first update, including after a whole-modifier reactivation. These
activation choices are explicit policies, not an original-runtime comparison.
The serialized history fields are ignored, as the class metadata requires.
The graph's declared character-property table is initialized to typed zero
values so property bindings can be populated by the caller. Events keep slot
and modifier-list order and enter the existing raised-event
queue. A failed list emits no partial modifier effects.

Graph linking remaps each slot event and preserves appended scalar defaults
when the parent has no initial-value table. Quad and variant default linking
is tracked separately by ANM-57. Event payload presence is decoded, while the
interpreter's existing event queue carries IDs only. The eleven `false_to_true`
regressions cover three independent slots, enable transitions, malformed binding
names, shared visits, list rejection, symbol remapping and a parent-machine
transition caused by a modifier event. The installed survey decodes every
behavior graph and verifies that all edge-slot bindings resolve. The full
installed scan found 428 graphs, 23 edge modifiers and 29 enabled slots, with
no slot payloads. The linked locomotion run now resolves 195 bindings with
four unbound, compared with the historical 194 and five above.

### Get time step modifier (2026-09-20)

`BSGetTimeStepModifier` inherits the `hkbModifier` enable field and stores an
authored `timeStep` real at offset 80 in the 64-bit layout. The layout comes
from `engine-refs/serde-hkx/assets/classes/BSGetTimeStepModifier.json`, which
also records the class signature and the 32-bit offset. The interpreter
decodes the authored value and treats a `timeStep` variable binding as output:
after an enabled modifier subtree runs, it writes the update interval passed to
`Interpreter::update`. An inactive modifier writes zero. Nested modifier lists
preserve the existing serialized traversal order for those outputs. The
synthetic packfile test checks the class field and binding decode, while the
interpreter tests check zero, unequal update intervals, disabled output and a
nested list. No original-runtime capture establishes finer behavior here.

### BSDecomposeVectorModifier (2026-09-20)

`BSDecomposeVectorModifier` stores an `hkVector4` at offset 80, followed by
real `x`, `y`, `z` and `w` fields at 96, 100, 104 and 108. Those fields come
from `engine-refs/serde-hkx/assets/classes/BSDecomposeVectorModifier.json` and
the matching Nemesis HKX compatibility declaration at
`engine-refs/Project-New-Reign---Nemesis-Main/external/hkxcmd/havok_2010_2_0/compat/BSDecomposeVectorModifier_0.cpp`.
The interpreter reads a bound `vector` from the graph quad table when present,
otherwise it uses the authored vector, then writes each component through its
`x`, `y`, `z` or `w` output binding. A disabled modifier publishes no
components. Outputs follow modifier-list traversal order, including nested
lists, so a later serialized modifier wins when two components share a target
variable. The synthetic tests cover authored and bound vectors, all four
components, disabled lifecycle and nested-list order.

Linked behavior references currently preserve word defaults only. A
`BSDecomposeVectorModifier` inside a linked sub-graph needs its vector supplied
by the parent graph until the linker imports quad defaults and remaps their
word-table slots. ANM-57 owns that linker work.

## 7. Not done and next

- Modifier evaluation remains incomplete. `BSIsActiveModifier`,
  `BSEventOnFalseToTrueModifier`, `BSDecomposeVectorModifier`,
  `BSGetTimeStepModifier`, `hkbEventDrivenModifier`, timer,
  rotate-character and events-from-range handling run in the interpreter.
  Dynamic bone-weight mask bindings and other unimplemented modifier classes
  still pass through. Their layouts are in `serde-hkx` like the rest.
- Transition details: equal-priority arbitration, the `abut at end` flag,
  `hkbGeneratorTransitionEffect`, `maxSimultaneousTransitions`, delayed timeout
  and interval boundary parity. Expression functions are ANM-49 and the
  undocumented troll comma condition is ANM-50.
- Blender details: dynamic bone mask bindings, `subtractLastChild`, the real
  weight curve once a capture from the game exists (5.4).
- Linking currently loads a file per reference, so `SprintBehavior` is
  spliced twice. That is correct, each reference is its own instance with its
  own state, but a shared decode with per-instance run-time state would cost
  less memory. Linked references preserve word defaults only, so a quad input
  inside a sub-graph needs the parent's value until ANM-57 imports them.

## Skyrim LE layout validation (ANM-07)

The x86 offsets and element sizes come from `offset_x86` and `size_x86` in
`engine-refs/serde-hkx/assets/classes/*.json`, including inherited members.
HavokLib's `source/format_old.cpp` selects four-byte or eight-byte pointer
fixups from the header. Its `source/packfile/hka_skeleton.cpp` likewise
selects architecture-specific member offsets and bone strides. The existing
Rust reader already resolves those fixups, so it uses these declarations
without importing another parser or a Havok SDK dependency.

The tables above retain their SE offsets. At read time the paired LE offsets
also account for vector alignment, which cannot be inferred by halving an
SE offset. For example, `hkbProjectData.worldUpWS` stays at 16 in both
layouts, while `hkaSkeleton.bones` moves from 40 to 24 and each `hkaBone`
shrinks from 16 to 8 bytes. `hkbNode.userData` reads four bytes on LE.
Raw `Obj` byte accessors continue to accept file byte offsets.

`tests/le_files.rs::nemesis_le_and_se_have_the_same_typed_models` recursively
compares all 37 paired Nemesis cache files. Set `SOE_NEMESIS_CACHE` to
`engine-refs/Project-New-Reign---Nemesis-Main/test environment/cached_behaviors`.
The fixed acceptance threshold is 35 behavior pairs and two character pairs.
The survey compares 10,958 graph nodes per architecture, bindings, transitions,
events, variable tables and character metadata. Only physical file addresses
are normalized. Raw pointer values retain nullness and target class checks.
It validates the fields in the current model, not unmodelled object payloads.

`tests/round_trip.rs` compares LE and SE skeleton, spline and binding models
and samples the same translated and rotated pose in both layouts. Synthetic
LE tests also check aligned project vectors, every project string table,
32-bit user data, consecutive trigger and binding records, payload fixups,
and rejection of negative or oversized array counts. The packfile builder's
`new_32bit` constructor writes the matching pointer width and array headers.

The existing unsupported-class and animation-codec boundaries apply equally
to LE and SE. This change does not add interpreters for classes that the
behavior model preserves as unknown nodes.

## 8. Packfile document writer (ANM-08, 2026-09-20)

`document::PackfileDocument` reads every section into editable payload bytes and
explicit local, global and virtual fixups. It preserves class signatures,
unknown object bytes, section tags and header extensions. `to_bytes` validates
all edits and regenerates section placement, section headers and relocation
tables. It does not retain or return the source file buffer. Section-relative
payload offsets stay fixed, so unknown class layouts need no guessed sizes.
Callers may edit fields, append payloads and retarget fixups. Inserting bytes
inside a payload requires updating every affected address. Typed runtime
models remain read-only views, they are not serializers.

The implementation follows local `serde-hkx/serde_hkx/src/bytes/ser/mod.rs`,
`bytes/serde/section_header.rs` and `bytes/de/parser/fixups.rs`, cross-checked
against `HavokLib/source/format_old.cpp` section loading, linking and writing.
Their typed serializers do not provide this crate's required opaque-object
preservation, so the document writer keeps complete section memory images.
There are no new dependencies.

The writer rejects overlapping sections, reversed or truncated table ranges,
incomplete records, non-padding bytes after a sentinel, pointer slots that
cross the payload end, overlapping or repeated pointer slots, invalid section
indices, out-of-range destinations, repeated virtual records and invalid class
or root addresses. Reading and writing both perform this validation. Nonempty
import/export tables and packfile versions other than little-endian Havok
2010.2 version 8 return `Unsupported`. The container accepts 4-byte and 8-byte
pointer slots, but this row's live corpus is SE only.

The declared corpus is every HKX in installed `Skyrim - *.bsa`, plus standalone
Update, Dawnguard, HearthFires and Dragonborn archives when present. This
installation has no standalone DLC archives. The
`anm_08_installed_packfiles_roundtrip` check requires zero failures and at least
7,699 files. It read, wrote and reread 7,699 files, all in the animation archive,
with equal section payloads and all relocation targets. Modeled equality covers
6,461 animation containers, 428 behavior graphs, 353 character records, 457
project records and 6,126 spline animations. The unchanged bytes include 7,219
opaque `hkp` objects. A separate installed idle-clip edit doubles the modeled
duration and verifies the reread value. Synthetic cases cover unknown classes,
nonempty extra sections, scalar and string edits, retargeted pointers, both
pointer widths, malformed tables and every truncation boundary of a fixture.

Validation does not execute the rewritten files in Havok or Skyrim. It verifies
the memory-image contract and this crate's modeled values. Object deletion,
compaction and semantic editing of unknown classes are outside this writer API.

## Authored bone masks, ANM-04

Blender children now retain the `hkbBoneWeightArray` rather than only a
pointer-presence flag. Each child's effective contribution is its blend
weight times its authored mask times its generated per-bone contribution.
Contributing poses normalize separately for each bone. A missing mask gives
weight one. A zero mask or an absent array entry gives no contribution, and
bones with no contribution retain the reference pose. Per-bone weights stay
in `interp::Output` through nested blenders and transition blends.

The format sources are the local serde-hkx class tables
`hkbBlenderGeneratorChild.json`, `hkbBoneWeightArray.json` and
`BSBoneSwitchGeneratorBoneData.json`. The latter establishes the shared mask
format needed by ANM-19. The listed nifly `src/Skin.cpp` source describes
vertex skin weights, so it does not define behavior-mask evaluation. The
implementation extends this interpreter's existing normalized blending and
uses the authored per-bone weights. No new dependencies were added.

Nine new synthetic tests check masked bones, zero and unspecified entries,
fractional masks, invalid weights and arrays, transition endpoints and nested
generators.
The installed suite passes 61 HKX tests, including a survey of 428 behavior
graphs and 1,094 masked children whose decoded weights equal their serialized
arrays. All 7,699 packfiles and 6,126 animations still decode, and the existing
vanilla idle/walk replay passes. This establishes the authored mask path.
ANM-53 separately owns dynamically bound mask references, including the
existing `MovingFlinchBlend.boneWeights` binding. ANM-04 has no prerequisite,
and ANM-19 and ANM-53 depend on ANM-04's decoded masks.

Treating absent entries in a short mask as zero is an unverified compatibility
choice. The class tables declare an array but do not specify its out-of-range
semantics. The editor rejects out-of-range editing indices. No original-runtime
capture was available to settle short-mask sampling. Full-length authored
masks and their packed values are covered by the installed corpus survey.

## Extracted-motion decoding, ANM-05

`SplineCompressedAnimation::extracted_motion` retains the optional
`hkaDefaultAnimatedReferenceFrame`. It replaces the pointer-presence boolean.
The reader keeps authored up and forward vectors, duration and every xyz
translation plus heading-angle sample. Sampling clamps to the clip endpoints,
linearly interpolates translation and uses shortest-arc spherical interpolation
about the authored up axis. Empty frames return identity and a single sample
is constant. Invalid timing, axes, non-finite values and malformed arrays
return errors. No new dependencies were added.

Sources consulted locally were
`serde-hkx/assets/classes/hkaDefaultAnimatedReferenceFrame.json` for offsets,
`HavokLib/source/packfile/hka_animated_reference_frame_default.cpp` for uniform
sample timing and `HavokLib/source/core_motion.cpp` for translation and rotation
interpolation. The Rust reader uses the existing checked packfile accessors
instead of adding a C++ library dependency. This is absolute motion decoding.
Applying motion to a host controller and blending extracted-motion outputs are
outside this decoding row.

`anm_05_authored_translation_and_rotation_sample_over_clip_lifetime` checks
known translations and headings at endpoints and between samples. Other
fixtures cover angular wrapping, a non-Z up axis, empty and constant frames,
missing motion, wrong object classes and invalid data. The installed survey
checks every packed sample and every non-null animation motion pointer.
The initial survey found 13 frames with 1,069 samples. Its guessed requirement
for more than 1,000 motion frames failed. The corrected coverage check uses
the previously recorded corpus totals of 7,699 packfiles and 6,126 animations,
rather than assuming most animations contain an optional motion object.

## ANM dependency review

The backlog's `depends_on` fields hold the execution prerequisites. The
existing interpreter already supports typed word variables and input bindings,
so the individual modifier and wrapper rows do not require transition
expression evaluation simply to read bound values. ANM-03, ANM-14 through
ANM-18, ANM-20 and ANM-21 can proceed independently of ANM-01.
ANM-19 uses ANM-04's bone-weight handling. Both
`BSBoneSwitchGeneratorBoneData.spBoneWeight` and
`hkbBlenderGeneratorChild.boneWeights` reference `hkbBoneWeightArray` in the
serde-hkx class tables. Transition arbitration, interval and transition-effect
work retains its ANM-01 prerequisite. ANM-13 retains its authored head-morph
and LIP-track prerequisites. Sharing `interp.rs` calls for isolated branches
and integration review even where no semantic prerequisite exists.

## 8. Ragdoll data (ANM-06, 2026-09-20)

`ragdoll::Ragdoll::from_packfile` reads the root `hkaRagdollInstance`.
`from_obj` also reads an instance discovered through the virtual fixups.
The result owns the low resolution skeleton, ordered rigid bodies, constraints,
and bone-to-body map. The map retains unmapped bones as `None`, and joint
endpoints use `BodyIndex` into the body array. A null joint endpoint means the
world. This is separate from the animation skeleton and its skeleton mapper.

Bodies include names, collision filters, contact materials, column-major
transforms, local centers of mass, inverse diagonal inertia and mass,
velocities, damping, time factors and gravity factors. Units remain as authored.
The transform translation is the fourth column, and the first three columns
are the rotation. `hkHalf` damping values use the upper 16 bits of an f32.

Shapes decode capsules, boxes, spheres, convex hull vertices and planes, and
translated or transformed convex children. Hull vertices are unpacked from
four-vector groups and discard the final group's unused lanes. Joint data
covers ragdoll and limited-hinge constraints, including both body-space frames,
angular limits in radians, friction, stabilization, pivot impulse settings,
motor targets and optional position motors. Null motor slots retain their
original positions.

Unknown shapes, joint classes, constraint modifiers and motor classes return
`Error::Unsupported`. Missing required pointers, duplicate bodies, invalid bone
maps, external body references, invalid hull counts, and cyclic or excessively
nested shapes return errors. The reader does not silently remove array slots.
Like the other readers at this checkpoint, these layouts require 64-bit files.
No new dependency or per-reference host node is introduced.

### Sources and scope

Layouts were read from `../engine-refs/serde-hkx/assets/classes/`, especially
`hkaRagdollInstance.json`, `hkpWorldObject.json`, `hkpEntity.json`,
`hkpMotion.json`, `hkMotionState.json`, `hkpConstraintInstance.json`, the
`hkpRagdollConstraintData*` and `hkpLimitedHingeConstraintData*` descriptions,
and their shape, motor and atom descriptions. The half-float conversion follows
`serde-hkx/crates/havok_types/src/half.rs`.
`../engine-refs/activeragdoll/src/RE/havok.cpp`, functions
`LimitedHingeToRagdollConstraint` and
`ConvertLimitedHingeDataToRagdollConstraintData`, establishes the body-space
frames and limit meanings. The existing bounds-checked packfile reader is used
instead of adding serde-hkx as a dependency. Only its class layouts and half
encoding are needed here. No Havok SDK source was consulted.

This is a data reader. Physics simulation, death activation, animation-to-ragdoll
pose mapping and host physics integration remain separate runtime work. The
reader does not claim support for every custom Havok class a mod can author.

### Validation

`tests/ragdoll.rs` checks reordered bone maps, unmapped bones, world endpoints,
material and mass properties, half-float damping, independent angular limits,
motor slots, convex vertex unpacking, and malformed references and shapes.
Two tests use `SKYRIM_DATA` and skip when it is unset:

- `anm_06_vanilla_human_has_typed_bodies_and_constraints`: the human skeleton
  yields 18 bodies and 17 constraints, comprising 9 ragdoll joints and 8 hinges.
  Transforming each joint's two pivots from body space into world space gives
  a maximum difference of 0.0000157 file units, below the preselected 0.05 bound.
- `anm_06_all_vanilla_skeleton_ragdolls_decode`: every ragdoll in every HKX file
  inside `Skyrim - Animations.bsa` decodes. The installed archive yielded
  48 ragdolls, 1009 bodies and 961 constraints, with no failures. The survey
  requires at least 40 ragdolls, 900 bodies and 850 constraints and checks body
  mass properties and skeleton relationships. It does not use a sample limit.

ANM-06 has no backlog prerequisite. Reading 32-bit ragdolls requires the layout
work in ANM-07. Consumers implementing death ragdolls can depend on this reader
without requiring behavior evaluation or extracted motion.

## Behavior transition edits, EDT-09

`BehaviorGraph::edit_transition_destination` changes the destination state of
one transition in an existing state machine. It checks that the source state,
transition slot and destination state exist, then updates the serialized
`toStateId` field in the transition's original array slot. It does not resize
the array or change conditions, events, priorities, flags or transition
effects. The caller serializes the changed opaque-preserving document with
`PackfileDocument::to_bytes` and can read the result back through the normal
packfile and behavior graph readers. Synthetic transitions without serialized
source offsets cannot be edited this way. The edit rejects a document whose
current serialized destination no longer matches the loaded graph.

The editor scope follows the Skyrim Behavior Editor's state machine and
transition editing model. Pandora Behaviour Engine+ describes typed graph
changes and reverting invalid edited nodes. This operation narrows its
supported edit to a destination state already declared in the machine, so it
can preserve all unrelated objects and fixups through the existing document
writer. The `edt_09_transition_edit` tests check a one-transition edit through
write and reread, plus rejection without mutation when the destination is
missing. They use a synthetic graph and do not claim to validate rewritten
files in Havok or Skyrim.
