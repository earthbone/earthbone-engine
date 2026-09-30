// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Binding, Node, NodeId, NodeKind, VariableBindingSet, VariableInfo, VariableType};
use soe_hkx::interp::{ClipLibrary, Interpreter};

#[test]
fn anm_30_fractional_rate_and_target_clamp_are_stable_across_updates() {
    let graph = BehaviorGraph { name: "interp".into(), variable_mode: 0, root: Some(NodeId(0)),
        data: BehaviorGraphData { variable_infos: vec![VariableInfo { ty: VariableType::Real, ..Default::default() }],
            variable_names: vec!["Result".into()], ..Default::default() },
        nodes: vec![
            Node { id: NodeId(0), offset: 0, class: "hkbModifierGenerator".into(), name: "root".into(), user_data: 0, bindings: None,
                kind: NodeKind::ModifierGenerator { modifier: Some(NodeId(1)), generator: None } },
            Node { id: NodeId(1), offset: 1, class: "BSInterpValueModifier".into(), name: "interp".into(), user_data: 0,
                bindings: Some(VariableBindingSet { bindings: vec![Binding { member_path: "result".into(), variable_index: 0, bit_index: -1, binding_type: 0 }], index_of_binding_to_enable: -1 }),
                kind: NodeKind::InterpValue { enable: true, source: 0.0, target: 1.0, result: 0.0, gain: 2.0 } }
        ] };
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.25);
    assert_eq!(vm.variables.real(0), Some(0.5));
    vm.update(1.0);
    assert_eq!(vm.variables.real(0), Some(1.0));
}
