// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The interpreter on hand-built graphs: a two-state machine driven by a
//! clip trigger and an external event, a blender in both weight modes, a
//! manual selector, variable bindings, clip modes. Clips are one-track
//! linear ramps so every expected pose is a number.

use soe_havok::animation::SplineCompressedAnimation;
use soe_havok::behavior::{transition_flags, BehaviorGraph, NodeId, NodeKind};
use soe_havok::interp::{Clip, ClipLibrary, Event, Interpreter};
use soe_havok::spline::encode;
use soe_havok::writer::PackfileBuilder;
use soe_havok::{Packfile, QsTransform};

const FRAMES: usize = 31;
const DT: f32 = 0.125;

fn pad4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

/// One track whose x rides linearly from `lo` to `hi` over one second at
/// 30 frames per second, static identity rotation and scale.
fn linear_clip(name: &str, lo: f32, hi: f32) -> Clip {
    let mut b = Vec::new();
    b.extend_from_slice(&[1 | (2 << 2), 0b0001_0000, 0b0000_0001, 0]);
    pad4(&mut b);
    let items = (FRAMES - 1) as u16;
    b.extend_from_slice(&items.to_le_bytes());
    b.push(1);
    b.push(0);
    for k in 0..FRAMES {
        b.push(k as u8);
    }
    b.push((FRAMES - 1) as u8);
    pad4(&mut b);
    b.extend_from_slice(&lo.to_le_bytes());
    b.extend_from_slice(&hi.to_le_bytes());
    for k in 0..FRAMES {
        let v = lo + (hi - lo) * k as f32 / (FRAMES - 1) as f32;
        b.extend_from_slice(&encode::point_16(lo, hi, v).to_le_bytes());
    }
    pad4(&mut b);
    b.extend_from_slice(&encode::quat_48([0.0, 0.0, 0.0, 1.0]));
    pad4(&mut b);
    let anim = SplineCompressedAnimation {
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
        data: b,
        endian: 0,
        has_extracted_motion: false,
    };
    Clip::from_parts(name, anim, &[1]).unwrap()
}

fn library() -> ClipLibrary {
    let mut lib = ClipLibrary::new();
    lib.insert(linear_clip("mt_idle", 0.0, 20.0));
    lib.insert(linear_clip("mt_walkforward", 100.0, 120.0));
    lib
}

fn reference_pose() -> Vec<QsTransform> {
    vec![
        QsTransform::IDENTITY,
        QsTransform {
            translation: [0.0, 0.0, 60.0, 0.0],
            ..QsTransform::IDENTITY
        },
    ]
}

/// The pelvis x the sampler gives for a clip at a time.
fn sampled_x(lib: &ClipLibrary, name: &str, t: f32) -> f32 {
    let mut pose = reference_pose();
    lib.get(name).unwrap().sample_into(t, &mut pose);
    pose[1].translation[0]
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

enum Root {
    Machine,
    Blender { parametric: bool },
    Selector,
}

/// A graph with events `idleStart` and `walkStart`, variables `Speed`
/// (real, 1.0) and `iState` (int, 0), and the root the caller asks for
/// over the two clips.
fn graph(root: &Root) -> Vec<u8> {
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
    b.array_strings(strings, 16, &["idleStart", "walkStart"]);
    b.array_strings(strings, 48, &["Speed", "iState"]);
    let infos = b.array_structs(data, 32, 6, 2);
    b.set_i8(infos, 4, 4);
    b.set_i8(infos + 6, 4, 3);
    b.array_u32(data, 64, &[0, 0]);
    b.array_i32(values, 16, &[1.0f32.to_bits() as i32, 0]);

    let idle = b.object("hkbClipGenerator", 272);
    b.string(idle, 56, "IdleClip");
    b.string(idle, 72, "mt_idle");
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
    b.string(walk, 72, "mt_walkforward");
    b.set_f32(walk, 100, 1.0);
    b.set_i8(walk, 114, 1);
    let bindings = b.object("hkbVariableBindingSet", 40);
    b.ptr(walk, 16, bindings);
    let bind = b.array_structs(bindings, 16, 40, 1);
    b.string(bind, 0, "playbackSpeed");
    b.set_i32(bind, 28, 0);
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
    }
    b.finish()
}

fn load(root: &Root) -> BehaviorGraph {
    let bytes = graph(root);
    let pf = Packfile::parse(&bytes).unwrap();
    BehaviorGraph::from_packfile(&pf).unwrap()
}

