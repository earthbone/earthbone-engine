// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{
    BehaviorGraph, BehaviorGraphData, ExpressionData, Node, NodeId, NodeKind, VariableInfo,
    VariableType,
};
use soe_hkx::interp::{ClipLibrary, Interpreter};

fn graph(
    expressions: Vec<ExpressionData>,
    names: &[&str],
    types: &[VariableType],
) -> BehaviorGraph {
    BehaviorGraph {
        name: "adversarial expressions".into(),
        variable_mode: 0,
        root: Some(NodeId(0)),
        data: BehaviorGraphData {
            variable_names: names.iter().map(|name| (*name).into()).collect(),
            variable_infos: types
                .iter()
                .map(|ty| VariableInfo {
                    ty: *ty,
                    ..Default::default()
                })
                .collect(),
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
                class: "hkbEvaluateExpressionModifier".into(),
                name: "expressions".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::EvaluateExpression {
                    enable: true,
                    expressions,
                },
            },
        ],
    }
}

fn assignment(expression: &str, target: i32) -> ExpressionData {
    ExpressionData {
        expression: expression.into(),
        assignment_variable_index: target,
        assignment_event_index: -1,
        event_mode: 0,
    }
}

#[test]
fn typed_integer_expression_writes_the_expected_result() {
    let graph = graph(
        vec![assignment("Count * 2 + 1", 1)],
        &["Count", "Result"],
        &[VariableType::Int32, VariableType::Int32],
    );
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    interpreter.variables.set_word(0, 7);

    interpreter.update(1.0 / 60.0);

    assert_eq!(interpreter.variables.word(1), Some(15));
    assert!(interpreter.unsupported().expression_modifiers.is_empty());
}

#[test]
fn later_entries_read_assignments_from_earlier_serialized_entries() {
    let graph = graph(
        vec![assignment("Input + 2", 1), assignment("First * 3", 2)],
        &["Input", "First", "Second"],
        &[VariableType::Real, VariableType::Real, VariableType::Real],
    );
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    interpreter.variables.set_real(0, 4.0);

    interpreter.update(1.0 / 60.0);

    assert_eq!(interpreter.variables.real(1), Some(6.0));
    assert_eq!(interpreter.variables.real(2), Some(18.0));
}

#[test]
fn unsupported_syntax_keeps_the_target_value_and_reports_the_expression() {
    let graph = graph(
        vec![assignment("Input + (1", 1)],
        &["Input", "Result"],
        &[VariableType::Real, VariableType::Real],
    );
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    interpreter.variables.set_real(0, 2.5);
    interpreter.variables.set_real(1, 9.0);

    interpreter.update(1.0 / 60.0);

    assert_eq!(interpreter.variables.real(1), Some(9.0));
    assert_eq!(interpreter.unsupported().expression_modifiers.len(), 1);
    assert!(interpreter.unsupported().expression_modifiers[0].contains("Syntax"));
}
