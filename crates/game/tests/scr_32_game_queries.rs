// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::Game;
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};
use std::sync::atomic::{AtomicUsize, Ordering};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");
const BASE: u32 = 0x800;
const ACTOR_BASE: u32 = 0x801;
const REF_NEAR: u32 = 0x810;
const REF_FAR: u32 = 0x811;
const ACTOR: u32 = 0x812;
const ACTOR_FAR: u32 = 0x813;

fn float(value: f32) -> Value {
    Value::Float(value)
}

fn le(value: u32) -> [u8; 4] {
    value.to_le_bytes()
}

fn placed(tag: [u8; 4], id: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = Vec::new();
    for value in position {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[0; 12]);
    Record::new(T(&tag), id)
        .sub(T(b"NAME"), le(base))
        .sub(T(b"DATA"), data)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    plugin.push_top(Record::new(T(b"STAT"), BASE).zsub(EDID, "SearchBase"));
    plugin.push_top(
        Record::new(T(b"NPC_"), ACTOR_BASE)
            .zsub(EDID, "SearchActorBase")
            .sub(T(b"ACBS"), [0; 24]),
    );
    plugin.push_top(placed(*b"REFR", REF_NEAR, BASE, [3.0, 4.0, 0.0]));
    plugin.push_top(placed(*b"REFR", REF_FAR, BASE, [6.0, 0.0, 0.0]));
    plugin.push_top(placed(*b"ACHR", ACTOR, ACTOR_BASE, [1.0, 0.0, 0.0]));
    plugin.push_top(placed(*b"ACHR", ACTOR_FAR, ACTOR_BASE, [0.0, 2.0, 0.0]));
    let bytes = plugin.to_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    let mut order = LoadOrder::new();
    order.add("Search.esm", plugin).unwrap();
    Game::new(order)
}

static PROBES: AtomicUsize = AtomicUsize::new(0);

fn call(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let id = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("Scr32Probe{id}");
    let mut fun = FunctionBuilder::new("Run", "Var")
        .global()
        .local("r", "Var");
    let mut operands = vec![s("Game"), s(name), ident("r")];
    for (index, arg) in args.iter().enumerate() {
        let param = format!("a{index}");
        fun = fun.param(
            &param,
            match arg {
                Value::Int(_) => "Int",
                Value::Float(_) => "Float",
                Value::Object(_) => "Form",
                _ => "Var",
            },
        );
        operands.push(ident(&param));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            fun.ins(CallStatic, operands)
                .ins(Return, vec![ident("r")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn object(game: &mut Game, id: u32) -> Value {
    let form = game.world().from_runtime_id(id).unwrap();
    Value::Object(game.object_of_form(form))
}

fn returned_form(game: &Game, value: &Value) -> Option<u32> {
    value
        .as_object()
        .and_then(|object| game.world().form_of(object))
        .map(|id| game.world().runtime_id(id))
}

#[test]
fn scr_32_find_closest_actor_uses_position_and_radius() {
    let mut game = game();
    let result = call(
        &mut game,
        "FindClosestActor",
        &[float(0.0), float(0.0), float(0.0), float(1.0)],
    );
    assert_eq!(returned_form(&game, &result), Some(ACTOR));
    let outside = call(
        &mut game,
        "FindClosestActor",
        &[float(0.0), float(0.0), float(0.0), float(0.99)],
    );
    assert_eq!(outside, Value::None);
}

#[test]
fn scr_32_find_random_actor_returns_an_actor_in_radius() {
    let mut game = game();
    let mut found = std::collections::BTreeSet::new();
    for _ in 0..16 {
        let result = call(
            &mut game,
            "FindRandomActor",
            &[float(0.0), float(0.0), float(0.0), float(2.0)],
        );
        found.insert(returned_form(&game, &result).unwrap());
    }
    assert_eq!(found, [ACTOR, ACTOR_FAR].into_iter().collect());
    assert_eq!(
        call(
            &mut game,
            "FindRandomActor",
            &[float(0.0), float(0.0), float(0.0), float(-1.0)],
        ),
        Value::None
    );
}

#[test]
fn scr_32_find_closest_reference_matches_base_and_current_state() {
    let mut game = game();
    let base = object(&mut game, BASE);
    let result = call(
        &mut game,
        "FindClosestReferenceOfType",
        &[
            base.clone(),
            float(0.0),
            float(0.0),
            float(0.0),
            float(10.0),
        ],
    );
    assert_eq!(returned_form(&game, &result), Some(REF_NEAR));
    {
        let mut world = game.world_mut();
        let moved = world.from_runtime_id(REF_FAR).unwrap();
        world.reference(moved).unwrap().position = [0.5, 0.0, 0.0];
    }
    let moved = call(
        &mut game,
        "FindClosestReferenceOfType",
        &[base.clone(), float(0.0), float(0.0), float(0.0), float(1.0)],
    );
    assert_eq!(returned_form(&game, &moved), Some(REF_FAR));
    let disabled = game.world().from_runtime_id(REF_FAR).unwrap();
    game.world_mut().reference(disabled).unwrap().enabled = false;
    let after_disable = call(
        &mut game,
        "FindClosestReferenceOfType",
        &[base, float(0.0), float(0.0), float(0.0), float(1.0)],
    );
    assert_eq!(after_disable, Value::None);
}

#[test]
fn scr_32_find_random_reference_returns_matching_base_in_radius() {
    let mut game = game();
    let base = object(&mut game, BASE);
    let mut found = std::collections::BTreeSet::new();
    for _ in 0..16 {
        let result = call(
            &mut game,
            "FindRandomReferenceOfType",
            &[base.clone(), float(0.0), float(0.0), float(0.0), float(7.0)],
        );
        found.insert(returned_form(&game, &result).unwrap());
    }
    assert_eq!(found, [REF_NEAR, REF_FAR].into_iter().collect());
}

#[test]
fn scr_32_get_form_ex2_accepts_unsigned_runtime_ids() {
    let mut game = game();
    let reference = game.world().from_runtime_id(REF_NEAR).unwrap();
    let base = game.world().from_runtime_id(BASE).unwrap();
    let dynamic = game.world_mut().place_at(base, reference, false);
    let runtime_id = game.world().runtime_id(dynamic);
    assert!(runtime_id >= 0x8000_0000);
    let result = call(&mut game, "GetFormEx2", &[Value::Int(runtime_id as i32)]);
    assert_eq!(returned_form(&game, &result), Some(runtime_id));
    assert_eq!(
        call(&mut game, "GetFormEx2", &[Value::Int(i32::MAX)]),
        Value::None
    );
}
