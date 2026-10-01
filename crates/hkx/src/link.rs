// SPDX-License-Identifier: GPL-3.0-or-later

//! Linking a referenced behavior graph into the graph that names it.
//!
//! `hkbBehaviorReferenceGenerator` serializes only a file name. Its
//! `behavior` pointer carries `SERIALIZE_IGNORED`, and so do
//! `hkbBehaviorGraph`'s `eventIdMap`, `variableIdMap` and
//! `characterPropertyIdMap`. The engine builds all four when it loads a
//! character, so a file on its own never says which node sits under a
//! reference, and the ids inside a referenced graph mean nothing until they
//! are mapped. That mapping is what this module does.
//!
//! Symbols map by name, which is the only thing two files share. A sub-graph
//! event or variable the parent already knows keeps the parent's id. One the
//! parent lacks is appended to the parent's tables, so a sub-graph can raise
//! an event the parent never declared without colliding with a parent id.
//! After that every id inside the sub-graph is rewritten, its node ids are
//! shifted past the parent's, and the reference node is pointed at the
//! sub-graph's root.
//!
//! Vanilla needs this to animate at all: `mt_behavior` reaches its standing
//! idle only through `Behaviors\IdleBehavior.hkx`, so an unlinked graph holds
//! the idle state at weight zero.

use std::collections::HashMap;
use std::sync::Arc;

use crate::behavior::{
    BehaviorGraph, BehaviorGraphData, Condition, EventInfo, EventProperty, ExpressionScope, Node,
    NodeId, NodeKind, StateInfo, TimeInterval, TransitionInfo, VariableBindingSet, VariableInfo,
};
use crate::{Error, Result};

/// Sub-graph id to parent id, by position, for events, variables and properties.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolMap {
    events: Vec<i32>,
    variables: Vec<i32>,
    properties: Vec<i32>,
}

impl SymbolMap {
    /// A negative id means "none" in every field that carries one, and an id
    /// past the sub-graph's table is a file defect, so both pass through
    /// rather than panicking or silently becoming zero.
    #[must_use]
    pub fn event(&self, id: i32) -> i32 {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.events.get(i))
            .copied()
            .unwrap_or(id)
    }

    /// As [`SymbolMap::event`], for variable indices.
    #[must_use]
    pub fn variable(&self, id: i32) -> i32 {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.variables.get(i))
            .copied()
            .unwrap_or(id)
    }

    /// As [`SymbolMap::event`], for character property indices.
    #[must_use]
    pub fn property(&self, id: i32) -> i32 {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.properties.get(i))
            .copied()
            .unwrap_or(id)
    }

    /// How many sub-graph events and variables the map covers.
    #[must_use]
    pub fn len(&self) -> (usize, usize) {
        (self.events.len(), self.variables.len())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.variables.is_empty() && self.properties.is_empty()
    }
}

/// What [`BehaviorGraph::link_all`] resolved and what it could not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkReport {
    /// File names spliced in, one entry per reference, so a file reached
    /// twice appears twice.
    pub linked: Vec<String>,
    /// Names the loader had nothing for. Each is reported once.
    pub missing: Vec<String>,
    /// Names that loaded but would not splice, with the reason.
    pub failed: Vec<(String, String)>,
    /// How deep the reference chain went.
    pub rounds: usize,
}

/// First index of each name, so a duplicate keeps the lower id the way a
/// lookup by name would.
fn index_by_name(names: &[String]) -> HashMap<String, i32> {
    let mut out: HashMap<String, i32> = HashMap::with_capacity(names.len());
    for (i, n) in names.iter().enumerate() {
        out.entry(n.clone())
            .or_insert_with(|| i32::try_from(i).unwrap_or(i32::MAX));
    }
    out
}

