// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Every packfile inside `Skyrim - Animations.bsa`, when `SKYRIM_DATA`
//! points at a Skyrim SE `Data/` folder: parse, count the classes, and
//! decode every spline compressed animation found. Then the interpreter
//! on the vanilla human skeleton with its idle and walk clips, checked
//! against the sampler, and a run of the vanilla `mt_behavior` graph.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use common::{graph, Root};
use soe_bsa::Archive;
use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{blend_transform, Clip, ClipLibrary, Interpreter};
use soe_hkx::skeleton::{AnimationContainer, Skeleton};
use soe_hkx::{Packfile, QsTransform};

const CHARACTER: &str = "meshes/actors/character/";
const PASSTHROUGH_MODIFIERS: &[(&str, usize, &str)] = &[
    ("BSDirectAtModifier", 23, "ANM-25"),
    ("BSEventEveryNEventsModifier", 85, "ANM-26"),
    ("BSEventOnDeactivateModifier", 98, "ANM-27"),
    ("BSIStateManagerModifier", 3, "ANM-29"),
    ("BSLimbIKModifier", 1, "ANM-31"),
    ("BSLookAtModifier", 128, "ANM-32"),
    ("BSModifyOnceModifier", 26, "ANM-33"),
    ("BSPassByTargetTriggerModifier", 1, "ANM-34"),
    ("BSRagdollContactListenerModifier", 50, "ANM-35"),
    ("BSSpeedSamplerModifier", 42, "ANM-36"),
    ("BSTimerModifier", 1, "ANM-37"),
    ("BSTweenerModifier", 5, "ANM-38"),
    ("hkbDampingModifier", 41, "ANM-39"),
    ("hkbEvaluateExpressionModifier", 438, "ANM-40"),
    ("hkbEventDrivenModifier", 158, "ANM-17"),
    ("hkbEventsFromRangeModifier", 3, "ANM-15"),
    ("hkbFootIkControlsModifier", 25, "ANM-41"),
    ("hkbFootIkModifier", 2, "ANM-42"),
    ("hkbGetUpModifier", 46, "ANM-43"),
    ("hkbKeyframeBonesModifier", 91, "ANM-44"),
    ("hkbModifierGenerator", 1909, "ANM-52"),
    ("hkbModifierList", 708, "ANM-51"),
    ("hkbPoweredRagdollControlsModifier", 93, "ANM-45"),
    ("hkbRigidBodyRagdollControlsModifier", 55, "ANM-46"),
    ("hkbRotateCharacterModifier", 6, "ANM-14"),
    ("hkbTransformVectorModifier", 1, "ANM-47"),
    ("hkbTimerModifier", 59, "ANM-16"),
    ("hkbTwistModifier", 17, "ANM-48"),
];

fn animations_archive() -> Option<Archive> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(Archive::open(&data.join("Skyrim - Animations.bsa")).expect("archive opens"))
}

fn container(archive: &Archive, path: &str) -> AnimationContainer {
    let bytes = archive.read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let pf = Packfile::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
    AnimationContainer::from_packfile(&pf).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn human_skeleton(archive: &Archive) -> Skeleton {
    let c = container(
        archive,
        &format!("{CHARACTER}character assets/skeleton.hkx"),
    );
    c.skeletons.into_iter().next().expect("a skeleton")
}

/// The behavior a reference names, `Behaviors\IdleBehavior.hkx`, from the
/// archive. Returns `None` when the archive has no such file, which
/// `link_all` reports rather than treating as a failure.
fn vanilla_behavior(archive: &Archive, name: &str) -> Option<BehaviorGraph> {
    let path = format!("{CHARACTER}{}", name.replace('\\', "/"));
    if !archive.contains(&path) {
        return None;
    }
    let bytes = archive
        .read(&path)
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    let pf = Packfile::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
    Some(BehaviorGraph::from_packfile(&pf).unwrap_or_else(|e| panic!("{path}: {e}")))
}

/// The clip a generator names, `Animations\mt_idle.hkx`, from the archive.
fn vanilla_clip(archive: &Archive, name: &str) -> Option<Clip> {
    let path = format!("{CHARACTER}{}", name.replace('\\', "/"));
    if !archive.contains(&path) {
        return None;
    }
    let c = container(archive, &path);
    Some(Clip::from_container(name, &c).unwrap_or_else(|e| panic!("{path}: {e}")))
}

fn max_pose_diff(a: &[QsTransform], b: &[QsTransform]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| {
            let t = (0..3)
                .map(|i| (x.translation[i] - y.translation[i]).abs())
                .fold(0.0f32, f32::max);
            let r = (0..4)
                .map(|i| (x.rotation[i] - y.rotation[i]).abs())
                .fold(0.0f32, f32::max);
            let s = (0..3)
                .map(|i| (x.scale[i] - y.scale[i]).abs())
                .fold(0.0f32, f32::max);
            t.max(r).max(s)
        })
        .fold(0.0f32, f32::max)
}

