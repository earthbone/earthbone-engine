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
    NodeId, NodeKind, StateMachine, TimeInterval, TransitionInfo, VariableBindingSet, VariableInfo,
    VariableType, FALSE_TO_TRUE_SLOTS, IS_ACTIVE_SLOTS,
};
use crate::expression::{Expression, ExpressionError};
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
/// variable's word is its index into the quad table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Variables {
    infos: Vec<VariableInfo>,
    names: Vec<String>,
    words: Vec<i32>,
    quads: Vec<[f32; 4]>,
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
        Variables {
            infos: data.variable_infos.clone(),
            names: data.variable_names.clone(),
            words,
            quads,
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
}

/// An event by graph-local id. Payloads are not carried yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub id: i32,
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
    ChildWeight(usize),
    SelectedGeneratorIndex,
    Enable,
    FalseToTrueEnable(usize),
    FalseToTrueValue(usize),
    /// Which state the machine enters on activation.
    StartStateId,
    /// A transition effect's blend duration in seconds.
    Duration,
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
    /// Output: the elapsed time of this graph update.
    TimeStep,
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
                | Target::TimeStep
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
    blend_parameter: Option<f32>,
    child_weights: Vec<Option<f32>>,
    selected: Option<usize>,
    enabled: Option<bool>,
    /// A bound `startStateId`, read when the machine activates.
    start_state: Option<i32>,
    /// A bound blend `duration`, on a transition effect.
    duration: Option<f32>,
    edge_enabled: [Option<bool>; FALSE_TO_TRUE_SLOTS],
    edge_value: [Option<bool>; FALSE_TO_TRUE_SLOTS],
    edge_previous: [bool; FALSE_TO_TRUE_SLOTS],
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
}

/// A generator's result for one frame: a local pose and how much of it
/// counts. Weight zero means "nothing to say", the reference pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub pose: Vec<QsTransform>,
    /// Contribution per bone, retained through nested blends and transitions.
    pub bone_weights: Vec<f32>,
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

