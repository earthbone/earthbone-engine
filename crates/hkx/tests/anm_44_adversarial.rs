// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

#[test]
fn keyframe_entries_with_negative_or_out_of_pose_indices_are_ignored() {
    let mut b=PackfileBuilder::new(); let root=b.object("hkRootLevelContainer",16); let graph=b.object("hkbBehaviorGraph",304);
    let v=b.array_structs(root,0,24,1); b.string(v,8,"hkbBehaviorGraph"); b.ptr(v,16,graph);
    let w=b.object("hkbModifierGenerator",88); let m=b.object("hkbKeyframeBonesModifier",104); b.ptr(graph,128,w); b.ptr(w,72,m); b.set_bool(m,72,true);
    let entries=b.array_structs(m,80,48,2);
    for (n,bone) in [-1i16,9].into_iter().enumerate(){ let at=entries+n as u32*48; b.set_vec4(at,0,[99.0;4]); b.set_vec4(at,16,[0.0,0.0,0.0,1.0]); b.set_i16(at,32,bone); b.set_bool(at,34,true); }
    let g=BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap(); let pose=vec![QsTransform::IDENTITY;2];
    let clips=ClipLibrary::new(); let mut vm=Interpreter::new(&g,&clips,pose.clone()); assert_eq!(vm.update(0.0).pose,pose);
}
