// SPDX-License-Identifier: GPL-3.0-or-later

//! Small x86 fixtures isolate pointer-sized fields and embedded array strides.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::character::ProjectData;
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Error, Packfile};

#[test]
fn le_project_preserves_vector_alignment_and_all_string_tables() {
    let mut b = PackfileBuilder::new_32bit();
    let root = b.object("hkRootLevelContainer", 12);
    let project = b.object("hkbProjectData", 48);
    let variant = b.array_structs(root, 0, 12, 1);
    b.string(variant, 0, "Project");
    b.string(variant, 4, "hkbProjectData");
    b.ptr(variant, 8, project);
    let strings = b.object("hkbProjectStringData", 76);
    b.set_vec4(project, 16, [0.0, 0.0, 1.0, 0.0]);
    b.ptr(project, 32, strings);
    b.set_i8(project, 36, 2);
    for (offset, values) in [
        (8, ["walk.hkx", "run.hkx"]),
        (20, ["master.hkx", "child.hkx"]),
        (32, ["male.hkx", "female.hkx"]),
        (44, ["Walk", "Run"]),
    ] {
        b.array_strings(strings, offset, &values);
    }
    for (offset, value) in [
        (56, "animations"),
        (60, "behaviors"),
        (64, "characters"),
        (68, "source"),
    ] {
        b.string(strings, offset, value);
    }
    let pf = Packfile::parse(&b.finish()).unwrap();
    let project = ProjectData::from_packfile(&pf).unwrap();
    assert_eq!(project.world_up_ws, [0.0, 0.0, 1.0, 0.0]);
    assert_eq!(project.default_event_mode, 2);
    assert_eq!(project.animation_filenames, ["walk.hkx", "run.hkx"]);
    assert_eq!(project.behavior_filenames, ["master.hkx", "child.hkx"]);
    assert_eq!(project.character_filenames, ["male.hkx", "female.hkx"]);
    assert_eq!(project.event_names, ["Walk", "Run"]);
    assert_eq!(
        (
            project.animation_path.as_str(),
            project.behavior_path.as_str(),
            project.character_path.as_str(),
            project.full_path_to_source.as_str()
        ),
        ("animations", "behaviors", "characters", "source")
    );
}

fn clip_file(trigger_count: i32) -> Vec<u8> {
    let mut b = PackfileBuilder::new_32bit();
    let root = b.object("hkRootLevelContainer", 12);
    let graph = b.object("hkbBehaviorGraph", 88);
    let variant = b.array_structs(root, 0, 12, 1);
    b.string(variant, 0, "Behavior");
    b.string(variant, 4, "hkbBehaviorGraph");
    b.ptr(variant, 8, graph);
    let clip = b.object("hkbClipGenerator", 176);
    let triggers = b.object("hkbClipTriggerArray", 20);
    let payload = b.object("hkbStringEventPayload", 12);
    let bindings = b.object("hkbVariableBindingSet", 24);
    b.ptr(graph, 76, clip);
    b.set_u32(clip, 28, 0xfedc_ba98);
    // The next field must never be consumed as the upper half of hkUlong.
    b.set_u32(clip, 32, 0xdead_beef);
    b.string(clip, 32, "Walk");
    b.string(clip, 40, "walk.hkx");
    b.ptr(clip, 44, triggers);
    b.ptr(clip, 8, bindings);
    let values = b.array_structs(bindings, 8, 32, 2);
    for (i, path) in ["playbackSpeed", "startTime"].iter().enumerate() {
        let at = values + i as u32 * 32;
        b.string(at, 0, path);
        b.set_i32(at, 20, 7 + i as i32);
        b.set_i8(at, 24, -1);
        b.set_i8(at, 25, i as i8);
    }
    b.set_i32(bindings, 20, -1);
    let values = b.array_structs(triggers, 8, 16, 2);
    for i in 0..2 {
        let at = values + i * 16;
        b.set_f32(at, 0, 0.25 + i as f32);
        b.set_i32(at, 4, 100 + i as i32);
        b.set_bool(at, 12, i == 1);
        b.set_bool(at, 13, i == 0);
        b.set_bool(at, 14, i == 1);
    }
    b.ptr(values, 8, payload);
    b.set_i32(triggers, 12, trigger_count);
    b.finish()
}

#[test]
fn le_clip_preserves_ulong_bindings_trigger_stride_and_payload_fixups() {
    let pf = Packfile::parse(&clip_file(2)).unwrap();
    let graph = BehaviorGraph::from_packfile(&pf).unwrap();
    let node = graph.node(graph.root.unwrap());
    assert_eq!(node.user_data, 0xfedc_ba98);
    assert_eq!(node.name, "Walk");
    let bindings = node.bindings.as_ref().unwrap();
    assert_eq!(bindings.bindings.len(), 2);
    assert_eq!(bindings.bindings[1].member_path, "startTime");
    assert_eq!(bindings.bindings[1].variable_index, 8);
    assert_eq!(bindings.bindings[1].binding_type, 1);
    assert_eq!(bindings.bindings[0].bit_index, -1);
    let NodeKind::Clip(clip) = &node.kind else {
        panic!("expected clip")
    };
    assert_eq!(clip.animation_name, "walk.hkx");
    assert_eq!(clip.triggers.len(), 2);
    assert!(clip.triggers[0].event.has_payload);
    assert!(!clip.triggers[1].event.has_payload);
    assert_eq!(clip.triggers[1].event.id, 101);
    assert_eq!(clip.triggers[1].local_time, 1.25);
    assert!(clip.triggers[1].relative_to_end_of_clip);
    assert!(clip.triggers[0].acyclic);
    assert!(clip.triggers[1].is_annotation);
}

#[test]
fn le_array_count_errors_are_reported_before_iteration() {
    for count in [-1, i32::MAX] {
        let pf = Packfile::parse(&clip_file(count)).unwrap();
        assert!(matches!(
            BehaviorGraph::from_packfile(&pf),
            Err(Error::BadFixup { .. } | Error::Truncated { .. })
        ));
    }
}