/// Merge the sub-graph's symbol tables into the parent's, appending what the
/// parent lacks, and return the id map.
fn merge_symbols(parent: &mut BehaviorGraphData, sub: &BehaviorGraphData) -> SymbolMap {
    // The lookups own their keys, because both loops grow the table they
    // were built from.
    let mut known = index_by_name(&parent.event_names);
    let mut events = Vec::with_capacity(sub.event_names.len());
    for (i, name) in sub.event_names.iter().enumerate() {
        match known.get(name) {
            Some(&id) => events.push(id),
            None => {
                let id = i32::try_from(parent.event_names.len()).unwrap_or(i32::MAX);
                parent.event_names.push(name.clone());
                parent.event_infos.push(
                    sub.event_infos
                        .get(i)
                        .copied()
                        .unwrap_or(EventInfo { flags: 0 }),
                );
                known.insert(name.clone(), id);
                events.push(id);
            }
        }
    }

    let mut known = index_by_name(&parent.variable_names);
    let mut variables = Vec::with_capacity(sub.variable_names.len());
    let mut appended: Vec<usize> = Vec::new();
    for (i, name) in sub.variable_names.iter().enumerate() {
        match known.get(name) {
            Some(&id) => variables.push(id),
            None => {
                let id = i32::try_from(parent.variable_names.len()).unwrap_or(i32::MAX);
                parent.variable_names.push(name.clone());
                known.insert(name.clone(), id);
                variables.push(id);
                appended.push(i);
            }
        }
    }
    if !appended.is_empty() {
        let existing = parent.variable_names.len() - appended.len();
        let values = parent.variable_initial_values.get_or_insert_with(|| {
            crate::behavior::VariableValueSet {
                word_values: Vec::new(),
                quad_values: Vec::new(),
                variant_values: Vec::new(),
            }
        });
        values.word_values.resize(existing, 0);
    }
    for &i in &appended {
        parent.variable_infos.push(
            sub.variable_infos
                .get(i)
                .copied()
                .unwrap_or(VariableInfo::default()),
        );
        parent
            .word_min_variable_values
            .push(sub.word_min_variable_values.get(i).copied().unwrap_or(0));
        parent
            .word_max_variable_values
            .push(sub.word_max_variable_values.get(i).copied().unwrap_or(0));
        let initial = sub
            .variable_initial_values
            .as_ref()
            .and_then(|v| v.word_values.get(i))
            .copied()
            .unwrap_or(0);
        if let Some(values) = parent.variable_initial_values.as_mut() {
            values.word_values.push(initial);
        }
    }
    let mut known = index_by_name(&parent.character_property_names);
    let mut properties = Vec::with_capacity(sub.character_property_names.len());
    for (i, name) in sub.character_property_names.iter().enumerate() {
        let id = if let Some(&id) = known.get(name) {
            id
        } else {
            let id = i32::try_from(parent.character_property_names.len()).unwrap_or(i32::MAX);
            parent.character_property_names.push(name.clone());
            parent.character_property_infos.push(
                sub.character_property_infos
                    .get(i)
                    .copied()
                    .unwrap_or_default(),
            );
            known.insert(name.clone(), id);
            id
        };
        properties.push(id);
    }
    SymbolMap {
        events,
        variables,
        properties,
    }
}

fn remap_event_property(e: &mut EventProperty, map: &SymbolMap) {
    e.id = map.event(e.id);
}

fn remap_interval(t: &mut TimeInterval, map: &SymbolMap) {
    t.enter_event_id = map.event(t.enter_event_id);
    t.exit_event_id = map.event(t.exit_event_id);
}

fn remap_transition(t: &mut TransitionInfo, map: &SymbolMap, offset: usize) {
    remap_interval(&mut t.trigger_interval, map);
    remap_interval(&mut t.initiate_interval, map);
    t.event_id = map.event(t.event_id);
    t.transition = t.transition.map(|n| shift(n, offset));
}

fn remap_bindings(b: &mut VariableBindingSet, map: &SymbolMap) {
    for binding in &mut b.bindings {
        match binding.binding_type {
            0 => binding.variable_index = map.variable(binding.variable_index),
            1 => binding.variable_index = map.property(binding.variable_index),
            _ => {}
        }
    }
}

fn remap_state(s: &mut StateInfo, map: &SymbolMap, offset: usize) {
    s.generator = s.generator.map(|n| shift(n, offset));
    for e in s
        .enter_notify_events
        .iter_mut()
        .chain(&mut s.exit_notify_events)
    {
        remap_event_property(e, map);
    }
    for t in &mut s.transitions {
        remap_transition(t, map, offset);
    }
    if let Some(b) = s.bindings.as_mut() {
        remap_bindings(b, map);
    }
}

fn shift(id: NodeId, offset: usize) -> NodeId {
    NodeId(id.0 + offset)
}

fn shift_opt(id: Option<NodeId>, offset: usize) -> Option<NodeId> {
    id.map(|n| shift(n, offset))
}

