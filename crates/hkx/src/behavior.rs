// SPDX-License-Identifier: GPL-3.0-or-later

//! The `hkb` behavior graph as data.
//!
//! A behavior file holds one `hkbBehaviorGraph`: a root generator, a
//! `hkbBehaviorGraphData` with the variable and event tables, and a graph
//! of nodes the root reaches through pointers. Nodes are generators (they
//! produce a pose), modifiers (they change one) or transition effects (they
//! blend between two generators during a state change). Skyrim adds its own
//! `BS*` node classes on top of the stock `hkb*` ones.
//!
//! This module turns that pointer graph into an arena of [`Node`]s indexed
//! by [`NodeId`], with the classes an interpreter needs decoded field by
//! field and everything else kept as [`NodeKind::Unknown`] with its class
//! name, so a graph never fails to load because of a node type this crate
//! has not modelled yet. Paired offsets cover the x86 and x64 `hk_2010.2.0-r1` layouts,
//! cross-checked against activeragdoll's runtime layouts where the two
//! overlap (`hkbNode`, `hkbStateMachine`, `hkbBlendingTransitionEffect`).

use std::collections::HashMap;
use std::sync::Arc;

use crate::packfile::{root_variant_of, Obj};
use crate::{Packfile, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VariableType {
    /// A variable an appended sub-graph declared with no info of its own
    /// reads as a bool, the narrowest word type.
    #[default]
    Bool,
    Int8,
    Int16,
    Int32,
    Real,
    Pointer,
    Vector3,
    Vector4,
    Quaternion,
    Invalid(i8),
}

impl VariableType {
    pub fn from_raw(v: i8) -> VariableType {
        match v {
            0 => VariableType::Bool,
            1 => VariableType::Int8,
            2 => VariableType::Int16,
            3 => VariableType::Int32,
            4 => VariableType::Real,
            5 => VariableType::Pointer,
            6 => VariableType::Vector3,
            7 => VariableType::Vector4,
            8 => VariableType::Quaternion,
            n => VariableType::Invalid(n),
        }
    }

    /// Whether the value lives in the word table (one `i32` slot) or the
    /// quad table (a 16-byte vector).
    pub fn is_word(&self) -> bool {
        !matches!(
            self,
            VariableType::Vector3 | VariableType::Vector4 | VariableType::Quaternion
        )
    }
}

/// `hkbVariableInfo`: a role (0 default, 1 file name, 2 bone index, 3 bone
/// index map, 4 event id, 5 variable index, 6 attribute index, 7 time),
/// role flags (1 ragdoll, 2 normalized, 4 not variable, 8 hidden, 16
/// output, 32 not character property) and a type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VariableInfo {
    pub role: i16,
    pub role_flags: i16,
    pub ty: VariableType,
}

impl VariableInfo {
    /// An `hkArray<hkbVariableInfo>` field. Elements are 6 bytes with a
    /// 2-byte alignment, so the array stride is 6.
    pub fn array(o: Obj<'_>, off: u32) -> Result<Vec<VariableInfo>> {
        o.array_structs(off, 6)?
            .into_iter()
            .map(|v| {
                Ok(VariableInfo {
                    role: v.i16(0)?,
                    role_flags: v.i16(2)?,
                    ty: VariableType::from_raw(v.i8(4)?),
                })
            })
            .collect()
    }
}

/// Flag slots a `BSIsActiveModifier` carries, from its layout.
pub const IS_ACTIVE_SLOTS: usize = 5;

/// `hkbEventInfo.flags`: 1 silent, 2 sync point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventInfo {
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariableValueSet {
    pub word_values: Vec<i32>,
    pub quad_values: Vec<[f32; 4]>,
    /// Object offsets of the variant (pointer) values, null as `None`.
    pub variant_values: Vec<Option<u32>>,
}

impl VariableValueSet {
    /// wordVariableValues 16, quadVariableValues 32, variantVariableValues 48.
    pub fn from_obj(o: Obj<'_>) -> Result<VariableValueSet> {
        o.expect_class("hkbVariableValueSet")?;
        Ok(VariableValueSet {
            word_values: o.array_i32(o.layout_offset(8, 16))?,
            quad_values: o.array_vec4(o.layout_offset(20, 32))?,
            variant_values: o
                .array_ptrs(o.layout_offset(32, 48))?
                .into_iter()
                .map(|p| p.map(|p| p.base))
                .collect(),
        })
    }
}

/// The graph's symbol tables: what variables, events, attributes and
/// character properties exist and what they are called.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BehaviorGraphData {
    pub attribute_defaults: Vec<f32>,
    pub variable_infos: Vec<VariableInfo>,
    pub character_property_infos: Vec<VariableInfo>,
    pub event_infos: Vec<EventInfo>,
    pub word_min_variable_values: Vec<i32>,
    pub word_max_variable_values: Vec<i32>,
    pub variable_initial_values: Option<VariableValueSet>,
    pub event_names: Vec<String>,
    pub attribute_names: Vec<String>,
    pub variable_names: Vec<String>,
    pub character_property_names: Vec<String>,
}

impl BehaviorGraphData {
    /// hkbBehaviorGraphData: attributeDefaults 16, variableInfos 32,
    /// characterPropertyInfos 48, eventInfos 64, wordMinVariableValues 80,
    /// wordMaxVariableValues 96, variableInitialValues 112, stringData 120.
    /// hkbBehaviorGraphStringData: eventNames 16, attributeNames 32,
    /// variableNames 48, characterPropertyNames 64.
    pub fn from_obj(o: Obj<'_>) -> Result<BehaviorGraphData> {
        o.expect_class("hkbBehaviorGraphData")?;
        let mut d = BehaviorGraphData {
            attribute_defaults: o.array_f32(o.layout_offset(8, 16))?,
            variable_infos: VariableInfo::array(o, o.layout_offset(20, 32))?,
            character_property_infos: VariableInfo::array(o, o.layout_offset(32, 48))?,
            event_infos: o
                .array_u32(o.layout_offset(44, 64))?
                .into_iter()
                .map(|flags| EventInfo { flags })
                .collect(),
            word_min_variable_values: o.array_i32(o.layout_offset(56, 80))?,
            word_max_variable_values: o.array_i32(o.layout_offset(68, 96))?,
            variable_initial_values: o
                .ptr(o.layout_offset(80, 112))?
                .map(VariableValueSet::from_obj)
                .transpose()?,
            ..Default::default()
        };
        if let Some(s) = o.ptr(o.layout_offset(84, 120))? {
            s.expect_class("hkbBehaviorGraphStringData")?;
            d.event_names = s.array_strings(s.layout_offset(8, 16))?;
            d.attribute_names = s.array_strings(s.layout_offset(20, 32))?;
            d.variable_names = s.array_strings(s.layout_offset(32, 48))?;
            d.character_property_names = s.array_strings(s.layout_offset(44, 64))?;
        }
        Ok(d)
    }