fn clip_named(g: &BehaviorGraph, name: &str) -> NodeId {
    g.find_by_name(name).unwrap().id
}

#[test]
fn the_sampler_and_the_clip_agree_on_the_ramp() {
    let lib = library();
    assert!(close(sampled_x(&lib, "mt_idle", 0.0), 0.0));
    assert!(close(sampled_x(&lib, "mt_idle", 0.5), 10.0));
    assert!(close(sampled_x(&lib, "mt_idle", 1.0), 20.0));
    assert!(close(sampled_x(&lib, "MT_WALKFORWARD", 0.25), 105.0));
    assert_eq!(lib.get("mt_idle").unwrap().track_to_bone(), &[1]);
}

#[test]
fn idle_plays_then_a_trigger_starts_the_walk_blend() {
    let g = load(&Root::Machine);
    let lib = library();
    let mut vm = Interpreter::new(&g, &lib, reference_pose());
    let root = g.root.unwrap();
    let idle = clip_named(&g, "IdleClip");
    let walk = clip_named(&g, "WalkClip");
    assert!(vm.unsupported().unbound.is_empty());
    assert_eq!(vm.bound_count(), 1);

    let out = vm.update(DT);
    assert_eq!(out.pose.len(), 2);
    assert_eq!(out.weight, 1.0);
    assert_eq!(
        out.pose[0],
        QsTransform::IDENTITY,
        "untouched bone keeps the reference"
    );
    assert!(
        close(out.pose[1].translation[2], 0.0),
        "the track drives the whole bone"
    );
    assert!(close(
        out.pose[1].translation[0],
        sampled_x(&lib, "mt_idle", DT)
    ));
    assert_eq!(vm.current_state_name(root), Some("Idle"));
    assert_eq!(vm.clip_time(idle), Some(DT));
    assert_eq!(vm.clip_time(walk), None);
    let changes = vm.take_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].to, 0);
    assert_eq!(changes[0].event, None);

    vm.update(DT);
    assert_eq!(vm.current_state_name(root), Some("Idle"));
    assert!(vm.take_events().is_empty());

    // Frame 3 crosses the acyclic trigger at 0.3 which raises walkStart.
    let out = vm.update(DT).clone();
    assert!(close(
        out.pose[1].translation[0],
        sampled_x(&lib, "mt_idle", 3.0 * DT)
    ));
    assert_eq!(vm.take_events(), vec![Event { id: 1 }]);
    assert_eq!(vm.current_state_name(root), Some("Walk"));
    assert!(vm.in_transition(root));
    let changes = vm.take_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        (changes[0].from, changes[0].to, changes[0].event),
        (Some(0), 1, Some(1))
    );
    assert_eq!(vm.clip_time(walk), Some(0.0));

    // Halfway through the linear 0.25 s blend: idle at 0.5, walk at 0.125.
    let out = vm.update(DT).clone();
    let from = sampled_x(&lib, "mt_idle", 4.0 * DT);
    let to = sampled_x(&lib, "mt_walkforward", DT);
    assert!(
        close(out.pose[1].translation[0], f32::midpoint(from, to)),
        "{out:?}"
    );
    assert!(vm.in_transition(root));

    // Blend done: pure walk at 0.25, idle deactivated.
    let out = vm.update(DT).clone();
    assert!(close(
        out.pose[1].translation[0],
        sampled_x(&lib, "mt_walkforward", 2.0 * DT)
    ));
    assert!(!vm.in_transition(root));
    assert_eq!(vm.clip_time(idle), None);
    assert_eq!(vm.active_clips(), vec![(walk, 2.0 * DT)]);

    // The wildcard on idleStart cuts straight back to Idle, restarted.
    vm.send_event_named("idleStart");
    let out = vm.update(DT).clone();
    assert_eq!(vm.current_state_name(root), Some("Idle"));
    assert!(!vm.in_transition(root));
    assert_eq!(vm.clip_time(walk), None);
    assert!(close(
        out.pose[1].translation[0],
        sampled_x(&lib, "mt_idle", DT)
    ));
    assert!(!vm.send_event_named("noSuchEvent"));
}

#[test]
fn a_bound_playback_speed_scales_the_walk() {
    let g = load(&Root::Machine);
    let lib = library();
    let mut vm = Interpreter::new(&g, &lib, reference_pose());
    let walk = clip_named(&g, "WalkClip");
    vm.update(DT);
    vm.send_event_named("walkStart");
    // An external event is handled before generate, so the walk clip
    // starts and advances in the same frame.
    vm.update(DT);
    assert_eq!(vm.clip_time(walk), Some(DT));
    assert!(vm.variables.set_by_name("Speed", 2.0));
    vm.update(DT);
    assert_eq!(vm.clip_time(walk), Some(3.0 * DT));
    vm.update(DT);
    assert_eq!(vm.clip_time(walk), Some(5.0 * DT));
}

