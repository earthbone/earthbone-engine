// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(dead_code)]

//! A hand-built locomotion graph for the interpreter tests: events
//! `idleStart` and `walkStart`, variables `Speed` (real, 1.0) and `iState`
//! (int, 0), two clip generators named `IdleClip` and `WalkClip`, and a
//! root of the caller's choice over them.

use soe_hkx::behavior::transition_flags;
use soe_hkx::writer::PackfileBuilder;

pub enum Root {
    /// A state machine `Locomotion` with states `Idle` and `Walk`, a 0.25 s
    /// linear blend from Idle to Walk on `walkStart`, and a wildcard back to
    /// Idle on `idleStart`. The idle clip carries an acyclic trigger at 0.3 s
    /// raising `walkStart` and a cyclic one 0.1 s before its end raising
    /// `idleStart`.
    Machine,
    /// A blender over both clips: weights 1 and 3, or parametric with the
    /// blend parameter bound to `Speed`.
    Blender { parametric: bool },
    /// A manual selector with its index bound to `iState`.
    Selector,
    /// A modifier generator whose deactivation modifier is bound to `Enabled`.
    Deactivate,
}

/// Both clips loop at speed 1, the walk's speed is bound to `Speed`.
pub fn graph(root: &Root, idle_animation: &str, walk_animation: &str) -> Vec<u8> {
    graph_with_masks(root, idle_animation, walk_animation, [None, None])
}

pub fn graph_with_masks(
    root: &Root,
    idle_animation: &str,
    walk_animation: &str,
    masks: [Option<&[f32]>; 2],
) -> Vec<u8> {
    graph_with_masks_and_pointer_binding(root, idle_animation, walk_animation, masks, false)
}

pub fn graph_with_bound_mask(
    root: &Root,
    idle_animation: &str,
    walk_animation: &str,
    masks: [Option<&[f32]>; 2],
) -> Vec<u8> {
    graph_with_masks_and_pointer_binding(root, idle_animation, walk_animation, masks, true)
}

