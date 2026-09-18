// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The interpreter on hand-built graphs: a two-state machine driven by a
//! clip trigger and an external event, a blender in both weight modes, a
//! manual selector, variable bindings, clip modes. Clips are one-track
//! linear ramps so every expected pose is a number.

mod common;

use common::{graph, Root};
use soe_havok::animation::SplineCompressedAnimation;
use soe_havok::behavior::{BehaviorGraph, NodeId, NodeKind};
use soe_havok::interp::{Clip, ClipLibrary, Event, Interpreter};
use soe_havok::spline::encode;
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

fn load(root: &Root) -> BehaviorGraph {
    let bytes = graph(root, "mt_idle", "mt_walkforward");
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
