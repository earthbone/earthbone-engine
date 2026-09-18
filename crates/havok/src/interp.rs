// SPDX-License-Identifier: GPL-3.0-or-later

//! The behavior graph interpreter: one [`Interpreter`] per graph instance,
//! ticked once per frame, producing a local-space pose.
//!
//! The update order is the one the Havok SDK headers publish: activate a
//! parent before its children, handle events outermost first, generate
//! recursively with children before their parent, then handle the events
//! nodes raised during the frame. What the headers do not publish (the
//! blender weight formula, transition arbitration, binding timing) is
//! filled with the plainest rule that fits and named in `docs/havok.md`
//! so a capture from the real engine can correct it.
//!
//! Only generators produce poses. Modifiers pass through, transition
//! effects are read by the state machine that owns them, behavior
//! references and unmodelled classes yield the reference pose at weight
//! zero and are counted, so a vanilla graph runs end to end and reports
//! what it could not evaluate.

use std::collections::HashMap;

use crate::animation::SplineCompressedAnimation;
use crate::behavior::{
    transition_flags, BehaviorGraph, BehaviorGraphData, BlenderGenerator, ClipGenerator, NodeId,
    NodeKind, StateMachine, TimeInterval, TransitionInfo, VariableBindingSet, VariableInfo,
    VariableType,
};
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
}

impl Clip {
    /// The first spline animation in a clip file and its binding.
    pub fn from_container(name: &str, container: &AnimationContainer) -> Result<Clip> {
        let animation = container
            .spline_animations()
            .next()
            .ok_or_else(|| Error::Unsupported(format!("{name}: no spline animation")))?
            .clone();
        let map = container
            .bindings
            .first()
            .map(|b| b.transform_track_to_bone_indices.clone())
            .unwrap_or_default();
        Clip::from_parts(name, animation, &map)
    }

    pub fn from_parts(
        name: &str,
        animation: SplineCompressedAnimation,
        track_to_bone: &[i16],
    ) -> Result<Clip> {
        let blocks = animation.decode()?;
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
        })
    }

    pub fn duration(&self) -> f32 {
        self.animation.duration
    }

    /// Bone index per track, as the binding maps it.
    pub fn track_to_bone(&self) -> &[usize] {
        &self.track_to_bone
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
    /// The event that caused it, `None` for activation.
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
    from_generator: Option<NodeId>,
    to_generator: Option<NodeId>,
    elapsed: f32,
    duration: f32,
    curve: i8,
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
    transition: Option<ActiveTransition>,
    blend_parameter: Option<f32>,
    child_weights: Vec<Option<f32>>,
    selected: Option<usize>,
    enabled: Option<bool>,
}

/// A generator's result for one frame: a local pose and how much of it
/// counts. Weight zero means "nothing to say", the reference pose.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub pose: Vec<QsTransform>,
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
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

pub struct Interpreter<'a> {
    graph: &'a BehaviorGraph,
    clips: &'a ClipLibrary,
    reference_pose: Vec<QsTransform>,
    pub variables: Variables,
    pub character_properties: Variables,
    nodes: Vec<Runtime>,
    bindings: Vec<ResolvedBinding>,
    pending: Vec<Event>,
    raised: Vec<Event>,
    handled: Vec<Event>,
    changes: Vec<StateChange>,
    unsupported: Unsupported,
    output: Output,
    frame: u64,
}

