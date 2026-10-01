// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn placed(tag: &[u8; 4], object: u32, base: u32, xyz: [f32; 3]) -> Record {
    let mut data = Vec::new();
    for v in xyz {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&[0; 12]);
    Record::new(Tag::new(tag), object)
        .sub(Tag::new(b"NAME"), base.to_le_bytes())
        .sub(Tag::new(b"DATA"), data)
}
fn fixture() -> Game {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(Record::new(Tag::new(b"STAT"), 0x800));
    p.push_top(Record::new(Tag::new(b"NPC_"), 0x801).sub(Tag::new(b"ACBS"), [0; 24]));
    p.push_top(placed(b"REFR", 0x810, 0x800, [1.0, 0.0, 0.0]));
    p.push_top(placed(b"ACHR", 0x811, 0x801, [1.0, 0.0, 0.0]));
    let mut o = LoadOrder::new();
    o.add("Search.esm", p).unwrap();
    Game::new(o)
}
fn call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr32Adv{name}{}", game.vm.time().to_bits());
    let mut f = FunctionBuilder::new("Run", "Var")
        .global()
        .local("r", "Var");
    let mut op = vec![s("Game"), s(name), ident("r")];
    for (i, v) in args.iter().enumerate() {
        let p = format!("a{i}");
        f = f.param(
            &p,
            if matches!(v, Value::Object(_)) {
                "Form"
            } else {
                "Var"
            },
        );
        op.push(ident(&p));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(f.ins(CallStatic, op).ins(Return, vec![ident("r")]).build())
        .bytes();
    game.load_script(&bytes).unwrap();
    let t = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(t).unwrap()
}
fn found(game: &Game, value: &Value) -> Option<u32> {
    value
        .as_object()
        .and_then(|o| game.world().form_of(o))
        .map(|form| game.world().runtime_id(form))
}
fn f(v: f32) -> Value {
    Value::Float(v)
}

#[test]
fn scr_32_queries_use_inclusive_3d_boundary_and_actor_filter() {
    let mut game = fixture();
    let closest = call(
        &mut game,
        "FindClosestActor",
        &[f(0.0), f(0.0), f(0.0), f(1.0)],
    );
    assert_eq!(found(&game, &closest), Some(0x811));
    let wrong = call(
        &mut game,
        "FindClosestReferenceOfType",
        &[Value::None, f(0.0), f(0.0), f(0.0), f(10.0)],
    );
    assert_eq!(wrong, Value::None);
    let outside = call(
        &mut game,
        "FindClosestActor",
        &[f(0.0), f(0.0), f(1.001), f(1.0)],
    );
    assert_eq!(outside, Value::None);
}

#[test]
fn scr_32_nonfinite_and_negative_search_inputs_return_none_without_panicking() {
    let mut game = fixture();
    for args in [
        [f(f32::NAN), f(0.0), f(0.0), f(5.0)],
        [f(0.0), f(0.0), f(0.0), f(f32::INFINITY)],
        [f(0.0), f(0.0), f(0.0), f(-0.1)],
    ] {
        assert_eq!(call(&mut game, "FindRandomActor", &args), Value::None);
    }
}

#[test]
fn scr_32_random_reference_requires_exact_base_and_includes_radius_edge() {
    let mut game = fixture();
    let base = Value::Object(game.object_of_form(id(0x800)));
    let result = call(
        &mut game,
        "FindRandomReferenceOfType",
        &[base, f(0.0), f(0.0), f(0.0), f(1.0)],
    );
    assert_eq!(found(&game, &result), Some(0x810));
    assert_eq!(
        call(
            &mut game,
            "FindRandomReferenceOfType",
            &[Value::None, f(0.0), f(0.0), f(0.0), f(10.0)]
        ),
        Value::None
    );
}

#[test]
fn scr_32_get_form_ex2_treats_signed_int_as_unsigned_for_both_signs() {
    let mut game = fixture();
    let player = game.world_mut().player();
    let dynamic = game.world_mut().place_at(id(0x800), player, false);
    let runtime = game.world().runtime_id(dynamic);
    assert!(runtime >= 0x8000_0000);
    let resolved = call(&mut game, "GetFormEx2", &[Value::Int(runtime as i32)]);
    assert_eq!(found(&game, &resolved), Some(runtime));
    assert_eq!(
        call(&mut game, "GetFormEx2", &[Value::Int(i32::MIN)]),
        Value::None
    );
}