    pub fn event_id(&self, name: &str) -> Option<i32> {
        self.event_names
            .iter()
            .position(|n| n == name)
            .map(|i| i as i32)
    }

    pub fn variable_index(&self, name: &str) -> Option<i32> {
        self.variable_names
            .iter()
            .position(|n| n == name)
            .map(|i| i as i32)
    }
}

/// `hkbVariableBindingSetBinding`: a node member driven by a variable or a
/// character property. `binding_type` 0 is variable, 1 character property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub member_path: String,
    pub variable_index: i32,
    pub bit_index: i8,
    pub binding_type: i8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableBindingSet {
    pub bindings: Vec<Binding>,
    pub index_of_binding_to_enable: i32,
}

impl VariableBindingSet {
    /// bindings 16 (40-byte elements: memberPath 0, variableIndex 28,
    /// bitIndex 32, bindingType 33), indexOfBindingToEnable 32.
    pub fn from_obj(o: Obj<'_>) -> Result<VariableBindingSet> {
        o.expect_class("hkbVariableBindingSet")?;
        Ok(VariableBindingSet {
            bindings: o
                .array_structs(o.layout_offset(8, 16), o.layout_offset(32, 40))?
                .into_iter()
                .map(|b| {
                    Ok(Binding {
                        member_path: b.string_or_empty(0)?,
                        variable_index: b.i32(b.layout_offset(20, 28))?,
                        bit_index: b.i8(b.layout_offset(24, 32))?,
                        binding_type: b.i8(b.layout_offset(25, 33))?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            index_of_binding_to_enable: o.i32(o.layout_offset(20, 32))?,
        })
    }
}

/// `hkbEventProperty`: an event id and whether a payload object is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventProperty {
    pub id: i32,
    pub has_payload: bool,
}

impl EventProperty {
    fn read(o: Obj<'_>, off: u32) -> Result<EventProperty> {
        Ok(EventProperty {
            id: o.i32(off)?,
            has_payload: o.ptr(off + o.layout_offset(4, 8))?.is_some(),
        })
    }

    fn array_from(o: Option<Obj<'_>>) -> Result<Vec<EventProperty>> {
        let Some(o) = o else {
            return Ok(Vec::new());
        };
        o.expect_class("hkbStateMachineEventPropertyArray")?;
        o.array_structs(o.layout_offset(8, 16), o.layout_offset(8, 16))?
            .into_iter()
            .map(|e| EventProperty::read(e, 0))
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeInterval {
    pub enter_event_id: i32,
    pub exit_event_id: i32,
    pub enter_time: f32,
    pub exit_time: f32,
}

impl TimeInterval {
    fn read(o: Obj<'_>, off: u32) -> Result<TimeInterval> {
        Ok(TimeInterval {
            enter_event_id: o.i32(off)?,
            exit_event_id: o.i32(off + 4)?,
            enter_time: o.f32(off + 8)?,
            exit_time: o.f32(off + 12)?,
        })
    }
}

/// `hkbStateMachineTransitionInfo.flags`.
pub mod transition_flags {
    pub const USE_TRIGGER_INTERVAL: u16 = 1;
    pub const USE_INITIATE_INTERVAL: u16 = 2;
    pub const UNINTERRUPTIBLE_WHILE_PLAYING: u16 = 4;
    pub const UNINTERRUPTIBLE_WHILE_DELAYED: u16 = 8;
    pub const DELAY_STATE_CHANGE: u16 = 16;
    pub const DISABLED: u16 = 32;
    pub const DISALLOW_RETURN_TO_PREVIOUS_STATE: u16 = 64;
    pub const DISALLOW_RANDOM_TRANSITION: u16 = 128;
    pub const DISABLE_CONDITION: u16 = 256;
    pub const ALLOW_SELF_TRANSITION_BY_TRANSITION_FROM_ANY_STATE: u16 = 512;
    pub const IS_GLOBAL_WILDCARD: u16 = 1024;
    pub const IS_LOCAL_WILDCARD: u16 = 2048;
    pub const FROM_NESTED_STATE_ID_IS_VALID: u16 = 4096;
    pub const TO_NESTED_STATE_ID_IS_VALID: u16 = 8192;
    pub const ABUT_AT_END_OF_FROM_GENERATOR: u16 = 16384;
}

/// Original expression symbol namespaces, retained across graph linking.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExpressionScope {
    pub(crate) variables: Vec<String>,
    pub(crate) properties: Vec<String>,
}

/// Serialized transition guard. Compiled expression caches are not serialized.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Condition {
    /// Source text in `hkbExpressionCondition` or legacy `hkbStringCondition`.
    Expression(String),
    /// Linked expression with its originating symbol namespaces.
    ScopedExpression {
        source: String,
        scope: Arc<ExpressionScope>,
    },
    /// A condition class without an evaluator. It never passes a guard.
    Unsupported(String),
}

impl Condition {
    pub(crate) fn expression(&self) -> Option<(&str, Option<&ExpressionScope>)> {
        match self {
            Self::Expression(source) => Some((source, None)),
            Self::ScopedExpression { source, scope } => Some((source, Some(scope))),
            Self::Unsupported(_) => None,
        }
    }

    fn from_obj(o: Obj<'_>) -> Result<Self> {
        let class = o.class().unwrap_or("unknown");
        if matches!(class, "hkbExpressionCondition" | "hkbStringCondition") {
            Ok(Self::Expression(o.string_or_empty(o.layout_offset(8, 16))?))
        } else {
            Ok(Self::Unsupported(class.to_string()))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionInfo {
    pub trigger_interval: TimeInterval,
    pub initiate_interval: TimeInterval,
    /// The transition effect node, when one is set.
    pub transition: Option<NodeId>,
    /// A decoded guard, or an unsupported condition class.
    pub condition: Option<Condition>,
    pub event_id: i32,
    pub to_state_id: i32,
    pub from_nested_state_id: i32,
    pub to_nested_state_id: i32,
    pub priority: i16,
    pub flags: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateInfo {
    pub name: String,
    pub state_id: i32,
    pub probability: f32,
    pub enable: bool,
    pub generator: Option<NodeId>,
    pub enter_notify_events: Vec<EventProperty>,
    pub exit_notify_events: Vec<EventProperty>,
    pub transitions: Vec<TransitionInfo>,
    pub bindings: Option<VariableBindingSet>,
}

/// `hkbStateMachine.startStateMode`: 0 default, 1 sync, 2 random, 3 chooser.
/// `selfTransitionMode`: 0 no transition, 1 transition to start state, 2
/// force transition to start state.
#[derive(Debug, Clone, PartialEq)]
pub struct StateMachine {
    pub event_to_send_when_state_or_transition_changes: EventProperty,
    pub start_state_id: i32,
    pub return_to_previous_state_event_id: i32,
    pub random_transition_event_id: i32,
    pub transition_to_next_higher_state_event_id: i32,
    pub transition_to_next_lower_state_event_id: i32,
    pub sync_variable_index: i32,
    pub wrap_around_state_id: bool,
    pub max_simultaneous_transitions: i8,
    pub start_state_mode: u8,
    pub self_transition_mode: u8,
    pub states: Vec<StateInfo>,
    pub wildcard_transitions: Vec<TransitionInfo>,
    pub has_start_state_chooser: bool,
}

impl StateMachine {
    pub fn state(&self, id: i32) -> Option<&StateInfo> {
        self.states.iter().find(|s| s.state_id == id)
    }
}

/// `hkbClipGenerator.mode`: 0 single play, 1 looping, 2 user controlled,
/// 3 ping pong. `flags`: 1 continue motion at end, 2 sync half cycle in
/// ping pong, 4 mirror, 8 force dense pose, 16 do not convert annotations
/// to triggers, 32 ignore motion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClipGenerator {
    pub animation_name: String,
    pub triggers: Vec<ClipTrigger>,
    pub crop_start_amount_local_time: f32,
    pub crop_end_amount_local_time: f32,
    pub start_time: f32,
    pub playback_speed: f32,
    pub enforced_duration: f32,
    pub user_controlled_time_fraction: f32,
    pub animation_binding_index: i16,
    pub mode: i8,
    pub flags: i8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipTrigger {
    pub local_time: f32,
    pub event: EventProperty,
    pub relative_to_end_of_clip: bool,
    pub acyclic: bool,
    pub is_annotation: bool,
}

/// Read an authored bone mask, retaining zero weights and array order.
///
/// # Errors
/// Rejects the wrong class, malformed arrays and negative or non-finite weights.
pub fn bone_weights(o: Obj<'_>) -> Result<Vec<f32>> {
    o.expect_class("hkbBoneWeightArray")?;
    if o.i32(o.layout_offset(32, 56))? < 0 {
        return Err(crate::Error::Unsupported(
            "negative bone mask length".into(),
        ));
    }
    let weights = o.array_f32(o.layout_offset(28, 48))?;
    if weights.len() != o.i32(o.layout_offset(32, 56))? as usize {
        return Err(crate::Error::NullPointer("nonempty bone mask array"));
    }
    if weights.iter().any(|w| !w.is_finite() || *w < 0.0) {
        return Err(crate::Error::Unsupported("invalid bone mask weight".into()));
    }
    Ok(weights)
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlenderChild {
    pub generator: Option<NodeId>,
    /// Per-bone mask. Missing masks give every bone weight one.
    pub bone_weights: Option<Vec<f32>>,
    pub weight: f32,
    pub world_from_model_weight: f32,
    pub bindings: Option<VariableBindingSet>,
}

/// `hkbBlenderGenerator.flags`: 1 sync, 4 smooth generator weights, 8 do
/// not deactivate children with zero weights, 16 parametric blend, 32
/// parametric blend is cyclic, 64 force dense pose.
#[derive(Debug, Clone, PartialEq)]
pub struct BlenderGenerator {
    pub reference_pose_weight_threshold: f32,
    pub blend_parameter: f32,
    pub min_cyclic_blend_parameter: f32,
    pub max_cyclic_blend_parameter: f32,
    pub index_of_sync_master_child: i16,
    pub flags: i16,
    pub subtract_last_child: bool,
    pub children: Vec<BlenderChild>,
}

/// `hkbTransitionEffect`: selfTransitionMode (0 continue if cyclic and
/// blend if acyclic, 1 continue, 2 reset, 3 blend), eventMode (0 default,
/// 1 process all, 2 ignore from generator, 3 ignore to generator).
#[derive(Debug, Clone, PartialEq)]
pub struct BlendingTransitionEffect {
    pub self_transition_mode: i8,
    pub event_mode: i8,
    pub duration: f32,
    pub to_generator_start_time_fraction: f32,
    /// 1 ignore from world-from-model, 2 sync, 4 ignore to world-from-model.
    pub flags: u16,
    /// 0 none, 1 transition until end of from generator, 2 cap duration.
    pub end_mode: i8,
    pub blend_curve: i8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratorTransitionEffect {
    pub self_transition_mode: i8,
    pub event_mode: i8,
    pub transition_generator: Option<NodeId>,
    pub blend_in_duration: f32,
    pub blend_out_duration: f32,
    pub sync_to_generator_start_time: bool,
}

/// Event slots carried by `BSEventOnFalseToTrueModifier`.
pub const FALSE_TO_TRUE_SLOTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FalseToTrueSlot {
    pub enable: bool,
    pub variable_to_test: bool,
    pub event: EventProperty,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    StateMachine(StateMachine),
    Clip(ClipGenerator),
    Blender(BlenderGenerator),
    ManualSelector {
        generators: Vec<Option<NodeId>>,
        selected_generator_index: i8,
        current_generator_index: i8,
    },
    ModifierGenerator {
        modifier: Option<NodeId>,
        generator: Option<NodeId>,
    },
    ModifierList {
        enable: bool,
        modifiers: Vec<Option<NodeId>>,
    },
    /// `hkbBehaviorReferenceGenerator`. The file serializes only the name,
    /// its `behavior` pointer is `SERIALIZE_IGNORED`, so `resolved` stays
    /// `None` until [`BehaviorGraph::link_reference`] splices the referenced
    /// graph in. See `crate::link`.
    BehaviorReference {
        behavior_name: String,
        resolved: Option<NodeId>,
    },
    BlendingTransition(BlendingTransitionEffect),
    GeneratorTransition(GeneratorTransitionEffect),
    /// `BSiStateTaggingGenerator`: sets the Skyrim iState variable while
    /// its child plays.
    IStateTagging {
        default_generator: Option<NodeId>,
        state_to_set_as: i32,
        priority: i32,
    },
    /// `BSSynchronizedClipGenerator`: paired animations (kill moves).
    SynchronizedClip {
        clip_generator: Option<NodeId>,
        sync_anim_prefix: String,
        sync_clip_ignore_mark_placement: bool,
        get_to_mark_time: f32,
        mark_error_threshold: f32,
        lead_character: bool,
        reorient_support_char: bool,
        apply_motion_from_root: bool,
        animation_binding_index: i16,
    },
    CyclicBlendTransition {
        blender_generator: Option<NodeId>,
        event_to_freeze_blend_value: EventProperty,
        event_to_cross_blend: EventProperty,
        blend_parameter: f32,
        transition_duration: f32,
        blend_curve: i8,
    },
    OffsetAnimation {
        default_generator: Option<NodeId>,
        offset_clip_generator: Option<NodeId>,
        offset_variable: f32,
        offset_range_start: f32,
        offset_range_end: f32,
    },
    BoneSwitch {
        default_generator: Option<NodeId>,
        /// Bone data children are kept as object offsets, they carry a
        /// generator each which is not walked yet.
        children: Vec<Option<u32>>,
    },
    EventDrivenModifier {
        enable: bool,
        modifier: Option<NodeId>,
        activate_event_id: i32,
        deactivate_event_id: i32,
        active_by_default: bool,
    },
    TimerModifier {
        enable: bool,
        alarm_time_seconds: f32,
        alarm_event: EventProperty,
    },
    /// `BSGetTimeStepModifier`: writes the current graph update interval to
    /// its `timeStep` output binding.
    GetTimeStepModifier {
        enable: bool,
        time_step: f32,
    },
    /// `BSIsActiveModifier`: five slots, each a flag the graph binds a
    /// variable to and an invert bit. Skyrim uses it to turn parts of a
    /// graph on from script, and its bindings are the bulk of what an
    /// unmodelled modifier leaves unbound.
    IsActiveModifier {
        enable: bool,
        is_active: [bool; IS_ACTIVE_SLOTS],
        invert_active: [bool; IS_ACTIVE_SLOTS],
    },
    FalseToTrueModifier {
        enable: bool,
        slots: [FalseToTrueSlot; FALSE_TO_TRUE_SLOTS],
    },
    /// `BSModifyOnceModifier`: activates its selected child modifiers on
    /// the corresponding parent lifecycle edge.
    ModifyOnce {
        enable: bool,
        on_activate: Option<NodeId>,
        on_deactivate: Option<NodeId>,
    },
    /// `BSDecomposeVectorModifier`: copies the four components of `vector`
    /// into its real outputs.
    DecomposeVector {
        enable: bool,
        vector: [f32; 4],
    },
    /// Any other `hkbModifier` subclass: only the enable flag is read.
    Modifier {
        enable: bool,
    },
    /// A generator class this crate has no fields for.
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: NodeId,
    /// Data offset of the object in the file, stable across reloads.
    pub offset: u32,
    pub class: String,
    pub name: String,
    pub user_data: u64,
    pub bindings: Option<VariableBindingSet>,
    pub kind: NodeKind,
}

impl Node {
    /// Direct children in graph order, for walks.
    pub fn children(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        match &self.kind {
            NodeKind::StateMachine(sm) => {
                for s in &sm.states {
                    out.extend(s.generator);
                    out.extend(s.transitions.iter().filter_map(|t| t.transition));
                }
                out.extend(sm.wildcard_transitions.iter().filter_map(|t| t.transition));
            }
            NodeKind::Blender(b) => out.extend(b.children.iter().filter_map(|c| c.generator)),
            NodeKind::ManualSelector { generators, .. } => {
                out.extend(generators.iter().flatten().copied());
            }
            NodeKind::ModifierGenerator {
                modifier,
                generator,
            } => {
                out.extend(*modifier);
                out.extend(*generator);
            }
            NodeKind::ModifierList { modifiers, .. } => {
                out.extend(modifiers.iter().flatten().copied());
            }
            NodeKind::GeneratorTransition(g) => out.extend(g.transition_generator),
            NodeKind::IStateTagging {
                default_generator, ..
            }
            | NodeKind::BoneSwitch {
                default_generator, ..
            } => out.extend(*default_generator),
            NodeKind::SynchronizedClip { clip_generator, .. } => out.extend(*clip_generator),
            NodeKind::CyclicBlendTransition {
                blender_generator, ..
            } => out.extend(*blender_generator),
            NodeKind::OffsetAnimation {
                default_generator,
                offset_clip_generator,
                ..
            } => {
                out.extend(*default_generator);
                out.extend(*offset_clip_generator);
            }
            NodeKind::EventDrivenModifier { modifier, .. } => out.extend(*modifier),
            NodeKind::ModifyOnce {
                on_activate,
                on_deactivate,
                ..
            } => {
                out.extend(*on_activate);
                out.extend(*on_deactivate);
            }
            // Only after linking. An unlinked reference has no child, which
            // is why an unlinked graph leaves the state it guards silent.
            NodeKind::BehaviorReference { resolved, .. } => out.extend(*resolved),
            NodeKind::Clip(_)
            | NodeKind::BlendingTransition(_)
            | NodeKind::TimerModifier { .. }
            | NodeKind::GetTimeStepModifier { .. }
            | NodeKind::IsActiveModifier { .. }
            | NodeKind::FalseToTrueModifier { .. }
            | NodeKind::DecomposeVector { .. }
            | NodeKind::Modifier { .. }
            | NodeKind::Unknown => {}
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BehaviorGraph {
    pub name: String,
    /// 0 discard variables when inactive, 1 keep them.
    pub variable_mode: i8,
    pub root: Option<NodeId>,
    pub data: BehaviorGraphData,
    pub nodes: Vec<Node>,
}

impl BehaviorGraph {
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }

    pub fn nodes_of_class<'a>(&'a self, class: &'a str) -> impl Iterator<Item = &'a Node> {
        self.nodes.iter().filter(move |n| n.class == class)
    }

    /// Animation names every clip generator refers to, deduplicated.
    pub fn clip_animation_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Clip(c) => Some(c.animation_name.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Names of behavior files this graph references.
    pub fn referenced_behaviors(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::BehaviorReference { behavior_name, .. } => Some(behavior_name.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// hkbBehaviorGraph: name 56 (hkbNode), variableMode 72,
    /// rootGenerator 128, data 136.
    pub fn from_obj(o: Obj<'_>) -> Result<BehaviorGraph> {
        o.expect_class("hkbBehaviorGraph")?;
        let data = match o.ptr(o.layout_offset(80, 136))? {
            Some(d) => BehaviorGraphData::from_obj(d)?,
            None => BehaviorGraphData::default(),
        };
        let mut loader = Loader {
            nodes: Vec::new(),
            by_offset: HashMap::new(),
            pending: Vec::new(),
        };
        let root = o.ptr(o.layout_offset(76, 128))?.map(|r| loader.node_id(r));
        loader.drain()?;
        Ok(BehaviorGraph {
            name: o.string_or_empty(o.layout_offset(32, 56))?,
            variable_mode: o.i8(o.layout_offset(40, 72))?,
            root,
            data,
            nodes: loader.nodes,
        })
    }

    pub fn from_packfile(pf: &Packfile) -> Result<BehaviorGraph> {
        Self::from_obj(root_variant_of(pf, "hkbBehaviorGraph")?)
    }
}

struct Loader<'a> {
    nodes: Vec<Node>,
    by_offset: HashMap<u32, NodeId>,
    pending: Vec<(NodeId, Obj<'a>)>,
}

impl<'a> Loader<'a> {
    /// Reserve an id for an object, queueing it for decoding on first
    /// sight. Cycles (a state machine that reaches itself through a
    /// behavior reference is the usual one) are fine: the id exists before
    /// the node is decoded.
    fn node_id(&mut self, o: Obj<'a>) -> NodeId {
        if let Some(&id) = self.by_offset.get(&o.base) {
            return id;
        }
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            id,
            offset: o.base,
            class: o.class().unwrap_or("<unnamed>").to_string(),
            name: String::new(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::Unknown,
        });
        self.by_offset.insert(o.base, id);
        self.pending.push((id, o));
        id
    }

    fn opt_id(&mut self, o: Option<Obj<'a>>) -> Option<NodeId> {
        o.map(|o| self.node_id(o))
    }

    fn drain(&mut self) -> Result<()> {
        while let Some((id, o)) = self.pending.pop() {
            let node = self.decode(id, o)?;
            self.nodes[id.0] = node;
        }
        Ok(())
    }

    fn transitions(&mut self, arr: Option<Obj<'a>>) -> Result<Vec<TransitionInfo>> {
        let Some(arr) = arr else {
            return Ok(Vec::new());
        };
        arr.expect_class("hkbStateMachineTransitionInfoArray")?;
        let mut out = Vec::new();
        for t in arr.array_structs(arr.layout_offset(8, 16), arr.layout_offset(60, 72))? {
            let transition = self.opt_id(t.ptr(32)?);
            out.push(TransitionInfo {
                trigger_interval: TimeInterval::read(t, 0)?,
                initiate_interval: TimeInterval::read(t, 16)?,
                transition,
                condition: t
                    .ptr(t.layout_offset(36, 40))?
                    .map(Condition::from_obj)
                    .transpose()?,
                event_id: t.i32(t.layout_offset(40, 48))?,
                to_state_id: t.i32(t.layout_offset(44, 52))?,
                from_nested_state_id: t.i32(t.layout_offset(48, 56))?,
                to_nested_state_id: t.i32(t.layout_offset(52, 60))?,
                priority: t.i16(t.layout_offset(56, 64))?,
                flags: t.u16(t.layout_offset(58, 66))?,
            });
        }
        Ok(out)
    }

    fn bindings(o: Obj<'a>) -> Result<Option<VariableBindingSet>> {
        o.ptr(o.layout_offset(8, 16))?
            .map(VariableBindingSet::from_obj)
            .transpose()
    }

    fn state_machine(&mut self, o: Obj<'a>) -> Result<StateMachine> {
        let mut states = Vec::new();
        for s in o
            .array_ptrs(o.layout_offset(92, 144))?
            .into_iter()
            .flatten()
        {
            s.expect_class("hkbStateMachineStateInfo")?;
            let generator = self.opt_id(s.ptr(s.layout_offset(52, 88))?);
            let transitions = self.transitions(s.ptr(s.layout_offset(48, 80))?)?;
            states.push(StateInfo {
                name: s.string_or_empty(s.layout_offset(56, 96))?,
                state_id: s.i32(s.layout_offset(60, 104))?,
                probability: s.f32(s.layout_offset(64, 108))?,
                enable: s.bool(s.layout_offset(68, 112))?,
                generator,
                enter_notify_events: EventProperty::array_from(s.ptr(s.layout_offset(40, 64))?)?,
                exit_notify_events: EventProperty::array_from(s.ptr(s.layout_offset(44, 72))?)?,
                transitions,
                bindings: Self::bindings(s)?,
            });
        }
        let wildcard_transitions = self.transitions(o.ptr(o.layout_offset(104, 160))?)?;
        Ok(StateMachine {
            event_to_send_when_state_or_transition_changes: EventProperty::read(
                o,
                o.layout_offset(40, 72),
            )?,
            start_state_id: o.i32(o.layout_offset(56, 104))?,
            return_to_previous_state_event_id: o.i32(o.layout_offset(60, 108))?,
            random_transition_event_id: o.i32(o.layout_offset(64, 112))?,
            transition_to_next_higher_state_event_id: o.i32(o.layout_offset(68, 116))?,
            transition_to_next_lower_state_event_id: o.i32(o.layout_offset(72, 120))?,
            sync_variable_index: o.i32(o.layout_offset(76, 124))?,
            wrap_around_state_id: o.bool(o.layout_offset(84, 132))?,
            max_simultaneous_transitions: o.i8(o.layout_offset(85, 133))?,
            start_state_mode: o.u8(o.layout_offset(86, 134))?,
            self_transition_mode: o.u8(o.layout_offset(87, 135))?,
            states,
            wildcard_transitions,
            has_start_state_chooser: o.ptr(o.layout_offset(52, 96))?.is_some(),
        })
    }

    fn clip(o: Obj<'a>) -> Result<ClipGenerator> {
        let mut triggers = Vec::new();
        if let Some(arr) = o.ptr(o.layout_offset(44, 80))? {
            arr.expect_class("hkbClipTriggerArray")?;
            for t in arr.array_structs(arr.layout_offset(8, 16), arr.layout_offset(16, 32))? {
                triggers.push(ClipTrigger {
                    local_time: t.f32(0)?,
                    event: EventProperty::read(t, t.layout_offset(4, 8))?,
                    relative_to_end_of_clip: t.bool(t.layout_offset(12, 24))?,
                    acyclic: t.bool(t.layout_offset(13, 25))?,
                    is_annotation: t.bool(t.layout_offset(14, 26))?,
                });
            }
        }
        Ok(ClipGenerator {
            animation_name: o.string_or_empty(o.layout_offset(40, 72))?,
            triggers,
            crop_start_amount_local_time: o.f32(o.layout_offset(48, 88))?,
            crop_end_amount_local_time: o.f32(o.layout_offset(52, 92))?,
            start_time: o.f32(o.layout_offset(56, 96))?,
            playback_speed: o.f32(o.layout_offset(60, 100))?,
            enforced_duration: o.f32(o.layout_offset(64, 104))?,
            user_controlled_time_fraction: o.f32(o.layout_offset(68, 108))?,
            animation_binding_index: o.i16(o.layout_offset(72, 112))?,
            mode: o.i8(o.layout_offset(74, 114))?,
            flags: o.i8(o.layout_offset(75, 115))?,
        })
    }

    fn blender(&mut self, o: Obj<'a>) -> Result<BlenderGenerator> {
        let mut children = Vec::new();
        for c in o.array_ptrs(o.layout_offset(64, 96))?.into_iter().flatten() {
            c.expect_class("hkbBlenderGeneratorChild")?;
            let generator = self.opt_id(c.ptr(c.layout_offset(32, 48))?);
            children.push(BlenderChild {
                generator,
                bone_weights: c
                    .ptr(c.layout_offset(36, 56))?
                    .map(bone_weights)
                    .transpose()?,
                weight: c.f32(c.layout_offset(40, 64))?,
                world_from_model_weight: c.f32(c.layout_offset(44, 68))?,
                bindings: Self::bindings(c)?,
            });
        }
        Ok(BlenderGenerator {
            reference_pose_weight_threshold: o.f32(o.layout_offset(40, 72))?,
            blend_parameter: o.f32(o.layout_offset(44, 76))?,
            min_cyclic_blend_parameter: o.f32(o.layout_offset(48, 80))?,
            max_cyclic_blend_parameter: o.f32(o.layout_offset(52, 84))?,
            index_of_sync_master_child: o.i16(o.layout_offset(56, 88))?,
            flags: o.i16(o.layout_offset(58, 90))?,
            subtract_last_child: o.bool(o.layout_offset(60, 92))?,
            children,
        })
    }

    fn decode(&mut self, id: NodeId, o: Obj<'a>) -> Result<Node> {
        let class = o.class().unwrap_or("<unnamed>").to_string();
        let kind = match class.as_str() {
            "hkbStateMachine" => NodeKind::StateMachine(self.state_machine(o)?),
            "hkbClipGenerator" => NodeKind::Clip(Self::clip(o)?),
            "hkbBlenderGenerator" | "hkbPoseMatchingGenerator" => {
                NodeKind::Blender(self.blender(o)?)
            }
            "hkbManualSelectorGenerator" => {
                let ptrs = o.array_ptrs(o.layout_offset(40, 72))?;
                let generators = ptrs.into_iter().map(|p| self.opt_id(p)).collect();
                NodeKind::ManualSelector {
                    generators,
                    selected_generator_index: o.i8(o.layout_offset(52, 88))?,
                    current_generator_index: o.i8(o.layout_offset(53, 89))?,
                }
            }
            "hkbModifierGenerator" => {
                let modifier = self.opt_id(o.ptr(o.layout_offset(40, 72))?);
                let generator = self.opt_id(o.ptr(o.layout_offset(44, 80))?);
                NodeKind::ModifierGenerator {
                    modifier,
                    generator,
                }
            }
            "hkbModifierList" => {
                let ptrs = o.array_ptrs(o.layout_offset(44, 80))?;
                let modifiers = ptrs.into_iter().map(|p| self.opt_id(p)).collect();
                NodeKind::ModifierList {
                    enable: o.bool(o.layout_offset(40, 72))?,
                    modifiers,
                }
            }
            "hkbBehaviorReferenceGenerator" => NodeKind::BehaviorReference {
                behavior_name: o.string_or_empty(o.layout_offset(40, 72))?,
                resolved: None,
            },
            "hkbBlendingTransitionEffect" => {
                NodeKind::BlendingTransition(BlendingTransitionEffect {
                    self_transition_mode: o.i8(o.layout_offset(40, 72))?,
                    event_mode: o.i8(o.layout_offset(41, 73))?,
                    duration: o.f32(o.layout_offset(44, 80))?,
                    to_generator_start_time_fraction: o.f32(o.layout_offset(48, 84))?,
                    flags: o.u16(o.layout_offset(52, 88))?,
                    end_mode: o.i8(o.layout_offset(54, 90))?,
                    blend_curve: o.i8(o.layout_offset(55, 91))?,
                })
            }
            "hkbGeneratorTransitionEffect" => {
                let transition_generator = self.opt_id(o.ptr(o.layout_offset(44, 80))?);
                NodeKind::GeneratorTransition(GeneratorTransitionEffect {
                    self_transition_mode: o.i8(o.layout_offset(40, 72))?,
                    event_mode: o.i8(o.layout_offset(41, 73))?,
                    transition_generator,
                    blend_in_duration: o.f32(o.layout_offset(48, 88))?,
                    blend_out_duration: o.f32(o.layout_offset(52, 92))?,
                    sync_to_generator_start_time: o.bool(o.layout_offset(56, 96))?,
                })
            }
            "BSiStateTaggingGenerator" => {
                let default_generator = self.opt_id(o.ptr(o.layout_offset(48, 80))?);
                NodeKind::IStateTagging {
                    default_generator,
                    state_to_set_as: o.i32(o.layout_offset(52, 88))?,
                    priority: o.i32(o.layout_offset(56, 92))?,
                }
            }
            "BSSynchronizedClipGenerator" => {
                let clip_generator = self.opt_id(o.ptr(o.layout_offset(48, 80))?);
                NodeKind::SynchronizedClip {
                    clip_generator,
                    sync_anim_prefix: o.string_or_empty(o.layout_offset(52, 88))?,
                    sync_clip_ignore_mark_placement: o.bool(o.layout_offset(56, 96))?,
                    get_to_mark_time: o.f32(o.layout_offset(60, 100))?,
                    mark_error_threshold: o.f32(o.layout_offset(64, 104))?,
                    lead_character: o.bool(o.layout_offset(68, 108))?,
                    reorient_support_char: o.bool(o.layout_offset(69, 109))?,
                    apply_motion_from_root: o.bool(o.layout_offset(70, 110))?,
                    animation_binding_index: o.i16(o.layout_offset(236, 296))?,
                }
            }
            "BSCyclicBlendTransitionGenerator" => {
                let blender_generator = self.opt_id(o.ptr(o.layout_offset(48, 80))?);
                NodeKind::CyclicBlendTransition {
                    blender_generator,
                    event_to_freeze_blend_value: EventProperty::read(o, o.layout_offset(52, 88))?,
                    event_to_cross_blend: EventProperty::read(o, o.layout_offset(60, 104))?,
                    blend_parameter: o.f32(o.layout_offset(68, 120))?,
                    transition_duration: o.f32(o.layout_offset(72, 124))?,
                    blend_curve: o.i8(o.layout_offset(76, 128))?,
                }
            }
            "BSOffsetAnimationGenerator" => {
                let default_generator = self.opt_id(o.ptr(o.layout_offset(48, 80))?);
                let offset_clip_generator = self.opt_id(o.ptr(o.layout_offset(64, 96))?);
                NodeKind::OffsetAnimation {
                    default_generator,
                    offset_clip_generator,
                    offset_variable: o.f32(o.layout_offset(68, 104))?,
                    offset_range_start: o.f32(o.layout_offset(72, 108))?,
                    offset_range_end: o.f32(o.layout_offset(76, 112))?,
                }
            }
            "BSBoneSwitchGenerator" => {
                let default_generator = self.opt_id(o.ptr(o.layout_offset(48, 80))?);
                NodeKind::BoneSwitch {
                    default_generator,
                    children: o
                        .array_ptrs(o.layout_offset(52, 88))?
                        .into_iter()
                        .map(|p| p.map(|p| p.base))
                        .collect(),
                }
            }
            "hkbEventDrivenModifier" => {
                let modifier = self.opt_id(o.ptr(o.layout_offset(44, 80))?);
                NodeKind::EventDrivenModifier {
                    enable: o.bool(o.layout_offset(40, 72))?,
                    modifier,
                    activate_event_id: o.i32(o.layout_offset(48, 88))?,
                    deactivate_event_id: o.i32(o.layout_offset(52, 92))?,
                    active_by_default: o.bool(o.layout_offset(56, 96))?,
                }
            }
            "hkbTimerModifier" => NodeKind::TimerModifier {
                enable: o.bool(o.layout_offset(40, 72))?,
                alarm_time_seconds: o.f32(o.layout_offset(44, 80))?,
                alarm_event: EventProperty::read(o, o.layout_offset(48, 88))?,
            },
            "BSGetTimeStepModifier" => NodeKind::GetTimeStepModifier {
                enable: o.bool(o.layout_offset(40, 72))?,
                time_step: o.f32(o.layout_offset(44, 80))?,
            },
            // Five (flag, invert) pairs packed one byte each from 80.
            "BSIsActiveModifier" => {
                let mut is_active = [false; IS_ACTIVE_SLOTS];
                let mut invert_active = [false; IS_ACTIVE_SLOTS];
                for (i, (a, inv)) in is_active.iter_mut().zip(&mut invert_active).enumerate() {
                    let at = o.layout_offset(44, 80) + 2 * u32::try_from(i).unwrap_or(0);
                    *a = o.bool(at)?;
                    *inv = o.bool(at + 1)?;
                }
                NodeKind::IsActiveModifier {
                    enable: o.bool(o.layout_offset(40, 72))?,
                    is_active,
                    invert_active,
                }
            }
            "BSEventOnFalseToTrueModifier" => {
                // EventToSend sits four bytes past its two flags on LE and
                // eight on SE, as serde-hkx declares the three slots.
                let event_offset = o.layout_offset(4, 8);
                let read_slot = |offset: u32| -> Result<FalseToTrueSlot> {
                    Ok(FalseToTrueSlot {
                        enable: o.bool(offset)?,
                        variable_to_test: o.bool(offset + 1)?,
                        event: EventProperty::read(o, offset + event_offset)?,
                    })
                };
                NodeKind::FalseToTrueModifier {
                    enable: o.bool(o.layout_offset(40, 72))?,
                    slots: [
                        read_slot(o.layout_offset(44, 80))?,
                        read_slot(o.layout_offset(56, 104))?,
                        read_slot(o.layout_offset(68, 128))?,
                    ],
                }
            }

            "BSModifyOnceModifier" => NodeKind::ModifyOnce {
                enable: o.bool(o.layout_offset(40, 72))?,
                on_activate: self.opt_id(o.ptr(o.layout_offset(48, 80))?),
                on_deactivate: self.opt_id(o.ptr(o.layout_offset(56, 96))?),
            },

            "BSDecomposeVectorModifier" => NodeKind::DecomposeVector {
                enable: o.bool(o.layout_offset(40, 72))?,
                vector: o.vec4(o.layout_offset(48, 80))?,
            },
            c if c.ends_with("Modifier") => NodeKind::Modifier {
                enable: o.bool(o.layout_offset(40, 72))?,
            },
            _ => NodeKind::Unknown,
        };
        Ok(Node {
            id,
            offset: o.base,
            class,
            name: o.string_or_empty(o.layout_offset(32, 56))?,
            user_data: o.ulong(o.layout_offset(28, 48))?,
            bindings: Self::bindings(o)?,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::PackfileBuilder;
    use crate::Error;

    /// A graph with one state machine of two states, each a clip, one
    /// transition on an event with a blending effect, and one wildcard.
    fn synthetic_graph() -> Vec<u8> {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        let graph = b.object("hkbBehaviorGraph", 304);
        let variant = b.array_structs(root, 0, 24, 1);
        b.string(variant, 0, "hkbBehaviorGraph");
        b.string(variant, 8, "hkbBehaviorGraph");
        b.ptr(variant, 16, graph);
        b.string(graph, 56, "test");
        b.set_i8(graph, 72, 1);

        let data = b.object("hkbBehaviorGraphData", 128);
        let strings = b.object("hkbBehaviorGraphStringData", 80);
        let values = b.object("hkbVariableValueSet", 64);
        b.ptr(graph, 136, data);
        b.ptr(data, 120, strings);
        b.ptr(data, 112, values);
        b.array_strings(strings, 16, &["idleStart", "walkStart"]);
        b.array_strings(strings, 48, &["Speed", "IsMoving", "Elapsed"]);
        let infos = b.array_structs(data, 32, 6, 3);
        b.set_i8(infos, 4, 4);
        b.set_i8(infos + 6, 4, 0);
        b.set_i8(infos + 12, 4, 4);
        b.array_u32(data, 64, &[0, 2]);
        b.array_i32(values, 16, &[0, 1]);

        let sm = b.object("hkbStateMachine", 264);
        b.string(sm, 56, "Locomotion");
        b.set_i32(sm, 72, -1);
        b.set_i32(sm, 104, 0);
        b.set_i32(sm, 108, -1);
        b.set_i8(sm, 133, 32);

        let wrapper = b.object("hkbModifierGenerator", 88);
        let time_step = b.object("BSGetTimeStepModifier", 88);
        b.ptr(graph, 128, wrapper);
        b.ptr(wrapper, 72, time_step);
        b.ptr(wrapper, 80, sm);
        b.string(time_step, 56, "Elapsed time");
        b.set_bool(time_step, 72, true);
        b.set_f32(time_step, 80, 0.75);
        let time_step_bindings = b.object("hkbVariableBindingSet", 40);
        b.ptr(time_step, 16, time_step_bindings);
        let time_step_binding = b.array_structs(time_step_bindings, 16, 40, 1);
        b.string(time_step_binding, 0, "timeStep");
        b.set_i32(time_step_binding, 28, 2);
        b.set_i32(time_step_bindings, 32, -1);

        let idle = b.object("hkbClipGenerator", 272);
        b.string(idle, 56, "IdleClip");
        b.string(idle, 72, "mt_idle");
        b.set_f32(idle, 100, 1.0);
        b.set_i8(idle, 114, 1);
        let triggers = b.object("hkbClipTriggerArray", 32);
        b.ptr(idle, 80, triggers);
        let t = b.array_structs(triggers, 16, 32, 1);
        b.set_f32(t, 0, 0.25);
        b.set_i32(t, 8, 1);
        b.set_bool(t, 25, true);

        let walk = b.object("hkbClipGenerator", 272);
        b.string(walk, 56, "WalkClip");
        b.string(walk, 72, "mt_walkforward");
        b.set_f32(walk, 100, 1.0);

        let effect = b.object("hkbBlendingTransitionEffect", 144);
        b.string(effect, 56, "Blend");
        b.set_f32(effect, 80, 0.3);
        b.set_i8(effect, 90, 2);

        let s0 = b.object("hkbStateMachineStateInfo", 120);
        b.string(s0, 96, "Idle");
        b.set_i32(s0, 104, 0);
        b.set_f32(s0, 108, 1.0);
        b.set_bool(s0, 112, true);
        b.ptr(s0, 88, idle);
        let tarr = b.object("hkbStateMachineTransitionInfoArray", 32);
        b.ptr(s0, 80, tarr);
        let tr = b.array_structs(tarr, 16, 72, 1);
        b.ptr(tr, 32, effect);
        b.set_i32(tr, 48, 1);
        b.set_i32(tr, 52, 1);
        b.set_i16(tr, 64, 0);
        b.set_u16(tr, 66, transition_flags::DISALLOW_RANDOM_TRANSITION);
        let enter = b.object("hkbStateMachineEventPropertyArray", 32);
        b.ptr(s0, 64, enter);
        let ev = b.array_structs(enter, 16, 16, 1);
        b.set_i32(ev, 0, 0);

        let s1 = b.object("hkbStateMachineStateInfo", 120);
        b.string(s1, 96, "Walk");
        b.set_i32(s1, 104, 1);
        b.set_f32(s1, 108, 1.0);
        b.set_bool(s1, 112, true);
        b.ptr(s1, 88, walk);
        let bindings = b.object("hkbVariableBindingSet", 40);
        b.ptr(walk, 16, bindings);
        let bind = b.array_structs(bindings, 16, 40, 1);
        b.string(bind, 0, "playbackSpeed");
        b.set_i32(bind, 28, 0);
        b.set_i32(bindings, 32, -1);

        b.array_ptrs(sm, 144, &[s0, s1]);
        let wild = b.object("hkbStateMachineTransitionInfoArray", 32);
        b.ptr(sm, 160, wild);
        let wt = b.array_structs(wild, 16, 72, 1);
        b.set_i32(wt, 48, 0);
        b.set_i32(wt, 52, 0);
        b.set_u16(wt, 66, transition_flags::IS_GLOBAL_WILDCARD);
        b.finish()
    }

    #[test]
    fn a_synthetic_graph_loads_into_the_model() {
        let bytes = synthetic_graph();
        let pf = Packfile::parse(&bytes).unwrap();
        let g = BehaviorGraph::from_packfile(&pf).unwrap();
        assert_eq!(g.name, "test");
        assert_eq!(g.variable_mode, 1);
        assert_eq!(g.data.event_names, vec!["idleStart", "walkStart"]);
        assert_eq!(g.data.variable_names, vec!["Speed", "IsMoving", "Elapsed"]);
        assert_eq!(g.data.variable_infos[0].ty, VariableType::Real);
        assert_eq!(g.data.variable_infos[1].ty, VariableType::Bool);
        assert_eq!(g.data.event_infos[1].flags, 2);
        assert_eq!(
            g.data.variable_initial_values.as_ref().unwrap().word_values,
            vec![0, 1]
        );
        assert_eq!(g.data.event_id("walkStart"), Some(1));

        let wrapper = g.node(g.root.unwrap());
        let NodeKind::ModifierGenerator {
            modifier: Some(time_step),
            generator: Some(sm),
        } = &wrapper.kind
        else {
            panic!("root is {:?}", wrapper.kind);
        };
        let root = g.node(*sm);
        assert_eq!(root.class, "hkbStateMachine");
        assert_eq!(root.name, "Locomotion");
        let NodeKind::StateMachine(sm) = &root.kind else {
            panic!("state machine is {:?}", root.kind);
        };
        let time_step_node = g.node(*time_step);
        let NodeKind::GetTimeStepModifier {
            enable,
            time_step: authored_time_step,
        } = &time_step_node.kind
        else {
            panic!("time modifier is {:?}", time_step_node.kind);
        };
        assert!(enable);
        assert_eq!(*authored_time_step, 0.75);
        assert_eq!(
            time_step_node.bindings.as_ref().unwrap().bindings[0].member_path,
            "timeStep"
        );
        assert_eq!(
            time_step_node.bindings.as_ref().unwrap().bindings[0].variable_index,
            2
        );
        assert_eq!(sm.max_simultaneous_transitions, 32);
        assert_eq!(sm.states.len(), 2);
        assert_eq!(sm.states[0].name, "Idle");
        assert_eq!(
            sm.states[0].enter_notify_events,
            vec![EventProperty {
                id: 0,
                has_payload: false
            }]
        );
        assert_eq!(sm.states[0].transitions.len(), 1);
        let tr = &sm.states[0].transitions[0];
        assert_eq!(tr.event_id, 1);
        assert_eq!(tr.to_state_id, 1);
        assert_eq!(tr.flags, transition_flags::DISALLOW_RANDOM_TRANSITION);
        let effect = g.node(tr.transition.unwrap());
        let NodeKind::BlendingTransition(e) = &effect.kind else {
            panic!("{:?}", effect.kind);
        };
        assert_eq!(e.duration, 0.3);
        assert_eq!(e.end_mode, 2);
        assert_eq!(sm.wildcard_transitions.len(), 1);
        assert_eq!(
            sm.wildcard_transitions[0].flags,
            transition_flags::IS_GLOBAL_WILDCARD
        );

        let idle = g.node(sm.states[0].generator.unwrap());
        let NodeKind::Clip(c) = &idle.kind else {
            panic!("{:?}", idle.kind);
        };
        assert_eq!(c.animation_name, "mt_idle");
        assert_eq!(c.mode, 1);
        assert_eq!(c.triggers.len(), 1);
        assert_eq!(c.triggers[0].local_time, 0.25);
        assert_eq!(c.triggers[0].event.id, 1);
        assert!(c.triggers[0].acyclic);

        let walk = g.node(sm.states[1].generator.unwrap());
        let bindings = walk.bindings.as_ref().unwrap();
        assert_eq!(bindings.bindings[0].member_path, "playbackSpeed");
        assert_eq!(bindings.bindings[0].variable_index, 0);

        assert_eq!(g.clip_animation_names(), vec!["mt_idle", "mt_walkforward"]);
        assert_eq!(root.children().len(), 3);
        assert_eq!(g.nodes.len(), 6);
    }

    #[test]
    fn a_missing_root_generator_is_an_empty_graph_not_an_error() {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        let graph = b.object("hkbBehaviorGraph", 304);
        let variant = b.array_structs(root, 0, 24, 1);
        b.string(variant, 8, "hkbBehaviorGraph");
        b.ptr(variant, 16, graph);
        let bytes = b.finish();
        let pf = Packfile::parse(&bytes).unwrap();
        let g = BehaviorGraph::from_packfile(&pf).unwrap();
        assert!(g.root.is_none());
        assert!(g.nodes.is_empty());
        assert!(g.data.event_names.is_empty());
    }

    #[test]
    fn a_root_of_the_wrong_class_is_reported() {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        let sk = b.object("hkaSkeleton", 120);
        let variant = b.array_structs(root, 0, 24, 1);
        b.string(variant, 8, "hkbBehaviorGraph");
        b.ptr(variant, 16, sk);
        let bytes = b.finish();
        let pf = Packfile::parse(&bytes).unwrap();
        let e = BehaviorGraph::from_packfile(&pf).unwrap_err();
        assert!(
            matches!(
                e,
                Error::WrongClass {
                    expected: "hkbBehaviorGraph",
                    ..
                }
            ),
            "{e}"
        );
    }
}