/// The vanilla skeleton, idle and walk clips, and the synthetic
/// locomotion graph over them: every frame's pose must match what the
/// sampler gives for the same clip times, including through the blend.
#[test]
fn vanilla_idle_and_walk_match_the_sampler() {
    let Some(archive) = animations_archive() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let started = Instant::now();
    let skeleton = human_skeleton(&archive);
    // The names `mt_behavior.hkx` uses for its clip generators.
    let idle_name = r"Animations\male\MT_Idle.HKX";
    let walk_name = r"Animations\male\MT_WalkForward.hkx";
    let mut lib = ClipLibrary::new();
    for name in [idle_name, walk_name] {
        lib.insert(vanilla_clip(&archive, name).unwrap_or_else(|| panic!("{name} missing")));
    }
    let idle_clip = lib.get(idle_name).unwrap();
    let walk_clip = lib.get(walk_name).unwrap();
    println!(
        "skeleton {} bones, idle {:.3} s {} tracks, walk {:.3} s {} tracks, loaded in {:.2}s",
        skeleton.bones.len(),
        idle_clip.duration(),
        idle_clip.animation.num_transform_tracks,
        walk_clip.duration(),
        walk_clip.animation.num_transform_tracks,
        started.elapsed().as_secs_f32()
    );
    assert_eq!(skeleton.bones.len(), 99);
    assert!(idle_clip.track_to_bone().iter().all(|&b| b < 99));

    let bytes = graph(&Root::Machine, idle_name, walk_name);
    let pf = Packfile::parse(&bytes).unwrap();
    let g = BehaviorGraph::from_packfile(&pf).unwrap();
    let root = g.root.unwrap();
    let idle = g.find_by_name("IdleClip").unwrap().id;
    let walk = g.find_by_name("WalkClip").unwrap().id;
    let mut vm = Interpreter::new(&g, &lib, skeleton.reference_pose.clone());

    // Tolerance: the interpreter samples through the same decoder, so any
    // difference is bookkeeping (time, bone map, blend), not quantization.
    // The blend alpha is recomputed from a frame count, which differs from
    // the interpreter's accumulated time by float rounding only.
    let tolerance = 1e-4;
    let dt = 1.0 / 60.0;
    let blend = 0.25;
    let mut worst = 0.0f32;
    let mut blend_frames = 0;
    let mut transition_started = None;
    let mut update_time = std::time::Duration::ZERO;
    // The idle clip's trigger at 0.3 s raises walkStart, so the transition
    // begins after that frame's generate and its first frame blends at 0.
    for frame in 1..=180 {
        let t0 = Instant::now();
        let out = vm.update(dt).clone();
        update_time += t0.elapsed();
        assert_eq!(out.pose.len(), 99);
        assert_eq!(out.weight, 1.0, "frame {frame}");
        let mut expected = skeleton.reference_pose.clone();
        if vm.in_transition(root) {
            let start = *transition_started.get_or_insert(frame);
            blend_frames += 1;
            let alpha = ((frame - start) as f32 * dt / blend).min(1.0);
            let mut from = skeleton.reference_pose.clone();
            idle_clip.sample_into(vm.clip_time(idle).unwrap(), &mut from);
            let mut to = skeleton.reference_pose.clone();
            walk_clip.sample_into(vm.clip_time(walk).unwrap(), &mut to);
            for (e, (a, b)) in expected.iter_mut().zip(from.iter().zip(&to)) {
                *e = blend_transform(a, b, alpha);
            }
        } else if vm.current_state_name(root) == Some("Idle") {
            let t = vm.clip_time(idle).unwrap();
            let expected_t = (frame as f32 * dt) % idle_clip.duration();
            assert!((t - expected_t).abs() < 1e-3, "frame {frame}: idle at {t}");
            idle_clip.sample_into(t, &mut expected);
        } else {
            assert_eq!(vm.current_state_name(root), Some("Walk"));
            assert_eq!(vm.clip_time(idle), None, "frame {frame}: idle still active");
            walk_clip.sample_into(vm.clip_time(walk).unwrap(), &mut expected);
        }
        let diff = max_pose_diff(&out.pose, &expected);
        worst = worst.max(diff);
        assert!(diff < tolerance, "frame {frame}: pose differs by {diff}");
    }
    let model = skeleton.to_model_space(vm.pose()).unwrap();
    let head = skeleton.bone_index("NPC Head [Head]").unwrap();
    println!(
        "180 frames at 1/60 s: worst pose diff {worst:e}, transition at frame {:?}, \
         {blend_frames} blend frames, head at z {:.1} in model space, {:.1} us per update",
        transition_started,
        model[head].translation[2],
        update_time.as_secs_f64() * 1e6 / 180.0
    );
    assert_eq!(vm.current_state_name(root), Some("Walk"));
    // 0.25 s at 1/60 s is 15 frames, one more when the summed time lands
    // a rounding error short of the duration.
    assert!((15..=16).contains(&blend_frames), "{blend_frames}");
    assert!(model[head].translation[2] > 100.0);
}

