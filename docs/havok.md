# soe-havok: Skyrim SE animation files and the behavior runtime design

Crate `crates/havok` reads the Havok 2010.2 packfiles Skyrim Special Edition
ships: skeletons, spline-compressed animation clips, character and project
descriptions, and behavior graphs. This document records what the crate
reads, where each layout came from, and the design for the behavior-graph
interpreter that the map's weeks 9 to 13 ask for. No Havok SDK code was
consulted or is required. The sources are the class descriptions serde-hkx
extracted from Skyrim's own files (`engine-refs/serde-hkx/assets/classes`),
the reverse-engineered runtime layouts in activeragdoll's
`include/RE/havok_behavior.h`, and HavokLib's spline decompressor for the
control point packings.

## 1. What the crate reads

| File kind | Root class | Skyrim path | Module |
| --- | --- | --- | --- |
| Rig | `hkaAnimationContainer` with one or two `hkaSkeleton` | `character assets/skeleton.hkx` | `skeleton` |
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
pointer size from the header and refuses 32-bit files with
`Error::PointerSize`, since the field offsets below are the 64-bit ones and
Skyrim LE needs a second table.

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
Float tracks (4 in `mt_jump`, the weapon and shield slots) are located but
not decoded yet, and neither is extracted motion, which Skyrim stores in a
separate `hkaDefaultAnimatedReferenceFrame` this crate does not read.

## 4. The behavior graph, as data

`behavior::BehaviorGraph` is an arena of nodes reached from the root
generator. Every node keeps its class, name, user data and variable
bindings, and the classes an interpreter needs are decoded into
`NodeKind` variants: state machines with their states and transitions,
clip generators with their triggers, blenders with their weighted
children, manual selectors, modifier generators and lists, behavior
references, blending and generator transition effects, the Skyrim `BS*`
generators listed above, and event-driven and timer modifiers. Any other
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
needs is loaded by this crate today except conditions (`hkbCondition`
expressions on transitions, flagged but not decoded) and the extracted
motion frame.

## 6. Not done and next

- 32-bit (Skyrim LE) layouts, wanted for Nemesis's LE cached behaviors.
- `hkbCondition` and `hkbExpressionCondition` expressions on transitions.
- Float tracks and extracted motion in clips, and the 16-bit and 24-bit
  rotation packings, which Skyrim files have not been seen to use.
- Ragdoll data (`hkaRagdollInstance`, `hkpRigidBody`) in `skeleton.hkx`,
  which the physics layer will want from the same file.
- The remaining `BS*` and `hkb*` node classes, driven by the unknown-class
  list the `0_master` test prints.
- A packfile writer that round-trips real files, so Nemesis-style
  patching can happen in Rust. The builder here writes valid files but
  does not yet copy unmodelled objects through.