fn push_unique(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
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

pub struct Interpreter<'a> {
    graph: &'a BehaviorGraph,
    clips: &'a ClipLibrary,
    reference_pose: Vec<QsTransform>,
    pub variables: Variables,
    pub character_properties: Variables,
    nodes: Vec<Runtime>,
    bindings: Vec<ResolvedBinding>,
    conditions: HashMap<Condition, std::result::Result<Expression, ExpressionError>>,
    expressions: Vec<Option<Vec<std::result::Result<Expression, ExpressionError>>>>,
    /// Modifier nodes in this frame's authored traversal order. Output
    /// bindings publish in this order rather than arena allocation order.
    modifier_order: Vec<NodeId>,
    pending: Vec<Event>,
    raised: Vec<Event>,
    handled: Vec<Event>,
    changes: Vec<StateChange>,
    unsupported: Unsupported,
    output: Output,
    expression_random_state: u64,
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
            }
        }
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
            conditions: HashMap::new(),
            expressions: vec![None; graph.nodes.len()],
            modifier_order: Vec::new(),
            pending: Vec::new(),
            raised: Vec::new(),
            handled: Vec::new(),
            changes: Vec::new(),
            unsupported: Unsupported::default(),
            output: Output {
                pose: reference_pose.clone(),
                bone_weights: vec![0.0; reference_pose.len()],
                weight: 0.0,
            },
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
                ("weight", Some(i), NodeKind::Blender(_)) => Some(Target::ChildWeight(i)),
                ("selectedGeneratorIndex", None, NodeKind::ManualSelector { .. }) => {
                    Some(Target::SelectedGeneratorIndex)
                }
                ("enable", None, _) => Some(Target::Enable),
                ("isActive", None, NodeKind::StateMachine(_)) => Some(Target::MachineIsActive),
                ("startStateId", None, NodeKind::StateMachine(_)) => Some(Target::StartStateId),
                ("duration", None, NodeKind::BlendingTransition(_)) => Some(Target::Duration),
                ("timeStep", None, NodeKind::GetTimeStepModifier { .. }) => Some(Target::TimeStep),
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
            self.update_time_step_modifiers(dt);
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
            let Some(value) = table.real(b.variable) else {
                continue;
            };
            let rt = &mut self.nodes[b.node.0];
            match b.target {
                Target::PlaybackSpeed => rt.playback_speed = Some(value),
                Target::UserControlledTimeFraction => rt.user_fraction = Some(value),
                Target::BlendParameter => rt.blend_parameter = Some(value),
                Target::ChildWeight(i) => {
                    if let Some(w) = rt.child_weights.get_mut(i) {
                        *w = Some(value);
                    }
                }
                Target::SelectedGeneratorIndex => {
                    rt.selected = usize::try_from(value as i32).ok();
                }
                Target::Enable => rt.enabled = Some(value != 0.0),
                Target::FalseToTrueEnable(slot) => rt.edge_enabled[slot] = Some(value != 0.0),
                Target::FalseToTrueValue(slot) => rt.edge_value[slot] = Some(value != 0.0),
                Target::StartStateId => rt.start_state = Some(value as i32),
                Target::Duration => rt.duration = Some(value),
                Target::DecomposeVector => rt.vector = table.quad(b.variable),
                // Written by apply_output_bindings, skipped above.
                Target::IsActive { .. }
                | Target::MachineIsActive
                | Target::DecomposeComponent(_)
                | Target::TimeStep => {}
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
            | NodeKind::GetTimeStepModifier { enable, .. }
            | NodeKind::IsActiveModifier { enable, .. }
            | NodeKind::FalseToTrueModifier { enable, .. }
            | NodeKind::EveryNEventsModifier { enable, .. }
            | NodeKind::DecomposeVector { enable, .. }
            | NodeKind::EvaluateExpression { enable, .. }
            | NodeKind::LookAtModifier { enable, .. }
            | NodeKind::DirectAt { enable, .. }
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
            if let NodeKind::ModifierList { modifiers, .. } = &node.kind {
                path[node_id.0] = true;
                stack.push((node_id, depth, true, enabled));
                for child in modifiers.iter().rev().flatten().copied() {
                    stack.push((child, depth + 1, false, enabled));
                }
            }
        }
    }

    /// Evaluate edge slots only during update, after list enable state is known.
    fn update_modifier(&mut self, id: NodeId) {
        if !self.nodes[id.0].active {
            return;
        }
        if let NodeKind::FalseToTrueModifier { slots, .. } = &self.graph.nodes[id.0].kind {
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
        if matches!(
            self.graph.nodes[id.0].kind,
            NodeKind::EvaluateExpression { .. }
        ) {
            self.evaluate_expressions(id);
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
            NodeKind::FalseToTrueModifier { .. } => {
                self.nodes[id.0].edge_previous = [false; FALSE_TO_TRUE_SLOTS];
            }
            NodeKind::EvaluateExpression { expressions, .. } => {
                self.nodes[id.0].expression_raised = vec![false; expressions.len()];
                self.nodes[id.0].expression_previous = vec![false; expressions.len()];
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
            NodeKind::IStateTagging {
                default_generator: generator,
                ..
            }
            | NodeKind::BoneSwitch {
                default_generator: generator,
                ..
            }
            | NodeKind::OffsetAnimation {
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
            NodeKind::BehaviorReference { resolved, .. } => {
                if let Some(g) = resolved {
                    self.activate(*g);
                }
            }
            NodeKind::ModifierList { .. }
            | NodeKind::BlendingTransition(_)
            | NodeKind::GeneratorTransition(_)
            | NodeKind::EventDrivenModifier { .. }
            | NodeKind::TimerModifier { .. }
            | NodeKind::GetTimeStepModifier { .. }
            | NodeKind::DirectAt { .. }
            | NodeKind::IsActiveModifier { .. }
            | NodeKind::DecomposeVector { .. }
            | NodeKind::LookAtModifier { .. }
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
            bone_weights: vec![0.0; self.reference_pose.len()],
            weight: 0.0,
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
                let modifier_start = self.modifier_order.len();
                if let Some(modifier) = modifier {
                    self.sync_modifier(*modifier);
                    for index in modifier_start..self.modifier_order.len() {
                        let node = self.modifier_order[index];
                        self.update_modifier(node);
                    }
                }
                let mut output = match generator {
                    Some(generator) => self.generate(*generator, dt),
                    None => self.reference_output(),
                };
                let applied_modifiers = self.modifier_order[modifier_start..].to_vec();
                for modifier in applied_modifiers {
                    if !self.nodes[modifier.0].active {
                        continue;
                    }
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
                        _ => {}
                    }
                }
                output
            }
            NodeKind::IStateTagging {
                default_generator: generator,
                ..
            }
            | NodeKind::BoneSwitch {
                default_generator: generator,
                ..
            }
            | NodeKind::OffsetAnimation {
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
            } => match generator {
                Some(g) => self.generate(*g, dt),
                None => self.reference_output(),
            },
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
            | NodeKind::GetTimeStepModifier { .. }
            | NodeKind::DirectAt { .. }
            | NodeKind::IsActiveModifier { .. }
            | NodeKind::FalseToTrueModifier { .. }
            | NodeKind::EveryNEventsModifier { .. }
            | NodeKind::DecomposeVector { .. }
            | NodeKind::EvaluateExpression { .. }
            | NodeKind::LookAtModifier { .. }
            | NodeKind::Modifier { .. } => self.reference_output(),
        }
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
            weight: 1.0,
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
        let mut acc_weight = 0.0;
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
            if out.weight <= 0.0 {
                continue;
            }
            let w = w / total;
            acc_weight += w;
            for (bone, (acc, next)) in pose.iter_mut().zip(&out.pose).enumerate() {
                let mask = child
                    .bone_weights
                    .as_ref()
                    .map_or(1.0, |m| m.get(bone).copied().unwrap_or(0.0));
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
            bone_weights,
            weight: acc_weight.min(1.0),
        }
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

/// Blend two outputs. A side at weight zero contributes nothing, so a
/// transition out of a reference pose is a cut to the other side.
pub fn blend_outputs(from: &Output, to: &Output, alpha: f32) -> Output {
    if alpha <= 0.0 {
        return from.clone();
    }
    if alpha >= 1.0 {
        return to.clone();
    }
    if from.weight <= 0.0 && to.weight <= 0.0 {
        return from.clone();
    }
    if from.weight <= 0.0 {
        return to.clone();
    }
    if to.weight <= 0.0 {
        return from.clone();
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
        bone_weights,
        weight: from.weight + (to.weight - from.weight) * alpha,
    }
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
    fn modifier_generator_does_not_activate_a_disabled_modifier() {
        let graph = modifier_generator_graph();
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_word(1, 0);
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.word(0), Some(0));
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
            bone_weights: vec![0.0],
            weight: 0.0,
        };
        let b = Output {
            pose: vec![QsTransform {
                translation: [5.0, 0.0, 0.0, 0.0],
                ..QsTransform::IDENTITY
            }],
            bone_weights: vec![1.0],
            weight: 1.0,
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
}
