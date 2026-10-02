// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode::{CallMethod, Return},
    Value,
};
fn id(n: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object: n }
}
fn game() -> Game {
    let mut d = vec![0; 48];
    d[28..32].copy_from_slice(&2u32.to_le_bytes());
    d[32..36].copy_from_slice(&3u32.to_le_bytes());
    d[40..44].copy_from_slice(&4096u32.to_le_bytes());
    d[44..48].copy_from_slice(&0.5f32.to_le_bytes());
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(Record::new(Tag(*b"SPGD"), 0x800).sub(Tag(*b"DATA"), d));
    let mut o = LoadOrder::new();
    o.add("P.esm", p).unwrap();
    Game::new(o)
}
fn call(g: &mut Game, name: &str, fade: f32) {
    let script = format!("Scr46Adv{name}");
    let bytes = ScriptBuilder::new(&script, "ShaderParticleGeometry")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("target", "ShaderParticleGeometry")
                .param("fade", "Float")
                .local("r", "None")
                .ins(
                    CallMethod,
                    vec![s(name), ident("target"), ident("r"), ident("fade")],
                )
                .ins(Return, vec![])
                .build(),
        )
        .bytes();
    g.load_script(&bytes).unwrap();
    let form = Value::Object(g.object_of_form(id(0x800)));
    let t =
        g.vm.call_global(&script, "Run", &[form, Value::Float(fade)])
            .unwrap();
    assert!(g.run_until_idle(32));
    assert!(g.vm.thread_result(t).is_some());
}
#[test]
fn scr_46_reapplying_same_form_restarts_one_layer_without_duplicates() {
    let mut g = game();
    call(&mut g, "Apply", 2.);
    g.tick(1.);
    assert_eq!(g.world().shader_particle_layers().len(), 1);
    call(&mut g, "Apply", 2.);
    assert_eq!(g.world().shader_particle_layers().len(), 1);
    assert_eq!(g.world().shader_particle_layers()[0].strength, 0.);
}
