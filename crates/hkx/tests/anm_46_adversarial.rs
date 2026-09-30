// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

#[test]
fn disabled_rigid_body_modifier_never_emits_controls_on_repeated_updates() {
    let mut b=PackfileBuilder::new(); let root=b.object("hkRootLevelContainer",16); let graph=b.object("hkbBehaviorGraph",304);
    let v=b.array_structs(root,0,24,1); b.string(v,8,"hkbBehaviorGraph"); b.ptr(v,16,graph);
    let w=b.object("hkbModifierGenerator",88); let m=b.object("hkbRigidBodyRagdollControlsModifier",160); b.ptr(graph,128,w); b.ptr(w,72,m); b.set_bool(m,72,false);
    let g=BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap(); let clips=ClipLibrary::new(); let mut vm=Interpreter::new(&g,&clips,vec![QsTransform::IDENTITY;2]);
    assert!(vm.update(0.1).rigid_body_ragdoll_controls.is_empty()); assert!(vm.update(0.1).rigid_body_ragdoll_controls.is_empty());
}
