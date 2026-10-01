// SPDX-License-Identifier: GPL-3.0-or-later

//! The behavior graph interpreter: one [`Interpreter`] per graph instance,
//! ticked once per frame, producing a local-space pose.
//!
//! The update order is the one the Havok SDK headers publish: activate a
//! parent before its children, handle events outermost first, generate
//! recursively with children before their parent, then handle the events
//! nodes raised during the frame. What the headers do not publish (the
//! blender weight formula, transition arbitration, binding timing) is
//! filled with the plainest rule that fits and named in `docs/hkx.md`
//! so a capture from the real engine can correct it.
//!
//! Only generators produce poses. Modifiers pass through, transition
//! effects are read by the state machine that owns them, behavior
//! references and unmodelled classes yield the reference pose at weight
//! zero and are counted, so a vanilla graph runs end to end and reports
//! what it could not evaluate.

use std::collections::HashMap;

use crate::animation::{Animation, SplineCompressedAnimation};
use crate::behavior::{
    transition_flags, BehaviorGraph, BehaviorGraphData, BlenderGenerator, ClipGenerator, Condition,
    FootIkControls, FootIkLeg, NodeId, NodeKind, PassByTargetTrigger, PoweredRagdollControlData,
    StateMachine, TimeInterval, TransitionInfo, VariableBindingSet, VariableInfo, VariableType,
    WorldFromModelModeData, FALSE_TO_TRUE_SLOTS, IS_ACTIVE_SLOTS,
};
use crate::expression::{Expression, ExpressionError};
use crate::ragdoll::BodyIndex;
use crate::skeleton::AnimationContainer;
use crate::spline::Block;
use crate::{Error, QsTransform, Result};

/// A clip decoded once and ready to sample.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub name: String,
    pub animation: SplineCompressedAnimation,
    blocks: Vec<Block>,
    /// Bone index per transform track. Identity when the binding had no map.
    track_to_bone: Vec<usize>,
    /// Slot index per float track, kept separate from bone indices.
    track_to_float_slot: Vec<usize>,
}

impl Clip {
    /// The first spline animation in a clip file and its binding.
    pub fn from_container(name: &str, container: &AnimationContainer) -> Result<Clip> {
        let (index, animation) = container
            .animations
            .iter()
            .enumerate()
            .find_map(|(index, animation)| match animation {
                Animation::SplineCompressed(animation) => Some((index, animation)),
                Animation::Unsupported { .. } => None,
            })
            .ok_or_else(|| Error::Unsupported(format!("{name}: no spline animation")))?;
        let binding = container
            .bindings
            .iter()
            .find(|binding| binding.animation == Some(index));
        let map = binding
            .map(|binding| binding.transform_track_to_bone_indices.as_slice())
            .unwrap_or_default();
        let float_map = binding
            .map(|binding| binding.float_track_to_float_slot_indices.as_slice())
            .unwrap_or_default();
        Clip::from_parts_with_float_binding(name, animation.clone(), map, float_map)
    }

    pub fn from_parts(
        name: &str,
        animation: SplineCompressedAnimation,
        track_to_bone: &[i16],
    ) -> Result<Clip> {
        Self::from_parts_with_float_binding(name, animation, track_to_bone, &[])
    }

    /// Construct a clip with independent transform and float binding maps.
    pub fn from_parts_with_float_binding(
        name: &str,
        animation: SplineCompressedAnimation,
        track_to_bone: &[i16],
        track_to_float_slot: &[i16],
    ) -> Result<Clip> {
        let blocks = animation.decode()?;
        let track_to_float_slot = if track_to_float_slot.is_empty() {
            (0..animation.num_float_tracks).collect()
        } else {
            track_to_float_slot
                .iter()
                .map(|&slot| usize::try_from(slot).unwrap_or(usize::MAX))
                .collect()
        };
        let track_to_bone = if track_to_bone.is_empty() {
            (0..animation.num_transform_tracks).collect()
        } else {
            track_to_bone
                .iter()
                .map(|&b| usize::try_from(b).unwrap_or(usize::MAX))
                .collect()
        };
        Ok(Clip {
            name: name.to_string(),
            animation,
            blocks,
            track_to_bone,
            track_to_float_slot,
        })
    }

    pub fn duration(&self) -> f32 {
        self.animation.duration
    }

    /// Bone index per track, as the binding maps it.
    pub fn track_to_bone(&self) -> &[usize] {
        &self.track_to_bone
    }

    /// Slot index per float track, as the binding maps it.
    pub fn track_to_float_slot(&self) -> &[usize] {
        &self.track_to_float_slot
    }

    /// Write driven float slots, preserving unbound reference values.
    pub fn sample_floats_into(&self, time: f32, slots: &mut [f32]) {
        let (block, local) = self.animation.locate(time);
        let Some(block) = self.blocks.get(block) else {
            return;
        };
        for (track, &slot) in self.track_to_float_slot.iter().enumerate() {
            if let (Some(track), Some(value)) = (block.float_tracks.get(track), slots.get_mut(slot))
            {
                *value = track.sample(local);
            }
        }
    }

    /// Write every track into the bone it binds. Bones no track drives
    /// keep what `pose` held, so pass the reference pose.
    pub fn sample_into(&self, time: f32, pose: &mut [QsTransform]) {
        let (block, local) = self.animation.locate(time);
        let Some(block) = self.blocks.get(block) else {
            return;
        };
        for (track, &bone) in self.track_to_bone.iter().enumerate() {
            if let (Some(t), Some(slot)) = (block.tracks.get(track), pose.get_mut(bone)) {
                *slot = t.sample(local);
            }
        }
    }
}

/// Clips by the name a clip generator uses, case-insensitive with either
/// slash. Vanilla names look like `Animations\mt_idle.hkx`.
#[derive(Debug, Default, Clone)]
pub struct ClipLibrary {
    clips: HashMap<String, Clip>,
}

/// A named synchronization mark in one participant's clip, in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncMarker {
    pub name: String,
    pub time: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct SyncParticipant {
    actor: u64,
    markers: Vec<SyncMarker>,
}

/// A two-actor synchronized animation timeline. The lead participant's time
/// drives the scene and each other clip is mapped between the same named marks.
/// The host must provide corresponding mark times from each active clip.
#[derive(Debug, Clone, PartialEq)]
pub struct SynchronizedClipScene {
    participants: [SyncParticipant; 2],
    common_markers: Vec<String>,
    lead: usize,
    elapsed: f32,
    active: bool,
}

impl SynchronizedClipScene {
    /// Pair two clips by their ordered shared marker names.
    pub fn new(
        first_actor: u64,
        first_markers: Vec<SyncMarker>,
        second_actor: u64,
        second_markers: Vec<SyncMarker>,
        lead_actor: u64,
    ) -> Result<Self> {
        if first_actor == second_actor || (lead_actor != first_actor && lead_actor != second_actor)
        {
            return Err(Error::Unsupported(
                "invalid synchronized clip actors".into(),
            ));
        }
        for markers in [&first_markers, &second_markers] {
            if markers
                .iter()
                .any(|m| m.name.is_empty() || !m.time.is_finite() || m.time < 0.0)
                || markers.windows(2).any(|w| w[0].time > w[1].time)
                || markers
                    .iter()
                    .map(|m| &m.name)
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != markers.len()
            {
                return Err(Error::Unsupported(
                    "invalid synchronized clip markers".into(),
                ));
            }
        }
        let common_markers: Vec<String> = first_markers
            .iter()
            .filter(|marker| second_markers.iter().any(|other| other.name == marker.name))
            .map(|marker| marker.name.clone())
            .collect();
        let second_common_times: Vec<f32> = common_markers
            .iter()
            .filter_map(|name| {
                second_markers
                    .iter()
                    .find(|marker| &marker.name == name)
                    .map(|marker| marker.time)
            })
            .collect();
        if common_markers.len() < 2
            || second_common_times.len() != common_markers.len()
            || second_common_times.windows(2).any(|w| w[0] > w[1])
        {
            return Err(Error::Unsupported(
                "synchronized clips need two ordered shared markers".into(),
            ));
        }
        let lead = usize::from(lead_actor == second_actor);
        Ok(Self {
            participants: [
                SyncParticipant {
                    actor: first_actor,
                    markers: first_markers,
                },
                SyncParticipant {
                    actor: second_actor,
                    markers: second_markers,
                },
            ],
            common_markers,
            lead,
            elapsed: 0.0,
            active: true,
        })
    }

    /// Advance the shared lead clock once per frame. Returns false after the
    /// last common marker has been reached or the scene has been cancelled.
    pub fn advance(&mut self, dt: f32) -> bool {
        if !self.active || !dt.is_finite() || dt < 0.0 {
            return false;
        }
        self.elapsed += dt;
        let end = self
            .marker_time(self.lead, self.common_markers.len() - 1)
            .unwrap_or(0.0);
        self.active = self.elapsed < end - self.marker_time(self.lead, 0).unwrap_or(0.0);
        self.active
    }

    /// Current clip-local time for an actor, aligned at every shared marker.
    pub fn time_for(&self, actor: u64) -> Option<f32> {
        if !self.active {
            return None;
        }
        let participant = self.participants.iter().position(|p| p.actor == actor)?;
        let lead_start = self.marker_time(self.lead, 0)?;
        let lead_time = lead_start + self.elapsed;
        let lead_marker_times: Vec<f32> = (0..self.common_markers.len())
            .filter_map(|i| self.marker_time(self.lead, i))
            .collect();
        let peer_marker_times: Vec<f32> = (0..self.common_markers.len())
            .filter_map(|i| self.marker_time(participant, i))
            .collect();
        if participant == self.lead {
            return Some(lead_time);
        }
        for index in 0..lead_marker_times.len().saturating_sub(1) {
            let start = lead_marker_times[index];
            let end = lead_marker_times[index + 1];
            if lead_time <= end {
                let fraction = if end > start {
                    (lead_time - start) / (end - start)
                } else {
                    1.0
                };
                return Some(
                    peer_marker_times[index]
                        + fraction * (peer_marker_times[index + 1] - peer_marker_times[index]),
                );
            }
        }
        peer_marker_times.last().copied()
    }

    /// Stop the scene and release both participants. Calling this repeatedly is safe.
    pub fn cancel(&mut self) -> [u64; 2] {
        self.active = false;
        [self.participants[0].actor, self.participants[1].actor]
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    fn marker_time(&self, participant: usize, marker: usize) -> Option<f32> {
        let name = self.common_markers.get(marker)?;
        self.participants
            .get(participant)?
            .markers
            .iter()
            .find(|m| &m.name == name)
            .map(|m| m.time)
    }
}

fn clip_key(name: &str) -> String {
    name.to_lowercase().replace('\\', "/")
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

impl ClipLibrary {
    pub fn new() -> ClipLibrary {
        ClipLibrary::default()
    }

    pub fn insert(&mut self, clip: Clip) {
        self.clips.insert(clip_key(&clip.name), clip);
    }

    pub fn get(&self, name: &str) -> Option<&Clip> {
        self.clips.get(&clip_key(name))
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}

/// One graph's variable values. Reals live in the word table as their bit
/// pattern, which is how `hkbVariableValueSet` stores them. A quad
/// variable's word is its index into the quad table. Pointer values use the
/// variant table and retain object offsets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Variables {
    infos: Vec<VariableInfo>,
    names: Vec<String>,
    words: Vec<i32>,
    quads: Vec<[f32; 4]>,
    pointers: Vec<Option<u32>>,
}

impl Variables {
    pub fn from_graph_data(data: &BehaviorGraphData) -> Variables {
        let count = data.variable_infos.len();
        let mut words = data
            .variable_initial_values
            .as_ref()
            .map(|v| v.word_values.clone())
            .unwrap_or_default();
        words.resize(count, 0);
        let quads = data
            .variable_initial_values
            .as_ref()
            .map(|v| v.quad_values.clone())
            .unwrap_or_default();
        let mut pointers = data
            .variable_initial_values
            .as_ref()
            .map(|v| v.variant_values.clone())
            .unwrap_or_default();
        pointers.resize(count, None);
        Variables {
            infos: data.variable_infos.clone(),
            names: data.variable_names.clone(),
            words,
            quads,
            pointers,
        }
    }

    /// A table from names and types alone, for character properties and
    /// tests. Every value starts at zero.
    pub fn from_infos(names: &[&str], infos: &[VariableInfo]) -> Variables {
        Variables {
            infos: infos.to_vec(),
            names: names.iter().map(|n| (*n).to_string()).collect(),
            words: vec![0; infos.len()],
            quads: Vec::new(),
            pointers: vec![None; infos.len()],
        }
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    pub fn name(&self, index: usize) -> Option<&str> {
        self.names.get(index).map(String::as_str)
    }

    pub fn ty(&self, index: usize) -> Option<VariableType> {
        self.infos.get(index).map(|i| i.ty)
    }

    pub fn word(&self, index: usize) -> Option<i32> {
        self.words.get(index).copied()
    }

    pub fn set_word(&mut self, index: usize, value: i32) -> bool {
        match self.words.get_mut(index) {
            Some(w) => {
                *w = value;
                true
            }
            None => false,
        }
    }

    /// Pointer value for a variable declared with Havok's pointer type.
    pub fn pointer(&self, index: usize) -> Option<Option<u32>> {
        (self.ty(index) == Some(VariableType::Pointer))
            .then(|| self.pointers.get(index).copied())
            .flatten()
    }

    /// Set a pointer variable to an object offset, or null.
    pub fn set_pointer(&mut self, index: usize, value: Option<u32>) -> bool {
        if self.ty(index) != Some(VariableType::Pointer) {
            return false;
        }
        match self.pointers.get_mut(index) {
            Some(pointer) => {
                *pointer = value;
                true
            }
            None => false,
        }
    }

    /// The value as a real whatever its type: reals decode their bits,
    /// bools and ints convert.
    pub fn real(&self, index: usize) -> Option<f32> {
        let word = self.word(index)?;
        Some(match self.ty(index) {
            Some(VariableType::Real) => f32::from_bits(word as u32),
            _ => word as f32,
        })
    }

    pub fn set_real(&mut self, index: usize, value: f32) -> bool {
        match self.ty(index) {
            Some(VariableType::Real) => self.set_word(index, value.to_bits() as i32),
            Some(VariableType::Bool) => self.set_word(index, i32::from(value != 0.0)),
            _ => self.set_word(index, value as i32),
        }
    }

    pub fn bool(&self, index: usize) -> Option<bool> {
        self.real(index).map(|v| v != 0.0)
    }

    pub fn set_bool(&mut self, index: usize, value: bool) -> bool {
        self.set_real(index, if value { 1.0 } else { 0.0 })
    }

    pub fn set_by_name(&mut self, name: &str, value: f32) -> bool {
        match self.index(name) {
            Some(i) => self.set_real(i, value),
            None => false,
        }
    }

    pub fn quad(&self, index: usize) -> Option<[f32; 4]> {
        let slot = usize::try_from(self.word(index)?).ok()?;
        self.quads.get(slot).copied()
    }

    fn set_quad(&mut self, index: usize, value: [f32; 4]) -> bool {
        let Some(slot) = self.word(index).and_then(|word| usize::try_from(word).ok()) else {
            return false;
        };
        match self.quads.get_mut(slot) {
            Some(quad) => {
                *quad = value;
                true
            }
            None => false,
        }
    }
}

/// An event by graph-local id. Payloads are not carried yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub id: i32,
}

/// Kind of runtime contact reported to a ragdoll contact listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RagdollContactKind {
    Contact,
    Throw,
}

#[derive(Debug, Clone)]
struct RagdollContact {
    body: BodyIndex,
    bone_to_body: Vec<Option<BodyIndex>>,
    kind: RagdollContactKind,
}

/// A local-Z correction supplied by the host's ground query for one ankle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FootIkGroundTarget {
    pub ankle_index: usize,
    pub vertical_correction: f32,
}

/// A state machine changed state during an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateChange {
    pub machine: NodeId,
    pub from: Option<i32>,
    pub to: i32,
    /// The event that caused it, `None` for activation or a condition-only transition.
    pub event: Option<i32>,
}

/// What a variable binding writes into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    PlaybackSpeed,
    UserControlledTimeFraction,
    BlendParameter,
    OffsetVariable,
    ChildWeight(usize),
    ChildBoneWeights(usize),
    SelectedGeneratorIndex,
    Enable,
    FalseToTrueEnable(usize),
    FalseToTrueValue(usize),
    EventRangeInput,
    /// Which state the machine enters on activation.
    StartStateId,
    /// A transition effect's blend duration in seconds.
    Duration,
    TwistAngle,
    /// Output: a `BSIsActiveModifier` slot writes whether it is active into
    /// the variable, inverted when the slot's invert bit is set. Skyrim uses
    /// it to let the rest of a graph, and script through the variable, see
    /// that a branch is running.
    IsActive {
        slot: usize,
        invert: bool,
    },
    /// Output: whether the state machine is active.
    MachineIsActive,
    /// Input: the quad vector which the modifier decomposes.
    DecomposeVector,
    /// Output: one component of a decomposed vector.
    DecomposeComponent(usize),
    DampingKp,
    DampingKi,
    DampingKd,
    DampingEnableScalar,
    DampingEnableVector,
    DampingRawValue,
    /// Output: the value after the damping step.
    DampingValue,
    DampingRawVector,
    /// Output: the vector after the damping step.
    DampingVector,
    /// Input and output vectors for `hkbTransformVectorModifier`.
    TransformVectorInput,
    TransformVectorOutput,
    TransformRotation,
    TransformTranslation,
    /// Output: the elapsed time of this graph update.
    TimeStep,
    SpeedSamplerState,
    SpeedSamplerDirection,
    SpeedSamplerGoalSpeed,
    /// Output: the active sampler's sampled speed.
    SpeedSamplerSpeedOut,
    /// Input bindings and output binding for `BSInterpValueModifier`.
    InterpFrom,
    InterpTo,
    InterpOutput,
}

impl Target {
    /// Output bindings write node state into the variable after the update.
    /// The rest read the variable into node state before it. The file does
    /// not say which a binding is (`hasOutputBinding` is
    /// `SERIALIZE_IGNORED`), so the member decides: these two are node state
    /// with no setter behind them.
    fn is_output(self) -> bool {
        matches!(
            self,
            Target::IsActive { .. }
                | Target::MachineIsActive
                | Target::DecomposeComponent(_)
                | Target::DampingValue
                | Target::DampingVector
                | Target::TransformVectorOutput
                | Target::TimeStep
                | Target::SpeedSamplerSpeedOut
                | Target::InterpOutput
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct ResolvedBinding {
    node: NodeId,
    target: Target,
    variable: usize,
    /// `binding_type` 1: read the character property table instead.
    property: bool,
}

#[derive(Debug, Clone, Copy)]
struct ActiveTransition {
    priority: i16,
    flags: u16,
    from_generator: Option<NodeId>,
    to_generator: Option<NodeId>,
    elapsed: f32,
    duration: f32,
    curve: i8,
    generator_transition: Option<GeneratorTransitionRuntime>,
}

enum ActiveEffect {
    Blend(f32, i8),
    Generator {
        generator: NodeId,
        blend_in_duration: f32,
        blend_out_duration: f32,
    },
}

#[derive(Debug, Clone, Copy)]
struct GeneratorTransitionRuntime {
    generator: NodeId,
    blend_in_duration: f32,
    blend_out_duration: f32,
    elapsed: f32,
    stage: GeneratorTransitionStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeneratorTransitionStage {
    BlendIn,
    Playing,
    BlendOut,
}

#[derive(Debug, Clone, Copy, Default)]
struct CyclicBlend {
    frozen_parameter: Option<f32>,
    transition: Option<CyclicBlendTransition>,
}

#[derive(Debug, Clone, Copy)]
struct CyclicBlendTransition {
    from: f32,
    to: f32,
    elapsed: f32,
    duration: f32,
    curve: i8,
}

#[derive(Debug, Clone)]
struct DelayedTransition {
    index: usize,
    info: TransitionInfo,
    event: Event,
}

#[derive(Debug, Clone, Copy, Default)]
struct IntervalState {
    trigger: bool,
    initiate: bool,
}

/// Per node run-time state. One flat record per node keeps the arena
/// simple, unused fields cost nothing to carry.
#[derive(Debug, Clone, Default)]
struct Runtime {
    active: bool,
    local_time: f32,
    synchronized_time: Option<f32>,
    at_end: bool,
    /// Loops completed, for acyclic triggers.
    passes: u32,
    playback_speed: Option<f32>,
    user_fraction: Option<f32>,
    current_state: Option<i32>,
    previous_state: Option<i32>,
    time_in_state: f32,
    transitions: Vec<ActiveTransition>,
    delayed: Option<DelayedTransition>,
    intervals: Vec<IntervalState>,
    time_step: f32,
    timer_elapsed: f32,
    timer_fired: bool,
    timer_updated_frame: u64,
    interp_source: Option<f32>,
    interp_target: Option<f32>,
    interp_result: Option<f32>,
    blend_parameter: Option<f32>,
    offset_variable: Option<f32>,
    child_weights: Vec<Option<f32>>,
    child_bone_weights_pointers: Vec<BoundMaskPointer>,
    selected: Option<usize>,
    enabled: Option<bool>,
    /// A bound `startStateId`, read when the machine activates.
    start_state: Option<i32>,
    /// A bound blend `duration`, on a transition effect.
    duration: Option<f32>,
    cyclic_blend: CyclicBlend,
    edge_enabled: [Option<bool>; FALSE_TO_TRUE_SLOTS],
    edge_value: [Option<bool>; FALSE_TO_TRUE_SLOTS],
    edge_previous: [bool; FALSE_TO_TRUE_SLOTS],
    rotation_angle: f32,
    event_range_input: Option<f32>,
    event_range_previous: Vec<bool>,
    event_modifier_active: bool,
    pass_by_previous_position: Option<[f32; 3]>,
    pass_by_target_passed: bool,
    /// A shared modifier node can be visited more than once in one update.
    timer_frame: u64,
    direct_at_heading: f32,
    direct_at_pitch: f32,
    direct_at_initialized: bool,
    every_n_seen: u32,
    every_n_interval: u32,
    every_n_outputs: u64,

    /// A bound vector input for `BSDecomposeVectorModifier`.
    vector: Option<[f32; 4]>,
    expression_raised: Vec<bool>,
    expression_previous: Vec<bool>,
    speed_sampler_goal_speed: Option<f32>,
    speed_sampler_speed_out: f32,
    tween_start: Option<QsTransform>,
    tween_elapsed: f32,
    damping: Option<DampingRuntime>,
    /// A target supplied by the host for `BSLimbIKModifier`.
    limb_ik_target: Option<[f32; 3]>,

    /// Elapsed update time for `hkbGetUpModifier`.
    get_up_elapsed: f32,
    transform_vector_in: Option<[f32; 4]>,
    transform_vector_out: Option<[f32; 4]>,
    transform_rotation: Option<[f32; 4]>,
    transform_translation: Option<[f32; 4]>,
    activated_this_frame: bool,
    twist_angle: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default)]
enum BoundMaskPointer {
    #[default]
    Unbound,
    Null,
    Pointer(u32),
}

/// Mutable PID input, output and history for one damping modifier instance.
#[derive(Debug, Clone, Default)]
struct DampingRuntime {
    kp: f32,
    ki: f32,
    kd: f32,
    enable_scalar: bool,
    enable_vector: bool,
    raw_value: f32,
    damped_value: f32,
    raw_vector: [f32; 4],
    damped_vector: [f32; 4],
    vector_error_sum: [f32; 4],
    vector_previous_error: [f32; 4],
    error_sum: f32,
    previous_error: f32,
}

impl DampingRuntime {
    fn from_kind(kind: &NodeKind) -> Option<Self> {
        let NodeKind::DampingModifier {
            kp,
            ki,
            kd,
            enable_scalar,
            enable_vector,
            raw_value,
            damped_value,
            raw_vector,
            damped_vector,
            vector_error_sum,
            vector_previous_error,
            error_sum,
            previous_error,
            ..
        } = kind
        else {
            return None;
        };
        Some(Self {
            kp: *kp,
            ki: *ki,
            kd: *kd,
            enable_scalar: *enable_scalar,
            enable_vector: *enable_vector,
            raw_value: *raw_value,
            damped_value: *damped_value,
            raw_vector: *raw_vector,
            damped_vector: *damped_vector,
            vector_error_sum: *vector_error_sum,
            vector_previous_error: *vector_previous_error,
            error_sum: *error_sum,
            previous_error: *previous_error,
        })
    }
}

/// A generator's result for one frame: a local pose and how much of it
/// counts. Weight zero means "nothing to say", the reference pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub pose: Vec<QsTransform>,
    /// Character transform emitted by generators, separate from the local
    /// skeleton pose.
    pub world_from_model: QsTransform,
    /// Contribution per bone, retained through nested blends and transitions.
    pub bone_weights: Vec<f32>,
    pub weight: f32,
    /// Authored foot IK controls from the active modifier path.
    pub foot_ik_controls: Option<FootIkControls>,
    /// Powered ragdoll controls emitted by active modifiers, with blend weight applied.
    pub ragdoll_controls: Vec<RagdollControlOutput>,
    /// Rigid-body ragdoll controls supplied by active behavior modifiers.
    pub rigid_body_ragdoll_controls: Vec<RigidBodyRagdollControlOutput>,
}

/// One bone's powered ragdoll control contribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RagdollControlOutput {
    pub bone: i16,
    pub control: PoweredRagdollControlData,
    pub world_from_model: WorldFromModelModeData,
    pub weight: f32,
}
/// One authored `hkbRigidBodyRagdollControlsModifier` contribution.
#[derive(Debug, Clone, PartialEq)]
pub struct RigidBodyRagdollControlOutput {
    /// Hierarchy gain, velocity damping, acceleration gain, velocity gain,
    /// position gain and limits, then snap gain and limits, in Havok order.
    pub control_data: [f32; 12],
    pub duration_to_blend: f32,
    /// `None` leaves the modifier's bone selection unspecified.
    pub bones: Option<Vec<i16>>,
    /// Contribution weight after the graph's blender or transition weight.
    pub weight: f32,
}

/// Counts of what the graph asked for that the interpreter could not do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unsupported {
    /// Clip generators whose animation the library does not hold.
    pub missing_clips: Vec<String>,
    /// Behavior reference generators reached during generate.
    pub behavior_references: Vec<String>,
    /// Node classes reached during generate with no evaluation.
    pub unknown_classes: Vec<String>,
    /// Binding member paths with no setter.
    pub unbound: Vec<String>,
    /// Condition classes, source expressions or values that cannot be evaluated.
    pub conditions: Vec<String>,
    /// Expression modifiers whose syntax, target or event mode is unsupported.
    pub expression_modifiers: Vec<String>,
    /// Modifier lists whose child pointer closes a cycle.
    pub modifier_list_cycles: Vec<String>,
    /// Modifier-list paths deeper than the interpreter's fixed safety limit.
    pub modifier_list_depth_limit: Vec<String>,
}

