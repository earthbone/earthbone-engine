// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

#[test]
fn twist_with_reversed_endpoints_stays_inside_inclusive_declared_span() {
    let mut b=PackfileBuilder::new(); let root=b.object("hkRootLevelContainer",16); let graph=b.object("hkbBehaviorGraph",304);
    let v=b.array_structs(root,0,24,1); b.string(v,8,"hkbBehaviorGraph"); b.ptr(v,16,graph);
    let w=b.object("hkbModifierGenerator",88); let m=b.object("hkbTwistModifier",144); b.ptr(graph,128,w); b.ptr(w,72,m); b.set_bool(m,72,true);
    b.set_vec4(m,80,[0.0,0.0,1.0,0.0]); b.set_f32(m,96,1.2); b.set_i16(m,100,3); b.set_i16(m,102,1); b.set_i8(m,104,0); b.set_i8(m,105,0); b.set_bool(m,106,true);
    let g=BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap(); assert!(g.nodes.iter().any(|n|matches!(n.kind,NodeKind::TwistModifier(_))));
    let clips=ClipLibrary::new(); let mut vm=Interpreter::new(&g,&clips,vec![QsTransform::IDENTITY;5]); let out=vm.update(0.0).pose.clone();
    assert_eq!(out[0].rotation,QsTransform::IDENTITY.rotation); assert_eq!(out[4].rotation,QsTransform::IDENTITY.rotation);
    assert_ne!(out[1].rotation,QsTransform::IDENTITY.rotation); assert_ne!(out[2].rotation,QsTransform::IDENTITY.rotation); assert_ne!(out[3].rotation,QsTransform::IDENTITY.rotation);
}