#[test]
fn a_looping_clip_wraps_and_fires_cyclic_triggers_every_pass() {
    // The selector root has no transitions, so the idle clip loops on and
    // its trigger events are only logged.
    let g = load(&Root::Selector);
    let lib = library();
    let mut vm = Interpreter::new(&g, &lib, reference_pose());
    let idle = clip_named(&g, "IdleClip");
    // Sixteen frames of 0.125 s is two full passes of the one second clip.
    let mut ids = Vec::new();
    for _ in 0..16 {
        vm.update(DT);
        ids.extend(vm.take_events().iter().map(|e| e.id));
    }
    // The acyclic trigger at 0.3 fires on the first pass only, the end
    // trigger at -0.1 (0.9 s) fires on both wraps.
    assert_eq!(ids, vec![1, 0, 0]);
    assert_eq!(vm.clip_time(idle), Some(0.0));
    assert!(!vm.clip_at_end(idle));
}

#[test]
fn a_blender_mixes_children_by_normalized_weight() {
    let g = load(&Root::Blender { parametric: false });
    let lib = library();
    let mut vm = Interpreter::new(&g, &lib, reference_pose());
    let out = vm.update(DT).clone();
    let idle = sampled_x(&lib, "mt_idle", DT);
    let walk = sampled_x(&lib, "mt_walkforward", DT);
    assert!(
        close(out.pose[1].translation[0], 0.25 * idle + 0.75 * walk),
        "{out:?}"
    );
    assert!(close(out.weight, 1.0));
    assert_eq!(vm.active_clips().len(), 2);
}

#[test]
fn a_parametric_blender_follows_its_bound_parameter() {
    let g = load(&Root::Blender { parametric: true });
    let lib = library();
    let mut vm = Interpreter::new(&g, &lib, reference_pose());
    let walk = clip_named(&g, "WalkClip");
    assert_eq!(vm.bound_count(), 2);
    // `Speed` is both the blend parameter and the walk's playback speed.
    vm.variables.set_by_name("Speed", 0.25);
    let out = vm.update(DT).clone();
    let idle = sampled_x(&lib, "mt_idle", DT);
    let walk_x = sampled_x(&lib, "mt_walkforward", 0.25 * DT);
    assert!(
        close(out.pose[1].translation[0], 0.75 * idle + 0.25 * walk_x),
        "{out:?}"
    );
    // Parameter past the last child: only the walk plays, the idle stops.
    vm.variables.set_by_name("Speed", 3.0);
    let out = vm.update(DT).clone();
    assert!(close(
        out.pose[1].translation[0],
        sampled_x(&lib, "mt_walkforward", 3.25 * DT)
    ));
    assert_eq!(vm.active_clips(), vec![(walk, 3.25 * DT)]);
}

#[test]
fn a_manual_selector_switches_on_its_bound_index() {
    let g = load(&Root::Selector);
    let lib = library();
    let mut vm = Interpreter::new(&g, &lib, reference_pose());
    let idle = clip_named(&g, "IdleClip");
    let walk = clip_named(&g, "WalkClip");
    let out = vm.update(DT).clone();
    assert!(close(
        out.pose[1].translation[0],
        sampled_x(&lib, "mt_idle", DT)
    ));
    assert_eq!(vm.active_clips(), vec![(idle, DT)]);
    vm.variables.set_by_name("iState", 1.0);
    let out = vm.update(DT).clone();
    assert!(close(
        out.pose[1].translation[0],
        sampled_x(&lib, "mt_walkforward", DT)
    ));
    assert_eq!(vm.active_clips(), vec![(walk, DT)]);
}

#[test]
fn a_missing_clip_yields_the_reference_pose_and_is_reported() {
    let g = load(&Root::Selector);
    let lib = ClipLibrary::new();
    let mut vm = Interpreter::new(&g, &lib, reference_pose());
    let out = vm.update(DT).clone();
    assert_eq!(out.weight, 0.0);
    assert_eq!(out.pose, reference_pose());
    assert_eq!(vm.unsupported().missing_clips, vec!["mt_idle".to_string()]);
    assert!(matches!(
        g.node(g.root.unwrap()).kind,
        NodeKind::ManualSelector { .. }
    ));
}
