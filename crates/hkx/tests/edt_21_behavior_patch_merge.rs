// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{
    BehaviorGraph, BehaviorPatch, BehaviorPatchDiagnostic, BehaviorPatchEdit, NodeKind,
};
use soe_hkx::Packfile;

fn graph() -> BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "idle.hkx", "walk.hkx");
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap()
}

fn patch(name: &str, node: &str, animation: &str) -> BehaviorPatch {
    BehaviorPatch {
        name: name.to_string(),
        edits: vec![BehaviorPatchEdit::SetClipAnimation {
            node: node.to_string(),
            animation: animation.to_string(),
        }],
    }
}

fn animation<'a>(graph: &'a BehaviorGraph, node: &str) -> &'a str {
    let node = graph.find_by_name(node).unwrap();
    let NodeKind::Clip(clip) = &node.kind else {
        panic!("expected a clip generator")
    };
    &clip.animation_name
}

#[test]
fn edt_21_named_patches_merge_in_name_order_independent_of_input_order() {
    let patches = [
        patch("z_walk", "WalkClip", "patched/walk.hkx"),
        patch("a_idle", "IdleClip", "patched/idle.hkx"),
    ];
    let mut forward = graph();
    let forward_report = forward.apply_patches(&patches);
    let mut reverse = graph();
    let reverse_report = reverse.apply_patches(&patches.into_iter().rev().collect::<Vec<_>>());

    assert_eq!(forward, reverse);
    assert_eq!(forward_report, reverse_report);
    assert_eq!(forward_report.applied, ["a_idle", "z_walk"]);
    assert!(forward_report.diagnostics.is_empty());
    assert_eq!(animation(&forward, "IdleClip"), "patched/idle.hkx");
    assert_eq!(animation(&forward, "WalkClip"), "patched/walk.hkx");
}

#[test]
fn edt_21_conflicting_edits_report_winner_and_apply_later_named_patch() {
    let patches = [
        patch("a_base", "WalkClip", "first/walk.hkx"),
        patch("z_override", "WalkClip", "last/walk.hkx"),
    ];
    let mut graph = graph();
    let report = graph.apply_patches(&patches);

    assert_eq!(animation(&graph, "WalkClip"), "last/walk.hkx");
    assert_eq!(
        report.diagnostics,
        [BehaviorPatchDiagnostic::Conflict {
            target: "node:WalkClip:animation".to_string(),
            previous_patch: "a_base".to_string(),
            winning_patch: "z_override".to_string(),
        }]
    );
}

#[test]
fn edt_21_rejected_patch_does_not_partially_change_graph() {
    let mut graph = graph();
    let before = graph.clone();
    let patch = BehaviorPatch {
        name: "invalid".to_string(),
        edits: vec![
            BehaviorPatchEdit::SetClipAnimation {
                node: "WalkClip".to_string(),
                animation: "partial/walk.hkx".to_string(),
            },
            BehaviorPatchEdit::SetClipAnimation {
                node: "missing".to_string(),
                animation: "missing.hkx".to_string(),
            },
        ],
    };

    let report = graph.apply_patches(&[patch]);

    assert_eq!(graph, before);
    assert!(report.applied.is_empty());
    assert!(matches!(
        report.diagnostics.as_slice(),
        [BehaviorPatchDiagnostic::Rejected { patch, reason }]
            if patch == "invalid" && reason == "unsupported: clip node \"missing\" does not exist"
    ));
}