/// The vanilla `mt_behavior.hkx` with every clip it names that the archive
/// holds: activate, tick, report what ran and what the interpreter could
/// not evaluate. The numbers go to `docs/hkx.md`.
#[test]
fn mt_behavior_runs_on_the_vanilla_clips() {
    let Some(archive) = animations_archive() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let started = Instant::now();
    let skeleton = human_skeleton(&archive);
    let path = format!("{CHARACTER}behaviors/mt_behavior.hkx");
    let bytes = archive.read(&path).expect("mt_behavior reads");
    let pf = Packfile::parse(&bytes).expect("mt_behavior parses");
    let mut g = BehaviorGraph::from_packfile(&pf).expect("mt_behavior loads");

    let before_nodes = g.nodes.len();
    let before_events = g.data.event_names.len();
    let before_variables = g.data.variable_names.len();
    let report = g.link_all(8, |name| vanilla_behavior(&archive, name));
    println!(
        "  linked {:?}, missing {:?}, failed {:?}, {} rounds",
        report.linked, report.missing, report.failed, report.rounds
    );
    println!(
        "  nodes {before_nodes} to {}, events {before_events} to {}, variables \
         {before_variables} to {}",
        g.nodes.len(),
        g.data.event_names.len(),
        g.data.variable_names.len()
    );
    assert!(
        report.failed.is_empty(),
        "a referenced graph would not splice: {:?}",
        report.failed
    );
    let g = g;

    let names = g.clip_animation_names();
    let mut lib = ClipLibrary::new();
    let mut missing = Vec::new();
    for name in &names {
        match vanilla_clip(&archive, name) {
            Some(c) => lib.insert(c),
            None => missing.push(name.clone()),
        }
    }
    println!(
        "mt_behavior: {} nodes, {} events, {} variables, {} clips named, {} loaded, {} missing, \
         {:.2}s",
        g.nodes.len(),
        g.data.event_names.len(),
        g.data.variable_names.len(),
        names.len(),
        lib.len(),
        missing.len(),
        started.elapsed().as_secs_f32()
    );
    for m in missing.iter().take(10) {
        println!("  missing {m}");
    }

    let mut vm = Interpreter::new(&g, &lib, skeleton.reference_pose.clone());
    println!(
        "  {} bindings resolved, {} unbound",
        vm.bound_count(),
        vm.unsupported().unbound.len()
    );
    let dt = 1.0 / 60.0;
    let t0 = Instant::now();
    for _ in 0..120 {
        vm.update(dt);
    }
    let idle_us = t0.elapsed().as_secs_f64() * 1e6 / 120.0;
    let states: Vec<String> = vm
        .active_states()
        .iter()
        .map(|(id, s)| format!("{}={s}", g.node(*id).name))
        .collect();
    let clips: Vec<String> = vm
        .active_clips()
        .iter()
        .map(|(id, t)| format!("{}@{t:.2}", g.node(*id).name))
        .collect();
    println!(
        "  after 2 s idle: weight {:.2}, states {states:?}",
        vm.output().weight
    );
    println!("  clips {clips:?}");
    let changes = vm.take_changes();
    println!(
        "  {} state changes, {} events handled",
        changes.len(),
        vm.take_events().len()
    );

    // Ask for a walk the way the game does: speed up and send the event.
    let speed = vm.variables.index("Speed");
    if let Some(i) = speed {
        vm.variables.set_real(i, 1.0);
    }
    let walk_events: Vec<&str> = g
        .data
        .event_names
        .iter()
        .filter(|n| n.eq_ignore_ascii_case("moveStart") || n.eq_ignore_ascii_case("walkStart"))
        .map(String::as_str)
        .collect();
    for e in &walk_events {
        vm.send_event_named(e);
    }
    let t0 = Instant::now();
    for _ in 0..120 {
        vm.update(dt);
    }
    let walk_us = t0.elapsed().as_secs_f64() * 1e6 / 120.0;
    let states: Vec<String> = vm
        .active_states()
        .iter()
        .map(|(id, s)| format!("{}={s}", g.node(*id).name))
        .collect();
    let clips: Vec<String> = vm
        .active_clips()
        .iter()
        .map(|(id, t)| format!("{}@{t:.2}", g.node(*id).name))
        .collect();
    let walk_changes = vm.take_changes().len();
    println!(
        "  after Speed=1 and {walk_events:?}: weight {:.2}, {walk_changes} state changes, \
         states {states:?}",
        vm.output().weight
    );
    println!("  clips {clips:?}");
    let u = vm.unsupported();
    println!(
        "  unsupported: {} missing clips, {} behavior references {:?}, {} unknown classes {:?}",
        u.missing_clips.len(),
        u.behavior_references.len(),
        u.behavior_references,
        u.unknown_classes.len(),
        u.unknown_classes
    );
    for b in u.unbound.iter().take(12) {
        println!("  unbound {b}");
    }
    println!("  {idle_us:.1} us per update idle, {walk_us:.1} us per update walking");
    assert_eq!(vm.pose().len(), 99);
    assert!(!vm.active_states().is_empty(), "the root never activated");
}

