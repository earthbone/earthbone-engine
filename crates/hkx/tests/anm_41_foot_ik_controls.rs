// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

fn foot_ik_graph(enabled: bool) -> BehaviorGraph {
    let mut builder = PackfileBuilder::new();
    let root = builder.object("hkRootLevelContainer", 16);
    let graph = builder.object("hkbBehaviorGraph", 304);
    let variant = builder.array_structs(root, 0, 24, 1);
    builder.string(variant, 0, "hkbBehaviorGraph");
    builder.string(variant, 8, "hkbBehaviorGraph");
    builder.ptr(variant, 16, graph);

    let wrapper = builder.object("hkbModifierGenerator", 88);
    builder.ptr(graph, 128, wrapper);
    let modifier = builder.object("hkbFootIkControlsModifier", 176);
    builder.string(modifier, 56, "FootControls");
    builder.set_bool(modifier, 72, enabled);
    for (index, value) in (1..=12).map(|n| n as f32).enumerate() {
        builder.set_f32(modifier, 80 + 4 * index as u32, value);
    }
    let legs = builder.array_structs(modifier, 128, 48, 1);
    builder.set_vec4(legs, 0, [1.0, 2.0, 3.0, 4.0]);
    builder.set_i32(legs, 16, 17);
    builder.set_f32(legs, 32, 0.75);
    builder.set_bool(legs, 36, true);
    builder.set_bool(legs, 37, true);
    builder.set_vec4(modifier, 144, [5.0, 6.0, 7.0, 8.0]);
    builder.set_vec4(modifier, 160, [0.0, 0.0, 0.6, 0.8]);
    builder.ptr(wrapper, 72, modifier);

    let packfile = Packfile::parse(&builder.finish()).unwrap();
    BehaviorGraph::from_packfile(&packfile).unwrap()
}

#[test]
fn anm_41_decodes_control_data_leg_entries_and_output_fields() {
    let graph = foot_ik_graph(true);
    let node = graph.find_by_name("FootControls").unwrap();
    let NodeKind::FootIkControls { controls, .. } = &node.kind else {
        panic!("unexpected foot IK node: {:?}", node.kind);
    };
    assert_eq!(
        [
            controls.gains.on_off,
            controls.gains.ground_ascending,
            controls.gains.ground_descending,
            controls.gains.foot_planted,
            controls.gains.foot_raised,
            controls.gains.foot_unlock,
            controls.gains.world_from_model_feedback,
            controls.gains.error_up_down_bias,
            controls.gains.align_world_from_model,
            controls.gains.hip_orientation,
            controls.gains.max_knee_angle_difference,
            controls.gains.ankle_orientation,
        ],
        [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,],
    );
    assert_eq!(controls.legs.len(), 1);
    assert_eq!(controls.legs[0].ground_position, [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(controls.legs[0].ungrounded_event.id, 17);
    assert!(!controls.legs[0].ungrounded_event.has_payload);
    assert_eq!(controls.legs[0].vertical_error, 0.75);
    assert!(controls.legs[0].hit_something);
    assert!(controls.legs[0].planted_in_model_space);
    assert_eq!(controls.error_out_translation, [5.0, 6.0, 7.0, 8.0]);
    assert_eq!(controls.align_with_ground_rotation, [0.0, 0.0, 0.6, 0.8]);

    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    let output = interpreter.update(1.0 / 60.0);
    assert_eq!(output.foot_ik_controls.as_ref(), Some(controls));
}

#[test]
fn anm_41_disabled_modifier_emits_no_control_track() {
    let graph = foot_ik_graph(false);
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    assert!(interpreter.update(1.0 / 60.0).foot_ik_controls.is_none());
}
