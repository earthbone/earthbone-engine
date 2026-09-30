// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{
    BehaviorGraph, BehaviorGraphData, Binding, Node, NodeId, NodeKind, VariableBindingSet,
    VariableInfo, VariableType, VariableValueSet,
};
use soe_hkx::interp::{ClipLibrary, Interpreter};

fn parent() -> BehaviorGraph {
    BehaviorGraph {
        name: "parent".into(),
        variable_mode: 0,
        root: Some(NodeId(0)),
        data: BehaviorGraphData {
            variable_names: vec!["parentQuad".into(), "parentPointer".into()],
            variable_infos: vec![
                VariableInfo {
                    ty: VariableType::Vector4,
                    ..Default::default()
                },
                VariableInfo {
                    ty: VariableType::Pointer,
                    ..Default::default()
                },
            ],
            variable_initial_values: Some(VariableValueSet {
                word_values: vec![0, 0],
                quad_values: vec![[10.0, 20.0, 30.0, 40.0]],
                variant_values: vec![Some(100)],
            }),
            ..Default::default()
        },
        nodes: vec![Node {
            id: NodeId(0),
            offset: 0,
            class: "hkbBehaviorReferenceGenerator".into(),
            name: "child reference".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::BehaviorReference {
                behavior_name: "child.hkx".into(),
                resolved: None,
            },
        }],
    }
}

fn child(name: &str, quad: [f32; 4], pointer: Option<u32>) -> BehaviorGraph {
    BehaviorGraph {
        name: name.into(),
        variable_mode: 0,
        root: Some(NodeId(0)),
        data: BehaviorGraphData {
            variable_names: vec![format!("{name}Quad"), format!("{name}Pointer")],
            variable_infos: vec![
                VariableInfo {
                    ty: VariableType::Quaternion,
                    ..Default::default()
                },
                VariableInfo {
                    ty: VariableType::Pointer,
                    ..Default::default()
                },
            ],
            variable_initial_values: Some(VariableValueSet {
                word_values: vec![0, 0],
                quad_values: vec![quad],
                variant_values: vec![pointer],
            }),
            ..Default::default()
        },
        nodes: vec![Node {
            id: NodeId(0),
            offset: 0,
            class: "hkbClipGenerator".into(),
            name: "child root".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::Unknown,
        }],
    }
}

#[test]
fn successive_links_keep_quad_and_variant_indices_disjoint() {
    let mut graph = parent();
    graph
        .link_reference(NodeId(0), child("first", [1.0, 2.0, 3.0, 4.0], Some(200)))
        .unwrap();
    let second_reference = graph.nodes.len();
    graph.nodes.push(Node {
        id: NodeId(second_reference),
        offset: 99,
        class: "hkbBehaviorReferenceGenerator".into(),
        name: "second reference".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::BehaviorReference {
            behavior_name: "second.hkx".into(),
            resolved: None,
        },
    });
    graph
        .link_reference(
            NodeId(second_reference),
            child("second", [5.0, 6.0, 7.0, 8.0], None),
        )
        .unwrap();

    let values = graph.data.variable_initial_values.as_ref().unwrap();
    assert_eq!(values.word_values, [0, 0, 1, 1, 2, 2]);
    assert_eq!(
        values.quad_values,
        [
            [10.0, 20.0, 30.0, 40.0],
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0]
        ]
    );
    assert_eq!(values.variant_values, [Some(100), Some(200), None]);
}

#[test]
fn missing_child_initial_value_tables_create_zero_defaults_without_losing_parent_values() {
    let mut graph = parent();
    let mut child = child("empty", [0.0; 4], None);
    child.data.variable_initial_values = None;
    graph.link_reference(NodeId(0), child).unwrap();
    let values = graph.data.variable_initial_values.as_ref().unwrap();
    assert_eq!(values.word_values, [0, 0, 0, 0]);
    assert_eq!(values.quad_values, [[10.0, 20.0, 30.0, 40.0]]);
    assert_eq!(values.variant_values, [Some(100)]);
}

#[test]
fn invalid_child_table_indices_do_not_alias_a_parent_default() {
    let mut graph = parent();
    let mut child = child("bad", [1.0, 2.0, 3.0, 4.0], Some(300));
    child
        .data
        .variable_initial_values
        .as_mut()
        .unwrap()
        .word_values = vec![99, 99];
    graph.link_reference(NodeId(0), child).unwrap();
    let values = graph.data.variable_initial_values.as_ref().unwrap();
    assert_eq!(values.word_values[2..], [99, 99]);
    assert_eq!(values.quad_values[0], [10.0, 20.0, 30.0, 40.0]);
    assert_eq!(values.variant_values, [Some(100), Some(300)]);
}

#[test]
fn linked_child_vector_binding_reads_the_offset_quad_default() {
    let mut graph = parent();
    let value = [1.25, -2.5, 3.75, -4.0];
    let sub = BehaviorGraph {
        name: "vector child".into(),
        variable_mode: 0,
        root: Some(NodeId(0)),
        data: BehaviorGraphData {
            variable_names: vec![
                "x".into(),
                "y".into(),
                "z".into(),
                "w".into(),
                "vector".into(),
            ],
            variable_infos: vec![
                VariableType::Real,
                VariableType::Real,
                VariableType::Real,
                VariableType::Real,
                VariableType::Vector4,
            ]
            .into_iter()
            .map(|ty| VariableInfo {
                ty,
                ..Default::default()
            })
            .collect(),
            variable_initial_values: Some(VariableValueSet {
                word_values: vec![0, 0, 0, 0, 0],
                quad_values: vec![value],
                variant_values: Vec::new(),
            }),
            ..Default::default()
        },
        nodes: vec![
            Node {
                id: NodeId(0),
                offset: 0,
                class: "hkbModifierGenerator".into(),
                name: "wrapper".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::ModifierGenerator {
                    modifier: Some(NodeId(1)),
                    generator: None,
                },
            },
            Node {
                id: NodeId(1),
                offset: 1,
                class: "BSDecomposeVectorModifier".into(),
                name: "decompose".into(),
                user_data: 0,
                bindings: Some(VariableBindingSet {
                    bindings: vec![("vector", 4), ("x", 0), ("y", 1), ("z", 2), ("w", 3)]
                        .into_iter()
                        .map(|(member_path, variable_index)| Binding {
                            member_path: member_path.into(),
                            variable_index,
                            bit_index: -1,
                            binding_type: 0,
                        })
                        .collect(),
                    index_of_binding_to_enable: -1,
                }),
                kind: NodeKind::DecomposeVector {
                    enable: true,
                    vector: [0.0; 4],
                },
            },
        ],
    };
    graph.link_reference(NodeId(0), sub).unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.update(1.0 / 60.0);
    assert_eq!(
        [
            vm.variables.real(2),
            vm.variables.real(3),
            vm.variables.real(4),
            vm.variables.real(5)
        ],
        [Some(1.25), Some(-2.5), Some(3.75), Some(-4.0)]
    );
}