/// Rounds of internally raised events handled per update before the rest
/// wait for the next frame. A graph whose states raise each other's
/// events forever would otherwise never return.
const MAX_EVENT_ROUNDS: usize = 8;

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
            character_properties: Variables::default(),
            nodes,
            bindings: Vec::new(),
            pending: Vec::new(),
            raised: Vec::new(),
            handled: Vec::new(),
            changes: Vec::new(),
            unsupported: Unsupported::default(),
            output: Output {
                pose: reference_pose.clone(),
                weight: 0.0,
            },
            reference_pose,
            frame: 0,
        };
        interp.resolve_bindings();
        interp
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
            .is_some_and(|n| n.transition.is_some())
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
            let output = self.generate(root, dt);
            self.output = output;
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
        &self.output
    }

    fn apply_bindings(&mut self) {
        for b in &self.bindings {
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
            }
        }
    }

    /// Whether a node's `enable` binding, when it has one, allows it.
    /// Modifiers pass through either way, so this only reports the value.
    pub fn node_enabled(&self, id: NodeId) -> bool {
        self.nodes
            .get(id.0)
            .is_none_or(|n| n.enabled.unwrap_or(true))
    }

    fn activate(&mut self, id: NodeId) {
        if self.nodes[id.0].active {
            return;
        }
        self.nodes[id.0].active = true;
        let graph = self.graph;
        match &graph.node(id).kind {
            NodeKind::Clip(c) => {
                let rt = &mut self.nodes[id.0];
                rt.local_time = c.start_time.max(0.0);
                rt.at_end = false;
                rt.passes = 0;
            }
            NodeKind::StateMachine(sm) => {
                let start = self.start_state(sm);
                self.nodes[id.0].current_state = None;
                self.nodes[id.0].previous_state = None;
                self.nodes[id.0].transition = None;
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
            NodeKind::ModifierGenerator { generator, .. }
            | NodeKind::IStateTagging {
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
            NodeKind::ModifierList { .. }
            | NodeKind::BehaviorReference { .. }
            | NodeKind::BlendingTransition(_)
            | NodeKind::GeneratorTransition(_)
            | NodeKind::EventDrivenModifier { .. }
            | NodeKind::TimerModifier { .. }
            | NodeKind::Modifier { .. }
            | NodeKind::Unknown => {}
        }
    }

    fn deactivate(&mut self, id: NodeId) {
        if !self.nodes[id.0].active {
            return;
        }
        let graph = self.graph;
        let node = graph.node(id);
        if let NodeKind::StateMachine(sm) = &node.kind {
            if let Some(current) = self.nodes[id.0].current_state {
                if let Some(state) = sm.state(current) {
                    for e in &state.exit_notify_events {
                        self.raise(e.id);
                    }
                }
            }
            if let Some(t) = self.nodes[id.0].transition.take() {
                if let Some(g) = t.from_generator {
                    self.deactivate(g);
                }
            }
            self.nodes[id.0].current_state = None;
        }
        for child in node.children() {
            self.deactivate(child);
        }
        self.nodes[id.0].active = false;
    }

    fn raise(&mut self, id: i32) {
        if id >= 0 {
            self.raised.push(Event { id });
        }
    }

    /// Mode 0 uses the start state, 1 reads the sync variable, 2 picks the
    /// most probable enabled state (a deterministic stand-in for random), 3
    /// has a chooser this crate does not decode and falls back to 0.
    fn start_state(&self, sm: &StateMachine) -> Option<i32> {
        let by_id = |state: i32| sm.state(state).filter(|s| s.enable).map(|s| s.state_id);
        let first_enabled = || sm.states.iter().find(|s| s.enable).map(|s| s.state_id);
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
    /// its current state (own list first, then wildcards, first match)
    /// names it, otherwise the event goes on to the active children.
    fn handle_event(&mut self, id: NodeId, ev: Event) -> bool {
        if !self.nodes[id.0].active {
            return false;
        }
        let graph = self.graph;
        let node = graph.node(id);
        match &node.kind {
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
        if let Some(t) = self.nodes[id.0].transition {
            out.extend(t.from_generator);
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
        let return_event = sm.return_to_previous_state_event_id;
        let candidate = state
            .transitions
            .iter()
            .chain(sm.wildcard_transitions.iter())
            .find(|t| Self::transition_matches(sm, current, t, ev))
            .cloned();
        let target = match candidate {
            Some(t) => Some((t.to_state_id, t)),
            None if return_event >= 0 && ev.id == return_event => self.nodes[id.0]
                .previous_state
                .map(|p| (p, TransitionInfo::plain(p))),
            None => None,
        };
        let Some((to, info)) = target else {
            return false;
        };
        self.start_transition(id, sm, current, to, &info, ev);
        true
    }

    /// The gate from `hkbStateMachine.inl`'s `shouldTransition` minus what
    /// this crate does not decode yet: conditions and trigger intervals
    /// are accepted as satisfied. A wildcard that would land on the
    /// current state is skipped unless it allows self transitions.
    fn transition_matches(sm: &StateMachine, current: i32, t: &TransitionInfo, ev: Event) -> bool {
        if t.flags & transition_flags::DISABLED != 0 || t.event_id != ev.id {
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
        if let Some(t) = self.nodes[id.0].transition.take() {
            self.finish_transition(id, t);
        }
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
                NodeKind::BlendingTransition(b) => Some((b.duration, b.blend_curve)),
                _ => None,
            })
            .filter(|(d, _)| *d > 0.0);
        match effect {
            Some((duration, curve)) if from_generator != to_generator => {
                self.nodes[id.0].transition = Some(ActiveTransition {
                    from_generator,
                    to_generator,
                    elapsed: 0.0,
                    duration,
                    curve,
                });
            }
            _ => {
                if from_generator != to_generator {
                    if let Some(g) = from_generator {
                        self.deactivate(g);
                    }
                }
            }
        }
        self.enter_state(id, sm, to, Some(ev.id));
    }

    fn finish_transition(&mut self, id: NodeId, t: ActiveTransition) {
        let current = self.nodes[id.0].current_state;
        let current_generator = match &self.graph.node(id).kind {
            NodeKind::StateMachine(sm) => {
                current.and_then(|c| sm.state(c)).and_then(|s| s.generator)
            }
            _ => None,
        };
        if let Some(g) = t.from_generator {
            if Some(g) != current_generator {
                self.deactivate(g);
            }
        }
    }

    fn reference_output(&self) -> Output {
        Output {
            pose: self.reference_pose.clone(),
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
            NodeKind::ModifierGenerator { generator, .. }
            | NodeKind::IStateTagging {
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
            NodeKind::BehaviorReference { behavior_name } => {
                push_unique(
                    &mut self.unsupported.behavior_references,
                    behavior_name.clone(),
                );
                self.reference_output()
            }
            NodeKind::Unknown => {
                push_unique(&mut self.unsupported.unknown_classes, node.class.clone());
                self.reference_output()
            }
            NodeKind::ModifierList { .. }
            | NodeKind::BlendingTransition(_)
            | NodeKind::GeneratorTransition(_)
            | NodeKind::EventDrivenModifier { .. }
            | NodeKind::TimerModifier { .. }
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
        Output { pose, weight: 1.0 }
    }

    fn generate_machine(&mut self, id: NodeId, sm: &StateMachine, dt: f32) -> Output {
        self.nodes[id.0].time_in_state += dt;
        let current_generator = self.nodes[id.0]
            .current_state
            .and_then(|c| sm.state(c))
            .and_then(|s| s.generator);
        if let Some(mut t) = self.nodes[id.0].transition {
            t.elapsed += dt;
            let alpha = blend_curve(t.curve, (t.elapsed / t.duration).clamp(0.0, 1.0));
            let from = match t.from_generator {
                Some(g) => self.generate(g, dt),
                None => self.reference_output(),
            };
            let to = match t.to_generator {
                Some(g) => self.generate(g, dt),
                None => self.reference_output(),
            };
            if t.elapsed >= t.duration {
                self.nodes[id.0].transition = None;
                self.finish_transition(id, t);
            } else {
                self.nodes[id.0].transition = Some(t);
            }
            return blend_outputs(&from, &to, alpha);
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
        let mut acc: Option<Vec<QsTransform>> = None;
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
            match &mut acc {
                None => {
                    acc = Some(out.pose);
                    acc_weight = w;
                }
                Some(pose) => {
                    let alpha = w / (acc_weight + w);
                    blend_into(pose, &out.pose, alpha);
                    acc_weight += w;
                }
            }
        }
        match acc {
            Some(pose) => Output {
                pose,
                weight: acc_weight.min(1.0),
            },
            None => self.reference_output(),
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
            trigger_interval: never,
            initiate_interval: never,
            transition: None,
            has_condition: false,
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

fn blend_into(acc: &mut [QsTransform], other: &[QsTransform], alpha: f32) {
    for (a, b) in acc.iter_mut().zip(other) {
        *a = blend_transform(a, b, alpha);
    }
}

/// Blend two outputs. A side at weight zero contributes nothing, so a
/// transition out of a reference pose is a cut to the other side.
pub fn blend_outputs(from: &Output, to: &Output, alpha: f32) -> Output {
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
    blend_into(&mut pose, &to.pose, alpha);
    Output {
        pose,
        weight: from.weight + (to.weight - from.weight) * alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            weight: 0.0,
        };
        let b = Output {
            pose: vec![QsTransform {
                translation: [5.0, 0.0, 0.0, 0.0],
                ..QsTransform::IDENTITY
            }],
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
