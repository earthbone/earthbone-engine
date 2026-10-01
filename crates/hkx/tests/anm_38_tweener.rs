// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

#[derive(Clone, Copy)]
struct TweenerConfig {
    enabled: bool,
    tween_position: bool,
    tween_rotation: bool,
    use_tween_duration: bool,
    tween_duration: f32,
    target_position: [f32; 4],
    target_rotation: [f32; 4],
}

fn tweener_packfile(config: TweenerConfig) -> Vec<u8> {
    let mut builder = PackfileBuilder::new();
    let root = builder.object("hkRootLevelContainer", 16);
    let graph = builder.object("hkbBehaviorGraph", 304);
    let variants = builder.array_structs(root, 0, 24, 1);
    builder.string(variants, 0, "hkbBehaviorGraph");
    builder.string(variants, 8, "hkbBehaviorGraph");
    builder.ptr(variants, 16, graph);

    let tweener = builder.object("BSTweenerModifier", 208);
    builder.string(tweener, 56, "Tweener");
    builder.set_bool(tweener, 72, config.enabled);
    builder.set_bool(tweener, 80, config.tween_position);
    builder.set_bool(tweener, 81, config.tween_rotation);
    builder.set_bool(tweener, 82, config.use_tween_duration);
    builder.set_f32(tweener, 84, config.tween_duration);
    builder.set_vec4(tweener, 96, config.target_position);
    builder.set_vec4(tweener, 112, config.target_rotation);
    builder.ptr(graph, 128, tweener);
    builder.finish()
}

fn executable_graph(config: TweenerConfig) -> BehaviorGraph {
    let bytes = tweener_packfile(config);
    let packfile = Packfile::parse(&bytes).unwrap();
    let mut graph = BehaviorGraph::from_packfile(&packfile).unwrap();
    let modifier = graph.root.unwrap();
    let source = NodeId(graph.nodes.len());
    graph.nodes.push(Node {
        id: source,
        offset: 0,
        class: "FixtureGenerator".into(),
        name: "Source".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::Unknown,
    });
    let wrapper = NodeId(graph.nodes.len());
    graph.nodes.push(Node {
        id: wrapper,
        offset: 0,
        class: "hkbModifierGenerator".into(),
        name: "Wrapper".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::ModifierGenerator {
            modifier: Some(modifier),
            generator: Some(source),
        },
    });
    graph.root = Some(wrapper);
    graph
}

#[test]
fn anm_38_tweener_decodes_authored_target_and_duration_fields() {
    let bytes = tweener_packfile(TweenerConfig {
        enabled: true,
        tween_position: true,
        tween_rotation: true,
        use_tween_duration: true,
        tween_duration: 0.75,
        target_position: [3.0, 4.0, 5.0, 0.0],
        target_rotation: [0.0, 0.0, 0.6, 0.8],
    });
    let packfile = Packfile::parse(&bytes).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();
    let NodeKind::Tweener {
        enable,
        tween_position,
        tween_rotation,
        use_tween_duration,
        tween_duration,
        target_position,
        target_rotation,
    } = graph.node(graph.root.unwrap()).kind
    else {
        panic!("BSTweenerModifier was not decoded");
    };
    assert!(enable && tween_position && tween_rotation && use_tween_duration);
    assert_eq!(tween_duration, 0.75);
    assert_eq!(target_position, [3.0, 4.0, 5.0, 0.0]);
    assert_eq!(target_rotation, [0.0, 0.0, 0.6, 0.8]);
}

#[test]
fn anm_38_tweener_reaches_target_position_and_rotation_at_configured_duration() {
    let graph = executable_graph(TweenerConfig {
        enabled: true,
        tween_position: true,
        tween_rotation: true,
        use_tween_duration: true,
        tween_duration: 0.5,
        target_position: [10.0, 4.0, 2.0, 0.0],
        target_rotation: [0.0, 0.0, 1.0, 0.0],
    });
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    let midpoint = interpreter.update(0.25).world_from_model;
    assert!((midpoint.translation[0] - 5.0).abs() < 0.001);
    assert!((midpoint.translation[1] - 2.0).abs() < 0.001);
    assert!((midpoint.rotation[2] - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.001);
    assert!((midpoint.rotation[3] - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.001);

    let target = interpreter.update(0.25).world_from_model;
    assert_eq!(target.translation, [10.0, 4.0, 2.0, 0.0]);
    assert_eq!(target.rotation, [0.0, 0.0, 1.0, 0.0]);
}

#[test]
fn anm_38_tweener_respects_channel_flags_and_modifier_enable() {
    let graph = executable_graph(TweenerConfig {
        enabled: true,
        tween_position: false,
        tween_rotation: true,
        use_tween_duration: true,
        tween_duration: 0.25,
        target_position: [10.0, 4.0, 2.0, 0.0],
        target_rotation: [0.0, 0.0, 1.0, 0.0],
    });
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    let output = interpreter.update(0.25).world_from_model;
    assert_eq!(output.translation, QsTransform::IDENTITY.translation);
    assert_eq!(output.rotation, [0.0, 0.0, 1.0, 0.0]);

    let graph = executable_graph(TweenerConfig {
        enabled: false,
        tween_position: true,
        tween_rotation: true,
        use_tween_duration: true,
        tween_duration: 0.25,
        target_position: [10.0, 4.0, 2.0, 0.0],
        target_rotation: [0.0, 0.0, 1.0, 0.0],
    });
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    let output = interpreter.update(1.0).world_from_model;
    assert_eq!(output, QsTransform::IDENTITY);

    let graph = executable_graph(TweenerConfig {
        enabled: true,
        tween_position: true,
        tween_rotation: false,
        use_tween_duration: true,
        tween_duration: 0.25,
        target_position: [10.0, 4.0, 2.0, 0.0],
        target_rotation: [0.0, 0.0, 1.0, 0.0],
    });
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    let output = interpreter.update(0.25).world_from_model;
    assert_eq!(output.translation, [10.0, 4.0, 2.0, 0.0]);
    assert_eq!(output.rotation, QsTransform::IDENTITY.rotation);
}

#[test]
fn anm_38_tweener_uses_runtime_duration_when_authored_duration_is_disabled() {
    let graph = executable_graph(TweenerConfig {
        enabled: true,
        tween_position: true,
        tween_rotation: true,
        use_tween_duration: false,
        tween_duration: 10.0,
        target_position: [10.0, 4.0, 2.0, 0.0],
        target_rotation: [0.0, 0.0, 1.0, 0.0],
    });
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    let output = interpreter.update(0.0).world_from_model;
    assert_eq!(output.translation, [10.0, 4.0, 2.0, 0.0]);
    assert_eq!(output.rotation, [0.0, 0.0, 1.0, 0.0]);
}
