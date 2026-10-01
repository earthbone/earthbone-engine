// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::Pending, Game};
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    ObjectId,
    Opcode::{CallMethod, Return},
    Value,
};
use std::sync::atomic::{AtomicUsize, Ordering};
static N: AtomicUsize = AtomicUsize::new(0);
fn id(n: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object: n }
}
fn game() -> Game {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(Record::new(Tag(*b"STAT"), 0x800));
    let mut r = Record::new(Tag(*b"REFR"), 0x900)
        .sub(Tag(*b"NAME"), 0x800u32.to_le_bytes())
        .sub(Tag(*b"DATA"), vec![0; 24]);
    r.placement.cell = Some(0x700);
    p.push_top(r);
    let mut o = LoadOrder::new();
    o.add("T.esm", p).unwrap();
    Game::new(o)
}
fn call(g: &mut Game, name: &str, args: &[f32]) {
    let script = format!("Scr42Adv{}", N.fetch_add(1, Ordering::Relaxed));
    let mut fun = FunctionBuilder::new("Run", "None")
        .global()
        .param("target", "ObjectReference")
        .local("r", "None");
    let mut ops = vec![s(name), ident("target"), ident("r")];
    for (n, _v) in args.iter().enumerate() {
        let a = format!("a{n}");
        fun = fun.param(&a, "Float");
        ops.push(ident(&a));
    }
    let bytes = ScriptBuilder::new(&script, "")
        .function(fun.ins(CallMethod, ops).ins(Return, vec![]).build())
        .bytes();
    g.load_script(&bytes).unwrap();
    let mut vs = vec![Value::Object(g.object_of_form(id(0x900)))];
    vs.extend(args.iter().copied().map(Value::Float));
    let t = g.vm.call_global(&script, "Run", &vs).unwrap();
    assert!(g.run_until_idle(32));
    assert!(g.vm.thread_result(t).is_some());
}
#[test]
fn scr_42_translation_progress_and_stop_report_failure() {
    let mut g = game();
    call(&mut g, "TranslateTo", &[10., 0., 0., 0., 0., 90., 10.]);
    g.world_mut()
        .tick_translations(0.5, &mut |_, _| ObjectId(1));
    let world = g.world();
    let r = world.reference_ref(id(0x900)).unwrap();
    assert_eq!(r.position, [5., 0., 0.]);
    assert!((r.rotation[2].to_degrees() - 45.).abs() < 0.01);
    drop(world);
    call(&mut g, "StopTranslation", &[]);
    let before = g.world().reference_ref(id(0x900)).unwrap().position;
    g.world_mut().tick_translations(2., &mut |_, _| ObjectId(1));
    assert_eq!(g.world().reference_ref(id(0x900)).unwrap().position, before);
    assert!(!g
        .world()
        .pending
        .iter()
        .any(|e| matches!(e,Pending::Event{name,..} if name=="OnTranslationComplete")));
}
#[test]
fn scr_42_spline_translation_snaps_to_absolute_endpoint() {
    let mut g = game();
    call(
        &mut g,
        "SplineTranslateTo",
        &[10., 0., 0., 0., 0., 0., 3., 10.],
    );
    g.world_mut().tick_translations(5., &mut |_, _| ObjectId(1));
    assert_eq!(
        g.world().reference_ref(id(0x900)).unwrap().position,
        [10., 0., 0.]
    );
    assert!(g
        .world()
        .pending
        .iter()
        .any(|e| matches!(e,Pending::Event{name,..} if name=="OnTranslationComplete")));
}