/// Rewrite one sub-graph node's ids into the parent's numbering.
fn remap_node(node: &mut Node, map: &SymbolMap, offset: usize) {
    node.id = shift(node.id, offset);
    if let Some(b) = node.bindings.as_mut() {
        remap_bindings(b, map);
    }
    match &mut node.kind {
        NodeKind::StateMachine(sm) => {
            remap_event_property(&mut sm.event_to_send_when_state_or_transition_changes, map);
            sm.return_to_previous_state_event_id = map.event(sm.return_to_previous_state_event_id);
            sm.random_transition_event_id = map.event(sm.random_transition_event_id);
            sm.transition_to_next_higher_state_event_id =
                map.event(sm.transition_to_next_higher_state_event_id);
            sm.transition_to_next_lower_state_event_id =
                map.event(sm.transition_to_next_lower_state_event_id);
            sm.sync_variable_index = map.variable(sm.sync_variable_index);
            for s in &mut sm.states {
                remap_state(s, map, offset);
            }
            for t in &mut sm.wildcard_transitions {
                remap_transition(t, map, offset);
            }
        }
        NodeKind::Clip(c) => {
            for t in &mut c.triggers {
                remap_event_property(&mut t.event, map);
            }
        }
        NodeKind::Blender(b) => {
            for child in &mut b.children {
                child.generator = shift_opt(child.generator, offset);
                if let Some(bind) = child.bindings.as_mut() {
                    remap_bindings(bind, map);
                }
            }
        }
        NodeKind::ManualSelector { generators, .. } => {
            for g in generators.iter_mut() {
                *g = shift_opt(*g, offset);
            }
        }
        NodeKind::ModifierGenerator {
            modifier,
            generator,
        } => {
            *modifier = shift_opt(*modifier, offset);
            *generator = shift_opt(*generator, offset);
        }
        NodeKind::ModifierList { modifiers, .. } => {
            for m in modifiers.iter_mut() {
                *m = shift_opt(*m, offset);
            }
        }
        NodeKind::BehaviorReference { resolved, .. } => {
            *resolved = shift_opt(*resolved, offset);
        }
        NodeKind::GeneratorTransition(g) => {
            g.transition_generator = shift_opt(g.transition_generator, offset);
        }
        NodeKind::IStateTagging {
            default_generator: child,
            ..
        }
        | NodeKind::SynchronizedClip {
            clip_generator: child,
            ..
        } => *child = shift_opt(*child, offset),
        NodeKind::CyclicBlendTransition {
            blender_generator,
            event_to_freeze_blend_value,
            event_to_cross_blend,
            ..
        } => {
            *blender_generator = shift_opt(*blender_generator, offset);
            remap_event_property(event_to_freeze_blend_value, map);
            remap_event_property(event_to_cross_blend, map);
        }
        NodeKind::OffsetAnimation {
            default_generator,
            offset_clip_generator,
            ..
        } => {
            *default_generator = shift_opt(*default_generator, offset);
            *offset_clip_generator = shift_opt(*offset_clip_generator, offset);
        }
        NodeKind::BoneSwitch {
            default_generator, ..
        } => *default_generator = shift_opt(*default_generator, offset),
        NodeKind::EventDrivenModifier {
            modifier,
            activate_event_id,
            deactivate_event_id,
            ..
        } => {
            *modifier = shift_opt(*modifier, offset);
            *activate_event_id = map.event(*activate_event_id);
            *deactivate_event_id = map.event(*deactivate_event_id);
        }
        NodeKind::TimerModifier { alarm_event, .. }
        | NodeKind::BsTimerModifier { alarm_event, .. } => remap_event_property(alarm_event, map),
        NodeKind::IStateManager {
            state_variable,
            state_data,
            ..
        } => {
            *state_variable = map.variable(*state_variable);
            for entry in state_data {
                entry.state_machine = shift(entry.state_machine, offset);
            }
        }
        NodeKind::EventOnDeactivateModifier { event, .. } => remap_event_property(event, map),
        NodeKind::FalseToTrueModifier { slots, .. } => {
            for slot in slots {
                remap_event_property(&mut slot.event, map);
            }
        }
        NodeKind::EvaluateExpression { expressions, .. } => {
            for expression in expressions {
                expression.assignment_variable_index =
                    map.variable(expression.assignment_variable_index);
                expression.assignment_event_index = map.event(expression.assignment_event_index);
            }
        }
        NodeKind::ModifyOnce {
            on_activate,
            on_deactivate,
            ..
        } => {
            *on_activate = shift_opt(*on_activate, offset);
            *on_deactivate = shift_opt(*on_deactivate, offset);
        }
        NodeKind::PassByTargetTrigger { trigger, .. } => {
            remap_event_property(&mut trigger.trigger_event, map);
        }
        NodeKind::RagdollContactListener { contact_event, .. } => {
            remap_event_property(contact_event, map);
        }
        NodeKind::FootIkControls { controls, .. } => {
            for leg in &mut controls.legs {
                remap_event_property(&mut leg.ungrounded_event, map);
            }
        }
        NodeKind::EveryNEventsModifier {
            event_to_check_for,
            event_to_send,
            ..
        } => {
            remap_event_property(event_to_check_for, map);
            remap_event_property(event_to_send, map);
        }
        // Nothing here carries a node id, an event or a variable. A blending
        // transition effect is timing only, the rest are leaves.
        NodeKind::BlendingTransition(_)
        | NodeKind::Tweener { .. }
        | NodeKind::GetTimeStepModifier { .. }
        | NodeKind::DirectAt { .. }
        | NodeKind::InterpValue { .. }
        | NodeKind::GetUpModifier { .. }
        | NodeKind::IsActiveModifier { .. }
        | NodeKind::TwistModifier(_)
        | NodeKind::DecomposeVector { .. }
        | NodeKind::SpeedSampler { .. }
        | NodeKind::DampingModifier { .. }
        | NodeKind::LookAtModifier { .. }
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

fn scope_conditions(nodes: &mut [Node], data: &BehaviorGraphData) {
    let scope = Arc::new(ExpressionScope {
        variables: data.variable_names.clone(),
        properties: data.character_property_names.clone(),
    });
    for node in nodes {
        let NodeKind::StateMachine(sm) = &mut node.kind else {
            continue;
        };
        for transition in sm
            .states
            .iter_mut()
            .flat_map(|state| &mut state.transitions)
            .chain(&mut sm.wildcard_transitions)
        {
            if let Some(Condition::Expression(source)) = &mut transition.condition {
                transition.condition = Some(Condition::ScopedExpression {
                    source: std::mem::take(source),
                    scope: Arc::clone(&scope),
                });
            }
        }
    }
}

impl BehaviorGraph {
    /// Splice `sub` in under the behavior reference at `at`, mapping its
    /// symbols into this graph's tables.
    ///
    /// Returns the node id the reference now resolves to.
    ///
    /// # Errors
    ///
    /// [`Error::Unsupported`] when `at` is not a behavior reference, or when
    /// the sub-graph has no root generator to splice.
    pub fn link_reference(&mut self, at: NodeId, mut sub: BehaviorGraph) -> Result<NodeId> {
        let Some(node) = self.nodes.get(at.0) else {
            return Err(Error::Unsupported(format!("no node {at:?} to link")));
        };
        if !matches!(node.kind, NodeKind::BehaviorReference { .. }) {
            return Err(Error::Unsupported(format!(
                "{} is {}, not a behavior reference",
                node.name, node.class
            )));
        }
        let Some(sub_root) = sub.root else {
            return Err(Error::Unsupported(format!(
                "{} has no root generator",
                sub.name
            )));
        };

        scope_conditions(&mut self.nodes, &self.data);
        scope_conditions(&mut sub.nodes, &sub.data);
        let map = merge_symbols(&mut self.data, &sub.data);
        let offset = self.nodes.len();
        let mut nodes = sub.nodes;
        for node in &mut nodes {
            remap_node(node, &map, offset);
        }
        self.nodes.append(&mut nodes);

        let root = shift(sub_root, offset);
        if let Some(NodeKind::BehaviorReference { resolved, .. }) =
            self.nodes.get_mut(at.0).map(|n| &mut n.kind)
        {
            *resolved = Some(root);
        }
        Ok(root)
    }

    /// Link every behavior reference this graph reaches, and every reference
    /// the linked graphs reach in turn, calling `load` for each file name.
    ///
    /// A file the loader cannot supply is left unresolved and named in the
    /// report rather than failing the link, since a graph runs with a
    /// reference missing, it just has nothing to play under it. The same file
    /// referenced twice is loaded twice, because each reference is a separate
    /// instance with its own run-time state.
    ///
    /// `rounds` bounds the walk so a graph that reaches itself terminates.
    pub fn link_all<F>(&mut self, rounds: usize, mut load: F) -> LinkReport
    where
        F: FnMut(&str) -> Option<BehaviorGraph>,
    {
        let mut report = LinkReport::default();
        for _ in 0..rounds {
            let pending = self.unresolved_references();
            if pending.is_empty() {
                break;
            }
            report.rounds += 1;
            let mut progressed = false;
            for (at, name) in pending {
                if report.missing.iter().any(|m| m == &name) {
                    continue;
                }
                let Some(sub) = load(&name) else {
                    report.missing.push(name);
                    continue;
                };
                match self.link_reference(at, sub) {
                    Ok(_) => {
                        report.linked.push(name);
                        progressed = true;
                    }
                    Err(e) => report.failed.push((name, e.to_string())),
                }
            }
            if !progressed {
                break;
            }
        }
        report
    }

    /// Every behavior reference that has not been linked yet, with the file
    /// it names.
    #[must_use]
    pub fn unresolved_references(&self) -> Vec<(NodeId, String)> {
        self.nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::BehaviorReference {
                    behavior_name,
                    resolved: None,
                } => Some((n.id, behavior_name.clone())),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::{BehaviorGraphData, ExpressionData, Node, VariableValueSet};

    fn data(events: &[&str], variables: &[&str]) -> BehaviorGraphData {
        BehaviorGraphData {
            event_names: events.iter().map(|s| (*s).to_string()).collect(),
            event_infos: vec![EventInfo { flags: 0 }; events.len()],
            variable_names: variables.iter().map(|s| (*s).to_string()).collect(),
            variable_infos: vec![VariableInfo::default(); variables.len()],
            word_min_variable_values: vec![0; variables.len()],
            word_max_variable_values: vec![1; variables.len()],
            variable_initial_values: Some(VariableValueSet {
                word_values: vec![0; variables.len()],
                quad_values: Vec::new(),
                variant_values: Vec::new(),
            }),
            ..Default::default()
        }
    }

    fn node(id: usize, name: &str, class: &str, kind: NodeKind) -> Node {
        Node {
            id: NodeId(id),
            offset: id as u32,
            class: class.to_string(),
            name: name.to_string(),
            user_data: 0,
            bindings: None,
            kind,
        }
    }

    fn reference_graph() -> BehaviorGraph {
        BehaviorGraph {
            name: "parent".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: data(&["shared", "parentOnly"], &["Speed"]),
            nodes: vec![node(
                0,
                "ref",
                "hkbBehaviorReferenceGenerator",
                NodeKind::BehaviorReference {
                    behavior_name: "Behaviors\\Sub.hkx".into(),
                    resolved: None,
                },
            )],
        }
    }

    fn sub_graph() -> BehaviorGraph {
        // Event 0 "subOnly" is new, event 1 "shared" exists in the parent at
        // index 0, so the ids must cross over rather than line up.
        let clip = crate::behavior::ClipGenerator {
            animation_name: "a.hkx".into(),
            triggers: vec![crate::behavior::ClipTrigger {
                local_time: 0.5,
                event: EventProperty {
                    id: 1,
                    has_payload: false,
                },
                relative_to_end_of_clip: false,
                acyclic: false,
                is_annotation: false,
            }],
            ..Default::default()
        };
        BehaviorGraph {
            name: "sub".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: data(&["subOnly", "shared"], &["subVar", "Speed"]),
            nodes: vec![node(0, "clip", "hkbClipGenerator", NodeKind::Clip(clip))],
        }
    }

    #[test]
    fn a_shared_event_keeps_the_parent_id_and_a_new_one_is_appended() {
        let mut parent = reference_graph();
        parent.link_reference(NodeId(0), sub_graph()).unwrap();
        // "subOnly" was not in the parent, so it lands after its two events.
        assert_eq!(parent.data.event_names, ["shared", "parentOnly", "subOnly"]);
        assert_eq!(parent.data.event_infos.len(), 3);
        // The clip trigger named sub event 1, "shared", which is parent 0.
        let NodeKind::Clip(c) = &parent.nodes[1].kind else {
            panic!("expected the spliced clip");
        };
        assert_eq!(c.triggers[0].event.id, 0);
    }

    #[test]
    fn anm_40_linked_expression_targets_follow_symbol_maps() {
        let mut parent = reference_graph();
        let expression = ExpressionData {
            expression: "subVar".into(),
            assignment_variable_index: 0,
            assignment_event_index: 0,
            event_mode: 0,
        };
        let sub = BehaviorGraph {
            name: "expression subgraph".into(),
            variable_mode: 0,
            root: Some(NodeId(0)),
            data: data(&["subOnly", "shared"], &["subVar", "Speed"]),
            nodes: vec![node(
                0,
                "evaluate",
                "hkbEvaluateExpressionModifier",
                NodeKind::EvaluateExpression {
                    enable: true,
                    expressions: vec![expression],
                },
            )],
        };

        parent.link_reference(NodeId(0), sub).unwrap();

        let NodeKind::EvaluateExpression { expressions, .. } = &parent.nodes[1].kind else {
            panic!("linked expression modifier was not retained");
        };
        assert_eq!(expressions[0].assignment_variable_index, 1);
        assert_eq!(expressions[0].assignment_event_index, 2);
    }

    #[test]
    fn a_shared_variable_keeps_its_parent_index_and_the_tables_stay_aligned() {
        let mut parent = reference_graph();
        parent.link_reference(NodeId(0), sub_graph()).unwrap();
        assert_eq!(parent.data.variable_names, ["Speed", "subVar"]);
        let n = parent.data.variable_names.len();
        assert_eq!(parent.data.variable_infos.len(), n);
        assert_eq!(parent.data.word_min_variable_values.len(), n);
        assert_eq!(parent.data.word_max_variable_values.len(), n);
        assert_eq!(
            parent
                .data
                .variable_initial_values
                .as_ref()
                .unwrap()
                .word_values
                .len(),
            n
        );
    }

    #[test]
    fn the_reference_resolves_to_the_spliced_root() {
        let mut parent = reference_graph();
        let root = parent.link_reference(NodeId(0), sub_graph()).unwrap();
        assert_eq!(root, NodeId(1));
        assert!(parent.unresolved_references().is_empty());
        let NodeKind::BehaviorReference { resolved, .. } = &parent.nodes[0].kind else {
            panic!("expected the reference");
        };
        assert_eq!(*resolved, Some(NodeId(1)));
        assert_eq!(
            parent.nodes[1].id,
            NodeId(1),
            "node ids shift with the graph"
        );
    }

    #[test]
    fn linking_a_node_that_is_not_a_reference_is_refused() {
        let mut parent = reference_graph();
        parent.nodes[0].kind = NodeKind::Unknown;
        assert!(parent.link_reference(NodeId(0), sub_graph()).is_err());
    }

    #[test]
    fn link_all_walks_a_chain_and_names_what_it_could_not_load() {
        // parent -> Sub.hkx -> Deeper.hkx, and Deeper names a file the
        // loader has nothing for.
        let mut sub = sub_graph();
        sub.nodes.push(node(
            1,
            "deeper",
            "hkbBehaviorReferenceGenerator",
            NodeKind::BehaviorReference {
                behavior_name: "Behaviors\\Deeper.hkx".into(),
                resolved: None,
            },
        ));
        let mut parent = reference_graph();
        let report = parent.link_all(8, |name| match name {
            "Behaviors\\Sub.hkx" => Some(sub.clone()),
            _ => None,
        });
        assert_eq!(report.linked, ["Behaviors\\Sub.hkx"]);
        assert_eq!(report.missing, ["Behaviors\\Deeper.hkx"]);
        assert!(report.failed.is_empty());
        assert_eq!(report.rounds, 2, "one round to link, one to find the gap");
    }

    #[test]
    fn a_reference_cycle_terminates_instead_of_growing_forever() {
        // A graph whose reference names its own file would splice forever
        // without the round bound.
        let mut parent = reference_graph();
        let mut calls = 0;
        let report = parent.link_all(4, |_| {
            calls += 1;
            let mut g = sub_graph();
            g.nodes.push(node(
                1,
                "again",
                "hkbBehaviorReferenceGenerator",
                NodeKind::BehaviorReference {
                    behavior_name: "Behaviors\\Sub.hkx".into(),
                    resolved: None,
                },
            ));
            Some(g)
        });
        assert_eq!(report.rounds, 4, "the bound stops it");
        assert_eq!(calls, 4);
    }

    #[test]
    fn an_unresolved_reference_is_listed_with_its_file() {
        let parent = reference_graph();
        assert_eq!(
            parent.unresolved_references(),
            vec![(NodeId(0), "Behaviors\\Sub.hkx".to_string())]
        );
    }
}