/// Every behavior modifier class whose interpreter path is still a no-op,
/// counted across the installed animation archive. The sorted inventory is
/// the worklist in `BACKLOG.csv` and `docs/hkx.md`.
#[test]
fn installed_behavior_passthrough_modifier_inventory() {
    let Some(archive) = animations_archive() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let mut classes = BTreeMap::new();
    let mut list_children = BTreeMap::new();
    let mut behavior_files = 0usize;
    for name in archive
        .files()
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".hkx"))
    {
        let bytes = archive.read(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let pf = Packfile::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        for graph_obj in pf.objects_of("hkbBehaviorGraph") {
            let graph = BehaviorGraph::from_obj(graph_obj)
                .unwrap_or_else(|e| panic!("{name}: behavior graph: {e}"));
            behavior_files += 1;
            for node in &graph.nodes {
                if let NodeKind::ModifierList { modifiers, .. } = &node.kind {
                    for child in modifiers.iter().flatten() {
                        *list_children
                            .entry(graph.node(*child).class.clone())
                            .or_insert(0usize) += 1;
                    }
                }
                if matches!(
                    node.kind,
                    NodeKind::Modifier { .. }
                        | NodeKind::ModifierGenerator { .. }
                        | NodeKind::ModifierList { .. }
                        | NodeKind::EventDrivenModifier { .. }
                        | NodeKind::TimerModifier { .. }
                ) {
                    *classes.entry(node.class.clone()).or_insert(0usize) += 1;
                }
            }
        }
    }
    println!("{behavior_files} behavior graphs, passthrough modifiers {classes:?}");
    println!("modifier-list children {list_children:?}");
    let expected = PASSTHROUGH_MODIFIERS
        .iter()
        .map(|(class, count, _)| ((*class).to_string(), *count))
        .collect();
    assert_eq!(classes, expected);
    for (class, _, row) in PASSTHROUGH_MODIFIERS {
        assert!(
            include_str!("../../../BACKLOG.csv").lines().any(|line| {
                let fields: Vec<&str> = line.split('\"').collect();
                fields.get(1) == Some(row)
                    && fields.get(5).is_some_and(|title| title.contains(class))
            }),
            "{class} has no follow-up row {row} named for that class"
        );
    }
}