fn distribute_twist(output: &mut Output, twist: &crate::behavior::TwistModifier, angle: f32) {
    if !angle.is_finite()
        || twist.axis_of_rotation[..3].iter().any(|v| !v.is_finite())
        || !matches!(twist.set_angle_method, 0 | 1)
        || !matches!(twist.rotation_axis_coordinates, 0 | 1)
    {
        return;
    }
    let start = usize::try_from(twist.start_bone_index).ok();
    let end = usize::try_from(twist.end_bone_index).ok();
    let (Some(start), Some(end)) = (start, end) else {
        return;
    };
    if start >= output.pose.len() || end >= output.pose.len() {
        return;
    }
    let axis = [
        twist.axis_of_rotation[0],
        twist.axis_of_rotation[1],
        twist.axis_of_rotation[2],
    ];
    let axis_length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if !axis_length.is_finite() || axis_length <= f32::EPSILON {
        return;
    }
    let axis = [
        axis[0] / axis_length,
        axis[1] / axis_length,
        axis[2] / axis_length,
    ];
    let count = start.abs_diff(end) + 1;
    let total_weight = if twist.set_angle_method == 0 {
        count as f32
    } else {
        (count * (count + 1)) as f32 / 2.0
    };
    let indices: Vec<usize> = if start <= end {
        (start..=end).collect()
    } else {
        (end..=start).rev().collect()
    };
    for (position, bone) in indices.into_iter().enumerate() {
        let weight = if twist.set_angle_method == 0 {
            1.0 / total_weight
        } else {
            (position + 1) as f32 / total_weight
        };
        let half = angle * weight * 0.5;
        let sine = half.sin();
        let delta = [axis[0] * sine, axis[1] * sine, axis[2] * sine, half.cos()];
        let rotation = output.pose[bone].rotation;
        output.pose[bone].rotation = if !twist.is_additive {
            delta
        } else if twist.rotation_axis_coordinates == 0 {
            quat_mul(delta, rotation)
        } else {
            quat_mul(rotation, delta)
        };
    }
}

fn false_to_true_slot(path: &str, prefix: &str) -> Option<usize> {
    let suffix = path.strip_prefix(prefix)?;
    if suffix.len() != 1 {
        return None;
    }
    suffix
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
        .filter(|slot| *slot < FALSE_TO_TRUE_SLOTS)
}