fn graph_with_masks_and_pointer_binding(
    root: &Root,
    idle_animation: &str,
    walk_animation: &str,
    masks: [Option<&[f32]>; 2],
    bind_bone_mask: bool,
) -> Vec<u8> {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
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
    b.array_strings(strings, 16, &["idleStart", "walkStart", "deactivated"]);
    let variable_names = if bind_bone_mask {
        &["BoneMask", "Speed", "iState", "Enabled"][..]
    } else {
        &["Speed", "iState", "Enabled"][..]
    };
    b.array_strings(strings, 48, variable_names);
    let infos = b.array_structs(data, 32, 6, variable_names.len());
    if bind_bone_mask {
        b.set_i8(infos, 4, 5);
        b.set_i8(infos + 6, 4, 4);
        b.set_i8(infos + 12, 4, 3);
        b.set_i8(infos + 18, 4, 1);
        b.array_i32(values, 16, &[0, 1.0f32.to_bits() as i32, 0, 1]);
    } else {
        b.set_i8(infos, 4, 4);
        b.set_i8(infos + 6, 4, 3);
        b.set_i8(infos + 12, 4, 1);
        b.array_i32(values, 16, &[1.0f32.to_bits() as i32, 0, 1]);
    }
    b.array_u32(data, 64, &vec![0; variable_names.len()]);

    let idle = b.object("hkbClipGenerator", 272);
    b.string(idle, 56, "IdleClip");
    b.string(idle, 72, idle_animation);
    b.set_f32(idle, 100, 1.0);
    b.set_i8(idle, 114, 1);
    let triggers = b.object("hkbClipTriggerArray", 32);
    b.ptr(idle, 80, triggers);
    let t = b.array_structs(triggers, 16, 32, 2);
    b.set_f32(t, 0, 0.3);
    b.set_i32(t, 8, 1);
    b.set_bool(t, 25, true);
    b.set_f32(t + 32, 0, -0.1);
    b.set_i32(t + 32, 8, 0);
    b.set_bool(t + 32, 24, true);

    let walk = b.object("hkbClipGenerator", 272);
    b.string(walk, 56, "WalkClip");
    b.string(walk, 72, walk_animation);
    b.set_f32(walk, 100, 1.0);
    b.set_i8(walk, 114, 1);
    let bindings = b.object("hkbVariableBindingSet", 40);
    b.ptr(walk, 16, bindings);
    let bind = b.array_structs(bindings, 16, 40, 1);
    b.string(bind, 0, "playbackSpeed");
    b.set_i32(bind, 28, i32::from(bind_bone_mask));
    b.set_i32(bindings, 32, -1);

    match root {
        Root::Machine => {
            let sm = b.object("hkbStateMachine", 264);
            b.ptr(graph, 128, sm);
            b.string(sm, 56, "Locomotion");
            b.set_i32(sm, 72, -1);
            b.set_i32(sm, 104, 0);
            b.set_i32(sm, 108, -1);
            b.set_i8(sm, 133, 32);

            let effect = b.object("hkbBlendingTransitionEffect", 144);
            b.string(effect, 56, "Blend");
            b.set_f32(effect, 80, 0.25);
            b.set_i8(effect, 90, 2);
            b.set_i8(effect, 91, 1);

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
            b.set_u16(tr, 66, transition_flags::DISALLOW_RANDOM_TRANSITION);

            let s1 = b.object("hkbStateMachineStateInfo", 120);
            b.string(s1, 96, "Walk");
            b.set_i32(s1, 104, 1);
            b.set_f32(s1, 108, 1.0);
            b.set_bool(s1, 112, true);
            b.ptr(s1, 88, walk);

            b.array_ptrs(sm, 144, &[s0, s1]);
            let wild = b.object("hkbStateMachineTransitionInfoArray", 32);
            b.ptr(sm, 160, wild);
            let wt = b.array_structs(wild, 16, 72, 1);
            b.set_i32(wt, 48, 0);
            b.set_i32(wt, 52, 0);
            b.set_u16(wt, 66, transition_flags::IS_GLOBAL_WILDCARD);
        }
        Root::Blender { parametric } => {
            let blender = b.object("hkbBlenderGenerator", 112);
            b.ptr(graph, 128, blender);
            b.string(blender, 56, "Blend");
            b.set_i16(blender, 88, -1);
            let c0 = b.object("hkbBlenderGeneratorChild", 72);
            let c1 = b.object("hkbBlenderGeneratorChild", 72);
            b.ptr(c0, 48, idle);
            b.ptr(c1, 48, walk);
            b.set_f32(c0, 68, 1.0);
            b.set_f32(c1, 68, 1.0);
            if *parametric {
                b.set_i16(blender, 90, 16);
                b.set_f32(c0, 64, 0.0);
                b.set_f32(c1, 64, 1.0);
                let bs = b.object("hkbVariableBindingSet", 40);
                b.ptr(blender, 16, bs);
                let bind = b.array_structs(bs, 16, 40, 1);
                b.string(bind, 0, "blendParameter");
                b.set_i32(bind, 28, 0);
                b.set_i32(bs, 32, -1);
            } else {
                b.set_f32(c0, 64, 1.0);
                b.set_f32(c1, 64, 3.0);
            }
            let mut first_mask = None;
            for (index, (child, mask)) in [c0, c1].into_iter().zip(masks).enumerate() {
                if let Some(weights) = mask {
                    let mask = b.object("hkbBoneWeightArray", 64);
                    b.array_f32(mask, 48, weights);
                    b.ptr(child, 56, mask);
                    if index == 0 {
                        first_mask = Some(mask);
                    }
                }
            }
            if bind_bone_mask {
                let mask = first_mask.expect("bound mask fixture requires a first mask");
                b.array_ptrs(values, 48, &[mask, 0, 0, 0]);
                let bs = b.object("hkbVariableBindingSet", 40);
                b.ptr(c0, 16, bs);
                let bind = b.array_structs(bs, 16, 40, 1);
                b.string(bind, 0, "boneWeights");
                b.set_i32(bind, 28, 0);
                b.set_i32(bs, 32, -1);
            }
            b.array_ptrs(blender, 96, &[c0, c1]);
        }
        Root::Selector => {
            let sel = b.object("hkbManualSelectorGenerator", 96);
            b.ptr(graph, 128, sel);
            b.string(sel, 56, "Select");
            b.array_ptrs(sel, 72, &[idle, walk]);
            let bs = b.object("hkbVariableBindingSet", 40);
            b.ptr(sel, 16, bs);
            let bind = b.array_structs(bs, 16, 40, 1);
            b.string(bind, 0, "selectedGeneratorIndex");
            b.set_i32(bind, 28, 1);
            b.set_i32(bs, 32, -1);
        }
        Root::Deactivate => {
            let modifier_generator = b.object("hkbModifierGenerator", 88);
            let event_modifier = b.object("BSEventOnDeactivateModifier", 96);
            b.ptr(graph, 128, modifier_generator);
            b.string(modifier_generator, 56, "DeactivateRoot");
            b.ptr(modifier_generator, 72, event_modifier);
            b.ptr(modifier_generator, 80, idle);
            b.string(event_modifier, 56, "DeactivateEvent");
            b.set_bool(event_modifier, 72, true);
            b.set_i32(event_modifier, 80, 2);

            let bindings = b.object("hkbVariableBindingSet", 40);
            b.ptr(event_modifier, 16, bindings);
            let binding = b.array_structs(bindings, 16, 40, 1);
            b.string(binding, 0, "enable");
            b.set_i32(binding, 28, 2);
            b.set_i32(bindings, 32, -1);
        }
    }
    b.finish()
}