#[test]
fn every_shipped_packfile_parses_and_animations_decode() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let limit: usize = std::env::var("SOE_HKX_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let started = Instant::now();
    let archive =
        soe_bsa::Archive::open(&data.join("Skyrim - Animations.bsa")).expect("archive opens");
    let names: Vec<String> = archive
        .files()
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".hkx"))
        .cloned()
        .collect();
    let mut files = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut by_error: BTreeMap<String, usize> = BTreeMap::new();
    let mut roots: BTreeMap<String, usize> = BTreeMap::new();
    let mut animations = 0usize;
    let mut decoded = 0usize;
    let mut decode_failures: Vec<String> = Vec::new();
    let mut tracks = 0usize;
    for name in names {
        if limit != 0 && files >= limit {
            break;
        }
        let bytes = archive.read(&name).expect("file reads");
        files += 1;
        let pf = match Packfile::parse(&bytes) {
            Ok(pf) => pf,
            Err(e) => {
                let text = e.to_string();
                let key = text.split(" at ").next().unwrap_or(&text).to_string();
                *by_error.entry(key).or_default() += 1;
                failures.push(format!("{name}: {text}"));
                continue;
            }
        };
        let root = pf
            .root()
            .ok()
            .and_then(|r| r.class().map(str::to_string))
            .unwrap_or_else(|| "?".into());
        *roots.entry(root).or_default() += 1;
        for obj in pf.objects_of("hkaSplineCompressedAnimation") {
            animations += 1;
            match SplineCompressedAnimation::from_obj(obj).and_then(|a| a.decode().map(|b| (a, b)))
            {
                Ok((anim, blocks)) => {
                    decoded += 1;
                    tracks += anim.sample_pose(&blocks, 0.0).len();
                }
                Err(e) => decode_failures.push(format!("{name}: {e}")),
            }
        }
    }
    println!(
        "{files} packfiles in {:.1}s: {} parse failures, roots {roots:?}",
        started.elapsed().as_secs_f32(),
        failures.len()
    );
    println!(
        "{animations} spline animations, {decoded} decoded, {tracks} tracks sampled, {} decode failures",
        decode_failures.len()
    );
    for (err, n) in &by_error {
        println!("  {n} x {err}");
    }
    for f in failures.iter().chain(decode_failures.iter()).take(40) {
        println!("  {f}");
    }
    let rate = (failures.len() + decode_failures.len()) as f64 / files.max(1) as f64;
    assert!(
        rate < 0.01,
        "{} of {files} packfiles failed",
        failures.len() + decode_failures.len()
    );
}

#[test]
fn anm_01_shipped_transition_expressions_are_decoded_and_diagnosed() {
    let Some(archive) = animations_archive() else {
        return;
    };
    let clips = ClipLibrary::new();
    let mut graphs = 0;
    let mut conditions = 0;
    let mut failures = Vec::new();
    for name in archive.files().iter().filter(|name| {
        std::path::Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("hkx"))
    }) {
        let bytes = archive.read(name).unwrap();
        let pf = Packfile::parse(&bytes).unwrap();
        if pf.objects_of("hkbBehaviorGraph").is_empty() {
            continue;
        }
        let graph = BehaviorGraph::from_packfile(&pf).unwrap();
        graphs += 1;
        let mut decoded_sources = std::collections::BTreeSet::new();
        for node in &graph.nodes {
            if let soe_hkx::behavior::NodeKind::StateMachine(sm) = &node.kind {
                for transition in sm
                    .states
                    .iter()
                    .flat_map(|state| &state.transitions)
                    .chain(&sm.wildcard_transitions)
                {
                    if let Some(soe_hkx::behavior::Condition::Expression(source)) =
                        &transition.condition
                    {
                        conditions += 1;
                        decoded_sources.insert(source.clone());
                    }
                }
            }
        }
        for class in ["hkbExpressionCondition", "hkbStringCondition"] {
            for condition in pf.objects_of(class) {
                let source = condition.string_or_empty(16).unwrap();
                assert!(
                    decoded_sources.contains(&source),
                    "{name}: missed serialized {class}: {source}"
                );
            }
        }
        let vm = Interpreter::new(&graph, &clips, Vec::new());
        failures.extend(
            vm.unsupported()
                .conditions
                .iter()
                .map(|error| format!("{name}: {error}")),
        );
    }
    eprintln!(
        "{graphs} behavior graphs, {conditions} guards, {} unsupported expressions",
        failures.len()
    );
    assert!(graphs > 100, "survey did not cover shipped behaviors");
    failures.sort();
    assert_eq!(failures, [
        "meshes/actors/draugr/behaviors/draugrbehavior.hkx: SpeedSampled < 5.0: UnknownSymbol(\"SpeedSampled\")",
        "meshes/actors/draugr/behaviors/draugrbehavior.hkx: SpeedSampled >= 5.0: UnknownSymbol(\"SpeedSampled\")",
        "meshes/actors/troll/behaviors/trollbehavior.hkx: (staggerDirection < .25) || (staggerDirection > .75) || (staggerMagnitude > .3 , < .7): Syntax(79)",
    ], "undeclared draugr variable and ANM-50 comma comparison stay closed");
}