fn crossed_target(previous: [f32; 3], current: [f32; 3], trigger: &PassByTargetTrigger) -> bool {
    let direction = [
        trigger.movement_direction[0],
        trigger.movement_direction[1],
        trigger.movement_direction[2],
    ];
    if !previous
        .iter()
        .chain(current.iter())
        .chain(direction.iter())
        .all(|value| value.is_finite())
        || !trigger.radius.is_finite()
        || trigger.radius < 0.0
    {
        return false;
    }
    let length = direction
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if length <= f32::EPSILON {
        return false;
    }
    let normal = direction.map(|value| value / length);
    let target = [
        trigger.target_position[0],
        trigger.target_position[1],
        trigger.target_position[2],
    ];
    if !target.iter().all(|value| value.is_finite()) {
        return false;
    }
    let before = dot(subtract(previous, target), normal);
    let after = dot(subtract(current, target), normal);
    if before > 0.0 || after <= 0.0 {
        return false;
    }
    let fraction = before / (before - after);
    let crossing =
        std::array::from_fn(|axis| previous[axis] + (current[axis] - previous[axis]) * fraction);
    let offset = subtract(crossing, target);
    let lateral = subtract(offset, scale(normal, dot(offset, normal)));
    dot(lateral, lateral) <= trigger.radius * trigger.radius
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

fn scale(vector: [f32; 3], scalar: f32) -> [f32; 3] {
    vector.map(|value| value * scalar)
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

fn offset_amount(value: f32, start: f32, end: f32) -> f32 {
    if !value.is_finite() || !start.is_finite() || !end.is_finite() {
        return 0.0;
    }
    if start == end {
        return if value >= end { 1.0 } else { 0.0 };
    }
    ((value - start) / (end - start)).clamp(0.0, 1.0)
}

fn apply_pose_offset(base: &mut Output, offset: &Output, amount: f32) {
    if amount == 0.0 {
        return;
    }
    for (base, offset) in base.pose.iter_mut().zip(&offset.pose) {
        for (base, offset) in base.translation[..3]
            .iter_mut()
            .zip(&offset.translation[..3])
        {
            *base += offset * amount;
        }
        let delta = scaled_rotation(offset.rotation, amount);
        base.rotation = multiply_rotation(base.rotation, delta);
        for (base, offset) in base.scale[..3].iter_mut().zip(&offset.scale[..3]) {
            *base *= 1.0 + (offset - 1.0) * amount;
        }
    }
}

fn scaled_rotation(mut rotation: [f32; 4], amount: f32) -> [f32; 4] {
    if rotation[3] < 0.0 {
        for component in &mut rotation {
            *component = -*component;
        }
    }
    let mut result = [
        rotation[0] * amount,
        rotation[1] * amount,
        rotation[2] * amount,
        1.0 + (rotation[3] - 1.0) * amount,
    ];
    normalize_rotation(&mut result);
    result
}

fn multiply_rotation(left: [f32; 4], right: [f32; 4]) -> [f32; 4] {
    let [x1, y1, z1, w1] = left;
    let [x2, y2, z2, w2] = right;
    let mut result = [
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
    ];
    normalize_rotation(&mut result);
    result
}

fn normalize_rotation(rotation: &mut [f32; 4]) {
    let norm = rotation
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm > f32::EPSILON {
        for value in rotation {
            *value /= norm;
        }
    }
}

fn every_n_interval(
    id: NodeId,
    outputs: u64,
    authored_maximum: i8,
    authored_minimum: i8,
    randomize: bool,
) -> u32 {
    let maximum = u32::try_from(authored_maximum).unwrap_or(1).max(1);
    if !randomize {
        return maximum;
    }
    let minimum = u32::try_from(authored_minimum)
        .unwrap_or(1)
        .clamp(1, maximum);
    let range = maximum - minimum + 1;
    let seed = u64::try_from(id.0).unwrap_or(0) ^ outputs.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mut value = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    minimum + u32::try_from(value % u64::from(range)).unwrap_or(0)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(vector: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(vector, vector).sqrt();
    (length.is_finite() && length > f32::EPSILON)
        .then(|| [vector[0] / length, vector[1] / length, vector[2] / length])
}

fn apply_foot_ik(
    output: &mut Output,
    legs: &[FootIkLeg],
    targets: &[FootIkGroundTarget],
    raycast_distance_up: f32,
    raycast_distance_down: f32,
    vertical_offset: f32,
    ground_ascending_gain: f32,
    ground_descending_gain: f32,
) {
    let up = raycast_distance_up.max(0.0);
    let down = raycast_distance_down.max(0.0);
    for leg in legs {
        let Some(target) = targets
            .iter()
            .find(|target| target.ankle_index == leg.ankle_index)
        else {
            continue;
        };
        let Some(ankle) = output.pose.get_mut(leg.ankle_index) else {
            continue;
        };
        let correction = (target.vertical_correction + vertical_offset).clamp(-down, up);
        let gain = if correction >= 0.0 {
            ground_ascending_gain
        } else {
            ground_descending_gain
        };
        if gain.is_finite() {
            ankle.translation[2] += correction * gain;
        }
    }
}

fn transform_vector(
    input: [f32; 4],
    rotation: [f32; 4],
    translation: [f32; 4],
    rotate_only: bool,
    inverse: bool,
) -> [f32; 4] {
    let mut vector = [input[0], input[1], input[2]];
    if inverse && !rotate_only {
        for (component, offset) in vector.iter_mut().zip(&translation[..3]) {
            *component -= offset;
        }
    }

    let q = if inverse {
        [-rotation[0], -rotation[1], -rotation[2], rotation[3]]
    } else {
        rotation
    };
    let [x, y, z, w] = q;
    let rotated = [
        (1.0 - 2.0 * (y * y + z * z)) * vector[0]
            + 2.0 * (x * y - z * w) * vector[1]
            + 2.0 * (x * z + y * w) * vector[2],
        2.0 * (x * y + z * w) * vector[0]
            + (1.0 - 2.0 * (x * x + z * z)) * vector[1]
            + 2.0 * (y * z - x * w) * vector[2],
        2.0 * (x * z - y * w) * vector[0]
            + 2.0 * (y * z + x * w) * vector[1]
            + (1.0 - 2.0 * (x * x + y * y)) * vector[2],
    ];
    vector = rotated;
    if !inverse && !rotate_only {
        for (component, offset) in vector.iter_mut().zip(&translation[..3]) {
            *component += offset;
        }
    }
    [vector[0], vector[1], vector[2], input[3]]
}

pub struct Interpreter<'a> {
    graph: &'a BehaviorGraph,
    clips: &'a ClipLibrary,
    reference_pose: Vec<QsTransform>,
    pub variables: Variables,
    pub character_properties: Variables,
    nodes: Vec<Runtime>,
    bindings: Vec<ResolvedBinding>,
    bone_weight_masks: HashMap<u32, Vec<f32>>,
    conditions: HashMap<Condition, std::result::Result<Expression, ExpressionError>>,
    expressions: Vec<Option<Vec<std::result::Result<Expression, ExpressionError>>>>,
    /// Modifier nodes in this frame's authored traversal order. Output
    /// bindings publish in this order rather than arena allocation order.
    modifier_order: Vec<NodeId>,
    pending: Vec<Event>,
    ragdoll_contacts: Vec<RagdollContact>,
    raised: Vec<Event>,
    handled: Vec<Event>,
    changes: Vec<StateChange>,
    unsupported: Unsupported,
    output: Output,
    expression_random_state: u64,
    character_position: Option<[f32; 3]>,
    foot_ik_ground_targets: Vec<FootIkGroundTarget>,
    frame: u64,
    condition_frames: Vec<u64>,
}

/// Rounds of internally raised events handled per update before the rest
/// wait for the next frame. A graph whose states raise each other's
/// events forever would otherwise never return.
const MAX_EVENT_ROUNDS: usize = 8;
/// A malformed modifier-list chain must not consume an unbounded call stack.
const MAX_MODIFIER_LIST_DEPTH: usize = 64;

impl<'a> Interpreter<'a> {
    /// Resolve every binding and size the run-time arena. The reference
    /// pose sets the bone count of every output.
    pub fn new(
        graph: &'a BehaviorGraph,
        clips: &'a ClipLibrary,
        reference_pose: Vec<QsTransform>,
    ) -> Interpreter<'a> {
        let mut nodes = vec![Runtime::default(); graph.nodes.len()];
        for node in &graph.nodes {
            if let NodeKind::Blender(b) = &node.kind {
                nodes[node.id.0].child_weights = vec![None; b.children.len()];
                nodes[node.id.0].child_bone_weights_pointers =
                    vec![BoundMaskPointer::Unbound; b.children.len()];
            }
            if let NodeKind::EventsFromRangeModifier { ranges, .. } = &node.kind {
                nodes[node.id.0].event_range_previous = vec![false; ranges.len()];
            }
            nodes[node.id.0].damping = DampingRuntime::from_kind(&node.kind);
        }
        let bone_weight_masks = graph
            .nodes
            .iter()
            .filter_map(|node| match &node.kind {
                NodeKind::Blender(blender) => Some(&blender.bone_weight_masks),
                _ => None,
            })
            .flatten()
            .map(|(&offset, weights)| (offset, weights.clone()))
            .collect();
        let mut interp = Interpreter {
            graph,
            clips,
            variables: Variables::from_graph_data(&graph.data),
            character_properties: Variables::from_infos(
                &graph
                    .data
                    .character_property_names
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                &graph.data.character_property_infos,
            ),
            nodes,
            bindings: Vec::new(),
            bone_weight_masks,
            conditions: HashMap::new(),
            expressions: vec![None; graph.nodes.len()],
            modifier_order: Vec::new(),
            pending: Vec::new(),
            ragdoll_contacts: Vec::new(),
            raised: Vec::new(),
            handled: Vec::new(),
            changes: Vec::new(),
            unsupported: Unsupported::default(),
            output: Output {
                pose: reference_pose.clone(),
                world_from_model: QsTransform::IDENTITY,
                bone_weights: vec![0.0; reference_pose.len()],
                weight: 0.0,
                foot_ik_controls: None,
                ragdoll_controls: Vec::new(),
                rigid_body_ragdoll_controls: Vec::new(),
            },
            character_position: None,
            foot_ik_ground_targets: Vec::new(),
            reference_pose,
            expression_random_state: 0x4d59_5df4_d0f3_3173,
            frame: 0,
            condition_frames: vec![0; graph.nodes.len()],
        };
        interp.resolve_bindings();
        interp.resolve_conditions();
        interp.resolve_expressions();
        interp
    }

    fn resolve_expressions(&mut self) {
        for node in &self.graph.nodes {
            let NodeKind::EvaluateExpression { expressions, .. } = &node.kind else {
                continue;
            };
            self.nodes[node.id.0].expression_raised = vec![false; expressions.len()];
            self.nodes[node.id.0].expression_previous = vec![false; expressions.len()];
            self.expressions[node.id.0] = Some(
                expressions
                    .iter()
                    .map(|data| {
                        Expression::compile(
                            &data.expression,
                            &self.variables,
                            &self.character_properties,
                            None,
                        )
                    })
                    .collect(),
            );
        }
    }

    fn resolve_conditions(&mut self) {
        for node in &self.graph.nodes {
            let NodeKind::StateMachine(sm) = &node.kind else {
                continue;
            };
            for transition in sm
                .states
                .iter()
                .flat_map(|state| &state.transitions)
                .chain(&sm.wildcard_transitions)
            {
                if transition.flags & transition_flags::DISABLE_CONDITION != 0 {
                    continue;
                }
                if let Some(condition) = &transition.condition {
                    if let Some((source, scope)) = condition.expression() {
                        let result =
                            self.conditions.entry(condition.clone()).or_insert_with(|| {
                                Expression::compile(
                                    source,
                                    &self.variables,
                                    &self.character_properties,
                                    scope,
                                )
                            });
                        if let Err(error) = result {
                            push_unique(
                                &mut self.unsupported.conditions,
                                format!("{source}: {error:?}"),
                            );
                        }
                    } else if let Condition::Unsupported(class) = condition {
                        push_unique(&mut self.unsupported.conditions, class.clone());
                    }
                }
            }
        }
    }

    fn condition_passes(&mut self, transition: &TransitionInfo) -> bool {
        if transition.flags & transition_flags::DISABLE_CONDITION != 0 {
            return true;
        }
        let Some(condition) = &transition.condition else {
            return true;
        };
        let Some((source, _)) = condition.expression() else {
            return false;
        };
        let Some(Ok(expression)) = self.conditions.get(condition) else {
            return false;
        };
        match expression.evaluate(
            &self.variables,
            &self.character_properties,
            &mut self.expression_random_state,
        ) {
            Ok(value) => value != 0.0,
            Err(error) => {
                push_unique(
                    &mut self.unsupported.conditions,
                    format!("{source}: {error:?}"),
                );
                false
            }
        }
    }

    /// Poll each active machine once, outermost first. A transition waits
    /// taken by a condition waits until the next frame before polling its new state.
    fn handle_conditions(&mut self, id: NodeId) {
        if !self.nodes[id.0].active || self.condition_frames[id.0] == self.frame {
            return;
        }
        self.condition_frames[id.0] = self.frame;
        let node = self.graph.node(id);
        let children = if let NodeKind::StateMachine(sm) = &node.kind {
            if self.machine_event(id, sm, Event { id: -1 }) {
                return;
            }
            self.active_generators_of_machine(id, sm)
        } else {
            node.children()
        };
        for child in children {
            self.handle_conditions(child);
        }
    }

    fn resolve_bindings(&mut self) {
        let graph = self.graph;
        for node in &graph.nodes {
            if let Some(set) = &node.bindings {
                self.resolve_set(node.id, set, None);
            }
            if let NodeKind::Blender(b) = &node.kind {
                for (i, child) in b.children.iter().enumerate() {
                    if let Some(set) = &child.bindings {
                        self.resolve_set(node.id, set, Some(i));
                    }
                }
            }
        }
    }

    fn resolve_set(&mut self, node: NodeId, set: &VariableBindingSet, child: Option<usize>) {
        let graph = self.graph;
        let kind = &graph.node(node).kind;
        for b in &set.bindings {
            let target = match (b.member_path.as_str(), child, kind) {
                ("playbackSpeed", None, NodeKind::Clip(_)) => Some(Target::PlaybackSpeed),
                ("userControlledTimeFraction", None, NodeKind::Clip(_)) => {
                    Some(Target::UserControlledTimeFraction)
                }
                ("blendParameter", None, NodeKind::Blender(_)) => Some(Target::BlendParameter),
                ("fOffsetVariable", None, NodeKind::OffsetAnimation { .. }) => {
                    Some(Target::OffsetVariable)
                }
                ("weight", Some(i), NodeKind::Blender(_)) => Some(Target::ChildWeight(i)),
                ("boneWeights", Some(i), NodeKind::Blender(_)) => Some(Target::ChildBoneWeights(i)),
                ("selectedGeneratorIndex", None, NodeKind::ManualSelector { .. }) => {
                    Some(Target::SelectedGeneratorIndex)
                }
                ("enable", None, _) => Some(Target::Enable),
                ("inputValue", None, NodeKind::EventsFromRangeModifier { .. }) => {
                    Some(Target::EventRangeInput)
                }
                ("isActive", None, NodeKind::StateMachine(_)) => Some(Target::MachineIsActive),
                ("startStateId", None, NodeKind::StateMachine(_)) => Some(Target::StartStateId),
                ("duration", None, NodeKind::BlendingTransition(_)) => Some(Target::Duration),
                ("twistAngle", None, NodeKind::TwistModifier(_)) => Some(Target::TwistAngle),
                ("timeStep", None, NodeKind::GetTimeStepModifier { .. }) => Some(Target::TimeStep),
                ("source", None, NodeKind::InterpValue { .. }) => Some(Target::InterpFrom),
                ("target", None, NodeKind::InterpValue { .. }) => Some(Target::InterpTo),
                ("result", None, NodeKind::InterpValue { .. }) => Some(Target::InterpOutput),
                ("fBlendParameter", None, NodeKind::CyclicBlendTransition { .. }) => {
                    Some(Target::BlendParameter)
                }
                (path, None, NodeKind::FalseToTrueModifier { .. }) => {
                    if let Some(slot) = false_to_true_slot(path, "bEnableEvent") {
                        Some(Target::FalseToTrueEnable(slot))
                    } else {
                        false_to_true_slot(path, "bVariableToTest").map(Target::FalseToTrueValue)
                    }
                }
                (path, None, NodeKind::IsActiveModifier { invert_active, .. })
                    if path.starts_with("bIsActive") =>
                {
                    path["bIsActive".len()..]
                        .parse::<usize>()
                        .ok()
                        .filter(|slot| *slot < IS_ACTIVE_SLOTS)
                        .map(|slot| Target::IsActive {
                            slot,
                            invert: invert_active[slot],
                        })
                }
                ("vector", None, NodeKind::DecomposeVector { .. }) => Some(Target::DecomposeVector),
                ("state", None, NodeKind::SpeedSampler { .. }) => Some(Target::SpeedSamplerState),
                ("direction", None, NodeKind::SpeedSampler { .. }) => {
                    Some(Target::SpeedSamplerDirection)
                }
                ("goalSpeed", None, NodeKind::SpeedSampler { .. }) => {
                    Some(Target::SpeedSamplerGoalSpeed)
                }
                ("speedOut", None, NodeKind::SpeedSampler { .. }) => {
                    Some(Target::SpeedSamplerSpeedOut)
                }
                ("kP", None, NodeKind::DampingModifier { .. }) => Some(Target::DampingKp),
                ("kI", None, NodeKind::DampingModifier { .. }) => Some(Target::DampingKi),
                ("kD", None, NodeKind::DampingModifier { .. }) => Some(Target::DampingKd),
                ("enableScalarDamping", None, NodeKind::DampingModifier { .. }) => {
                    Some(Target::DampingEnableScalar)
                }
                ("enableVectorDamping", None, NodeKind::DampingModifier { .. }) => {
                    Some(Target::DampingEnableVector)
                }
                ("rawValue", None, NodeKind::DampingModifier { .. }) => {
                    Some(Target::DampingRawValue)
                }
                ("dampedValue", None, NodeKind::DampingModifier { .. }) => {
                    Some(Target::DampingValue)
                }
                ("rawVector", None, NodeKind::DampingModifier { .. }) => {
                    Some(Target::DampingRawVector)
                }
                ("dampedVector", None, NodeKind::DampingModifier { .. }) => {
                    Some(Target::DampingVector)
                }
                ("vectorIn", None, NodeKind::TransformVector { .. }) => {
                    Some(Target::TransformVectorInput)
                }
                ("vectorOut", None, NodeKind::TransformVector { .. }) => {
                    Some(Target::TransformVectorOutput)
                }
                ("rotation", None, NodeKind::TransformVector { .. }) => {
                    Some(Target::TransformRotation)
                }
                ("translation", None, NodeKind::TransformVector { .. }) => {
                    Some(Target::TransformTranslation)
                }
                ("x", None, NodeKind::DecomposeVector { .. }) => {
                    Some(Target::DecomposeComponent(0))
                }
                ("y", None, NodeKind::DecomposeVector { .. }) => {
                    Some(Target::DecomposeComponent(1))
                }
                ("z", None, NodeKind::DecomposeVector { .. }) => {
                    Some(Target::DecomposeComponent(2))
                }
                ("w", None, NodeKind::DecomposeVector { .. }) => {
                    Some(Target::DecomposeComponent(3))
                }
                (path, _, _) if path.starts_with("children:") && path.ends_with("/weight") => path
                    ["children:".len()..path.len() - "/weight".len()]
                    .parse()
                    .ok()
                    .map(Target::ChildWeight),
                _ => None,
            };
            let variable = usize::try_from(b.variable_index).ok();
            match (target, variable) {
                (Some(Target::ChildBoneWeights(child)), Some(variable))
                    if (if b.binding_type == 1 {
                        self.character_properties.ty(variable)
                    } else {
                        self.variables.ty(variable)
                    }) == Some(VariableType::Pointer)
                        && child
                            < match kind {
                                NodeKind::Blender(blender) => blender.children.len(),
                                _ => 0,
                            } =>
                {
                    self.bindings.push(ResolvedBinding {
                        node,
                        target: Target::ChildBoneWeights(child),
                        variable,
                        property: b.binding_type == 1,
                    });
                }
                (Some(Target::ChildBoneWeights(_)), _) => push_unique(
                    &mut self.unsupported.unbound,
                    format!(
                        "{}.{} ({})",
                        graph.node(node).name,
                        b.member_path,
                        graph.node(node).class
                    ),
                ),
                (Some(target), Some(variable)) => self.bindings.push(ResolvedBinding {
                    node,
                    target,
                    variable,
                    property: b.binding_type == 1,
                }),
                _ => push_unique(
                    &mut self.unsupported.unbound,
                    format!(
                        "{}.{} ({})",
                        graph.node(node).name,
                        b.member_path,
                        graph.node(node).class
                    ),
                ),
            }
        }
    }

    pub fn graph(&self) -> &'a BehaviorGraph {
        self.graph
    }

    /// Replace the per-ankle local-Z corrections used by future updates.
    ///
    /// Non-finite corrections are ignored.
    pub fn set_foot_ik_ground_targets(&mut self, targets: &[FootIkGroundTarget]) {
        self.foot_ik_ground_targets.clear();
        self.foot_ik_ground_targets.extend(
            targets
                .iter()
                .copied()
                .filter(|target| target.vertical_correction.is_finite()),
        );
    }

    pub fn unsupported(&self) -> &Unsupported {
        &self.unsupported
    }

    /// Bindings that resolved to a setter.
    pub fn bound_count(&self) -> usize {
        self.bindings.len()
    }

    /// The pose the last update produced, local space, one per bone.
    pub fn pose(&self) -> &[QsTransform] {
        &self.output.pose
    }

    pub fn output(&self) -> &Output {
        &self.output
    }

    /// State changes since the last [`Self::take_changes`].
    pub fn changes(&self) -> &[StateChange] {
        &self.changes
    }

    pub fn take_changes(&mut self) -> Vec<StateChange> {
        std::mem::take(&mut self.changes)
    }

    /// Every event delivered to the graph since the last
    /// [`Self::take_events`], external ones and the ones nodes raised.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.handled)
    }

    /// Queue an event for the next update.
    pub fn send_event(&mut self, id: i32) {
        self.pending.push(Event { id });
    }

    /// Queue an event by name. False when the graph has no such event.
    pub fn send_event_named(&mut self, name: &str) -> bool {
        match self.graph.data.event_id(name) {
            Some(id) => {
                self.send_event(id);
                true
            }
            None => false,
        }
    }

    /// Queue a ragdoll contact for the next update. The contact is delivered
    /// only to active listeners whose declared bones map to `body`.
    pub fn send_ragdoll_contact(
        &mut self,
        body: BodyIndex,
        bone_to_body: &[Option<BodyIndex>],
        kind: RagdollContactKind,
    ) {
        self.ragdoll_contacts.push(RagdollContact {
            body,
            bone_to_body: bone_to_body.to_vec(),
            kind,
        });
    }

    pub fn is_active(&self, node: NodeId) -> bool {
        self.nodes.get(node.0).is_some_and(|n| n.active)
    }

    /// A state machine's current state id.
    pub fn current_state(&self, machine: NodeId) -> Option<i32> {
        self.nodes.get(machine.0).and_then(|n| n.current_state)
    }

    /// A state machine's current state name.
    pub fn current_state_name(&self, machine: NodeId) -> Option<&'a str> {
        let id = self.current_state(machine)?;
        match &self.graph.node(machine).kind {
            NodeKind::StateMachine(sm) => sm.state(id).map(|s| s.name.as_str()),
            _ => None,
        }
    }

    /// Whether a state machine is mid-transition.
    pub fn in_transition(&self, machine: NodeId) -> bool {
        self.nodes
            .get(machine.0)
            .is_some_and(|n| !n.transitions.is_empty())
    }

    /// Number of transition effects currently retained by a state machine.
    pub fn active_transition_count(&self, machine: NodeId) -> usize {
        self.nodes
            .get(machine.0)
            .map_or(0, |node| node.transitions.len())
    }

    /// A clip generator's local time, when it is active.
    pub fn clip_time(&self, clip: NodeId) -> Option<f32> {
        let rt = self.nodes.get(clip.0)?;
        (rt.active && matches!(self.graph.node(clip).kind, NodeKind::Clip(_)))
            .then_some(rt.local_time)
    }

    /// Set the externally coordinated local time for a clip wrapped by a
    /// `BSSynchronizedClipGenerator`. Call once for each actor before update.
    pub fn set_synchronized_clip_time(&mut self, synchronized: NodeId, time: f32) -> bool {
        let Some(node) = self.graph.nodes.get(synchronized.0) else {
            return false;
        };
        let NodeKind::SynchronizedClip {
            clip_generator: Some(clip),
            ..
        } = node.kind
        else {
            return false;
        };
        if !time.is_finite()
            || time < 0.0
            || !matches!(
                self.graph.nodes.get(clip.0).map(|n| &n.kind),
                Some(NodeKind::Clip(_))
            )
        {
            return false;
        }
        if let Some(runtime) = self.nodes.get_mut(clip.0) {
            runtime.synchronized_time = Some(time);
            true
        } else {
            false
        }
    }

    /// Clear externally coordinated time when a synchronized scene ends.
    pub fn clear_synchronized_clip_time(&mut self, synchronized: NodeId) -> bool {
        let Some(node) = self.graph.nodes.get(synchronized.0) else {
            return false;
        };
        let NodeKind::SynchronizedClip {
            clip_generator: Some(clip),
            ..
        } = node.kind
        else {
            return false;
        };
        let Some(runtime) = self.nodes.get_mut(clip.0) else {
            return false;
        };
        runtime.synchronized_time = None;
        true
    }

    /// Whether a single-play clip has reached its end.
    pub fn clip_at_end(&self, clip: NodeId) -> bool {
        self.nodes.get(clip.0).is_some_and(|n| n.active && n.at_end)
    }

    /// Every active clip generator with its local time.
    pub fn active_clips(&self) -> Vec<(NodeId, f32)> {
        self.graph
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Clip(_)))
            .filter_map(|n| self.clip_time(n.id).map(|t| (n.id, t)))
            .collect()
    }

    /// Every active state machine with its current state name.
    pub fn active_states(&self) -> Vec<(NodeId, &'a str)> {
        self.graph
            .nodes
            .iter()
            .filter(|n| self.is_active(n.id))
            .filter_map(|n| self.current_state_name(n.id).map(|s| (n.id, s)))
            .collect()
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Set the character's world position for pass-by-target modifiers.
    /// Call before `update` so the interpreter compares this sample with the
    /// previous one. Position and modifier targets must use the same space.
    pub fn set_character_position(&mut self, position: [f32; 3]) {
        self.character_position = Some(position);
    }

    /// Set the model-space cast target for a limb IK modifier. The host owns
    /// the cast query because the HKX interpreter has no collision world.
    pub fn set_limb_ik_target(&mut self, id: NodeId, target: Option<[f32; 3]>) -> bool {
        let Some(node) = self.graph.nodes.get(id.0) else {
            return false;
        };
        if !matches!(node.kind, NodeKind::LimbIk { .. }) {
            return false;
        }
        self.nodes[id.0].limb_ik_target =
            target.filter(|point| point.iter().all(|v| v.is_finite()));
        true
    }

    /// One frame: bindings, activation, events, generate, raised events.
    pub fn update(&mut self, dt: f32) -> &Output {
        self.frame += 1;
        self.apply_bindings();
        if let Some(root) = self.graph.root {
            if !self.nodes[root.0].active {
                self.activate(root);
            }
            let pending = std::mem::take(&mut self.pending);
            for ev in pending {
                self.handled.push(ev);
                self.handle_event(root, ev);
            }
            self.handle_conditions(root);
            self.modifier_order.clear();
            let output = self.generate(root, dt);
            self.output = output;
            self.update_i_state_managers();
            self.update_time_step_modifiers(dt);
            self.update_ragdoll_contact_modifiers();
            self.update_pass_by_target_modifiers();
            self.update_interp_value_modifiers(dt);
            for _ in 0..MAX_EVENT_ROUNDS {
                if self.raised.is_empty() {
                    break;
                }
                let raised = std::mem::take(&mut self.raised);
                for ev in raised {
                    self.handled.push(ev);
                    self.handle_event(root, ev);
                }
            }
        }
        self.apply_output_bindings();
        &self.output
    }

    fn apply_keyframe_bones_modifier(&self, id: NodeId, output: &mut Output) {
        if !self.nodes[id.0].active {
            return;
        }
        let NodeKind::KeyframeBonesModifier {
            keyframe_info,
            keyframed_bones,
            ..
        } = &self.graph.node(id).kind
        else {
            return;
        };
        for info in keyframe_info.iter().filter(|info| info.is_valid) {
            if keyframed_bones
                .as_ref()
                .is_some_and(|bones| !bones.contains(&info.bone_index))
            {
                continue;
            }
            let Ok(bone) = usize::try_from(info.bone_index) else {
                continue;
            };
            let Some(transform) = output.pose.get_mut(bone) else {
                continue;
            };
            transform.translation = info.position;
            transform.rotation = info.rotation;
            if let Some(weight) = output.bone_weights.get_mut(bone) {
                *weight = 1.0;
            }
        }
    }

    /// The modifier records the interval supplied to this graph update. It
    /// runs after generation so only the modifier subtree reached this frame
    /// can publish it through an output binding.
    fn update_time_step_modifiers(&mut self, dt: f32) {
        for node in &self.modifier_order {
            if matches!(
                self.graph.node(*node).kind,
                NodeKind::GetTimeStepModifier { .. }
            ) {
                self.nodes[node.0].time_step = if self.nodes[node.0].active { dt } else { 0.0 };
            }
        }
    }

    fn update_pass_by_target_modifiers(&mut self) {
        let Some(current) = self.character_position else {
            return;
        };
        let mut emitted = Vec::new();
        for node in self.modifier_order.clone() {
            if !self.nodes[node.0].active {
                continue;
            }
            let NodeKind::PassByTargetTrigger { trigger, .. } = &self.graph.node(node).kind else {
                continue;
            };
            let runtime = &mut self.nodes[node.0];
            if !runtime.pass_by_target_passed {
                if let Some(previous) = runtime.pass_by_previous_position {
                    if crossed_target(previous, current, trigger) {
                        runtime.pass_by_target_passed = true;
                        emitted.push(trigger.trigger_event.id);
                    }
                }
            }
            runtime.pass_by_previous_position = Some(current);
        }
        for event in emitted {
            self.raise(event);
        }
    }

    /// Deliver queued contacts after modifier activation has settled for this
    /// frame. Havok's throw flag and rigid-body list are runtime-only fields,
    /// so both contact kinds use the serialized contact event.
    fn update_ragdoll_contact_modifiers(&mut self) {
        let contacts = std::mem::take(&mut self.ragdoll_contacts);
        for contact in contacts {
            match contact.kind {
                RagdollContactKind::Contact | RagdollContactKind::Throw => {}
            }
            let mut emitted = Vec::new();
            for node in self.modifier_order.clone() {
                if !self.nodes[node.0].active {
                    continue;
                }
                let NodeKind::RagdollContactListener {
                    contact_event,
                    bones,
                    ..
                } = &self.graph.node(node).kind
                else {
                    continue;
                };
                let matches_body = bones.iter().any(|bone| {
                    usize::try_from(*bone)
                        .ok()
                        .and_then(|index| contact.bone_to_body.get(index))
                        .is_some_and(|mapped| *mapped == Some(contact.body))
                });
                if matches_body {
                    emitted.push(contact_event.id);
                }
            }
            for event in emitted {
                self.raise(event);
            }
        }
    }

    /// Publish the value mapped to each active manager's current state.
    fn update_i_state_managers(&mut self) {
        for node in self.modifier_order.clone() {
            if !self.nodes[node.0].active {
                continue;
            }
            let NodeKind::IStateManager {
                state_variable,
                state_data,
                ..
            } = &self.graph.node(node).kind
            else {
                continue;
            };
            let variable = usize::try_from(*state_variable).ok();
            let Some(variable) = variable.filter(|index| *index < self.variables.len()) else {
                continue;
            };
            let value = state_data.iter().find_map(|entry| {
                (self.nodes.get(entry.state_machine.0)?.current_state == Some(entry.state_id))
                    .then_some(entry.state_to_set_as)
            });
            if let Some(value) = value {
                self.variables.set_word(variable, value);
            }
        }
    }

    /// Advance active interpolation modifiers by their authored rate.
    fn update_interp_value_modifiers(&mut self, dt: f32) {
        for node in &self.modifier_order {
            let NodeKind::InterpValue {
                source: authored_source,
                target: authored_target,
                gain,
                ..
            } = &self.graph.node(*node).kind
            else {
                continue;
            };
            let runtime = &mut self.nodes[node.0];
            if !runtime.active {
                continue;
            }
            let source = runtime
                .interp_source
                .or(runtime.interp_result)
                .unwrap_or(*authored_source);
            let target = runtime.interp_target.unwrap_or(*authored_target);
            let distance = target - source;
            let step = gain.max(0.0) * dt.max(0.0);
            runtime.interp_result = Some(if distance > 0.0 {
                (source + step).min(target)
            } else if distance < 0.0 {
                (source - step).max(target)
            } else {
                target
            });
        }
    }

    /// Write every output binding's node state back into its variable.
    ///
    /// Runs after the update, so a variable reports the frame that just ran.
    /// Character properties are read-only to the graph, so an output binding
    /// on one is skipped.
    fn apply_output_bindings(&mut self) {
        for node in self.modifier_order.clone() {
            for i in 0..self.bindings.len() {
                let b = self.bindings[i];
                if b.node == node {
                    self.apply_output_binding(b);
                }
            }
        }
        for i in 0..self.bindings.len() {
            let b = self.bindings[i];
            if !self.modifier_order.contains(&b.node) {
                self.apply_output_binding(b);
            }
        }
    }

    fn apply_output_binding(&mut self, b: ResolvedBinding) {
        if !b.target.is_output() || b.property {
            return;
        }
        match b.target {
            Target::IsActive { invert, .. } => {
                let Some(runtime) = self.nodes.get(b.node.0) else {
                    return;
                };
                self.variables
                    .set_word(b.variable, i32::from(runtime.active != invert));
            }
            Target::MachineIsActive => {
                let Some(runtime) = self.nodes.get(b.node.0) else {
                    return;
                };
                self.variables
                    .set_word(b.variable, i32::from(runtime.active));
            }
            Target::DecomposeComponent(component) => {
                let Some(runtime) = self.nodes.get(b.node.0) else {
                    return;
                };
                if !runtime.active {
                    return;
                }
                let NodeKind::DecomposeVector { vector, .. } = &self.graph.node(b.node).kind else {
                    return;
                };
                self.variables
                    .set_real(b.variable, runtime.vector.unwrap_or(*vector)[component]);
            }
            Target::TimeStep => {
                let Some(runtime) = self.nodes.get(b.node.0) else {
                    return;
                };
                self.variables.set_real(b.variable, runtime.time_step);
            }
            Target::SpeedSamplerSpeedOut => {
                let Some(runtime) = self.nodes.get(b.node.0) else {
                    return;
                };
                if runtime.active {
                    self.variables
                        .set_real(b.variable, runtime.speed_sampler_speed_out);
                }
            }
            Target::InterpOutput => {
                if let Some(result) = self
                    .nodes
                    .get(b.node.0)
                    .and_then(|runtime| runtime.interp_result)
                {
                    self.variables.set_real(b.variable, result);
                }
            }
            Target::DampingValue => {
                if let Some(damping) = self
                    .nodes
                    .get(b.node.0)
                    .and_then(|node| node.damping.as_ref())
                {
                    self.variables.set_real(b.variable, damping.damped_value);
                }
            }
            Target::DampingVector => {
                if let Some(damping) = self
                    .nodes
                    .get(b.node.0)
                    .and_then(|node| node.damping.as_ref())
                {
                    self.variables.set_quad(b.variable, damping.damped_vector);
                }
            }
            Target::TransformVectorOutput => {
                let Some(runtime) = self.nodes.get(b.node.0) else {
                    return;
                };
                if !runtime.active {
                    return;
                }
                let value =
                    runtime
                        .transform_vector_out
                        .or_else(|| match &self.graph.node(b.node).kind {
                            NodeKind::TransformVector { vector_out, .. } => Some(*vector_out),
                            _ => None,
                        });
                if let Some(value) = value {
                    self.variables.set_quad(b.variable, value);
                }
            }
            _ => {}
        }
    }

    fn apply_bindings(&mut self) {
        for b in &self.bindings {
            if b.target.is_output() {
                continue;
            }
            let table = if b.property {
                &self.character_properties
            } else {
                &self.variables
            };
            if let Target::ChildBoneWeights(child) = b.target {
                if let Some(pointer) = table.pointer(b.variable) {
                    if let Some(pointers) = self.nodes[b.node.0]
                        .child_bone_weights_pointers
                        .get_mut(child)
                    {
                        *pointers = match pointer {
                            Some(pointer) => BoundMaskPointer::Pointer(pointer),
                            None => BoundMaskPointer::Null,
                        };
                    }
                }
                continue;
            }
            let rt = &mut self.nodes[b.node.0];
            if b.target == Target::DampingRawVector {
                if let Some(vector) = table.quad(b.variable) {
                    if let Some(damping) = rt.damping.as_mut() {
                        damping.raw_vector = vector;
                    }
                }
                continue;
            }
            let Some(value) = table.real(b.variable) else {
                continue;
            };
            match b.target {
                Target::PlaybackSpeed => rt.playback_speed = Some(value),
                Target::UserControlledTimeFraction => rt.user_fraction = Some(value),
                Target::BlendParameter => rt.blend_parameter = Some(value),
                Target::OffsetVariable => rt.offset_variable = Some(value),
                Target::ChildWeight(i) => {
                    if let Some(w) = rt.child_weights.get_mut(i) {
                        *w = Some(value);
                    }
                }
                Target::SelectedGeneratorIndex => {
                    rt.selected = usize::try_from(value as i32).ok();
                }
                Target::Enable => rt.enabled = Some(value != 0.0),
                Target::InterpFrom => rt.interp_source = Some(value),
                Target::InterpTo => rt.interp_target = Some(value),
                Target::FalseToTrueEnable(slot) => rt.edge_enabled[slot] = Some(value != 0.0),
                Target::FalseToTrueValue(slot) => rt.edge_value[slot] = Some(value != 0.0),
                Target::StartStateId => rt.start_state = Some(value as i32),
                Target::Duration => rt.duration = Some(value),
                Target::EventRangeInput => rt.event_range_input = Some(value),
                Target::TwistAngle => rt.twist_angle = Some(value),
                Target::DecomposeVector => rt.vector = table.quad(b.variable),
                Target::SpeedSamplerGoalSpeed => rt.speed_sampler_goal_speed = Some(value),
                // Output targets are skipped above. State and direction are
                // recognized inputs but do not affect the sampled speed.
                Target::DampingKp => {
                    if let Some(state) = rt.damping.as_mut() {
                        state.kp = value;
                    }
                }
                Target::DampingKi => {
                    if let Some(state) = rt.damping.as_mut() {
                        state.ki = value;
                    }
                }
                Target::DampingKd => {
                    if let Some(state) = rt.damping.as_mut() {
                        state.kd = value;
                    }
                }
                Target::DampingEnableScalar => {
                    if let Some(state) = rt.damping.as_mut() {
                        state.enable_scalar = value != 0.0;
                    }
                }
                Target::DampingEnableVector => {
                    if let Some(state) = rt.damping.as_mut() {
                        state.enable_vector = value != 0.0;
                    }
                }
                Target::DampingRawValue => {
                    if let Some(state) = rt.damping.as_mut() {
                        state.raw_value = value;
                    }
                }
                Target::TransformVectorInput => rt.transform_vector_in = table.quad(b.variable),
                Target::TransformRotation => rt.transform_rotation = table.quad(b.variable),
                Target::TransformTranslation => {
                    rt.transform_translation = table.quad(b.variable);
                }
                // Written by apply_output_bindings, skipped above.
                Target::IsActive { .. }
                | Target::MachineIsActive
                | Target::DecomposeComponent(_)
                | Target::TimeStep
                | Target::SpeedSamplerState
                | Target::SpeedSamplerDirection
                | Target::SpeedSamplerSpeedOut
                | Target::DampingRawVector
                | Target::DampingValue
                | Target::DampingVector
                | Target::InterpOutput
                | Target::ChildBoneWeights(_)
                | Target::TransformVectorOutput => {}
            }
        }
    }

    /// Whether a modifier's authored `enable` value, overridden by a binding
    /// when present, allows it to run.
    pub fn node_enabled(&self, id: NodeId) -> bool {
        let Some(node) = self.graph.nodes.get(id.0) else {
            return false;
        };
        let authored = match &node.kind {
            NodeKind::ModifierList { enable, .. }
            | NodeKind::EventDrivenModifier { enable, .. }
            | NodeKind::TimerModifier { enable, .. }
            | NodeKind::EventsFromRangeModifier { enable, .. }
            | NodeKind::BsTimerModifier { enable, .. }
            | NodeKind::Tweener { enable, .. }
            | NodeKind::EventOnDeactivateModifier { enable, .. }
            | NodeKind::GetTimeStepModifier { enable, .. }
            | NodeKind::InterpValue { enable, .. }
            | NodeKind::IsActiveModifier { enable, .. }
            | NodeKind::FalseToTrueModifier { enable, .. }
            | NodeKind::ModifyOnce { enable, .. }
            | NodeKind::PassByTargetTrigger { enable, .. }
            | NodeKind::RagdollContactListener { enable, .. }
            | NodeKind::DecomposeVector { enable, .. }
            | NodeKind::EvaluateExpression { enable, .. }
            | NodeKind::RotateCharacterModifier { enable, .. }
            | NodeKind::SpeedSampler { enable, .. }
            | NodeKind::DampingModifier { enable, .. }
            | NodeKind::FootIkControls { enable, .. }
            | NodeKind::EveryNEventsModifier { enable, .. }
            | NodeKind::LookAtModifier { enable, .. }
            | NodeKind::DirectAt { enable, .. }
            | NodeKind::IStateManager { enable, .. }
            | NodeKind::LimbIk { enable, .. }
            | NodeKind::FootIk { enable, .. }
            | NodeKind::GetUpModifier { enable, .. }
            | NodeKind::KeyframeBonesModifier { enable, .. }
            | NodeKind::PoweredRagdollControls { enable, .. }
            | NodeKind::RigidBodyRagdollControlsModifier { enable, .. }
            | NodeKind::TransformVector { enable, .. }
            | NodeKind::TwistModifier(crate::behavior::TwistModifier { enable, .. })
            | NodeKind::Modifier { enable } => *enable,
            _ => true,
        };
        self.nodes
            .get(id.0)
            .and_then(|node| node.enabled)
            .unwrap_or(authored)
    }

    /// Apply a modifier subtree in serialized order without following a
    /// malformed list cycle or an unbounded list chain.
    fn sync_modifier(&mut self, id: NodeId) {
        let mut path = vec![false; self.nodes.len()];
        let mut stack = vec![(id, 0, false, true)];
        while let Some((node_id, depth, leaving, parent_enabled)) = stack.pop() {
            if node_id.0 >= self.nodes.len() || node_id.0 >= self.graph.nodes.len() {
                continue;
            }
            if leaving {
                path[node_id.0] = false;
                continue;
            }
            let node = &self.graph.nodes[node_id.0];
            if path[node_id.0] {
                push_unique(
                    &mut self.unsupported.modifier_list_cycles,
                    format!("{} ({})", node.name, node.class),
                );
                self.deactivate(id);
                return;
            }
            if depth >= MAX_MODIFIER_LIST_DEPTH {
                push_unique(
                    &mut self.unsupported.modifier_list_depth_limit,
                    format!("{} ({})", node.name, node.class),
                );
                self.deactivate(id);
                return;
            }
            let enabled = parent_enabled && self.node_enabled(node_id);
            if enabled {
                self.activate(node_id);
            } else {
                self.deactivate(node_id);
            }
            self.modifier_order.push(node_id);
            let modifier_children = match &node.kind {
                NodeKind::ModifierList { modifiers, .. } => modifiers
                    .iter()
                    .flatten()
                    .copied()
                    .map(|child| (child, enabled))
                    .collect::<Vec<_>>(),
                NodeKind::EventDrivenModifier { modifier, .. } => modifier
                    .iter()
                    .copied()
                    .map(|child| {
                        (
                            child,
                            enabled && self.nodes[node_id.0].event_modifier_active,
                        )
                    })
                    .collect(),
                _ => Vec::new(),
            };
            if !modifier_children.is_empty() {
                path[node_id.0] = true;
                stack.push((node_id, depth, true, enabled));
                for (child, child_enabled) in modifier_children.into_iter().rev() {
                    stack.push((child, depth + 1, false, child_enabled));
                }
            }
        }
    }

    /// Evaluate edge slots only during update, after list enable state is known.
    fn update_modifier(&mut self, id: NodeId, dt: f32) {
        if !self.nodes[id.0].active {
            return;
        }
        if let NodeKind::BsTimerModifier {
            alarm_time_seconds,
            alarm_event,
            reset_alarm,
            ..
        } = &self.graph.nodes[id.0].kind
        {
            if self.nodes[id.0].timer_frame != self.frame {
                let alarm_time_seconds = *alarm_time_seconds;
                let alarm_event_id = alarm_event.id;
                let reset_alarm = *reset_alarm;
                let runtime = &mut self.nodes[id.0];
                runtime.timer_frame = self.frame;
                let mut fire = false;
                if !runtime.timer_fired {
                    if dt.is_finite() && dt > 0.0 {
                        runtime.timer_elapsed += dt;
                    }
                    if runtime.timer_elapsed >= alarm_time_seconds {
                        fire = true;
                        if reset_alarm {
                            runtime.timer_elapsed = 0.0;
                        } else {
                            runtime.timer_fired = true;
                        }
                    }
                }
                if fire {
                    self.raise(alarm_event_id);
                }
            }
        }
        match &self.graph.nodes[id.0].kind {
            NodeKind::RotateCharacterModifier {
                degrees_per_second,
                speed_multiplier,
                ..
            } => {
                self.nodes[id.0].rotation_angle +=
                    degrees_per_second.to_radians() * speed_multiplier * dt;
            }
            NodeKind::FalseToTrueModifier { slots, .. } => {
                for (index, slot) in slots.iter().enumerate() {
                    let runtime = &mut self.nodes[id.0];
                    let active = runtime.edge_enabled[index].unwrap_or(slot.enable)
                        && runtime.edge_value[index].unwrap_or(slot.variable_to_test);
                    let rising = active && !runtime.edge_previous[index];
                    runtime.edge_previous[index] = active;
                    if rising {
                        self.raise(slot.event.id);
                    }
                }
            }
            NodeKind::EventsFromRangeModifier {
                input_value,
                lower_bound,
                ranges,
                ..
            } => {
                let value = self.nodes[id.0].event_range_input.unwrap_or(*input_value);
                let mut lower = *lower_bound;
                let mut raised = Vec::new();
                for (index, (upper, range)) in ranges.iter().enumerate() {
                    let active = value.is_finite() && value >= lower && value < *upper;
                    let previous = self.nodes[id.0]
                        .event_range_previous
                        .get(index)
                        .copied()
                        .unwrap_or(false);
                    if active && (range.event_mode == 1 || !previous) {
                        raised.push(range.event.id);
                    }
                    if let Some(previous) = self.nodes[id.0].event_range_previous.get_mut(index) {
                        *previous = active;
                    }
                    lower = *upper;
                }
                for event in raised {
                    self.raise(event);
                }
            }
            NodeKind::EvaluateExpression { .. } => self.evaluate_expressions(id),
            NodeKind::SpeedSampler { goal_speed, .. } => {
                let runtime = &mut self.nodes[id.0];
                runtime.speed_sampler_speed_out =
                    runtime.speed_sampler_goal_speed.unwrap_or(*goal_speed);
            }
            NodeKind::TransformVector {
                rotation,
                translation,
                vector_in,
                rotate_only,
                inverse,
                compute_on_activate,
                compute_on_modify,
                ..
            } => {
                let runtime = &mut self.nodes[id.0];
                let should_compute =
                    *compute_on_modify || (runtime.activated_this_frame && *compute_on_activate);
                runtime.activated_this_frame = false;
                if should_compute {
                    let input = runtime.transform_vector_in.unwrap_or(*vector_in);
                    let rotation = runtime.transform_rotation.unwrap_or(*rotation);
                    let translation = runtime.transform_translation.unwrap_or(*translation);
                    runtime.transform_vector_out = Some(transform_vector(
                        input,
                        rotation,
                        translation,
                        *rotate_only,
                        *inverse,
                    ));
                }
            }
            NodeKind::DampingModifier { .. } => {
                let runtime = &mut self.nodes[id.0];
                if let Some(state) = runtime.damping.as_mut() {
                    let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
                    if state.enable_scalar && dt > 0.0 {
                        let error = state.raw_value - state.damped_value;
                        state.error_sum += error * dt;
                        let derivative = (error - state.previous_error) / dt;
                        state.damped_value +=
                            (state.kp * error + state.ki * state.error_sum + state.kd * derivative)
                                * dt;
                        state.previous_error = error;
                    }
                    if state.enable_vector && dt > 0.0 {
                        for axis in 0..4 {
                            let error = state.raw_vector[axis] - state.damped_vector[axis];
                            state.vector_error_sum[axis] += error * dt;
                            let derivative = (error - state.vector_previous_error[axis]) / dt;
                            state.damped_vector[axis] += (state.kp * error
                                + state.ki * state.vector_error_sum[axis]
                                + state.kd * derivative)
                                * dt;
                            state.vector_previous_error[axis] = error;
                        }
                    }
                }
            }
            NodeKind::TimerModifier {
                alarm_time_seconds,
                alarm_event,
                ..
            } => {
                let runtime = &mut self.nodes[id.0];
                if !runtime.timer_fired && runtime.timer_updated_frame != self.frame {
                    runtime.timer_updated_frame = self.frame;
                    runtime.timer_elapsed += dt.max(0.0);
                    if runtime.timer_elapsed >= *alarm_time_seconds {
                        runtime.timer_fired = true;
                        self.raise(alarm_event.id);
                    }
                }
            }
            _ => {}
        }
    }

    fn evaluate_expressions(&mut self, id: NodeId) {
        let NodeKind::EvaluateExpression { expressions, .. } = &self.graph.nodes[id.0].kind else {
            return;
        };
        let expressions = expressions.clone();
        let compiled = self.expressions[id.0].clone().unwrap_or_default();
        for (index, (data, expression)) in expressions.iter().zip(compiled).enumerate() {
            let Ok(expression) = expression else {
                let error = expression.err().unwrap_or(ExpressionError::InvalidValue);
                push_unique(
                    &mut self.unsupported.expression_modifiers,
                    format!(
                        "{}.{}: {error:?}",
                        self.graph.nodes[id.0].name, data.expression
                    ),
                );
                continue;
            };
            let value = match expression.evaluate(
                &self.variables,
                &self.character_properties,
                &mut self.expression_random_state,
            ) {
                Ok(value) => value,
                Err(error) => {
                    push_unique(
                        &mut self.unsupported.expression_modifiers,
                        format!(
                            "{}.{}: {error:?}",
                            self.graph.nodes[id.0].name, data.expression
                        ),
                    );
                    continue;
                }
            };
            if data.assignment_variable_index >= 0 {
                let assignment = usize::try_from(data.assignment_variable_index).ok();
                let supported = assignment.is_some_and(|target| {
                    matches!(
                        self.variables.ty(target),
                        Some(
                            VariableType::Bool
                                | VariableType::Int8
                                | VariableType::Int16
                                | VariableType::Int32
                                | VariableType::Real
                        )
                    )
                });
                if let Some(target) = assignment.filter(|_| supported) {
                    self.variables.set_real(target, value);
                } else {
                    push_unique(
                        &mut self.unsupported.expression_modifiers,
                        format!(
                            "{}.{}: invalid assignment target {}",
                            self.graph.nodes[id.0].name,
                            data.expression,
                            data.assignment_variable_index
                        ),
                    );
                }
            }
            if !matches!(data.event_mode, 0..=3) {
                push_unique(
                    &mut self.unsupported.expression_modifiers,
                    format!(
                        "{}.{}: invalid event mode {}",
                        self.graph.nodes[id.0].name, data.expression, data.event_mode
                    ),
                );
                continue;
            }
            let truth = value != 0.0;
            let previous = self.nodes[id.0].expression_previous[index];
            let raised = self.nodes[id.0].expression_raised[index];
            let send = match data.event_mode {
                0 => truth && !raised,
                1 => truth,
                2 => truth && !previous,
                3 => truth || raised,
                _ => false,
            };
            self.nodes[id.0].expression_previous[index] = truth;
            if send {
                self.nodes[id.0].expression_raised[index] = true;
                if let Some(event) = usize::try_from(data.assignment_event_index)
                    .ok()
                    .and_then(|event| self.graph.data.event_names.get(event))
                    .and_then(|name| self.graph.data.event_id(name))
                {
                    self.raise(event);
                } else if data.assignment_event_index >= 0 {
                    push_unique(
                        &mut self.unsupported.expression_modifiers,
                        format!(
                            "{}.{}: invalid event index {}",
                            self.graph.nodes[id.0].name,
                            data.expression,
                            data.assignment_event_index
                        ),
                    );
                }
            }
        }
    }

    fn apply_modifier_output(&self, id: NodeId, output: &mut Output) {
        let NodeKind::RotateCharacterModifier {
            axis_of_rotation, ..
        } = &self.graph.node(id).kind
        else {
            return;
        };
        let Some(root) = output.pose.first_mut() else {
            return;
        };
        let axis_length = axis_of_rotation[..3]
            .iter()
            .map(|component| component * component)
            .sum::<f32>()
            .sqrt();
        if axis_length <= f32::EPSILON {
            return;
        }
        let half_angle = self.nodes[id.0].rotation_angle * 0.5;
        let (sin, cos) = half_angle.sin_cos();
        let rotation = [
            axis_of_rotation[0] / axis_length * sin,
            axis_of_rotation[1] / axis_length * sin,
            axis_of_rotation[2] / axis_length * sin,
            cos,
        ];
        root.rotation = quat_mul(rotation, root.rotation);
    }

    fn apply_tweener(&mut self, id: NodeId, output: &mut Output, dt: f32) {
        let NodeKind::Tweener {
            tween_position,
            tween_rotation,
            use_tween_duration,
            tween_duration,
            target_position,
            target_rotation,
            ..
        } = &self.graph.node(id).kind
        else {
            return;
        };
        if !self.nodes[id.0].active {
            return;
        }
        let start = *self.nodes[id.0]
            .tween_start
            .get_or_insert(output.world_from_model);
        self.nodes[id.0].tween_elapsed += dt.max(0.0);
        let duration = if *use_tween_duration {
            (*tween_duration).max(0.0)
        } else {
            0.0
        };
        let alpha = if duration > 0.0 {
            (self.nodes[id.0].tween_elapsed / duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let mut target = start;
        if *tween_position {
            target.translation = *target_position;
        }
        if *tween_rotation {
            target.rotation = normalized_quaternion(*target_rotation);
        }
        output.world_from_model = blend_transform(&start, &target, alpha);
    }

    /// Evaluate edge slots only during update, after list enable state is known.
    fn apply_direct_at(&mut self, id: NodeId, output: &mut Output, dt: f32) {
        let NodeKind::DirectAt {
            direct_at_target,
            source_bone_index,
            start_bone_index,
            end_bone_index,
            limit_heading_degrees,
            limit_pitch_degrees,
            offset_heading_degrees,
            offset_pitch_degrees,
            on_gain,
            off_gain,
            target_location,
            direct_at_camera,
            direct_at_camera_location,
            current_heading_offset,
            current_pitch_offset,
            ..
        } = &self.graph.node(id).kind
        else {
            return;
        };
        if !self.nodes[id.0].active || output.weight <= 0.0 {
            return;
        }
        if !self.nodes[id.0].direct_at_initialized {
            self.nodes[id.0].direct_at_heading = *current_heading_offset;
            self.nodes[id.0].direct_at_pitch = *current_pitch_offset;
            self.nodes[id.0].direct_at_initialized = true;
        }
        let (heading, pitch, gain) = if *direct_at_target || *direct_at_camera {
            let Ok(source) = usize::try_from(*source_bone_index) else {
                return;
            };
            let Some(source) = output.pose.get(source) else {
                return;
            };
            let target = if *direct_at_camera {
                *direct_at_camera_location
            } else {
                [target_location[0], target_location[1], target_location[2]]
            };
            let delta = [
                target[0] - source.translation[0],
                target[1] - source.translation[1],
                target[2] - source.translation[2],
            ];
            if !delta.iter().all(|v| v.is_finite()) {
                return;
            }
            let horizontal = delta[0].hypot(delta[1]);
            let mut heading = delta[0].atan2(delta[1]).to_degrees() + *offset_heading_degrees;
            let mut pitch = delta[2].atan2(horizontal).to_degrees() + *offset_pitch_degrees;
            if !heading.is_finite()
                || !pitch.is_finite()
                || !limit_heading_degrees.is_finite()
                || !limit_pitch_degrees.is_finite()
            {
                return;
            }
            heading = heading.clamp(-limit_heading_degrees.abs(), limit_heading_degrees.abs());
            pitch = pitch.clamp(-limit_pitch_degrees.abs(), limit_pitch_degrees.abs());
            (heading, pitch, *on_gain)
        } else {
            (0.0, 0.0, *off_gain)
        };
        if !gain.is_finite() || !dt.is_finite() || dt < 0.0 {
            return;
        }
        let fraction = (gain.max(0.0) * dt).clamp(0.0, 1.0);
        let runtime = &mut self.nodes[id.0];
        runtime.direct_at_heading += (heading - runtime.direct_at_heading) * fraction;
        runtime.direct_at_pitch += (pitch - runtime.direct_at_pitch) * fraction;

        let (Ok(start), Ok(end)) = (
            usize::try_from(*start_bone_index),
            usize::try_from(*end_bone_index),
        ) else {
            return;
        };
        if start > end || end >= output.pose.len() {
            return;
        }
        let count = (end - start + 1) as f32;
        let yaw = runtime.direct_at_heading.to_radians() / count;
        let elevation = runtime.direct_at_pitch.to_radians() / count;
        let delta_rotation = quat_mul(
            [0.0, 0.0, (yaw * 0.5).sin(), (yaw * 0.5).cos()],
            [(elevation * 0.5).sin(), 0.0, 0.0, (elevation * 0.5).cos()],
        );
        for bone in &mut output.pose[start..=end] {
            bone.rotation = quat_mul(delta_rotation, bone.rotation);
        }
    }

    fn activate(&mut self, id: NodeId) {
        if self.nodes[id.0].active {
            return;
        }
        self.nodes[id.0].active = true;
        self.nodes[id.0].activated_this_frame = true;
        let graph = self.graph;
        match &graph.node(id).kind {
            NodeKind::EveryNEventsModifier {
                number_of_events_before_send,
                minimum_number_of_events_before_send,
                randomize_number_of_events,
                ..
            } => {
                let runtime = &mut self.nodes[id.0];
                runtime.every_n_seen = 0;
                runtime.every_n_outputs = 0;
                runtime.every_n_interval = every_n_interval(
                    id,
                    0,
                    *number_of_events_before_send,
                    *minimum_number_of_events_before_send,
                    *randomize_number_of_events,
                );
            }

            NodeKind::GetUpModifier { .. } => self.nodes[id.0].get_up_elapsed = 0.0,

            NodeKind::FalseToTrueModifier { .. } => {
                self.nodes[id.0].edge_previous = [false; FALSE_TO_TRUE_SLOTS];
            }
            NodeKind::EvaluateExpression { expressions, .. } => {
                self.nodes[id.0].expression_raised = vec![false; expressions.len()];
                self.nodes[id.0].expression_previous = vec![false; expressions.len()];
            }
            NodeKind::RotateCharacterModifier { .. } => {
                self.nodes[id.0].rotation_angle = 0.0;
            }
            NodeKind::TimerModifier { .. } => {
                self.nodes[id.0].timer_elapsed = 0.0;
                self.nodes[id.0].timer_fired = false;
                self.nodes[id.0].timer_updated_frame = 0;
            }
            NodeKind::BsTimerModifier { .. } => {
                let runtime = &mut self.nodes[id.0];
                runtime.timer_elapsed = 0.0;
                runtime.timer_fired = false;
                runtime.timer_frame = 0;
            }
            NodeKind::Tweener { .. } => {
                self.nodes[id.0].tween_start = None;
                self.nodes[id.0].tween_elapsed = 0.0;
            }
            NodeKind::Clip(c) => {
                let rt = &mut self.nodes[id.0];
                rt.local_time = c.start_time.max(0.0);
                rt.at_end = false;
                rt.passes = 0;
            }
            NodeKind::StateMachine(sm) => {
                let start = self.start_state(id, sm);
                self.nodes[id.0].current_state = None;
                self.nodes[id.0].previous_state = None;
                self.nodes[id.0].transitions.clear();
                if let Some(start) = start {
                    self.enter_state(id, sm, start, None);
                }
            }
            NodeKind::EventDrivenModifier {
                active_by_default, ..
            } => {
                self.nodes[id.0].event_modifier_active = *active_by_default;
            }
            NodeKind::Blender(b) => {
                let keep_zero = b.flags & 8 != 0;
                for (i, child) in b.children.iter().enumerate() {
                    let weight = self.nodes[id.0].child_weights[i].unwrap_or(child.weight);
                    if let Some(g) = child.generator {
                        if weight > 0.0 || keep_zero {
                            self.activate(g);
                        }
                    }
                }
            }
            NodeKind::ManualSelector {
                generators,
                selected_generator_index,
                ..
            } => {
                let index = self.nodes[id.0]
                    .selected
                    .unwrap_or(usize::try_from(*selected_generator_index).unwrap_or(0));
                self.nodes[id.0].selected = Some(index);
                if let Some(Some(g)) = generators.get(index) {
                    self.activate(*g);
                }
            }
            NodeKind::ModifierGenerator {
                modifier,
                generator,
            } => {
                if let Some(modifier) = modifier {
                    self.sync_modifier(*modifier);
                }
                if let Some(generator) = generator {
                    self.activate(*generator);
                }
            }
            NodeKind::ModifyOnce { on_activate, .. } => {
                if let Some(child) = on_activate {
                    self.sync_modifier(*child);
                }
            }
            NodeKind::IStateTagging {
                default_generator: generator,
                ..
            }
            | NodeKind::SynchronizedClip {
                clip_generator: generator,
                ..
            }
            | NodeKind::CyclicBlendTransition {
                blender_generator: generator,
                ..
            } => {
                if let Some(g) = generator {
                    self.activate(*g);
                }
            }
            NodeKind::OffsetAnimation {
                default_generator,
                offset_clip_generator,
                ..
            } => {
                if let Some(generator) = default_generator {
                    self.activate(*generator);
                }
                if let Some(generator) = offset_clip_generator {
                    self.activate(*generator);
                }
            }
            NodeKind::BoneSwitch {
                default_generator,
                children,
            } => {
                if let Some(generator) = default_generator {
                    self.activate(*generator);
                }
                for generator in children.iter().filter_map(|child| child.generator) {
                    self.activate(generator);
                }
            }
            NodeKind::BehaviorReference { resolved, .. } => {
                if let Some(g) = resolved {
                    self.activate(*g);
                }
            }
            NodeKind::ModifierList { .. }
            | NodeKind::BlendingTransition(_)
            | NodeKind::GeneratorTransition(_)
            | NodeKind::EventsFromRangeModifier { .. }
            | NodeKind::EventOnDeactivateModifier { .. }
            | NodeKind::GetTimeStepModifier { .. }
            | NodeKind::DirectAt { .. }
            | NodeKind::InterpValue { .. }
            | NodeKind::IsActiveModifier { .. }
            | NodeKind::PassByTargetTrigger { .. }
            | NodeKind::RagdollContactListener { .. }
            | NodeKind::TwistModifier(_)
            | NodeKind::DecomposeVector { .. }
            | NodeKind::SpeedSampler { .. }
            | NodeKind::DampingModifier { .. }
            | NodeKind::FootIkControls { .. }
            | NodeKind::LookAtModifier { .. }
            | NodeKind::IStateManager { .. }
            | NodeKind::LimbIk { .. }
            | NodeKind::FootIk { .. }
            | NodeKind::KeyframeBonesModifier { .. }
            | NodeKind::PoweredRagdollControls { .. }
            | NodeKind::RigidBodyRagdollControlsModifier { .. }
            | NodeKind::TransformVector { .. }
            | NodeKind::Modifier { .. }
            | NodeKind::Unknown => {}
        }
    }

    fn deactivate(&mut self, id: NodeId) {
        let mut seen = vec![false; self.nodes.len()];
        let mut stack = vec![id];
        while let Some(node_id) = stack.pop() {
            if node_id.0 >= self.nodes.len() || node_id.0 >= self.graph.nodes.len() {
                continue;
            }
            if seen[node_id.0] || !self.nodes[node_id.0].active {
                continue;
            }
            seen[node_id.0] = true;
            let node = &self.graph.nodes[node_id.0];
            let mut children = node.children();
            if let NodeKind::ModifyOnce {
                on_deactivate: Some(child),
                ..
            } = &node.kind
            {
                let start = self.modifier_order.len();
                self.sync_modifier(*child);
                for index in start..self.modifier_order.len() {
                    let child_node = self.modifier_order[index];
                    self.update_modifier(child_node, 0.0);
                }
            }
            if let NodeKind::EventOnDeactivateModifier { event, .. } = &node.kind {
                self.raise(event.id);
            }
            if let NodeKind::StateMachine(sm) = &node.kind {
                if let Some(current) = self.nodes[node_id.0].current_state {
                    if let Some(state) = sm.state(current) {
                        for e in &state.exit_notify_events {
                            self.raise(e.id);
                        }
                    }
                }
                let runtime = &mut self.nodes[node_id.0];
                children.extend(runtime.transitions.iter().filter_map(|t| t.from_generator));
                runtime.transitions.clear();
                runtime.delayed = None;
                runtime.intervals.clear();
                runtime.current_state = None;
            }
            self.nodes[node_id.0].active = false;
            self.nodes[node_id.0].time_step = 0.0;
            if matches!(node.kind, NodeKind::GetUpModifier { .. }) {
                self.nodes[node_id.0].get_up_elapsed = 0.0;
            }
            stack.extend(children);
        }
    }

    fn raise(&mut self, id: i32) {
        if id >= 0 {
            self.raised.push(Event { id });
        }
    }

    /// Mode 0 uses the start state, 1 reads the sync variable, 2 picks the
    /// most probable enabled state (a deterministic stand-in for random), 3
    /// has a chooser this crate does not decode and falls back to 0.
    fn start_state(&self, id: NodeId, sm: &StateMachine) -> Option<i32> {
        let by_id = |state: i32| sm.state(state).filter(|s| s.enable).map(|s| s.state_id);
        let first_enabled = || sm.states.iter().find(|s| s.enable).map(|s| s.state_id);
        // A bound startStateId wins over the stored one in every mode, since
        // that is the point of binding it.
        if let Some(state) = self.nodes[id.0].start_state.and_then(by_id) {
            return Some(state);
        }
        match sm.start_state_mode {
            1 => usize::try_from(sm.sync_variable_index)
                .ok()
                .and_then(|i| self.variables.word(i))
                .and_then(by_id)
                .or_else(|| by_id(sm.start_state_id))
                .or_else(first_enabled),
            2 => sm
                .states
                .iter()
                .filter(|s| s.enable)
                .max_by(|a, b| a.probability.total_cmp(&b.probability))
                .map(|s| s.state_id)
                .or_else(|| by_id(sm.start_state_id)),
            _ => by_id(sm.start_state_id).or_else(first_enabled),
        }
    }

    fn enter_state(&mut self, id: NodeId, sm: &StateMachine, state: i32, event: Option<i32>) {
        let from = self.nodes[id.0].current_state;
        let Some(info) = sm.state(state) else {
            return;
        };
        self.nodes[id.0].previous_state = from;
        self.nodes[id.0].current_state = Some(state);
        self.nodes[id.0].time_in_state = 0.0;
        self.nodes[id.0].delayed = None;
        self.nodes[id.0].intervals = info
            .transitions
            .iter()
            .chain(&sm.wildcard_transitions)
            .map(|transition| IntervalState {
                trigger: transition.trigger_interval.enter_event_id < 0,
                initiate: transition.initiate_interval.enter_event_id < 0,
            })
            .collect();
        if let Some(g) = info.generator {
            self.activate(g);
        }
        for e in &info.enter_notify_events {
            self.raise(e.id);
        }
        self.raise(sm.event_to_send_when_state_or_transition_changes.id);
        self.changes.push(StateChange {
            machine: id,
            from,
            to: state,
            event,
        });
    }

    /// Deliver one event: a state machine takes it if a transition from
    /// its current state names it. Highest priority wins, with authored order
    /// breaking ties. Otherwise the event goes on to the active children.
    fn handle_event(&mut self, id: NodeId, ev: Event) -> bool {
        if !self.nodes[id.0].active {
            return false;
        }
        let graph = self.graph;
        let node = graph.node(id);
        match &node.kind {
            NodeKind::EveryNEventsModifier {
                event_to_check_for,
                event_to_send,
                number_of_events_before_send,
                minimum_number_of_events_before_send,
                randomize_number_of_events,
                ..
            } => {
                if ev.id == event_to_check_for.id && ev.id >= 0 {
                    let runtime = &mut self.nodes[id.0];
                    runtime.every_n_seen = runtime.every_n_seen.saturating_add(1);
                    if runtime.every_n_seen >= runtime.every_n_interval {
                        runtime.every_n_seen = 0;
                        runtime.every_n_outputs = runtime.every_n_outputs.saturating_add(1);
                        runtime.every_n_interval = every_n_interval(
                            id,
                            runtime.every_n_outputs,
                            *number_of_events_before_send,
                            *minimum_number_of_events_before_send,
                            *randomize_number_of_events,
                        );
                        self.raise(event_to_send.id);
                    }
                }
                false
            }
            NodeKind::StateMachine(sm) => {
                if self.machine_event(id, sm, ev) {
                    return true;
                }
                let generators = self.active_generators_of_machine(id, sm);
                let mut consumed = false;
                for g in generators {
                    consumed |= self.handle_event(g, ev);
                }
                consumed
            }
            NodeKind::EventDrivenModifier {
                modifier,
                activate_event_id,
                deactivate_event_id,
                ..
            } => {
                if ev.id == *activate_event_id {
                    self.nodes[id.0].event_modifier_active = true;
                } else if ev.id == *deactivate_event_id {
                    self.nodes[id.0].event_modifier_active = false;
                }
                let Some(child) = modifier else {
                    return ev.id == *activate_event_id || ev.id == *deactivate_event_id;
                };
                if self.nodes[id.0].event_modifier_active {
                    self.sync_modifier(*child);
                    if self.nodes[child.0].active {
                        self.handle_event(*child, ev)
                    } else {
                        false
                    }
                } else {
                    self.deactivate(*child);
                    false
                }
            }
            NodeKind::CyclicBlendTransition {
                blender_generator,
                event_to_freeze_blend_value,
                event_to_cross_blend,
                blend_parameter,
                transition_duration,
                blend_curve,
            } => {
                let child_parameter =
                    blender_generator.and_then(|blender| self.blender_parameter(blender));
                let target_parameter = self.nodes[id.0].blend_parameter.unwrap_or(*blend_parameter);
                if ev.id == event_to_freeze_blend_value.id && ev.id >= 0 {
                    let parameter = child_parameter.unwrap_or(target_parameter);
                    let runtime = &mut self.nodes[id.0].cyclic_blend;
                    runtime.frozen_parameter = Some(parameter);
                    runtime.transition = None;
                    if let Some(blender) = blender_generator {
                        self.nodes[blender.0].blend_parameter = Some(parameter);
                    }
                } else if ev.id == event_to_cross_blend.id && ev.id >= 0 {
                    let from = self.nodes[id.0]
                        .cyclic_blend
                        .frozen_parameter
                        .or(child_parameter)
                        .unwrap_or(target_parameter);
                    let runtime = &mut self.nodes[id.0].cyclic_blend;
                    runtime.frozen_parameter = None;
                    if *transition_duration > 0.0 {
                        runtime.transition = Some(CyclicBlendTransition {
                            from,
                            to: target_parameter,
                            elapsed: 0.0,
                            duration: *transition_duration,
                            curve: *blend_curve,
                        });
                    } else {
                        runtime.transition = None;
                    }
                    if let Some(blender) = blender_generator {
                        self.nodes[blender.0].blend_parameter =
                            Some(if *transition_duration > 0.0 {
                                from
                            } else {
                                target_parameter
                            });
                    }
                }
                blender_generator.is_some_and(|generator| self.handle_event(generator, ev))
            }
            NodeKind::Clip(_) => false,
            _ => {
                let mut consumed = false;
                for child in node.children() {
                    if self.nodes[child.0].active {
                        consumed |= self.handle_event(child, ev);
                    }
                }
                consumed
            }
        }
    }

    fn blender_parameter(&self, id: NodeId) -> Option<f32> {
        let NodeKind::Blender(blender) = &self.graph.node(id).kind else {
            return None;
        };
        Some(
            self.nodes[id.0]
                .blend_parameter
                .unwrap_or(blender.blend_parameter),
        )
    }

    fn update_cyclic_blend(&mut self, id: NodeId, blender: NodeId, dt: f32) {
        let runtime = &mut self.nodes[id.0].cyclic_blend;
        let parameter = if let Some(transition) = &mut runtime.transition {
            transition.elapsed = (transition.elapsed + dt).min(transition.duration);
            let alpha = blend_curve(transition.curve, transition.elapsed / transition.duration);
            let parameter = transition.from + (transition.to - transition.from) * alpha;
            if transition.elapsed >= transition.duration {
                runtime.transition = None;
            }
            parameter
        } else if let Some(parameter) = runtime.frozen_parameter {
            parameter
        } else {
            return;
        };
        self.nodes[blender.0].blend_parameter = Some(parameter);
    }

    fn active_generators_of_machine(&self, id: NodeId, sm: &StateMachine) -> Vec<NodeId> {
        let mut out = Vec::new();
        for transition in &self.nodes[id.0].transitions {
            for generator in [
                transition.from_generator,
                transition.to_generator,
                transition
                    .generator_transition
                    .map(|active| active.generator),
            ]
            .into_iter()
            .flatten()
            {
                if !out.contains(&generator) {
                    out.push(generator);
                }
            }
        }
        if let Some(current) = self.nodes[id.0].current_state {
            if let Some(g) = sm.state(current).and_then(|s| s.generator) {
                if !out.contains(&g) {
                    out.push(g);
                }
            }
        }
        out
    }

    fn machine_event(&mut self, id: NodeId, sm: &StateMachine, ev: Event) -> bool {
        let Some(current) = self.nodes[id.0].current_state else {
            return false;
        };
        let Some(state) = sm.state(current) else {
            return false;
        };
        if ev.id >= 0 {
            for (index, info) in state
                .transitions
                .iter()
                .chain(&sm.wildcard_transitions)
                .enumerate()
            {
                let interval = &mut self.nodes[id.0].intervals[index];
                update_interval_event(&info.trigger_interval, &mut interval.trigger, ev.id);
                update_interval_event(&info.initiate_interval, &mut interval.initiate, ev.id);
            }
        }
        if self.nodes[id.0].transitions.iter().any(|transition| {
            transition.flags & transition_flags::UNINTERRUPTIBLE_WHILE_PLAYING != 0
        }) {
            return false;
        }
        if self.nodes[id.0].delayed.as_ref().is_some_and(|delayed| {
            delayed.info.flags & transition_flags::UNINTERRUPTIBLE_WHILE_DELAYED != 0
        }) {
            return self.start_delayed(id, sm);
        }
        let return_event = sm.return_to_previous_state_event_id;
        let mut candidates: Vec<_> = state
            .transitions
            .iter()
            .chain(sm.wildcard_transitions.iter())
            .enumerate()
            .filter(|(index, info)| {
                Self::transition_matches(sm, current, info, ev)
                    && (info.flags & transition_flags::USE_TRIGGER_INTERVAL == 0
                        || interval_open(
                            &info.trigger_interval,
                            self.nodes[id.0].intervals[*index].trigger,
                            self.nodes[id.0].time_in_state,
                        ))
            })
            .collect();
        candidates.sort_by_key(|(_, transition)| std::cmp::Reverse(transition.priority));
        let candidate = candidates
            .into_iter()
            .find(|(_, transition)| self.condition_passes(transition))
            .map(|(index, info)| (index, info.clone()));
        let target = match candidate {
            Some(pair) => Some(pair),
            None if return_event >= 0 && ev.id == return_event => self.nodes[id.0]
                .previous_state
                .map(|p| (usize::MAX, TransitionInfo::plain(p))),
            None => None,
        };
        let Some((index, info)) = target else {
            return self.start_delayed(id, sm);
        };
        let runtime = &self.nodes[id.0];
        if runtime
            .transitions
            .last()
            .is_some_and(|active| info.priority < active.priority)
            || runtime
                .delayed
                .as_ref()
                .is_some_and(|delayed| info.priority < delayed.info.priority)
        {
            return self.start_delayed(id, sm);
        }
        if info.flags & transition_flags::USE_INITIATE_INTERVAL != 0
            && !interval_open(
                &info.initiate_interval,
                runtime.intervals[index].initiate,
                runtime.time_in_state,
            )
        {
            self.nodes[id.0].delayed = Some(DelayedTransition {
                index,
                info,
                event: ev,
            });
        } else {
            self.start_transition(id, sm, current, info.to_state_id, &info, ev);
        }
        true
    }

    /// An admitted request retains its condition result and triggering event.
    fn start_delayed(&mut self, id: NodeId, sm: &StateMachine) -> bool {
        let runtime = &self.nodes[id.0];
        let Some(delayed) = &runtime.delayed else {
            return false;
        };
        if !interval_open(
            &delayed.info.initiate_interval,
            runtime.intervals[delayed.index].initiate,
            runtime.time_in_state,
        ) || runtime.transitions.iter().any(|active| {
            active.flags & transition_flags::UNINTERRUPTIBLE_WHILE_PLAYING != 0
                || active.priority > delayed.info.priority
        }) {
            return false;
        }
        let Some(current) = runtime.current_state else {
            return false;
        };
        let delayed = delayed.clone();
        self.start_transition(
            id,
            sm,
            current,
            delayed.info.to_state_id,
            &delayed.info,
            delayed.event,
        );
        true
    }

    /// Match event or condition-only transitions. Wildcard self transitions require their flag.
    fn transition_matches(sm: &StateMachine, current: i32, t: &TransitionInfo, ev: Event) -> bool {
        if t.flags & transition_flags::DISABLED != 0 || t.event_id != ev.id {
            return false;
        }
        if ev.id == -1 && t.condition.is_none() {
            return false;
        }
        let wildcard = transition_flags::IS_GLOBAL_WILDCARD | transition_flags::IS_LOCAL_WILDCARD;
        let self_ok = transition_flags::ALLOW_SELF_TRANSITION_BY_TRANSITION_FROM_ANY_STATE;
        if t.to_state_id == current && t.flags & wildcard != 0 && t.flags & self_ok == 0 {
            return false;
        }
        sm.state(t.to_state_id).is_some_and(|s| s.enable)
    }

    fn start_transition(
        &mut self,
        id: NodeId,
        sm: &StateMachine,
        from: i32,
        to: i32,
        info: &TransitionInfo,
        ev: Event,
    ) {
        let from_generator = sm.state(from).and_then(|s| s.generator);
        let to_generator = sm.state(to).and_then(|s| s.generator);
        if let Some(state) = sm.state(from) {
            for e in &state.exit_notify_events {
                self.raise(e.id);
            }
        }
        let graph = self.graph;
        let effect = info
            .transition
            .and_then(|e| match &graph.node(e).kind {
                // A bound duration on the effect node overrides the stored
                // one, which is how a graph makes a blend speed depend on a
                // variable.
                NodeKind::BlendingTransition(b) => {
                    let d = self.nodes[e.0].duration.unwrap_or(b.duration);
                    Some(ActiveEffect::Blend(d, b.blend_curve))
                }
                NodeKind::GeneratorTransition(effect) => {
                    effect
                        .transition_generator
                        .map(|generator| ActiveEffect::Generator {
                            generator,
                            blend_in_duration: effect.blend_in_duration.max(0.0),
                            blend_out_duration: effect.blend_out_duration.max(0.0),
                        })
                }
                _ => None,
            })
            .filter(|effect| match effect {
                ActiveEffect::Blend(duration, _) => *duration > 0.0,
                ActiveEffect::Generator { .. } => true,
            });
        match effect {
            Some(ActiveEffect::Blend(duration, curve))
                if from_generator != to_generator && sm.max_simultaneous_transitions > 0 =>
            {
                let capacity = sm.max_simultaneous_transitions as usize;
                let length = self.nodes[id.0].transitions.len();
                if length >= capacity {
                    self.end_transitions(id, sm, length - capacity + 1);
                }
                self.nodes[id.0].transitions.push(ActiveTransition {
                    priority: info.priority,
                    flags: info.flags,
                    from_generator,
                    to_generator,
                    elapsed: 0.0,
                    duration,
                    curve,
                    generator_transition: None,
                });
            }
            Some(ActiveEffect::Generator {
                generator,
                blend_in_duration,
                blend_out_duration,
            }) if from_generator != to_generator && sm.max_simultaneous_transitions > 0 => {
                let capacity = sm.max_simultaneous_transitions as usize;
                let length = self.nodes[id.0].transitions.len();
                if length >= capacity {
                    self.end_transitions(id, sm, length - capacity + 1);
                }
                self.activate(generator);
                self.nodes[id.0].transitions.push(ActiveTransition {
                    priority: info.priority,
                    flags: info.flags,
                    from_generator,
                    to_generator,
                    elapsed: 0.0,
                    duration: 0.0,
                    curve: 1,
                    generator_transition: Some(GeneratorTransitionRuntime {
                        generator,
                        blend_in_duration,
                        blend_out_duration,
                        elapsed: 0.0,
                        stage: if blend_in_duration > 0.0 {
                            GeneratorTransitionStage::BlendIn
                        } else {
                            GeneratorTransitionStage::Playing
                        },
                    }),
                });
            }
            _ => {
                self.end_transitions(id, sm, self.nodes[id.0].transitions.len());
                if from_generator != to_generator {
                    if let Some(g) = from_generator {
                        self.deactivate(g);
                    }
                }
            }
        }
        self.enter_state(id, sm, to, (ev.id >= 0).then_some(ev.id));
    }

    /// End a prefix of the transition chain and release generators no longer used.
    fn end_transitions(&mut self, id: NodeId, sm: &StateMachine, count: usize) {
        let before = self.active_generators_of_machine(id, sm);
        self.nodes[id.0].transitions.drain(..count);
        let after = self.active_generators_of_machine(id, sm);
        for generator in before {
            if !after.contains(&generator) {
                self.deactivate(generator);
            }
        }
    }

    fn reference_output(&self) -> Output {
        Output {
            pose: self.reference_pose.clone(),
            world_from_model: QsTransform::IDENTITY,
            bone_weights: vec![0.0; self.reference_pose.len()],
            weight: 0.0,
            foot_ik_controls: None,
            ragdoll_controls: Vec::new(),
            rigid_body_ragdoll_controls: Vec::new(),
        }
    }

    /// Generate a node's pose for this frame, children first.
    fn generate(&mut self, id: NodeId, dt: f32) -> Output {
        if !self.nodes[id.0].active {
            return self.reference_output();
        }
        let graph = self.graph;
        let node = graph.node(id);
        match &node.kind {
            NodeKind::Clip(clip) => self.generate_clip(id, clip, dt),
            NodeKind::StateMachine(sm) => self.generate_machine(id, sm, dt),
            NodeKind::Blender(b) => self.generate_blender(id, b, dt),
            NodeKind::ManualSelector { generators, .. } => {
                let wanted = self.nodes[id.0].selected.unwrap_or(0);
                let target = generators.get(wanted).copied().flatten();
                let active: Vec<NodeId> = generators
                    .iter()
                    .flatten()
                    .copied()
                    .filter(|g| self.nodes[g.0].active)
                    .collect();
                for g in active {
                    if Some(g) != target {
                        self.deactivate(g);
                    }
                }
                match target {
                    Some(g) => {
                        self.activate(g);
                        self.generate(g, dt)
                    }
                    None => self.reference_output(),
                }
            }
            NodeKind::ModifierGenerator {
                modifier,
                generator,
            } => {
                let start = self.modifier_order.len();
                let modifier_start = self.modifier_order.len();
                if let Some(modifier) = modifier {
                    self.sync_modifier(*modifier);
                    for index in modifier_start..self.modifier_order.len() {
                        let node = self.modifier_order[index];
                        self.update_modifier(node, dt);
                    }
                }
                let modifiers = self.modifier_order[modifier_start..].to_vec();
                let mut output = match generator {
                    Some(generator) => self.generate(*generator, dt),
                    None => self.reference_output(),
                };
                for index in start..self.modifier_order.len() {
                    let node = self.modifier_order[index];
                    self.apply_tweener(node, &mut output, dt);
                    if self.nodes[node.0].active {
                        if let NodeKind::FootIkControls { controls, .. } =
                            &self.graph.node(node).kind
                        {
                            output.foot_ik_controls = Some(controls.clone());
                        }
                    }
                }
                let active_modifiers: Vec<_> = modifiers
                    .into_iter()
                    .filter(|modifier| self.nodes[modifier.0].active)
                    .collect();
                for modifier in active_modifiers {
                    self.apply_keyframe_bones_modifier(modifier, &mut output);
                    self.apply_modifier_output(modifier, &mut output);
                    match &self.graph.node(modifier).kind {
                        NodeKind::LookAtModifier {
                            bones,
                            eye_bones,
                            limit_angle_degrees,
                            on_gain,
                            use_bone_gains,
                            target_location,
                            look_at_target,
                            look_at_camera,
                            look_at_camera_location,
                            ..
                        } => {
                            let target = if *look_at_target {
                                Some([target_location[0], target_location[1], target_location[2]])
                            } else if *look_at_camera {
                                Some(*look_at_camera_location)
                            } else {
                                None
                            };
                            if let Some(target) = target {
                                apply_look_at(
                                    &mut output.pose,
                                    bones.iter().chain(eye_bones),
                                    target,
                                    *limit_angle_degrees,
                                    *on_gain,
                                    *use_bone_gains,
                                );
                            }
                        }
                        NodeKind::DirectAt { .. } => {
                            self.apply_direct_at(modifier, &mut output, dt);
                        }
                        NodeKind::LimbIk { .. } => {
                            self.apply_limb_ik(modifier, &mut output);
                        }
                        NodeKind::FootIk {
                            legs,
                            raycast_distance_up,
                            raycast_distance_down,
                            vertical_offset,
                            ground_ascending_gain,
                            ground_descending_gain,
                            ..
                        } => {
                            apply_foot_ik(
                                &mut output,
                                legs,
                                &self.foot_ik_ground_targets,
                                *raycast_distance_up,
                                *raycast_distance_down,
                                *vertical_offset,
                                *ground_ascending_gain,
                                *ground_descending_gain,
                            );
                        }
                        NodeKind::GetUpModifier { .. } => {
                            self.apply_get_up_modifier(modifier, dt, &mut output);
                        }
                        NodeKind::PoweredRagdollControls {
                            control,
                            bones,
                            world_from_model,
                            bone_weights,
                            ..
                        } => {
                            output
                                .ragdoll_controls
                                .extend(bones.iter().zip(bone_weights).map(|(&bone, &weight)| {
                                    RagdollControlOutput {
                                        bone,
                                        control: *control,
                                        world_from_model: *world_from_model,
                                        weight,
                                    }
                                }));
                        }
                        NodeKind::RigidBodyRagdollControlsModifier {
                            control_data,
                            duration_to_blend,
                            bones,
                            ..
                        } => {
                            output.rigid_body_ragdoll_controls.push(
                                RigidBodyRagdollControlOutput {
                                    control_data: *control_data,
                                    duration_to_blend: *duration_to_blend,
                                    bones: bones.clone(),
                                    weight: 1.0,
                                },
                            );
                        }
                        NodeKind::TwistModifier(_) => {
                            self.apply_twist_modifier(modifier, &mut output);
                        }
                        _ => {}
                    }
                }
                output
            }
            NodeKind::IStateTagging {
                default_generator: generator,
                ..
            }
            | NodeKind::SynchronizedClip {
                clip_generator: generator,
                ..
            } => match generator {
                Some(g) => self.generate(*g, dt),
                None => self.reference_output(),
            },
            NodeKind::OffsetAnimation {
                default_generator,
                offset_clip_generator,
                offset_variable,
                offset_range_start,
                offset_range_end,
            } => {
                let mut base = default_generator
                    .map(|generator| self.generate(generator, dt))
                    .unwrap_or_else(|| self.reference_output());
                let Some(offset_generator) = offset_clip_generator else {
                    return base;
                };
                let offset = self.generate(*offset_generator, dt);
                if offset.weight <= 0.0 {
                    return base;
                }
                let value = self.nodes[id.0].offset_variable.unwrap_or(*offset_variable);
                let amount = offset_amount(value, *offset_range_start, *offset_range_end);
                apply_pose_offset(&mut base, &offset, amount);
                base
            }
            NodeKind::BoneSwitch {
                default_generator,
                children,
            } => self.generate_bone_switch(*default_generator, children, dt),
            NodeKind::CyclicBlendTransition {
                blender_generator: Some(generator),
                ..
            } => {
                self.update_cyclic_blend(id, *generator, dt);
                self.generate(*generator, dt)
            }
            // A linked reference is a pass-through to the graph that was
            // spliced under it. An unlinked one has nothing to play and is
            // reported, which is what holds the vanilla standing idle silent
            // until `IdleBehavior.hkx` is linked in.
            NodeKind::BehaviorReference {
                behavior_name,
                resolved,
            } => match resolved {
                Some(g) => self.generate(*g, dt),
                None => {
                    push_unique(
                        &mut self.unsupported.behavior_references,
                        behavior_name.clone(),
                    );
                    self.reference_output()
                }
            },
            NodeKind::Unknown => {
                push_unique(&mut self.unsupported.unknown_classes, node.class.clone());
                self.reference_output()
            }
            // Modifiers are not generators. Reaching one here means a
            // generator slot held a modifier, so there is no pose to give.
            NodeKind::ModifierList { .. }
            | NodeKind::BlendingTransition(_)
            | NodeKind::GeneratorTransition(_)
            | NodeKind::EventDrivenModifier { .. }
            | NodeKind::TimerModifier { .. }
            | NodeKind::EventsFromRangeModifier { .. }
            | NodeKind::BsTimerModifier { .. }
            | NodeKind::Tweener { .. }
            | NodeKind::EventOnDeactivateModifier { .. }
            | NodeKind::GetTimeStepModifier { .. }
            | NodeKind::DirectAt { .. }
            | NodeKind::InterpValue { .. }
            | NodeKind::IsActiveModifier { .. }
            | NodeKind::FalseToTrueModifier { .. }
            | NodeKind::ModifyOnce { .. }
            | NodeKind::PassByTargetTrigger { .. }
            | NodeKind::RagdollContactListener { .. }
            | NodeKind::EveryNEventsModifier { .. }
            | NodeKind::TwistModifier(_)
            | NodeKind::DecomposeVector { .. }
            | NodeKind::EvaluateExpression { .. }
            | NodeKind::RotateCharacterModifier { .. }
            | NodeKind::CyclicBlendTransition {
                blender_generator: None,
                ..
            }
            | NodeKind::SpeedSampler { .. }
            | NodeKind::DampingModifier { .. }
            | NodeKind::FootIkControls { .. }
            | NodeKind::LookAtModifier { .. }
            | NodeKind::IStateManager { .. }
            | NodeKind::LimbIk { .. }
            | NodeKind::FootIk { .. }
            | NodeKind::GetUpModifier { .. }
            | NodeKind::KeyframeBonesModifier { .. }
            | NodeKind::PoweredRagdollControls { .. }
            | NodeKind::RigidBodyRagdollControlsModifier { .. }
            | NodeKind::TransformVector { .. }
            | NodeKind::Modifier { .. } => self.reference_output(),
        }
    }

    /// Rotate the authored limb's local bone rotations toward the host cast
    /// target. The whole correction is bounded by `limitAngleDegrees` and
    /// scaled by gain. Other bones and translations remain unchanged.
    fn apply_limb_ik(&self, id: NodeId, output: &mut Output) {
        let node = &self.graph.nodes[id.0];
        let NodeKind::LimbIk {
            limit_angle_degrees,
            start_bone_index,
            end_bone_index,
            gain,
            ..
        } = &node.kind
        else {
            return;
        };
        let Some(target) = self.nodes[id.0].limb_ik_target else {
            return;
        };
        if !self.nodes[id.0].active || *gain == 0.0 || *limit_angle_degrees == 0.0 {
            return;
        }
        let start = usize::try_from(*start_bone_index).unwrap_or(usize::MAX);
        let end = usize::try_from(*end_bone_index).unwrap_or(usize::MAX);
        let Some(pivot) = output.pose.get(start).map(|t| t.translation) else {
            return;
        };
        let Some(endpoint) = output.pose.get(end).map(|t| t.translation) else {
            return;
        };
        let from = [
            endpoint[0] - pivot[0],
            endpoint[1] - pivot[1],
            endpoint[2] - pivot[2],
        ];
        let to = [
            target[0] - pivot[0],
            target[1] - pivot[1],
            target[2] - pivot[2],
        ];
        let (Some(from), Some(to)) = (normalize(from), normalize(to)) else {
            return;
        };
        let cosine = dot(from, to).clamp(-1.0, 1.0);
        let axis = normalize(cross(from, to)).or_else(|| {
            if cosine < 0.0 {
                let basis = if from[0].abs() < 0.8 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                normalize(cross(from, basis))
            } else {
                None
            }
        });
        let Some(axis) = axis else {
            return;
        };
        let angle = (cosine.acos() * gain).min(limit_angle_degrees.to_radians());
        if !angle.is_finite() || angle <= f32::EPSILON {
            return;
        }
        let bone_count = (end - start + 1) as f32;
        let (sin, cos) = (angle / bone_count * 0.5).sin_cos();
        let correction = [axis[0] * sin, axis[1] * sin, axis[2] * sin, cos];
        for transform in output.pose.iter_mut().take(end + 1).skip(start) {
            transform.rotation = quat_mul(correction, transform.rotation);
            if let Some(rotation) = normalize_quaternion(transform.rotation) {
                transform.rotation = rotation;
            }
        }
    }

    /// Apply ground alignment to the three bones selected by the modifier.
    fn apply_get_up_modifier(&mut self, id: NodeId, dt: f32, output: &mut Output) {
        let NodeKind::GetUpModifier {
            ground_normal,
            duration,
            align_with_ground_duration,
            root_bone_index,
            other_bone_index,
            another_bone_index,
            ..
        } = &self.graph.node(id).kind
        else {
            return;
        };
        if !self.nodes[id.0].active || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let normal_length = ground_normal[..3]
            .iter()
            .map(|component| component * component)
            .sum::<f32>()
            .sqrt();
        if !normal_length.is_finite() || normal_length <= f32::EPSILON {
            return;
        }
        let normal = [
            ground_normal[0] / normal_length,
            ground_normal[1] / normal_length,
            ground_normal[2] / normal_length,
        ];
        self.nodes[id.0].get_up_elapsed += dt;
        let elapsed = self.nodes[id.0].get_up_elapsed;
        let align = if align_with_ground_duration.is_finite() && *align_with_ground_duration > 0.0 {
            (elapsed / align_with_ground_duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let fade = if duration.is_finite() && *duration > 0.0 {
            (1.0 - elapsed / duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let amount = align * fade;
        if amount <= 0.0 {
            return;
        }
        let dot = normal[2].clamp(-1.0, 1.0);
        let mut correction = if dot < -0.999_999 {
            [1.0, 0.0, 0.0, 0.0]
        } else {
            [-normal[1], normal[0], 0.0, 1.0 + dot]
        };
        correction = normalize_quaternion(correction).unwrap_or(QsTransform::IDENTITY.rotation);
        let partial = blend_transform(
            &QsTransform::IDENTITY,
            &QsTransform {
                rotation: correction,
                ..QsTransform::IDENTITY
            },
            amount,
        )
        .rotation;
        let mut visited = Vec::with_capacity(3);
        for index in [*root_bone_index, *other_bone_index, *another_bone_index] {
            let Ok(index) = usize::try_from(index) else {
                continue;
            };
            if visited.contains(&index) {
                continue;
            }
            visited.push(index);
            let Some(bone) = output.pose.get_mut(index) else {
                continue;
            };
            let original = bone.rotation;
            bone.rotation = normalize_quaternion(quat_mul(original, partial))
                .unwrap_or(QsTransform::IDENTITY.rotation);
        }
    }

    fn apply_twist_modifier(&self, id: NodeId, output: &mut Output) {
        let Some(node) = self.graph.nodes.get(id.0) else {
            return;
        };
        let NodeKind::TwistModifier(twist) = &node.kind else {
            return;
        };
        let Some(runtime) = self.nodes.get(id.0) else {
            return;
        };
        if !runtime.active {
            return;
        }
        distribute_twist(
            output,
            twist,
            runtime.twist_angle.unwrap_or(twist.twist_angle),
        );
    }

    /// The clip's playable length after the crops, `enforcedDuration`
    /// when set.
    fn clip_duration(clip: &ClipGenerator, animation: f32) -> f32 {
        let d = if clip.enforced_duration > 0.0 {
            clip.enforced_duration
        } else {
            animation
        };
        (d - clip.crop_start_amount_local_time - clip.crop_end_amount_local_time).max(0.0)
    }

    /// Advance the playhead by `mode`, fire the triggers the playhead
    /// crossed, sample. Mode 0 stops at the end, 1 loops, 2 follows the
    /// user fraction, 3 ping pongs.
    fn generate_clip(&mut self, id: NodeId, clip: &ClipGenerator, dt: f32) -> Output {
        let Some(source) = self.clips.get(&clip.animation_name) else {
            push_unique(
                &mut self.unsupported.missing_clips,
                clip.animation_name.clone(),
            );
            return self.reference_output();
        };
        let duration = Self::clip_duration(clip, source.duration());
        let rt = &mut self.nodes[id.0];
        let speed = rt.playback_speed.unwrap_or(clip.playback_speed);
        let previous = rt.local_time;
        let mut wrapped = false;
        if let Some(time) = rt.synchronized_time {
            rt.local_time = time.min(duration);
            rt.at_end = rt.local_time >= duration;
        } else {
            match clip.mode {
                2 => {
                    let f = rt
                        .user_fraction
                        .unwrap_or(clip.user_controlled_time_fraction);
                    rt.local_time = f.clamp(0.0, 1.0) * duration;
                }
                1 => {
                    rt.local_time += dt * speed;
                    if duration > 0.0 {
                        while rt.local_time >= duration {
                            rt.local_time -= duration;
                            rt.passes += 1;
                            wrapped = true;
                        }
                        while rt.local_time < 0.0 {
                            rt.local_time += duration;
                            wrapped = true;
                        }
                    } else {
                        rt.local_time = 0.0;
                    }
                }
                3 => {
                    rt.local_time += dt * speed;
                    if duration > 0.0 {
                        let period = 2.0 * duration;
                        let mut t = rt.local_time.rem_euclid(period);
                        if t > duration {
                            t = period - t;
                        }
                        rt.local_time = t;
                    }
                }
                _ => {
                    rt.local_time += dt * speed;
                    if rt.local_time >= duration {
                        rt.local_time = duration;
                        rt.at_end = true;
                    }
                    if rt.local_time < 0.0 {
                        rt.local_time = 0.0;
                    }
                }
            }
        }
        let now = rt.local_time;
        let passes = rt.passes;
        let mut fired = Vec::new();
        for t in &clip.triggers {
            let at = if t.relative_to_end_of_clip {
                duration + t.local_time
            } else {
                t.local_time
            };
            let crossed = if wrapped {
                at > previous || at <= now
            } else {
                previous < at && at <= now
            };
            let first_pass = passes == 0 || (wrapped && passes == 1 && at > previous);
            if crossed && (!t.acyclic || first_pass) {
                fired.push(t.event.id);
            }
        }
        for e in fired {
            self.raise(e);
        }
        let mut pose = self.reference_pose.clone();
        source.sample_into(now + clip.crop_start_amount_local_time, &mut pose);
        Output {
            bone_weights: vec![1.0; pose.len()],
            pose,
            world_from_model: QsTransform::IDENTITY,
            weight: 1.0,
            foot_ik_controls: None,
            ragdoll_controls: Vec::new(),
            rigid_body_ragdoll_controls: Vec::new(),
        }
    }

    fn generate_machine(&mut self, id: NodeId, sm: &StateMachine, dt: f32) -> Output {
        self.nodes[id.0].time_in_state += dt;
        let current_generator = self.nodes[id.0]
            .current_state
            .and_then(|c| sm.state(c))
            .and_then(|s| s.generator);
        if !self.nodes[id.0].transitions.is_empty() {
            for transition in &mut self.nodes[id.0].transitions {
                if let Some(generator_transition) = &mut transition.generator_transition {
                    generator_transition.elapsed += dt;
                } else {
                    transition.elapsed += dt;
                }
            }
            let transitions = self.nodes[id.0].transitions.clone();
            let generators = self.active_generators_of_machine(id, sm);
            let samples: Vec<_> = generators
                .into_iter()
                .map(|generator| (generator, self.generate(generator, dt)))
                .collect();
            let sample = |generator: Option<NodeId>| {
                samples
                    .iter()
                    .find(|(id, _)| Some(*id) == generator)
                    .map_or_else(|| self.reference_output(), |(_, output)| output.clone())
            };
            let mut output = sample(transitions[0].from_generator);
            let mut completed = 0;
            let mut generator_transition_updates = Vec::new();
            for (index, transition) in transitions.iter().enumerate() {
                let target = sample(transition.to_generator);
                if let Some(mut generator_transition) = transition.generator_transition {
                    let transition_output = sample(Some(generator_transition.generator));
                    match generator_transition.stage {
                        GeneratorTransitionStage::BlendIn => {
                            let alpha = (generator_transition.elapsed
                                / generator_transition.blend_in_duration)
                                .clamp(0.0, 1.0);
                            output = blend_outputs(&output, &transition_output, alpha);
                            if alpha >= 1.0 {
                                generator_transition.stage = GeneratorTransitionStage::Playing;
                                generator_transition.elapsed = 0.0;
                            }
                        }
                        GeneratorTransitionStage::Playing => {
                            output = transition_output;
                            if self.nodes[generator_transition.generator.0].at_end {
                                generator_transition.stage = GeneratorTransitionStage::BlendOut;
                                generator_transition.elapsed = 0.0;
                                if generator_transition.blend_out_duration == 0.0 {
                                    output = target;
                                    completed = index + 1;
                                }
                            }
                        }
                        GeneratorTransitionStage::BlendOut => {
                            let alpha = (generator_transition.elapsed
                                / generator_transition.blend_out_duration)
                                .clamp(0.0, 1.0);
                            output = blend_outputs(&transition_output, &target, alpha);
                            if alpha >= 1.0 {
                                output = target;
                                completed = index + 1;
                            }
                        }
                    }
                    if completed <= index {
                        generator_transition_updates.push((index, generator_transition));
                    }
                } else if transition.elapsed >= transition.duration {
                    output = target;
                    completed = index + 1;
                } else {
                    let alpha = blend_curve(
                        transition.curve,
                        (transition.elapsed / transition.duration).clamp(0.0, 1.0),
                    );
                    output = blend_outputs(&output, &target, alpha);
                }
            }
            for (index, generator_transition) in generator_transition_updates {
                self.nodes[id.0].transitions[index].generator_transition =
                    Some(generator_transition);
            }
            if completed > 0 {
                self.end_transitions(id, sm, completed);
            }
            return output;
        }
        match current_generator {
            Some(g) => self.generate(g, dt),
            None => self.reference_output(),
        }
    }

    /// Weights are the children's `weight` fields or their bindings,
    /// normalized. A parametric blend (flag 16) treats each child's weight
    /// as a position on the blend parameter axis and interpolates between
    /// the two neighbours of the parameter. Sync (flag 1) drives every
    /// direct clip child from the master's normalized time.
    fn generate_blender(&mut self, id: NodeId, b: &BlenderGenerator, dt: f32) -> Output {
        let keep_zero = b.flags & 8 != 0;
        let raw: Vec<f32> = b
            .children
            .iter()
            .enumerate()
            .map(|(i, c)| self.nodes[id.0].child_weights[i].unwrap_or(c.weight))
            .collect();
        let weights = if b.flags & 16 != 0 {
            let param = self.nodes[id.0]
                .blend_parameter
                .unwrap_or(b.blend_parameter);
            parametric_weights(&raw, param)
        } else {
            raw
        };
        let total: f32 = weights.iter().filter(|w| **w > 0.0).sum();
        if b.flags & 1 != 0 {
            self.sync_children(b, &weights, dt);
        }
        let mut pose = self.reference_pose.clone();
        let mut bone_weights = vec![0.0; pose.len()];
        let mut ragdoll_controls = Vec::new();
        let mut rigid_body_ragdoll_controls = Vec::new();
        let mut acc_weight = 0.0;
        let mut foot_ik_controls = None;
        let mut foot_ik_contribution = 0.0;
        for (i, child) in b.children.iter().enumerate() {
            let Some(g) = child.generator else {
                continue;
            };
            let w = weights[i];
            if w <= 0.0 || total <= 0.0 {
                if keep_zero {
                    if self.nodes[g.0].active {
                        let _ = self.generate(g, dt);
                    }
                } else {
                    self.deactivate(g);
                }
                continue;
            }
            self.activate(g);
            let out = self.generate(g, dt);
            let w = w / total;
            ragdoll_controls.extend(out.ragdoll_controls.iter().map(|control| {
                RagdollControlOutput {
                    weight: control.weight * w,
                    ..*control
                }
            }));
            rigid_body_ragdoll_controls.extend(out.rigid_body_ragdoll_controls.into_iter().map(
                |mut control| {
                    control.weight *= w;
                    control
                },
            ));
            if out.weight <= 0.0 {
                continue;
            }
            let control_contribution = w * out.weight;
            if control_contribution > foot_ik_contribution {
                if let Some(controls) = &out.foot_ik_controls {
                    foot_ik_controls = Some(controls.clone());
                    foot_ik_contribution = control_contribution;
                }
            }
            acc_weight += w;
            for (bone, (acc, next)) in pose.iter_mut().zip(&out.pose).enumerate() {
                let mask = match self.nodes[id.0].child_bone_weights_pointers[i] {
                    BoundMaskPointer::Pointer(pointer) => self
                        .bone_weight_masks
                        .get(&pointer)
                        .map_or(0.0, |mask| mask.get(bone).copied().unwrap_or(0.0)),
                    BoundMaskPointer::Null => 1.0,
                    BoundMaskPointer::Unbound => child
                        .bone_weights
                        .as_ref()
                        .map_or(1.0, |mask| mask.get(bone).copied().unwrap_or(0.0)),
                };
                let contribution = w * mask * out.bone_weights.get(bone).copied().unwrap_or(0.0);
                if contribution <= 0.0 {
                    continue;
                }
                let total = bone_weights[bone] + contribution;
                *acc = if bone_weights[bone] == 0.0 {
                    *next
                } else {
                    blend_transform(acc, next, contribution / total)
                };
                bone_weights[bone] = total;
            }
        }
        Output {
            pose,
            world_from_model: QsTransform::IDENTITY,
            bone_weights,
            weight: acc_weight.min(1.0),
            foot_ik_controls,
            ragdoll_controls,
            rigid_body_ragdoll_controls,
        }
    }

    /// Overlay bone-switch children on the default generator's pose.
    fn generate_bone_switch(
        &mut self,
        default_generator: Option<NodeId>,
        children: &[crate::behavior::BoneSwitchChild],
        dt: f32,
    ) -> Output {
        let mut output = default_generator
            .map(|generator| {
                self.activate(generator);
                self.generate(generator, dt)
            })
            .unwrap_or_else(|| self.reference_output());
        for child in children {
            let Some(generator) = child.generator else {
                continue;
            };
            self.activate(generator);
            let overlay = self.generate(generator, dt);
            for (bone, ((pose, weight), overlay_pose)) in output
                .pose
                .iter_mut()
                .zip(&mut output.bone_weights)
                .zip(&overlay.pose)
                .enumerate()
            {
                let mask = child
                    .bone_weights
                    .as_ref()
                    .map_or(1.0, |weights| weights.get(bone).copied().unwrap_or(0.0))
                    .clamp(0.0, 1.0);
                let overlay_part = mask * overlay.bone_weights.get(bone).copied().unwrap_or(0.0);
                let default_part = *weight * (1.0 - overlay_part);
                let combined = default_part + overlay_part;
                if overlay_part > 0.0 && combined > 0.0 {
                    *pose = if default_part == 0.0 {
                        *overlay_pose
                    } else {
                        blend_transform(pose, overlay_pose, overlay_part / combined)
                    };
                }
                *weight = combined.min(1.0);
            }
            output.weight = output.weight.max(overlay.weight);
        }
        output
    }

    fn sync_children(&mut self, b: &BlenderGenerator, weights: &[f32], dt: f32) {
        let master = usize::try_from(b.index_of_sync_master_child)
            .ok()
            .filter(|&m| m < b.children.len())
            .or_else(|| weights.iter().position(|w| *w > 0.0));
        let Some(master) = master else {
            return;
        };
        let Some(master_gen) = b.children[master].generator else {
            return;
        };
        let Some(fraction) = self.clip_fraction_after(master_gen, dt) else {
            return;
        };
        for (i, child) in b.children.iter().enumerate() {
            if i == master {
                continue;
            }
            let Some(g) = child.generator else {
                continue;
            };
            if let Some(duration) = self.clip_length(g) {
                let rt = &mut self.nodes[g.0];
                // The child then advances by `dt` in its own generate, so
                // park it one step behind the master's phase.
                let speed = rt.playback_speed.unwrap_or(1.0);
                rt.local_time = (fraction * duration - dt * speed).rem_euclid(duration);
            }
        }
    }

    fn clip_length(&self, id: NodeId) -> Option<f32> {
        match &self.graph.node(id).kind {
            NodeKind::Clip(c) => {
                let source = self.clips.get(&c.animation_name)?;
                let d = Self::clip_duration(c, source.duration());
                (d > 0.0).then_some(d)
            }
            _ => None,
        }
    }

    /// Where a clip will be after this frame's advance, as a fraction.
    fn clip_fraction_after(&self, id: NodeId, dt: f32) -> Option<f32> {
        let duration = self.clip_length(id)?;
        let NodeKind::Clip(c) = &self.graph.node(id).kind else {
            return None;
        };
        let rt = &self.nodes[id.0];
        let speed = rt.playback_speed.unwrap_or(c.playback_speed);
        Some((rt.local_time + dt * speed).rem_euclid(duration) / duration)
    }
}

/// Bounds are half-open. A zero time exit has no upper bound.
fn interval_open(interval: &TimeInterval, event_open: bool, time: f32) -> bool {
    if interval.enter_event_id >= 0 || interval.exit_event_id >= 0 {
        event_open
    } else {
        interval.enter_time.is_finite()
            && interval.exit_time.is_finite()
            && interval.enter_time >= 0.0
            && interval.exit_time >= 0.0
            && time >= interval.enter_time
            && (interval.exit_time == 0.0 || time < interval.exit_time)
    }
}

fn update_interval_event(interval: &TimeInterval, open: &mut bool, event: i32) {
    if interval.enter_event_id == event {
        *open = true;
    }
    if interval.exit_event_id == event {
        *open = false;
    }
}

impl TransitionInfo {
    /// A transition with no effect and no flags, used for the implicit
    /// return-to-previous-state transition.
    fn plain(to_state_id: i32) -> TransitionInfo {
        let never = TimeInterval {
            enter_event_id: -1,
            exit_event_id: -1,
            enter_time: 0.0,
            exit_time: 0.0,
        };
        TransitionInfo {
            source_offset: None,
            trigger_interval: never,
            initiate_interval: never,
            transition: None,
            condition: None,
            event_id: -1,
            to_state_id,
            from_nested_state_id: -1,
            to_nested_state_id: -1,
            priority: 0,
            flags: 0,
        }
    }
}

/// The two children whose positions bracket `param` share the blend,
/// linearly by distance. Outside the range the nearest child gets it all.
fn parametric_weights(positions: &[f32], param: f32) -> Vec<f32> {
    let mut out = vec![0.0; positions.len()];
    if positions.is_empty() {
        return out;
    }
    let mut order: Vec<usize> = (0..positions.len()).collect();
    order.sort_by(|&a, &b| positions[a].total_cmp(&positions[b]));
    let first = order[0];
    let last = order[order.len() - 1];
    if param <= positions[first] {
        out[first] = 1.0;
        return out;
    }
    if param >= positions[last] {
        out[last] = 1.0;
        return out;
    }
    for pair in order.windows(2) {
        let (lo, hi) = (pair[0], pair[1]);
        let (a, b) = (positions[lo], positions[hi]);
        if a <= param && param <= b {
            let span = b - a;
            let t = if span > 0.0 { (param - a) / span } else { 0.0 };
            out[lo] = 1.0 - t;
            out[hi] = t;
            return out;
        }
    }
    out[last] = 1.0;
    out
}

/// `hkbBlendCurveUtils` curves: 0 smooth, 1 linear, 2 linear to smooth,
/// 3 smooth to linear.
fn blend_curve(curve: i8, t: f32) -> f32 {
    match curve {
        1 => t,
        2 => 1.0 - (1.0 - t) * (1.0 - t),
        3 => t * t,
        _ => t * t * (3.0 - 2.0 * t),
    }
}

/// Blend two transforms: lerp translation and scale, nlerp rotation on
/// the shorter arc.
fn apply_look_at<'a>(
    pose: &mut [QsTransform],
    bones: impl Iterator<Item = &'a crate::behavior::LookAtBone>,
    target: [f32; 3],
    global_limit_degrees: f32,
    global_gain: f32,
    use_bone_gains: bool,
) {
    for bone in bones.filter(|bone| bone.enabled) {
        let Ok(index) = usize::try_from(bone.index) else {
            continue;
        };
        let Some(transform) = pose.get_mut(index) else {
            continue;
        };
        let direction = [
            target[0] - transform.translation[0],
            target[1] - transform.translation[1],
            target[2] - transform.translation[2],
        ];
        let Some(direction) = normalize3(direction) else {
            continue;
        };
        let Some(forward) = normalize3(quat_rotate3(
            transform.rotation,
            [
                bone.forward_axis[0],
                bone.forward_axis[1],
                bone.forward_axis[2],
            ],
        )) else {
            continue;
        };
        let dot = dot3(forward, direction).clamp(-1.0, 1.0);
        let angle = dot.acos();
        if angle <= f32::EPSILON {
            continue;
        }
        let limit = global_limit_degrees
            .min(bone.limit_angle_degrees)
            .max(0.0)
            .to_radians();
        let gain = if use_bone_gains {
            bone.on_gain
        } else {
            global_gain
        }
        .clamp(0.0, 1.0);
        let step = angle.min(limit) * gain;
        if step <= f32::EPSILON {
            continue;
        }
        let delta = rotation_between(forward, direction, step);
        transform.rotation = quat_mul(delta, transform.rotation);
        if let Some(rotation) = normalize_quaternion(transform.rotation) {
            transform.rotation = rotation;
        }
    }
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize3(vector: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot3(vector, vector).sqrt();
    (length.is_finite() && length > f32::EPSILON)
        .then(|| [vector[0] / length, vector[1] / length, vector[2] / length])
}

fn normalize_quaternion(q: [f32; 4]) -> Option<[f32; 4]> {
    let length = q
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    (length.is_finite() && length > f32::EPSILON).then(|| q.map(|component| component / length))
}

fn quat_rotate3(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let vector = [v[0], v[1], v[2], 0.0];
    let conjugate = [-q[0], -q[1], -q[2], q[3]];
    let result = quat_mul(quat_mul(q, vector), conjugate);
    [result[0], result[1], result[2]]
}

fn rotation_between(from: [f32; 3], to: [f32; 3], angle: f32) -> [f32; 4] {
    let mut axis = [
        from[1] * to[2] - from[2] * to[1],
        from[2] * to[0] - from[0] * to[2],
        from[0] * to[1] - from[1] * to[0],
    ];
    if normalize3(axis).is_none() {
        axis = if from[0].abs() < from[1].abs() && from[0].abs() < from[2].abs() {
            [0.0, -from[2], from[1]]
        } else if from[1].abs() < from[2].abs() {
            [-from[2], 0.0, from[0]]
        } else {
            [-from[1], from[0], 0.0]
        };
    }
    let axis = normalize3(axis).unwrap_or([0.0, 0.0, 1.0]);
    let half = angle * 0.5;
    [
        axis[0] * half.sin(),
        axis[1] * half.sin(),
        axis[2] * half.sin(),
        half.cos(),
    ]
}

pub fn blend_transform(a: &QsTransform, b: &QsTransform, alpha: f32) -> QsTransform {
    let lerp = |x: f32, y: f32| x + (y - x) * alpha;
    let dot: f32 = a.rotation.iter().zip(&b.rotation).map(|(x, y)| x * y).sum();
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let mut q = [0.0f32; 4];
    for ((out, x), y) in q.iter_mut().zip(&a.rotation).zip(&b.rotation) {
        *out = lerp(*x, sign * y);
    }
    let len = q.iter().map(|c| c * c).sum::<f32>().sqrt();
    if len > 0.0 {
        for c in &mut q {
            *c /= len;
        }
    } else {
        q = a.rotation;
    }
    QsTransform {
        translation: [
            lerp(a.translation[0], b.translation[0]),
            lerp(a.translation[1], b.translation[1]),
            lerp(a.translation[2], b.translation[2]),
            0.0,
        ],
        rotation: q,
        scale: [
            lerp(a.scale[0], b.scale[0]),
            lerp(a.scale[1], b.scale[1]),
            lerp(a.scale[2], b.scale[2]),
            0.0,
        ],
    }
}

fn normalized_quaternion(mut value: [f32; 4]) -> [f32; 4] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if length > 0.0 && length.is_finite() {
        for component in &mut value {
            *component /= length;
        }
        value
    } else {
        QsTransform::IDENTITY.rotation
    }
}

/// Blend two outputs. A side at weight zero contributes nothing, so a
/// transition out of a reference pose is a cut to the other side.
/// Blend pose outputs and their independent ragdoll control contributions.
pub fn blend_outputs(from: &Output, to: &Output, alpha: f32) -> Output {
    if alpha <= 0.0 {
        return from.clone();
    }
    if alpha >= 1.0 {
        return to.clone();
    }
    let ragdoll_controls = from
        .rigid_body_ragdoll_controls
        .iter()
        .cloned()
        .map(|mut control| {
            control.weight *= 1.0 - alpha;
            control
        })
        .chain(
            to.rigid_body_ragdoll_controls
                .iter()
                .cloned()
                .map(|mut control| {
                    control.weight *= alpha;
                    control
                }),
        )
        .collect();
    if from.weight <= 0.0 && to.weight <= 0.0 {
        let mut output = from.clone();
        output.ragdoll_controls = blend_ragdoll_controls(from, to, alpha);
        output.rigid_body_ragdoll_controls = ragdoll_controls;
        return output;
    }
    if from.weight <= 0.0 {
        let mut output = to.clone();
        output.ragdoll_controls = blend_ragdoll_controls(from, to, alpha);
        output.rigid_body_ragdoll_controls = ragdoll_controls;
        return output;
    }
    if to.weight <= 0.0 {
        let mut output = from.clone();
        output.ragdoll_controls = blend_ragdoll_controls(from, to, alpha);
        output.rigid_body_ragdoll_controls = ragdoll_controls;
        return output;
    }
    let mut pose = from.pose.clone();
    let mut bone_weights = vec![0.0; pose.len()];
    for (bone, (a, b)) in pose.iter_mut().zip(&to.pose).enumerate() {
        let from_weight = from.bone_weights.get(bone).copied().unwrap_or(0.0) * (1.0 - alpha);
        let to_weight = to.bone_weights.get(bone).copied().unwrap_or(0.0) * alpha;
        let total = from_weight + to_weight;
        if total > 0.0 {
            *a = blend_transform(a, b, to_weight / total);
        }
        bone_weights[bone] = total;
    }
    Output {
        pose,
        world_from_model: blend_transform(&from.world_from_model, &to.world_from_model, alpha),
        bone_weights,
        weight: from.weight + (to.weight - from.weight) * alpha,
        foot_ik_controls: match (&from.foot_ik_controls, &to.foot_ik_controls) {
            (Some(a), Some(b)) => Some(if alpha < 0.5 { a.clone() } else { b.clone() }),
            (Some(controls), None) | (None, Some(controls)) => Some(controls.clone()),
            (None, None) => None,
        },
        ragdoll_controls: blend_ragdoll_controls(from, to, alpha),
        rigid_body_ragdoll_controls: ragdoll_controls,
    }
}

fn blend_ragdoll_controls(from: &Output, to: &Output, alpha: f32) -> Vec<RagdollControlOutput> {
    from.ragdoll_controls
        .iter()
        .map(|control| RagdollControlOutput {
            weight: control.weight * (1.0 - alpha),
            ..*control
        })
        .chain(
            to.ragdoll_controls
                .iter()
                .map(|control| RagdollControlOutput {
                    weight: control.weight * alpha,
                    ..*control
                }),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::behavior::{
        Binding, ExpressionData, LookAtBone, Node, VariableBindingSet, VariableValueSet,
    };
    use crate::spline::encode;

    fn pad4(bytes: &mut Vec<u8>) {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
    }

    /// One transform track at a non-reference x position. This is encoded as
    /// a spline clip so the wrapper test exercises the normal sampling path.
    fn modifier_generator_clip() -> Clip {
        const FRAMES: usize = 31;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[1 | (2 << 2), 0b0001_0000, 0b0000_0001, 0]);
        pad4(&mut bytes);
        let items = (FRAMES - 1) as u16;
        bytes.extend_from_slice(&items.to_le_bytes());
        bytes.push(1);
        bytes.push(0);
        for frame in 0..FRAMES {
            bytes.push(frame as u8);
        }
        bytes.push((FRAMES - 1) as u8);
        pad4(&mut bytes);
        bytes.extend_from_slice(&4.0f32.to_le_bytes());
        bytes.extend_from_slice(&4.0f32.to_le_bytes());
        for _ in 0..FRAMES {
            bytes.extend_from_slice(&encode::point_16(4.0, 4.0, 4.0).to_le_bytes());
        }
        pad4(&mut bytes);
        bytes.extend_from_slice(&encode::quat_48([0.0, 0.0, 0.0, 1.0]));
        pad4(&mut bytes);
        let animation = SplineCompressedAnimation {
            duration: (FRAMES - 1) as f32 / 30.0,
            num_transform_tracks: 1,
            num_float_tracks: 0,
            annotation_tracks: vec![],
            num_frames: FRAMES,
            num_blocks: 1,
            max_frames_per_block: 256,
            mask_and_quantization_size: 4,
            block_duration: 256.0 / 30.0,
            block_inverse_duration: 30.0 / 256.0,
            frame_duration: 1.0 / 30.0,
            block_offsets: vec![0],
            float_block_offsets: vec![0],
            transform_offsets: vec![4],
            float_offsets: vec![],
            data: bytes,
            endian: 0,
            extracted_motion: None,
        };
        Clip::from_parts("modifier-pose", animation, &[0]).unwrap()
    }

    fn anm_14_rotation_graph(enable: bool) -> BehaviorGraph {
        BehaviorGraph {
            name: "rotate character".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData::default(),
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: Some(NodeId(2)),
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "hkbRotateCharacterModifier".into(),
                    name: "rotate".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::RotateCharacterModifier {
                        enable,
                        degrees_per_second: 45.0,
                        speed_multiplier: 2.0,
                        axis_of_rotation: [0.0, 0.0, 1.0, 0.0],
                    },
                },
                Node {
                    id: NodeId(2),
                    offset: 2,
                    class: "hkbClipGenerator".into(),
                    name: "empty".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::Clip(ClipGenerator::default()),
                },
            ],
        }
    }

    fn direct_at_test_graph(config: NodeKind) -> BehaviorGraph {
        BehaviorGraph {
            name: "direct at".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData::default(),
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: Some(NodeId(2)),
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "BSDirectAtModifier".into(),
                    name: "direct".into(),
                    user_data: 0,
                    bindings: None,
                    kind: config,
                },
                Node {
                    id: NodeId(2),
                    offset: 2,
                    class: "hkbClipGenerator".into(),
                    name: "pose".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::Clip(ClipGenerator {
                        animation_name: "modifier-pose".into(),
                        ..Default::default()
                    }),
                },
            ],
        }
    }

    #[test]
    fn anm_14_enabled_modifier_rotates_root_by_elapsed_time_only_while_enabled() {
        let graph = anm_14_rotation_graph(true);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);

        let first = vm.update(0.5).pose[0].rotation;
        assert!((first[2] - (std::f32::consts::FRAC_PI_8).sin()).abs() < 1e-5);
        assert!((first[3] - (std::f32::consts::FRAC_PI_8).cos()).abs() < 1e-5);

        let second = vm.update(0.5).pose[0].rotation;
        assert!((second[2] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
        assert!((second[3] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);

        vm.nodes[1].enabled = Some(false);
        let disabled = vm.update(1.0).pose[0].rotation;
        assert_eq!(disabled, QsTransform::IDENTITY.rotation);

        vm.nodes[1].enabled = Some(true);
        assert_eq!(vm.update(0.5).pose[0].rotation, first);
    }

    #[test]
    fn anm_14_disabled_modifier_leaves_root_rotation_unchanged() {
        let graph = anm_14_rotation_graph(false);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
        assert_eq!(
            vm.update(2.0).pose[0].rotation,
            QsTransform::IDENTITY.rotation
        );
    }

    fn direct_at_config() -> NodeKind {
        NodeKind::DirectAt {
            enable: true,
            direct_at_target: true,
            source_bone_index: 0,
            start_bone_index: 1,
            end_bone_index: 2,
            limit_heading_degrees: 30.0,
            limit_pitch_degrees: 20.0,
            offset_heading_degrees: 5.0,
            offset_pitch_degrees: -5.0,
            on_gain: 100.0,
            off_gain: 100.0,
            target_location: [10.0, 0.0, 10.0, 0.0],
            user_info: 0,
            direct_at_camera: false,
            direct_at_camera_location: [0.0; 3],
            active: false,
            current_heading_offset: 0.0,
            current_pitch_offset: 0.0,
        }
    }

    fn direct_at_fixture(config: NodeKind) -> (BehaviorGraph, ClipLibrary) {
        let mut clips = ClipLibrary::new();
        clips.insert(modifier_generator_clip());
        (direct_at_test_graph(config), clips)
    }

    #[test]
    fn anm_25_direct_at_rotates_the_declared_chain_with_heading_and_pitch_limits() {
        let (graph, clips) = direct_at_fixture(direct_at_config());
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        vm.update(1.0);
        let pose = &vm.output().pose;
        assert_eq!(
            pose[0].translation[0], 4.0,
            "the source pose remains intact"
        );
        assert_eq!(pose[1].translation, QsTransform::IDENTITY.translation);
        assert_eq!(pose[1].scale, QsTransform::IDENTITY.scale);
        let (yaw, pitch) = (7.5f32.to_radians(), 5.0f32.to_radians());
        let expected = [
            pitch.sin() * yaw.cos(),
            pitch.sin() * yaw.sin(),
            pitch.cos() * yaw.sin(),
            pitch.cos() * yaw.cos(),
        ];
        for (actual, expected) in pose[1].rotation.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
        }
        assert_eq!(
            pose[1].rotation, pose[2].rotation,
            "the declared chain shares the rotation"
        );
        assert_eq!(
            pose[3].rotation,
            QsTransform::IDENTITY.rotation,
            "bones outside the chain stay unchanged"
        );
    }

    #[test]
    fn anm_25_direct_at_gain_smooths_target_and_releases_when_target_is_off() {
        let mut config = direct_at_config();
        if let NodeKind::DirectAt { on_gain, .. } = &mut config {
            *on_gain = 0.5;
        }
        let (graph, clips) = direct_at_fixture(config);
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        vm.update(1.0);
        let half_gain = vm.output().pose[1].rotation;
        assert!(
            half_gain[2] > 0.06 && half_gain[2] < 0.07,
            "gain moves halfway to the limited angle: {half_gain:?}"
        );

        let mut released = direct_at_config();
        if let NodeKind::DirectAt {
            direct_at_target,
            direct_at_camera,
            ..
        } = &mut released
        {
            *direct_at_target = false;
            *direct_at_camera = false;
        }
        let (graph, clips) = direct_at_fixture(released);
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        vm.nodes[1].direct_at_heading = 30.0;
        vm.nodes[1].direct_at_pitch = 20.0;
        vm.update(1.0);
        assert_eq!(vm.output().pose[1].rotation, QsTransform::IDENTITY.rotation);
    }

    #[test]
    fn anm_25_direct_at_camera_enable_and_bad_chain_follow_declared_fields() {
        let mut config = direct_at_config();
        if let NodeKind::DirectAt {
            direct_at_camera,
            direct_at_target,
            direct_at_camera_location,
            target_location,
            ..
        } = &mut config
        {
            *direct_at_camera = true;
            *direct_at_target = false;
            *direct_at_camera_location = [0.0, 10.0, 0.0];
            *target_location = [10.0, 0.0, 10.0, 0.0];
        }
        let (graph, clips) = direct_at_fixture(config);
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        vm.update(1.0);
        assert!(
            vm.output().pose[1].rotation[2] < 0.0,
            "camera target takes precedence over the authored location"
        );

        let mut bad_chain = direct_at_config();
        if let NodeKind::DirectAt { end_bone_index, .. } = &mut bad_chain {
            *end_bone_index = 99;
        }
        let (graph, clips) = direct_at_fixture(bad_chain);
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        vm.update(1.0);
        assert_eq!(vm.output().pose[1].rotation, QsTransform::IDENTITY.rotation);

        let mut disabled = direct_at_config();
        if let NodeKind::DirectAt { enable, .. } = &mut disabled {
            *enable = false;
        }
        let (graph, clips) = direct_at_fixture(disabled);
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        vm.update(1.0);
        assert_eq!(vm.output().pose[1].rotation, QsTransform::IDENTITY.rotation);
    }

    /// A graph of one `BSIsActiveModifier` with a binding on one slot, so the
    /// output path can be driven without a packfile.
    fn is_active_graph(invert: bool, bound_slot: usize) -> BehaviorGraph {
        let mut invert_active = [false; IS_ACTIVE_SLOTS];
        // `bound_slot` may be out of range on purpose, to check the binding
        // is refused rather than clamped.
        if let Some(slot) = invert_active.get_mut(bound_slot) {
            *slot = invert;
        }
        BehaviorGraph {
            name: "g".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: vec!["bRunning".into()],
                variable_infos: vec![VariableInfo::default()],
                ..Default::default()
            },
            nodes: vec![Node {
                id: NodeId(0),
                offset: 0,
                class: "BSIsActiveModifier".into(),
                name: "mod".into(),
                user_data: 0,
                bindings: Some(VariableBindingSet {
                    bindings: vec![Binding {
                        member_path: format!("bIsActive{bound_slot}"),
                        variable_index: 0,
                        bit_index: -1,
                        binding_type: 0,
                    }],
                    index_of_binding_to_enable: -1,
                }),
                kind: NodeKind::IsActiveModifier {
                    enable: true,
                    is_active: [false; IS_ACTIVE_SLOTS],
                    invert_active,
                },
            }],
        }
    }

    fn time_step_node(id: usize, name: &str, variable: i32, enable: bool) -> Node {
        Node {
            id: NodeId(id),
            offset: u32::try_from(id).unwrap(),
            class: "BSGetTimeStepModifier".into(),
            name: name.into(),
            user_data: 0,
            bindings: Some(VariableBindingSet {
                bindings: vec![Binding {
                    member_path: "timeStep".into(),
                    variable_index: variable,
                    bit_index: -1,
                    binding_type: 0,
                }],
                index_of_binding_to_enable: -1,
            }),
            kind: NodeKind::GetTimeStepModifier {
                enable,
                time_step: 0.0,
            },
        }
    }

    fn time_step_graph(enable: bool) -> BehaviorGraph {
        BehaviorGraph {
            name: "time step".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: vec!["Elapsed".into()],
                variable_infos: vec![VariableInfo {
                    ty: VariableType::Real,
                    ..Default::default()
                }],
                ..Default::default()
            },
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                time_step_node(1, "time step", 0, enable),
            ],
        }
    }

    fn evaluate_expression_graph(
        expressions: Vec<ExpressionData>,
        variable_names: &[&str],
        variable_types: &[VariableType],
        event_names: &[&str],
    ) -> BehaviorGraph {
        let mut nodes = vec![Node {
            id: NodeId(0),
            offset: 0,
            class: "hkbModifierGenerator".into(),
            name: "wrapper".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::ModifierGenerator {
                modifier: Some(NodeId(1)),
                generator: None,
            },
        }];
        nodes.push(Node {
            id: NodeId(1),
            offset: 1,
            class: "hkbEvaluateExpressionModifier".into(),
            name: "evaluate".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::EvaluateExpression {
                enable: true,
                expressions,
            },
        });
        BehaviorGraph {
            name: "expressions".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: variable_names.iter().map(|name| (*name).into()).collect(),
                variable_infos: variable_types
                    .iter()
                    .map(|ty| VariableInfo {
                        ty: *ty,
                        ..Default::default()
                    })
                    .collect(),
                event_names: event_names.iter().map(|name| (*name).into()).collect(),
                ..Default::default()
            },
            nodes,
        }
    }

    fn bstimer_graph(enable: bool, alarm_time_seconds: f32, reset_alarm: bool) -> BehaviorGraph {
        BehaviorGraph {
            name: "timer".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                event_names: vec!["Alarm".into()],
                ..Default::default()
            },
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "BSTimerModifier".into(),
                    name: "alarm".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::BsTimerModifier {
                        enable,
                        alarm_time_seconds,
                        alarm_event: crate::behavior::EventProperty {
                            id: 0,
                            has_payload: false,
                        },
                        reset_alarm,
                    },
                },
            ],
        }
    }

    fn interp_value_graph(source: f32, target: f32, gain: f32) -> BehaviorGraph {
        let binding = |member_path: &str, variable_index| Binding {
            member_path: member_path.into(),
            variable_index,
            bit_index: -1,
            binding_type: 0,
        };
        BehaviorGraph {
            name: "interp value".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: vec!["Value".into(), "Target".into()],
                variable_infos: vec![
                    VariableInfo {
                        ty: VariableType::Real,
                        ..Default::default()
                    };
                    2
                ],
                ..Default::default()
            },
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "BSInterpValueModifier".into(),
                    name: "interpolation".into(),
                    user_data: 0,
                    bindings: Some(VariableBindingSet {
                        bindings: vec![
                            binding("source", 0),
                            binding("target", 1),
                            binding("result", 0),
                        ],
                        index_of_binding_to_enable: -1,
                    }),
                    kind: NodeKind::InterpValue {
                        enable: true,
                        source,
                        target,
                        result: source,
                        gain,
                    },
                },
            ],
        }
    }

    #[test]
    fn anm_30_interpolates_by_authored_rate_and_frame_delta() {
        let graph = interp_value_graph(1.0, 5.0, 2.0);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_real(0, 1.0);
        vm.variables.set_real(1, 5.0);

        vm.update(0.5);

        assert_eq!(vm.variables.real(0), Some(2.0));
    }

    #[test]
    fn anm_30_uses_authored_source_and_target_when_unbound() {
        let mut graph = interp_value_graph(1.0, 5.0, 2.0);
        graph.nodes[1]
            .bindings
            .as_mut()
            .unwrap()
            .bindings
            .retain(|binding| binding.member_path == "result");
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());

        vm.update(0.5);
        assert_eq!(vm.variables.real(0), Some(2.0));
        vm.update(0.5);
        assert_eq!(vm.variables.real(0), Some(3.0));
    }

    #[test]
    fn anm_30_clamps_upward_interpolation_at_its_target() {
        let graph = interp_value_graph(1.0, 1.5, 2.0);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_real(0, 1.0);
        vm.variables.set_real(1, 1.5);

        vm.update(0.5);

        assert_eq!(vm.variables.real(0), Some(1.5));
    }

    #[test]
    fn anm_30_interpolates_downward_and_clamps_at_its_target() {
        let graph = interp_value_graph(4.0, 2.0, 3.0);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_real(0, 4.0);
        vm.variables.set_real(1, 2.0);

        vm.update(1.0);

        assert_eq!(vm.variables.real(0), Some(2.0));
    }

    fn nested_time_step_graph() -> BehaviorGraph {
        BehaviorGraph {
            name: "nested time step".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: vec!["OuterElapsed".into(), "NestedElapsed".into()],
                variable_infos: vec![
                    VariableInfo {
                        ty: VariableType::Real,
                        ..Default::default()
                    },
                    VariableInfo {
                        ty: VariableType::Real,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "hkbModifierList".into(),
                    name: "outer list".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierList {
                        enable: true,
                        modifiers: vec![Some(NodeId(2)), Some(NodeId(3))],
                    },
                },
                time_step_node(2, "outer time step", 0, true),
                Node {
                    id: NodeId(3),
                    offset: 3,
                    class: "hkbModifierList".into(),
                    name: "nested list".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierList {
                        enable: true,
                        modifiers: vec![Some(NodeId(4))],
                    },
                },
                time_step_node(4, "nested time step", 1, true),
            ],
        }
    }

    fn modifier_generator_graph() -> BehaviorGraph {
        let mut graph = is_active_graph(false, 0);
        graph.root = Some(NodeId(0));
        graph.data.variable_names.push("Enabled".into());
        graph.data.variable_infos.push(VariableInfo::default());
        graph.nodes[0].id = NodeId(1);
        graph.nodes[0]
            .bindings
            .as_mut()
            .unwrap()
            .bindings
            .push(Binding {
                member_path: "enable".into(),
                variable_index: 1,
                bit_index: -1,
                binding_type: 0,
            });
        graph.nodes.insert(
            0,
            Node {
                id: NodeId(0),
                offset: 0,
                class: "hkbModifierGenerator".into(),
                name: "wrapper".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::ModifierGenerator {
                    modifier: Some(NodeId(1)),
                    generator: Some(NodeId(2)),
                },
            },
        );
        graph.nodes.push(Node {
            id: NodeId(2),
            offset: 2,
            class: "hkbClipGenerator".into(),
            name: "pose".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::Clip(ClipGenerator {
                animation_name: "modifier-pose".into(),
                ..Default::default()
            }),
        });
        graph
    }

    fn twist_graph(enable: bool) -> BehaviorGraph {
        let mut graph = modifier_generator_graph();
        graph.nodes[1].class = "hkbTwistModifier".into();
        graph.nodes[1].name = "authored twist".into();
        graph.nodes[1].kind = NodeKind::TwistModifier(crate::behavior::TwistModifier {
            enable,
            axis_of_rotation: [0.0, 0.0, 1.0, 0.0],
            twist_angle: std::f32::consts::PI,
            start_bone_index: 1,
            end_bone_index: 2,
            set_angle_method: 0,
            rotation_axis_coordinates: 1,
            is_additive: true,
        });
        graph
    }

    fn is_active_node(
        id: usize,
        name: &str,
        output_variable: usize,
        enable_variable: Option<usize>,
    ) -> Node {
        let mut bindings = vec![Binding {
            member_path: "bIsActive0".into(),
            variable_index: i32::try_from(output_variable).unwrap(),
            bit_index: -1,
            binding_type: 0,
        }];
        if let Some(variable) = enable_variable {
            bindings.push(Binding {
                member_path: "enable".into(),
                variable_index: i32::try_from(variable).unwrap(),
                bit_index: -1,
                binding_type: 0,
            });
        }
        Node {
            id: NodeId(id),
            offset: u32::try_from(id).unwrap(),
            class: "BSIsActiveModifier".into(),
            name: name.into(),
            user_data: 0,
            bindings: Some(VariableBindingSet {
                bindings,
                index_of_binding_to_enable: -1,
            }),
            kind: NodeKind::IsActiveModifier {
                enable: true,
                is_active: [false; IS_ACTIVE_SLOTS],
                invert_active: [false; IS_ACTIVE_SLOTS],
            },
        }
    }

    fn modifier_list_graph(list_enable: bool, bind_list_enable: bool) -> BehaviorGraph {
        let list_bindings = bind_list_enable.then(|| VariableBindingSet {
            bindings: vec![Binding {
                member_path: "enable".into(),
                variable_index: 2,
                bit_index: -1,
                binding_type: 0,
            }],
            index_of_binding_to_enable: -1,
        });
        BehaviorGraph {
            name: "modifier-list".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: vec![
                    "first active".into(),
                    "second active".into(),
                    "list enabled".into(),
                    "second enabled".into(),
                    "nested active".into(),
                ],
                variable_infos: vec![VariableInfo::default(); 5],
                ..Default::default()
            },
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: Some(NodeId(6)),
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "hkbModifierList".into(),
                    name: "outer".into(),
                    user_data: 0,
                    bindings: list_bindings,
                    // This order intentionally differs from the arena order.
                    kind: NodeKind::ModifierList {
                        enable: list_enable,
                        modifiers: vec![Some(NodeId(3)), Some(NodeId(2)), Some(NodeId(4))],
                    },
                },
                is_active_node(2, "second", 1, Some(3)),
                is_active_node(3, "first", 0, None),
                Node {
                    id: NodeId(4),
                    offset: 4,
                    class: "hkbModifierList".into(),
                    name: "nested".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierList {
                        enable: true,
                        modifiers: vec![Some(NodeId(5))],
                    },
                },
                is_active_node(5, "nested child", 4, None),
                Node {
                    id: NodeId(6),
                    offset: 6,
                    class: "hkbClipGenerator".into(),
                    name: "pose".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::Clip(ClipGenerator {
                        animation_name: "modifier-pose".into(),
                        ..Default::default()
                    }),
                },
            ],
        }
    }

    fn decompose_node(
        id: usize,
        name: &str,
        enable: bool,
        vector: [f32; 4],
        bind_vector: bool,
        output_variables: [usize; 4],
    ) -> Node {
        let mut bindings = ["x", "y", "z", "w"]
            .into_iter()
            .zip(output_variables)
            .map(|(member_path, variable_index)| Binding {
                member_path: member_path.into(),
                variable_index: i32::try_from(variable_index).unwrap(),
                bit_index: -1,
                binding_type: 0,
            })
            .collect::<Vec<_>>();
        if bind_vector {
            bindings.insert(
                0,
                Binding {
                    member_path: "vector".into(),
                    variable_index: 4,
                    bit_index: -1,
                    binding_type: 0,
                },
            );
        }
        Node {
            id: NodeId(id),
            offset: u32::try_from(id).unwrap(),
            class: "BSDecomposeVectorModifier".into(),
            name: name.into(),
            user_data: 0,
            bindings: Some(VariableBindingSet {
                bindings,
                index_of_binding_to_enable: -1,
            }),
            kind: NodeKind::DecomposeVector { enable, vector },
        }
    }

    fn decompose_graph(enable: bool, bind_vector: bool) -> BehaviorGraph {
        BehaviorGraph {
            name: "decompose vector".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: vec![
                    "x".into(),
                    "y".into(),
                    "z".into(),
                    "w".into(),
                    "input".into(),
                ],
                variable_infos: vec![
                    VariableInfo {
                        ty: VariableType::Real,
                        ..Default::default()
                    },
                    VariableInfo {
                        ty: VariableType::Real,
                        ..Default::default()
                    },
                    VariableInfo {
                        ty: VariableType::Real,
                        ..Default::default()
                    },
                    VariableInfo {
                        ty: VariableType::Real,
                        ..Default::default()
                    },
                    VariableInfo {
                        ty: VariableType::Vector4,
                        ..Default::default()
                    },
                ],
                variable_initial_values: Some(VariableValueSet {
                    word_values: vec![0, 0, 0, 0, 0],
                    quad_values: vec![[1.25, -2.5, 3.75, -4.0]],
                    variant_values: Vec::new(),
                }),
                ..Default::default()
            },
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                decompose_node(
                    1,
                    "decompose",
                    enable,
                    [9.0, 8.0, 7.0, 6.0],
                    bind_vector,
                    [0, 1, 2, 3],
                ),
            ],
        }
    }

    fn damping_graph(kp: f32, ki: f32, kd: f32) -> BehaviorGraph {
        let bindings = [
            ("rawValue", 0, 0),
            ("dampedValue", 1, 0),
            ("rawVector", 2, 0),
            ("dampedVector", 3, 0),
        ]
        .into_iter()
        .map(|(member_path, variable_index, binding_type)| Binding {
            member_path: member_path.into(),
            variable_index,
            bit_index: -1,
            binding_type,
        })
        .collect();
        BehaviorGraph {
            name: "damping".into(),
            variable_mode: 1,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: vec![
                    "Target".into(),
                    "Damped".into(),
                    "VectorTarget".into(),
                    "DampedVector".into(),
                ],
                variable_infos: vec![
                    VariableInfo {
                        ty: VariableType::Real,
                        ..Default::default()
                    },
                    VariableInfo {
                        ty: VariableType::Real,
                        ..Default::default()
                    },
                    VariableInfo {
                        ty: VariableType::Vector4,
                        ..Default::default()
                    },
                    VariableInfo {
                        ty: VariableType::Vector4,
                        ..Default::default()
                    },
                ],
                variable_initial_values: Some(VariableValueSet {
                    word_values: vec![10.0f32.to_bits() as i32, 0, 0, 1],
                    quad_values: vec![[8.0, -4.0, 2.0, 1.0], [0.0; 4]],
                    variant_values: Vec::new(),
                }),
                ..Default::default()
            },
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "hkbDampingModifier".into(),
                    name: "damping".into(),
                    user_data: 0,
                    bindings: Some(VariableBindingSet {
                        bindings,
                        index_of_binding_to_enable: -1,
                    }),
                    kind: NodeKind::DampingModifier {
                        enable: true,
                        kp,
                        ki,
                        kd,
                        enable_scalar: true,
                        enable_vector: true,
                        raw_value: 0.0,
                        damped_value: 0.0,
                        raw_vector: [0.0; 4],
                        damped_vector: [0.0; 4],
                        vector_error_sum: [0.0; 4],
                        vector_previous_error: [0.0; 4],
                        error_sum: 0.0,
                        previous_error: 0.0,
                    },
                },
            ],
        }
    }

    fn look_at_graph(
        enable: bool,
        bones: Vec<LookAtBone>,
        eye_bones: Vec<LookAtBone>,
        global_limit_degrees: f32,
    ) -> BehaviorGraph {
        BehaviorGraph {
            name: "look at".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData::default(),
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "BSLookAtModifier".into(),
                    name: "look".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::LookAtModifier {
                        enable,
                        bones,
                        eye_bones,
                        limit_angle_degrees: global_limit_degrees,
                        on_gain: 1.0,
                        off_gain: 0.0,
                        use_bone_gains: false,
                        target_location: [0.0, 10.0, 0.0, 0.0],
                        look_at_target: true,
                        look_at_camera: false,
                        look_at_camera_location: [0.0; 3],
                    },
                },
            ],
        }
    }

    #[test]
    fn anm_39_changing_scalar_input_follows_the_authored_damping_curve() {
        let graph = damping_graph(2.0, 0.0, 0.0);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());

        vm.update(0.25);
        assert_eq!(vm.variables.real(1), Some(5.0));
        vm.update(0.25);
        assert_eq!(vm.variables.real(1), Some(7.5));

        vm.variables.set_real(0, 20.0);
        vm.update(0.25);
        assert_eq!(vm.variables.real(1), Some(13.75));
    }

    #[test]
    fn anm_39_vector_damping_and_disabled_damping_keep_their_contracts() {
        let graph = damping_graph(1.0, 0.0, 0.0);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());

        vm.update(0.25);
        assert_eq!(vm.variables.quad(3), Some([2.0, -1.0, 0.5, 0.25]));
        assert_eq!(vm.variables.real(1), Some(2.5));

        vm.variables.set_real(0, 20.0);
        if let Some(damping) = vm.nodes[1].damping.as_mut() {
            damping.enable_scalar = false;
            damping.enable_vector = false;
        }
        vm.update(0.25);
        assert_eq!(vm.variables.real(1), Some(2.5));
        assert_eq!(vm.variables.quad(3), Some([2.0, -1.0, 0.5, 0.25]));

        if let Some(damping) = vm.nodes[1].damping.as_mut() {
            damping.enable_scalar = true;
            damping.enable_vector = true;
        }
        vm.nodes[1].enabled = Some(false);
        vm.update(0.25);
        assert_eq!(vm.variables.real(1), Some(2.5));
        assert_eq!(vm.variables.quad(3), Some([2.0, -1.0, 0.5, 0.25]));
    }

    #[test]
    fn anm_39_integral_gain_accumulates_error_over_frame_time() {
        let graph = damping_graph(0.0, 1.0, 0.0);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());

        vm.update(0.25);

        assert_eq!(vm.variables.real(1), Some(0.625));
    }

    #[test]
    fn anm_39_derivative_gain_uses_the_change_in_error() {
        let graph = damping_graph(0.0, 0.0, 0.5);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());

        vm.update(0.25);

        assert_eq!(vm.variables.real(1), Some(5.0));
    }

    #[test]
    fn anm_39_zero_or_invalid_time_does_not_advance_damping_history() {
        let graph = damping_graph(2.0, 1.0, 0.5);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());

        vm.update(0.0);
        assert_eq!(vm.variables.real(1), Some(0.0));
        vm.update(-0.25);
        assert_eq!(vm.variables.real(1), Some(0.0));
        vm.update(f32::NAN);
        assert_eq!(vm.variables.real(1), Some(0.0));
        vm.update(0.25);
        assert!(vm.variables.real(1).is_some_and(f32::is_finite));
    }

    #[derive(Clone, Copy)]
    struct TransformOptions {
        enable: bool,
        inverse: bool,
        rotate_only: bool,
        compute_on_activate: bool,
        compute_on_modify: bool,
    }

    impl Default for TransformOptions {
        fn default() -> Self {
            Self {
                enable: true,
                inverse: false,
                rotate_only: false,
                compute_on_activate: true,
                compute_on_modify: true,
            }
        }
    }

    fn transform_vector_graph(options: TransformOptions) -> BehaviorGraph {
        let rotation = [
            0.0,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ];
        BehaviorGraph {
            name: "transform vector".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: vec!["input".into(), "output".into()],
                variable_infos: vec![
                    VariableInfo {
                        ty: VariableType::Vector4,
                        ..Default::default()
                    },
                    VariableInfo {
                        ty: VariableType::Vector4,
                        ..Default::default()
                    },
                ],
                variable_initial_values: Some(VariableValueSet {
                    word_values: vec![0, 1],
                    quad_values: vec![[1.0, 0.0, 0.0, 7.0], [0.0; 4]],
                    variant_values: Vec::new(),
                }),
                ..Default::default()
            },
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "hkbTransformVectorModifier".into(),
                    name: "transform".into(),
                    user_data: 0,
                    bindings: Some(VariableBindingSet {
                        bindings: vec![
                            Binding {
                                member_path: "vectorIn".into(),
                                variable_index: 0,
                                bit_index: -1,
                                binding_type: 0,
                            },
                            Binding {
                                member_path: "vectorOut".into(),
                                variable_index: 1,
                                bit_index: -1,
                                binding_type: 0,
                            },
                        ],
                        index_of_binding_to_enable: -1,
                    }),
                    kind: NodeKind::TransformVector {
                        enable: options.enable,
                        rotation,
                        translation: [10.0, 0.0, 0.0, 0.0],
                        vector_in: [99.0; 4],
                        vector_out: [5.0; 4],
                        rotate_only: options.rotate_only,
                        inverse: options.inverse,
                        compute_on_activate: options.compute_on_activate,
                        compute_on_modify: options.compute_on_modify,
                    },
                },
            ],
        }
    }

    fn look_bone(index: i16, enabled: bool, limit_angle_degrees: f32) -> LookAtBone {
        LookAtBone {
            index,
            forward_axis: [0.0, 0.0, 1.0, 0.0],
            limit_angle_degrees,
            on_gain: 1.0,
            off_gain: 0.0,
            enabled,
        }
    }

    fn look_at_vm<'a>(
        graph: &'a BehaviorGraph,
        clips: &'a ClipLibrary,
        pose: Vec<QsTransform>,
    ) -> Interpreter<'a> {
        Interpreter::new(graph, clips, pose)
    }

    #[test]
    fn anm_32_rotates_only_enabled_authored_look_at_bones() {
        let graph = look_at_graph(
            true,
            vec![look_bone(0, true, 45.0), look_bone(1, false, 45.0)],
            vec![look_bone(2, true, 45.0)],
            45.0,
        );
        let clips = ClipLibrary::new();
        let mut vm = look_at_vm(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        let output = vm.update(1.0 / 60.0);

        assert!(output.pose[0].rotation[0].abs() > 0.1);
        assert_eq!(output.pose[1].rotation, QsTransform::IDENTITY.rotation);
        assert!(output.pose[2].rotation[0].abs() > 0.1);
        assert_eq!(output.pose[3].rotation, QsTransform::IDENTITY.rotation);
    }

    #[test]
    fn anm_32_uses_the_smaller_global_and_bone_angle_limit() {
        let graph = look_at_graph(true, vec![look_bone(0, true, 10.0)], Vec::new(), 30.0);
        let clips = ClipLibrary::new();
        let mut vm = look_at_vm(&graph, &clips, vec![QsTransform::IDENTITY]);
        let output = vm.update(1.0 / 60.0);
        let forward = quat_rotate3(output.pose[0].rotation, [0.0, 0.0, 1.0]);
        let angle = forward[1].atan2(forward[2]).to_degrees();

        assert!((angle - 10.0).abs() < 0.01, "{angle}");
    }

    #[test]
    fn anm_32_respects_the_global_limit_when_the_bone_limit_is_larger() {
        let graph = look_at_graph(true, vec![look_bone(0, true, 40.0)], Vec::new(), 15.0);
        let clips = ClipLibrary::new();
        let mut vm = look_at_vm(&graph, &clips, vec![QsTransform::IDENTITY]);
        let output = vm.update(1.0 / 60.0);
        let forward = quat_rotate3(output.pose[0].rotation, [0.0, 0.0, 1.0]);
        let angle = forward[1].atan2(forward[2]).to_degrees();

        assert!((angle - 15.0).abs() < 0.01, "{angle}");
    }

    #[test]
    fn anm_32_disabled_modifier_leaves_every_bone_unchanged() {
        let graph = look_at_graph(false, vec![look_bone(0, true, 45.0)], Vec::new(), 45.0);
        let clips = ClipLibrary::new();
        let mut vm = look_at_vm(&graph, &clips, vec![QsTransform::IDENTITY]);
        let output = vm.update(1.0 / 60.0);

        assert_eq!(output.pose[0], QsTransform::IDENTITY);
    }

    fn assert_vector_close(actual: Option<[f32; 4]>, expected: [f32; 4]) {
        let actual = actual.unwrap();
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 0.00001);
        }
    }

    #[test]
    fn anm_47_authored_rotation_and_translation_map_the_bound_input_vector() {
        let graph = transform_vector_graph(TransformOptions::default());
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_vector_close(vm.variables.quad(1), [10.0, 1.0, 0.0, 7.0]);
        assert!(vm.unsupported().unbound.is_empty());
    }

    #[test]
    fn anm_47_inverse_transform_removes_translation_before_inverse_rotation() {
        let graph = transform_vector_graph(TransformOptions {
            inverse: true,
            ..Default::default()
        });
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.variables.set_quad(0, [10.0, 1.0, 0.0, 3.0]);
        vm.update(1.0 / 60.0);
        assert_vector_close(vm.variables.quad(1), [1.0, 0.0, 0.0, 3.0]);
    }

    #[test]
    fn anm_47_rotate_only_omits_translation_and_disabled_modifier_preserves_output() {
        let graph = transform_vector_graph(TransformOptions {
            rotate_only: true,
            ..Default::default()
        });
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_vector_close(vm.variables.quad(1), [0.0, 1.0, 0.0, 7.0]);

        let graph = transform_vector_graph(TransformOptions {
            enable: false,
            ..Default::default()
        });
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.quad(1), Some([0.0; 4]));
    }

    #[test]
    fn anm_47_compute_flags_control_activation_and_modify_recalculation() {
        let graph = transform_vector_graph(TransformOptions {
            compute_on_activate: false,
            compute_on_modify: false,
            ..Default::default()
        });
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.quad(1), Some([5.0; 4]));

        let graph = transform_vector_graph(TransformOptions {
            compute_on_modify: false,
            ..Default::default()
        });
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_vector_close(vm.variables.quad(1), [10.0, 1.0, 0.0, 7.0]);
        vm.variables.set_quad(0, [0.0, 1.0, 0.0, 2.0]);
        vm.update(1.0 / 60.0);
        assert_vector_close(vm.variables.quad(1), [10.0, 1.0, 0.0, 7.0]);
    }

    #[test]
    fn decompose_vector_uses_authored_and_bound_inputs_for_all_components() {
        let lib = ClipLibrary::new();
        let authored = decompose_graph(true, false);
        let mut vm = Interpreter::new(&authored, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_eq!(
            [
                vm.variables.real(0),
                vm.variables.real(1),
                vm.variables.real(2),
                vm.variables.real(3),
            ],
            [Some(9.0), Some(8.0), Some(7.0), Some(6.0)]
        );

        let bound = decompose_graph(true, true);
        let mut vm = Interpreter::new(&bound, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_eq!(
            [
                vm.variables.real(0),
                vm.variables.real(1),
                vm.variables.real(2),
                vm.variables.real(3),
            ],
            [Some(1.25), Some(-2.5), Some(3.75), Some(-4.0)]
        );
        assert!(vm.unsupported().unbound.is_empty());
    }

    #[test]
    fn anm_57_linked_child_quad_default_drives_bound_vector_modifier() {
        let mut parent = BehaviorGraph {
            name: "parent with a quad default".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData {
                variable_names: vec!["parent vector".into()],
                variable_infos: vec![VariableInfo {
                    ty: VariableType::Vector4,
                    ..Default::default()
                }],
                variable_initial_values: Some(VariableValueSet {
                    word_values: vec![0],
                    quad_values: vec![[90.0, 80.0, 70.0, 60.0]],
                    variant_values: Vec::new(),
                }),
                ..Default::default()
            },
            nodes: vec![Node {
                id: NodeId(0),
                offset: 0,
                class: "hkbBehaviorReferenceGenerator".into(),
                name: "child graph".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::BehaviorReference {
                    behavior_name: "child.hkx".into(),
                    resolved: None,
                },
            }],
        };
        parent
            .link_reference(NodeId(0), decompose_graph(true, true))
            .unwrap();

        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&parent, &clips, Vec::new());
        vm.update(1.0 / 60.0);

        assert_eq!(
            [
                vm.variables.real(1),
                vm.variables.real(2),
                vm.variables.real(3),
                vm.variables.real(4),
            ],
            [Some(1.25), Some(-2.5), Some(3.75), Some(-4.0)]
        );
    }

    #[test]
    fn a_disabled_decompose_vector_modifier_preserves_its_outputs() {
        let graph = decompose_graph(false, true);
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.variables.set_real(0, 10.0);
        vm.variables.set_real(1, 20.0);
        vm.variables.set_real(2, 30.0);
        vm.variables.set_real(3, 40.0);
        vm.update(1.0 / 60.0);
        assert!(!vm.is_active(NodeId(1)));
        assert_eq!(
            [
                vm.variables.real(0),
                vm.variables.real(1),
                vm.variables.real(2),
                vm.variables.real(3),
            ],
            [Some(10.0), Some(20.0), Some(30.0), Some(40.0)]
        );
    }

    #[test]
    fn nested_modifier_lists_publish_decomposed_components_in_serialized_order() {
        let mut graph = decompose_graph(true, false);
        graph.nodes[0].kind = NodeKind::ModifierGenerator {
            modifier: Some(NodeId(1)),
            generator: None,
        };
        graph.nodes[1] = Node {
            id: NodeId(1),
            offset: 1,
            class: "hkbModifierList".into(),
            name: "outer".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::ModifierList {
                enable: true,
                modifiers: vec![Some(NodeId(2)), Some(NodeId(3))],
            },
        };
        graph.nodes.push(Node {
            id: NodeId(2),
            offset: 2,
            class: "hkbModifierList".into(),
            name: "nested".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::ModifierList {
                enable: true,
                modifiers: vec![Some(NodeId(4))],
            },
        });
        graph.nodes.push(decompose_node(
            3,
            "later",
            true,
            [2.0, 0.0, 0.0, 0.0],
            false,
            [0, 1, 2, 3],
        ));
        graph.nodes.push(decompose_node(
            4,
            "nested first",
            true,
            [1.0, 0.0, 0.0, 0.0],
            false,
            [0, 1, 2, 3],
        ));

        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.real(0), Some(2.0));
        assert_eq!(
            graph.node(NodeId(1)).children(),
            vec![NodeId(2), NodeId(3)],
            "the later serialized modifier overwrites the nested output"
        );
    }

    #[test]
    fn modifier_generator_activates_outputs_deactivates_and_preserves_pose() {
        let graph = modifier_generator_graph();
        let pose = vec![QsTransform::IDENTITY];
        let mut clips = ClipLibrary::new();
        clips.insert(modifier_generator_clip());
        let mut vm = Interpreter::new(&graph, &clips, pose);
        vm.variables.set_word(1, 1);
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.word(0), Some(1));
        assert_eq!(vm.output().pose[0].translation[0], 4.0);
        vm.variables.set_word(1, 0);
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.word(0), Some(0));
        assert_eq!(vm.output().pose[0].translation[0], 4.0);
        vm.variables.set_word(1, 1);
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.word(0), Some(1));
        vm.deactivate(NodeId(0));
        vm.apply_output_bindings();
        assert_eq!(vm.variables.word(0), Some(0));
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.word(0), Some(1));
    }

    #[test]
    fn anm_42_ground_target_adjusts_declared_ankle_and_preserves_body_pose() {
        let mut graph = modifier_generator_graph();
        graph.nodes[1].bindings = None;
        graph.nodes[1].kind = NodeKind::FootIk {
            enable: true,
            legs: vec![
                FootIkLeg {
                    hip_index: 0,
                    knee_index: 1,
                    ankle_index: 2,
                    max_ankle_height: 1.0,
                    min_ankle_height: -1.0,
                },
                FootIkLeg {
                    hip_index: 0,
                    knee_index: 1,
                    ankle_index: 3,
                    max_ankle_height: 1.0,
                    min_ankle_height: -1.0,
                },
            ],
            raycast_distance_up: 0.8,
            raycast_distance_down: 0.5,
            vertical_offset: 0.25,
            ground_ascending_gain: 0.5,
            ground_descending_gain: 0.75,
        };
        let source_clip = modifier_generator_clip();
        let clip = Clip::from_parts("modifier-pose", source_clip.animation, &[2]).unwrap();
        let mut clips = ClipLibrary::new();
        clips.insert(clip);
        let mut pose = vec![QsTransform::IDENTITY; 4];
        pose[0].translation = [3.0, 4.0, 5.0, 0.0];
        pose[1].translation = [1.0, 2.0, 3.0, 0.0];
        let mut vm = Interpreter::new(&graph, &clips, pose);

        vm.update(1.0 / 60.0);
        let before = vm.output().pose.clone();
        vm.set_foot_ik_ground_targets(&[
            FootIkGroundTarget {
                ankle_index: 2,
                vertical_correction: 0.75,
            },
            FootIkGroundTarget {
                ankle_index: 3,
                vertical_correction: -0.4,
            },
        ]);
        vm.update(1.0 / 60.0);

        assert_eq!(
            vm.output().pose[0],
            before[0],
            "root body transform changed"
        );
        assert_eq!(vm.output().pose[1], before[1], "knee transform changed");
        assert_eq!(vm.output().pose[2].translation[0], before[2].translation[0]);
        assert_eq!(vm.output().pose[2].translation[1], before[2].translation[1]);
        let upward = vm.output().pose[2].translation[2] - before[2].translation[2];
        assert!((upward - 0.4).abs() < 0.0001, "upward correction: {upward}");
        let downward = vm.output().pose[3].translation[2] - before[3].translation[2];
        assert!(
            (downward + 0.1125).abs() < 0.0001,
            "downward correction: {downward}"
        );

        vm.update(1.0 / 60.0);
        assert_eq!(
            vm.output().pose[0],
            before[0],
            "body pose drifted across frames"
        );
    }

    #[test]
    fn anm_42_disabled_modifier_ignores_ground_targets() {
        let mut graph = modifier_generator_graph();
        graph.nodes[1].bindings = None;
        graph.nodes[1].kind = NodeKind::FootIk {
            enable: false,
            legs: vec![FootIkLeg {
                hip_index: 0,
                knee_index: 1,
                ankle_index: 0,
                max_ankle_height: 1.0,
                min_ankle_height: -1.0,
            }],
            raycast_distance_up: 1.0,
            raycast_distance_down: 1.0,
            vertical_offset: 0.0,
            ground_ascending_gain: 1.0,
            ground_descending_gain: 1.0,
        };
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
        vm.set_foot_ik_ground_targets(&[FootIkGroundTarget {
            ankle_index: 0,
            vertical_correction: 1.0,
        }]);
        vm.update(1.0 / 60.0);
        assert!(!vm.is_active(NodeId(1)));
        assert_eq!(vm.output().pose[0], QsTransform::IDENTITY);
    }

    #[test]
    fn modifier_generator_does_not_activate_a_disabled_modifier() {
        let graph = modifier_generator_graph();
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_word(1, 0);
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.word(0), Some(0));
    }

    #[test]
    fn anm_48_authored_twist_distributes_rotation_only_across_declared_chain() {
        let graph = twist_graph(true);
        let mut clips = ClipLibrary::new();
        clips.insert(modifier_generator_clip());
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        vm.variables.set_word(1, 1);
        let output = vm.update(1.0 / 60.0);
        let half = std::f32::consts::FRAC_PI_4;
        let expected = [0.0, 0.0, half.sin(), half.cos()];
        assert_eq!(output.pose[0].rotation, QsTransform::IDENTITY.rotation);
        assert_eq!(output.pose[1].rotation, expected);
        assert_eq!(output.pose[2].rotation, expected);
        assert_eq!(output.pose[3].rotation, QsTransform::IDENTITY.rotation);
        assert_eq!(output.pose[0].translation[0], 4.0);
    }

    #[test]
    fn anm_48_disabled_twist_leaves_wrapped_generator_pose_unchanged() {
        let graph = twist_graph(false);
        let mut clips = ClipLibrary::new();
        clips.insert(modifier_generator_clip());
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        let output = vm.update(1.0 / 60.0);
        assert_eq!(output.pose[1].rotation, QsTransform::IDENTITY.rotation);
        assert_eq!(output.pose[2].rotation, QsTransform::IDENTITY.rotation);
    }

    #[test]
    fn anm_48_twist_angle_binding_overrides_the_authored_angle() {
        let mut graph = twist_graph(true);
        graph.data.variable_names.push("TwistAngle".into());
        graph.data.variable_infos.push(VariableInfo {
            ty: VariableType::Real,
            ..Default::default()
        });
        graph.nodes[1].bindings = Some(VariableBindingSet {
            bindings: vec![Binding {
                member_path: "twistAngle".into(),
                variable_index: 2,
                bit_index: -1,
                binding_type: 0,
            }],
            index_of_binding_to_enable: -1,
        });
        let mut clips = ClipLibrary::new();
        clips.insert(modifier_generator_clip());
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 4]);
        vm.variables
            .set_word(2, (std::f32::consts::FRAC_PI_2).to_bits() as i32);
        let output = vm.update(1.0 / 60.0);
        let half = std::f32::consts::FRAC_PI_8;
        let expected = [0.0, 0.0, half.sin(), half.cos()];
        assert_eq!(output.pose[1].rotation, expected);
        assert_eq!(output.pose[2].rotation, expected);
    }

    #[test]
    fn anm_48_ramped_twist_increases_each_chain_bone_increment() {
        let mut output = Output {
            pose: vec![QsTransform::IDENTITY; 4],
            world_from_model: QsTransform::IDENTITY,
            bone_weights: vec![1.0; 4],
            weight: 1.0,
            foot_ik_controls: None,
            ragdoll_controls: Vec::new(),
            rigid_body_ragdoll_controls: Vec::new(),
        };
        let twist = crate::behavior::TwistModifier {
            enable: true,
            axis_of_rotation: [0.0, 0.0, 1.0, 0.0],
            twist_angle: std::f32::consts::PI,
            start_bone_index: 0,
            end_bone_index: 2,
            set_angle_method: 1,
            rotation_axis_coordinates: 1,
            is_additive: true,
        };
        distribute_twist(&mut output, &twist, twist.twist_angle);
        let expected_angles = [
            std::f32::consts::PI / 6.0,
            std::f32::consts::PI / 3.0,
            std::f32::consts::PI / 2.0,
        ];
        for (bone, angle) in expected_angles.into_iter().enumerate() {
            assert!((output.pose[bone].rotation[2] - (angle / 2.0).sin()).abs() < 0.0001);
        }
        assert_eq!(output.pose[3].rotation, QsTransform::IDENTITY.rotation);
    }

    #[test]
    fn anm_48_twist_rejects_invalid_or_out_of_pose_chain_indices() {
        let mut output = Output {
            pose: vec![QsTransform::IDENTITY; 2],
            world_from_model: QsTransform::IDENTITY,
            bone_weights: vec![1.0; 2],
            weight: 1.0,
            foot_ik_controls: None,
            ragdoll_controls: Vec::new(),
            rigid_body_ragdoll_controls: Vec::new(),
        };
        let twist = crate::behavior::TwistModifier {
            enable: true,
            axis_of_rotation: [0.0, 0.0, 1.0, 0.0],
            twist_angle: 1.0,
            start_bone_index: 1,
            end_bone_index: 2,
            set_angle_method: 0,
            rotation_axis_coordinates: 1,
            is_additive: true,
        };
        distribute_twist(&mut output, &twist, twist.twist_angle);
        assert!(output
            .pose
            .iter()
            .all(|bone| bone.rotation == QsTransform::IDENTITY.rotation));
    }

    #[test]
    fn modifier_list_uses_serialized_order_and_updates_nested_child_outputs() {
        let graph = modifier_list_graph(true, true);
        assert_eq!(
            graph.node(NodeId(1)).children(),
            vec![NodeId(3), NodeId(2), NodeId(4)],
            "the list's authored child order differs from arena order"
        );
        let mut clips = ClipLibrary::new();
        clips.insert(modifier_generator_clip());
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);

        vm.variables.set_word(2, 1);
        vm.variables.set_word(3, 1);
        vm.update(1.0 / 60.0);
        assert_eq!(
            [
                vm.variables.word(0),
                vm.variables.word(1),
                vm.variables.word(4)
            ],
            [Some(1), Some(1), Some(1)]
        );
        assert_eq!(vm.output().pose[0].translation[0], 4.0);

        vm.variables.set_word(2, 0);
        vm.update(1.0 / 60.0);
        assert_eq!(
            [
                vm.variables.word(0),
                vm.variables.word(1),
                vm.variables.word(4)
            ],
            [Some(0), Some(0), Some(0)],
            "disabling the list cleans up direct and nested children"
        );
        assert_eq!(vm.output().pose[0].translation[0], 4.0);

        vm.variables.set_word(2, 1);
        vm.variables.set_word(3, 0);
        vm.update(1.0 / 60.0);
        assert_eq!(
            [
                vm.variables.word(0),
                vm.variables.word(1),
                vm.variables.word(4)
            ],
            [Some(1), Some(0), Some(1)],
            "a child enable binding affects only that child"
        );

        vm.variables.set_word(3, 1);
        vm.update(1.0 / 60.0);
        assert_eq!(
            [
                vm.variables.word(0),
                vm.variables.word(1),
                vm.variables.word(4)
            ],
            [Some(1), Some(1), Some(1)]
        );
    }

    #[test]
    fn modifier_list_publishes_shared_outputs_in_serialized_order() {
        let mut graph = modifier_list_graph(true, true);
        graph.nodes[2].bindings.as_mut().unwrap().bindings[0].variable_index = 0;
        if let NodeKind::IsActiveModifier { invert_active, .. } = &mut graph.nodes[2].kind {
            invert_active[0] = true;
        }
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.variables.set_word(2, 1);
        vm.variables.set_word(3, 1);
        vm.update(1.0 / 60.0);
        assert_eq!(
            vm.variables.word(0),
            Some(0),
            "the second serialized child overwrites the first despite lower arena index"
        );
    }

    #[test]
    fn a_disabled_child_publishes_before_a_later_enabled_shared_output() {
        let mut graph = modifier_list_graph(true, true);
        graph.nodes[2].bindings.as_mut().unwrap().bindings[0].variable_index = 0;
        if let NodeKind::IsActiveModifier { enable, .. } = &mut graph.nodes[3].kind {
            *enable = false;
        }
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.variables.set_word(2, 1);
        vm.variables.set_word(3, 1);
        vm.update(1.0 / 60.0);
        assert_eq!(
            vm.variables.word(0),
            Some(1),
            "the enabled second child wins after the disabled first child"
        );
    }

    #[test]
    fn an_authored_disabled_modifier_list_never_activates_its_children() {
        let graph = modifier_list_graph(false, false);
        let mut clips = ClipLibrary::new();
        clips.insert(modifier_generator_clip());
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
        vm.update(1.0 / 60.0);
        assert_eq!(
            [
                vm.variables.word(0),
                vm.variables.word(1),
                vm.variables.word(4)
            ],
            [Some(0), Some(0), Some(0)]
        );
        assert_eq!(vm.output().pose[0].translation[0], 4.0);
    }

    #[test]
    fn time_step_modifier_writes_each_graph_update_interval() {
        let graph = time_step_graph(true);
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());

        vm.update(0.0);
        assert_eq!(vm.variables.real(0), Some(0.0));
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.real(0), Some(1.0 / 60.0));
        vm.update(0.125);
        assert_eq!(vm.variables.real(0), Some(0.125));
    }

    #[test]
    fn anm_37_enabled_timer_emits_its_event_at_the_declared_elapsed_time() {
        let graph = bstimer_graph(true, 0.75, false);
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());

        vm.update(0.25);
        assert!(vm.take_events().is_empty(), "the alarm is not due yet");
        vm.update(0.5);
        assert_eq!(vm.take_events(), vec![Event { id: 0 }]);
    }

    #[test]
    fn anm_37_disabled_timer_does_not_emit_its_event() {
        let graph = bstimer_graph(false, 0.5, false);
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());

        vm.update(1.0);

        assert!(vm.take_events().is_empty(), "a disabled alarm stays silent");
    }

    #[test]
    fn anm_37_reset_alarm_restarts_its_elapsed_time_after_emitting() {
        let graph = bstimer_graph(true, 0.5, true);
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());

        vm.update(0.5);
        assert_eq!(vm.take_events(), vec![Event { id: 0 }]);
        vm.update(0.25);
        assert!(
            vm.take_events().is_empty(),
            "the reset interval is incomplete"
        );
        vm.update(0.25);
        assert_eq!(vm.take_events(), vec![Event { id: 0 }]);
    }

    #[test]
    fn anm_37_nonresetting_alarm_fires_once_per_activation() {
        let graph = bstimer_graph(true, 0.5, false);
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());

        vm.update(0.5);
        assert_eq!(vm.take_events(), vec![Event { id: 0 }]);
        vm.update(1.0);
        assert!(vm.take_events().is_empty(), "a spent alarm does not repeat");
    }

    #[test]
    fn anm_37_reactivation_restarts_the_runtime_timer() {
        let mut graph = bstimer_graph(true, 0.5, false);
        graph.data.variable_names.push("Enabled".into());
        graph.data.variable_infos.push(VariableInfo::default());
        graph.nodes[1].bindings = Some(VariableBindingSet {
            bindings: vec![Binding {
                member_path: "enable".into(),
                variable_index: 0,
                bit_index: -1,
                binding_type: 0,
            }],
            index_of_binding_to_enable: -1,
        });
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.variables.set_word(0, 1);

        vm.update(0.25);
        vm.variables.set_word(0, 0);
        vm.update(0.25);
        vm.variables.set_word(0, 1);
        vm.update(0.25);
        assert!(
            vm.take_events().is_empty(),
            "the reactivated timer starts at zero"
        );
        vm.update(0.25);
        assert_eq!(vm.take_events(), vec![Event { id: 0 }]);
    }

    #[test]
    fn disabled_time_step_modifier_clears_its_output() {
        let graph = time_step_graph(false);
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.variables.set_real(0, 4.0);

        vm.update(0.125);

        assert_eq!(vm.variables.real(0), Some(0.0));
        assert!(!vm.is_active(NodeId(1)));
    }

    #[test]
    fn anm_40_typed_expression_writes_its_expected_result() {
        let graph = evaluate_expression_graph(
            vec![ExpressionData {
                expression: "Speed * 2 + 1".into(),
                assignment_variable_index: 1,
                assignment_event_index: -1,
                event_mode: 0,
            }],
            &["Speed", "Result"],
            &[VariableType::Real, VariableType::Real],
            &[],
        );
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_real(0, 2.5);

        vm.update(1.0 / 60.0);

        assert_eq!(vm.variables.real(1), Some(6.0));
        assert!(vm.unsupported().expression_modifiers.is_empty());
    }

    #[test]
    fn anm_40_unsupported_expression_syntax_is_reported_without_assignment() {
        let graph = evaluate_expression_graph(
            vec![ExpressionData {
                expression: "Speed + (1".into(),
                assignment_variable_index: 1,
                assignment_event_index: -1,
                event_mode: 0,
            }],
            &["Speed", "Result"],
            &[VariableType::Real, VariableType::Real],
            &[],
        );
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_real(0, 2.5);
        vm.variables.set_real(1, 9.0);

        vm.update(1.0 / 60.0);

        assert_eq!(vm.variables.real(1), Some(9.0));
        assert_eq!(vm.unsupported().expression_modifiers.len(), 1);
        assert!(vm.unsupported().expression_modifiers[0].contains("Syntax"));
    }

    #[test]
    fn anm_40_expression_event_modes_follow_their_declared_trigger_rules() {
        let graph = evaluate_expression_graph(
            (0..4)
                .map(|event_mode| ExpressionData {
                    expression: "Gate".into(),
                    assignment_variable_index: -1,
                    assignment_event_index: event_mode,
                    event_mode: i8::try_from(event_mode).unwrap(),
                })
                .collect(),
            &["Gate"],
            &[VariableType::Bool],
            &["Once", "True", "Rising", "Sticky"],
        );
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_bool(0, true);
        vm.update(1.0 / 60.0);
        assert_eq!(
            vm.take_events()
                .iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );

        vm.update(1.0 / 60.0);
        assert_eq!(
            vm.take_events()
                .iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );

        vm.variables.set_bool(0, false);
        vm.update(1.0 / 60.0);
        assert_eq!(
            vm.take_events()
                .iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            vec![3]
        );

        vm.variables.set_bool(0, true);
        vm.update(1.0 / 60.0);
        assert_eq!(
            vm.take_events()
                .iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn nested_modifier_lists_update_each_time_step_output() {
        let graph = nested_time_step_graph();
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());

        vm.update(0.2);

        assert_eq!(vm.variables.real(0), Some(0.2));
        assert_eq!(vm.variables.real(1), Some(0.2));
        assert!(vm.is_active(NodeId(2)));
        assert!(vm.is_active(NodeId(4)));
    }

    #[test]
    fn a_cyclic_modifier_list_is_deactivated_and_reported() {
        let mut graph = modifier_list_graph(true, false);
        if let NodeKind::ModifierList { modifiers, .. } = &mut graph.nodes[1].kind {
            modifiers.rotate_right(1);
        }
        if let NodeKind::ModifierList { modifiers, .. } = &mut graph.nodes[4].kind {
            modifiers.push(Some(NodeId(1)));
        }
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_eq!(vm.unsupported().modifier_list_cycles.len(), 1);
        assert!(!vm.is_active(NodeId(1)));
        assert!(!vm.is_active(NodeId(2)));
        assert!(!vm.is_active(NodeId(3)));
        assert!(!vm.is_active(NodeId(4)));
    }

    #[test]
    fn an_excessively_deep_modifier_list_is_deactivated_and_reported() {
        let mut nodes = vec![Node {
            id: NodeId(0),
            offset: 0,
            class: "hkbModifierGenerator".into(),
            name: "wrapper".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::ModifierGenerator {
                modifier: Some(NodeId(1)),
                generator: None,
            },
        }];
        for id in 1..=MAX_MODIFIER_LIST_DEPTH + 1 {
            nodes.push(Node {
                id: NodeId(id),
                offset: u32::try_from(id).unwrap(),
                class: "hkbModifierList".into(),
                name: format!("list {id}"),
                user_data: 0,
                bindings: None,
                kind: NodeKind::ModifierList {
                    enable: true,
                    modifiers: (id < MAX_MODIFIER_LIST_DEPTH + 1)
                        .then_some(Some(NodeId(id + 1)))
                        .into_iter()
                        .collect(),
                },
            });
        }
        let graph = BehaviorGraph {
            name: "deep modifier list".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData::default(),
            nodes,
        };
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_eq!(vm.unsupported().modifier_list_depth_limit.len(), 1);
        assert!(!vm.is_active(NodeId(MAX_MODIFIER_LIST_DEPTH + 1)));
    }

    #[test]
    fn an_out_of_range_node_is_not_enabled() {
        let graph = modifier_list_graph(true, false);
        let lib = ClipLibrary::new();
        let vm = Interpreter::new(&graph, &lib, Vec::new());
        assert!(!vm.node_enabled(NodeId(usize::MAX)));
    }

    #[test]
    fn an_is_active_slot_writes_the_variable_when_the_node_runs() {
        let g = is_active_graph(false, 0);
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&g, &lib, Vec::new());
        assert!(
            vm.unsupported().unbound.is_empty(),
            "the slot must bind: {:?}",
            vm.unsupported().unbound
        );
        assert_eq!(vm.variables.word(0), Some(0), "before the first update");
        vm.update(1.0 / 60.0);
        assert_eq!(
            vm.variables.word(0),
            Some(1),
            "the root activates, so the modifier reports active"
        );
    }

    #[test]
    fn the_invert_bit_flips_what_the_slot_writes() {
        let g = is_active_graph(true, 0);
        let lib = ClipLibrary::new();
        let mut vm = Interpreter::new(&g, &lib, Vec::new());
        vm.update(1.0 / 60.0);
        assert_eq!(
            vm.variables.word(0),
            Some(0),
            "active with the invert bit set writes false"
        );
    }

    #[test]
    fn a_slot_past_the_five_the_class_carries_stays_unbound() {
        let g = is_active_graph(false, 9);
        let lib = ClipLibrary::new();
        let vm = Interpreter::new(&g, &lib, Vec::new());
        assert_eq!(vm.unsupported().unbound.len(), 1);
    }

    #[test]
    fn anm_45_modifier_outputs_each_authored_bone_weight() {
        let control = PoweredRagdollControlData {
            max_force: 80.0,
            tau: 0.6,
            damping: 0.1,
            proportional_recovery_velocity: 2.0,
            constant_recovery_velocity: 0.4,
        };
        let world_from_model = WorldFromModelModeData {
            pose_matching_bones: [1, 2, 3],
            mode: 4,
        };
        let mut graph = BehaviorGraph {
            name: "anm45".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData::default(),
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "hkbPoweredRagdollControlsModifier".into(),
                    name: "ragdoll".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::PoweredRagdollControls {
                        enable: true,
                        control,
                        bones: vec![4, 9],
                        world_from_model,
                        bone_weights: vec![0.2, 0.85],
                    },
                },
            ],
        };
        let output = Interpreter::new(&graph, &ClipLibrary::new(), Vec::new())
            .update(1.0 / 60.0)
            .clone();
        assert_eq!(output.ragdoll_controls.len(), 2);
        assert_eq!(output.ragdoll_controls[0].bone, 4);
        assert_eq!(output.ragdoll_controls[0].weight, 0.2);
        assert_eq!(output.ragdoll_controls[1].bone, 9);
        assert_eq!(output.ragdoll_controls[1].weight, 0.85);
        assert_eq!(output.ragdoll_controls[1].control, control);
        assert_eq!(
            output.ragdoll_controls[1].world_from_model,
            world_from_model
        );
        if let NodeKind::PoweredRagdollControls { enable, .. } = &mut graph.nodes[1].kind {
            *enable = false;
        }
        let library = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &library, Vec::new());
        let disabled = vm.update(1.0 / 60.0);
        assert!(disabled.ragdoll_controls.is_empty());
    }

    #[test]
    fn anm_45_ragdoll_control_weights_are_scaled_by_blend_contributions() {
        let control = RagdollControlOutput {
            bone: 5,
            control: PoweredRagdollControlData {
                max_force: 10.0,
                tau: 0.5,
                damping: 0.2,
                proportional_recovery_velocity: 1.0,
                constant_recovery_velocity: 0.1,
            },
            world_from_model: WorldFromModelModeData {
                pose_matching_bones: [0, 1, 2],
                mode: 4,
            },
            weight: 0.8,
        };
        let output = Output {
            pose: vec![QsTransform::IDENTITY],
            world_from_model: QsTransform::IDENTITY,
            bone_weights: vec![1.0],
            weight: 1.0,
            foot_ik_controls: None,
            ragdoll_controls: vec![control],
            rigid_body_ragdoll_controls: Vec::new(),
        };
        let blended = blend_outputs(&output, &output, 0.25);
        assert_eq!(blended.ragdoll_controls.len(), 2);
        assert_eq!(blended.ragdoll_controls[0].weight, 0.6);
        assert_eq!(blended.ragdoll_controls[1].weight, 0.2);
    }

    #[test]
    fn parametric_weights_bracket_the_parameter() {
        let w = parametric_weights(&[0.0, 1.0, 2.0], 0.25);
        assert!((w[0] - 0.75).abs() < 1e-6);
        assert!((w[1] - 0.25).abs() < 1e-6);
        assert_eq!(w[2], 0.0);
        assert_eq!(parametric_weights(&[0.0, 1.0], -1.0), vec![1.0, 0.0]);
        assert_eq!(parametric_weights(&[0.0, 1.0], 5.0), vec![0.0, 1.0]);
        assert_eq!(parametric_weights(&[2.0, 0.0], 1.0), vec![0.5, 0.5]);
        assert!(parametric_weights(&[], 1.0).is_empty());
    }

    #[test]
    fn blend_curves_hit_their_ends() {
        for c in 0..4 {
            assert_eq!(blend_curve(c, 0.0), 0.0);
            assert_eq!(blend_curve(c, 1.0), 1.0);
        }
        assert_eq!(blend_curve(1, 0.25), 0.25);
        assert!((blend_curve(0, 0.5) - 0.5).abs() < 1e-6);
        assert!(blend_curve(3, 0.5) < 0.5);
        assert!(blend_curve(2, 0.5) > 0.5);
    }

    #[test]
    fn blending_transforms_takes_the_short_arc() {
        let a = QsTransform::IDENTITY;
        let b = QsTransform {
            rotation: [0.0, 0.0, 0.0, -1.0],
            translation: [2.0, 0.0, 0.0, 0.0],
            ..QsTransform::IDENTITY
        };
        let m = blend_transform(&a, &b, 0.5);
        assert!((m.rotation[3] - 1.0).abs() < 1e-6, "{m:?}");
        assert!((m.translation[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_zero_weight_side_does_not_blend() {
        let a = Output {
            pose: vec![QsTransform::IDENTITY],
            world_from_model: QsTransform::IDENTITY,
            bone_weights: vec![0.0],
            weight: 0.0,
            foot_ik_controls: None,
            ragdoll_controls: Vec::new(),
            rigid_body_ragdoll_controls: Vec::new(),
        };
        let b = Output {
            pose: vec![QsTransform {
                translation: [5.0, 0.0, 0.0, 0.0],
                ..QsTransform::IDENTITY
            }],
            world_from_model: QsTransform::IDENTITY,
            bone_weights: vec![1.0],
            weight: 1.0,
            foot_ik_controls: None,
            ragdoll_controls: Vec::new(),
            rigid_body_ragdoll_controls: Vec::new(),
        };
        assert_eq!(blend_outputs(&a, &b, 0.1), b);
        assert_eq!(blend_outputs(&b, &a, 0.9), b);
        let m = blend_outputs(&b, &b, 0.5);
        assert_eq!(m.pose[0].translation[0], 5.0);
    }

    #[test]
    fn variables_round_trip_reals_through_words() {
        let infos = [
            VariableInfo {
                role: 0,
                role_flags: 0,
                ty: VariableType::Real,
            },
            VariableInfo {
                role: 0,
                role_flags: 0,
                ty: VariableType::Bool,
            },
            VariableInfo {
                role: 0,
                role_flags: 0,
                ty: VariableType::Int32,
            },
        ];
        let mut v = Variables::from_infos(&["Speed", "IsMoving", "iState"], &infos);
        assert!(v.set_by_name("Speed", 1.5));
        assert_eq!(v.real(0), Some(1.5));
        assert!(v.set_by_name("IsMoving", 1.0));
        assert_eq!(v.bool(1), Some(true));
        assert_eq!(v.word(1), Some(1));
        assert!(v.set_by_name("iState", 7.0));
        assert_eq!(v.word(2), Some(7));
        assert_eq!(v.real(2), Some(7.0));
        assert!(!v.set_by_name("Missing", 1.0));
        assert_eq!(v.real(9), None);
        assert_eq!(v.name(0), Some("Speed"));
        assert_eq!(v.len(), 3);
        assert!(!v.is_empty());
    }

    fn anm_35_contact_graph(enabled: bool) -> BehaviorGraph {
        BehaviorGraph {
            name: "ragdoll contact".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: BehaviorGraphData::default(),
            nodes: vec![
                Node {
                    id: NodeId(0),
                    offset: 0,
                    class: "hkbModifierGenerator".into(),
                    name: "wrapper".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::ModifierGenerator {
                        modifier: Some(NodeId(1)),
                        generator: None,
                    },
                },
                Node {
                    id: NodeId(1),
                    offset: 1,
                    class: "BSRagdollContactListenerModifier".into(),
                    name: "contact listener".into(),
                    user_data: 0,
                    bindings: None,
                    kind: NodeKind::RagdollContactListener {
                        enable: enabled,
                        contact_event: crate::behavior::EventProperty {
                            id: 17,
                            has_payload: false,
                        },
                        bones: vec![1, 2],
                    },
                },
            ],
        }
    }

    #[test]
    fn anm_35_contact_listener_emits_for_a_declared_bone_mapped_to_the_contact_body() {
        let graph = anm_35_contact_graph(true);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        let mapping = [Some(BodyIndex(0)), Some(BodyIndex(3)), Some(BodyIndex(3))];
        vm.send_ragdoll_contact(BodyIndex(3), &mapping, RagdollContactKind::Contact);
        vm.update(1.0 / 60.0);
        assert_eq!(vm.take_events(), vec![Event { id: 17 }]);
    }

    #[test]
    fn anm_35_contact_listener_emits_throw_event_for_its_mapped_body() {
        let graph = anm_35_contact_graph(true);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        let mapping = [Some(BodyIndex(4)), Some(BodyIndex(3)), None];
        vm.send_ragdoll_contact(BodyIndex(3), &mapping, RagdollContactKind::Throw);
        vm.update(1.0 / 60.0);
        assert_eq!(vm.take_events(), vec![Event { id: 17 }]);
    }

    #[test]
    fn anm_35_contact_listener_ignores_unmapped_and_undeclared_bodies() {
        let graph = anm_35_contact_graph(true);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        let mapping = [Some(BodyIndex(4)), Some(BodyIndex(3)), None];
        vm.send_ragdoll_contact(BodyIndex(4), &mapping, RagdollContactKind::Contact);
        vm.update(1.0 / 60.0);
        assert!(vm.take_events().is_empty());
    }

    #[test]
    fn anm_35_disabled_contact_listener_emits_no_event() {
        let graph = anm_35_contact_graph(false);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        let mapping = [Some(BodyIndex(0)), Some(BodyIndex(3)), Some(BodyIndex(4))];
        vm.send_ragdoll_contact(BodyIndex(3), &mapping, RagdollContactKind::Contact);
        vm.update(1.0 / 60.0);
        assert!(vm.take_events().is_empty());
    }
}
