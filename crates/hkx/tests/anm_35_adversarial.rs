// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter, RagdollContactKind};
use soe_hkx::ragdoll::BodyIndex;
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn contact_graph() -> BehaviorGraph {
    let mut b = PackfileBuilder::new();
    let root = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(root, 0, 24, 1);
    b.string(variant, 0, "hkbBehaviorGraph");
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    let wrapper = b.object("hkbModifierGenerator", 88);
    let listener = b.object("BSRagdollContactListenerModifier", 136);
    let bones = b.object("hkbBoneIndexArray", 64);
    b.ptr(graph, 128, wrapper);
    b.ptr(wrapper, 72, listener);
    b.set_bool(listener, 72, true);
    b.set_i32(listener, 88, 17);
    b.ptr(listener, 104, bones);
    b.array_i16(bones, 48, &[1, 4]);
    BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap()
}

#[test]
fn anm_35_contact_from_any_mapped_declared_bone_emits_once() {
    let graph = contact_graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_ragdoll_contact(BodyIndex(9), &[None, Some(BodyIndex(9))], RagdollContactKind::Contact);
    vm.update(0.0);
    assert_eq!(vm.take_events().iter().map(|event| event.id).collect::<Vec<_>>(), [17]);
}

#[test]
fn anm_35_unmapped_or_undeclared_bodies_emit_nothing() {
    let graph = contact_graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_ragdoll_contact(BodyIndex(8), &[None, Some(BodyIndex(9))], RagdollContactKind::Contact);
    vm.send_ragdoll_contact(BodyIndex(9), &[None, Some(BodyIndex(8))], RagdollContactKind::Throw);
    vm.update(0.0);
    assert!(vm.take_events().is_empty());
}

#[test]
fn anm_35_throw_from_mapped_declared_bone_emits_contact_event() {
    let graph = contact_graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_ragdoll_contact(BodyIndex(12), &[None, None, None, None, Some(BodyIndex(12))], RagdollContactKind::Throw);
    vm.update(0.0);
    assert_eq!(vm.take_events().iter().map(|event| event.id).collect::<Vec<_>>(), [17]);
}