#[test]
fn installed_behavior_bone_masks_decode_without_losing_authored_weights() {
    let Some(archive) = animations_archive() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let mut graphs = 0;
    let mut masks = 0;
    for path in archive
        .files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".hkx"))
    {
        let pf = Packfile::parse(&archive.read(path).unwrap()).unwrap();
        if pf.objects_of("hkbBehaviorGraph").is_empty() {
            continue;
        }
        let graph = BehaviorGraph::from_packfile(&pf).unwrap_or_else(|e| panic!("{path}: {e}"));
        graphs += 1;
        for node in &graph.nodes {
            let soe_hkx::behavior::NodeKind::Blender(blender) = &node.kind else {
                continue;
            };
            let raw_children = pf.object(node.offset).array_ptrs(96).unwrap();
            for (child, raw) in blender
                .children
                .iter()
                .zip(raw_children.into_iter().flatten())
            {
                let raw_mask = raw.ptr(56).unwrap().map(|m| m.array_f32(48).unwrap());
                assert_eq!(child.bone_weights, raw_mask, "{path}");
                if raw_mask.is_some() {
                    masks += 1;
                }
            }
        }
    }
    assert!(
        graphs > 0 && masks > 0,
        "exercise authored masks in installed graphs"
    );
    println!("authored bone masks: {graphs} graphs, {masks} masked children");
}

#[test]
fn every_shipped_false_to_true_modifier_decodes_all_three_slots() {
    let Some(archive) = animations_archive() else {
        return;
    };
    // The prior full behavior inventory contains 428 graphs. Scan the whole archive.
    let mut graphs = 0;
    let mut modifiers = 0;
    let mut enabled_slots = 0;
    let mut payloads = 0;
    for name in archive
        .files()
        .iter()
        .filter(|name| name.to_lowercase().ends_with(".hkx"))
    {
        let bytes = archive.read(name).unwrap();
        let pack = Packfile::parse(&bytes).unwrap();
        for object in pack.objects_of("hkbBehaviorGraph") {
            let graph = BehaviorGraph::from_obj(object).unwrap();
            graphs += 1;
            let clips = ClipLibrary::new();
            let vm = Interpreter::new(&graph, &clips, vec![]);
            for node in graph.nodes_of_class("BSEventOnFalseToTrueModifier") {
                let soe_hkx::behavior::NodeKind::FalseToTrueModifier { slots, .. } = &node.kind
                else {
                    panic!("{name}: {} was not decoded", node.name)
                };
                modifiers += 1;
                enabled_slots += slots.iter().filter(|slot| slot.enable).count();
                payloads += slots.iter().filter(|slot| slot.event.has_payload).count();
                assert!(
                    !vm.unsupported().unbound.iter().any(|binding| {
                        binding.starts_with(&format!("{}.bVariableToTest", node.name))
                            || binding.starts_with(&format!("{}.bEnableEvent", node.name))
                    }),
                    "{name}: {} has an unresolved edge binding",
                    node.name
                );
            }
        }
    }
    assert!(graphs >= 428, "only {graphs} behavior graphs scanned");
    assert!(modifiers > 0, "the survey did not reach any edge modifiers");
    println!("{graphs} graphs, {modifiers} false-to-true modifiers, {enabled_slots} enabled slots, {payloads} slot payloads");
}
