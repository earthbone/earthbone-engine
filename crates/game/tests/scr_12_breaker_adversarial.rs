// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial SCR-12 checks grounded in the local CK Papyrus pages.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{plugin::flags, Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR_BASE: u32 = 0x700;
const TARGET_BASE: u32 = 0x701;
const CELL: u32 = 0x702;
const ACTOR: u32 = 0x703;
const TARGET: u32 = 0x704;
const FILTER: u32 = 0x705;
static PROBES: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader { flags: flags::MASTER, ..Default::default() });
    plugin.push_top(npc::build_record(ACTOR_BASE, "ActorBase", "Actor", npc::Config::default(), npc::Stats::default()));
    plugin.push_top(npc::build_record(TARGET_BASE, "TargetBase", "Target", npc::Config::default(), npc::Stats::default()));
    plugin.push_top(Record::new(T(b"FLST"), FILTER).zsub(T(b"EDID"), "Filter"));
    let cell = plugin.push_record(Record::new(T(b"CELL"), CELL).zsub(T(b"EDID"), "Cell").sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    for (form, edid, base) in [(ACTOR, "ActorRef", ACTOR_BASE), (TARGET, "TargetRef", TARGET_BASE)] {
        let index = plugin.push_record(
            Record::new(T(b"ACHR"), form)
                .zsub(T(b"EDID"), edid)
                .sub(T(b"NAME"), base.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
        temporary.entries.push(Entry::Record(index));
    }
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut subblock = Group::new(GroupKind::InteriorSubBlock(0));
    subblock.entries.push(Entry::Record(cell));
    subblock.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(subblock));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);

    let mut order = LoadOrder::new();
    order.add("SCR12Breaker.esm", Plugin::parse(&plugin.to_bytes()).unwrap()).unwrap();
    Game::new(order)
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Str(_) => "String",
        _ => "Form",
    }
}

fn call(game: &mut Game, target: u32, method: &str, args: &[Value]) -> Value {
    let index = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("Scr12BreakerProbe{index}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Actor")
        .local("result", "Var");
    let mut operands = vec![s(method), ident("target"), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let name = format!("arg{index}");
        function = function.param(&name, type_of(value));
        operands.push(ident(&name));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(function.ins(CallMethod, operands).ins(Return, vec![ident("result")]).build())
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(id(target)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn clear_look_at_is_idempotent_and_target_link_survives_save_load() {
    let mut current = game();
    let actor = id(ACTOR);
    let target = id(TARGET);
    current.world_mut().set_actor_look_at(actor, target).unwrap();
    assert_eq!(call(&mut current, ACTOR, "ClearLookAt", &[]), Value::None);
    assert_eq!(current.world_mut().actor_look_at(actor), None);
    assert_eq!(call(&mut current, ACTOR, "ClearLookAt", &[]), Value::None);

    current.world_mut().set_actor_look_at(actor, target).unwrap();
    let saved = current.world_mut().save_state();
    let loaded = game();
    loaded.world_mut().load_state(&saved).unwrap();
    assert_eq!(loaded.world_mut().actor_look_at(id(ACTOR)), Some(target));
}

#[test]
fn expression_native_uses_ck_default_and_rejects_invalid_strength_without_mutation() {
    let mut game = game();
    assert_eq!(call(&mut game, ACTOR, "SetExpressionOverride", &[Value::Int(12)]), Value::None);
    assert_eq!(game.world_mut().facial_expression(id(ACTOR)), Some((12, 100)));

    let invalid = call(&mut game, ACTOR, "SetExpressionOverride", &[Value::Int(12), Value::Int(101)]);
    assert_eq!(invalid, Value::None);
    assert_eq!(game.world_mut().facial_expression(id(ACTOR)), Some((12, 100)));
    assert!(game.traces().iter().any(|trace| trace.contains("expression strength 101")));
}

#[test]
fn expression_clear_is_repeatable_and_restores_actor_mood_expression() {
    let mut game = game();
    game.world_mut().set_actor_value(id(ACTOR), "mood", 3.0).unwrap();
    assert_eq!(call(&mut game, ACTOR, "SetExpressionOverride", &[Value::Int(2), Value::Int(0)]), Value::None);
    assert_eq!(game.world_mut().facial_expression(id(ACTOR)), Some((2, 0)));
    assert_eq!(call(&mut game, ACTOR, "ClearExpressionOverride", &[]), Value::None);
    assert_eq!(game.world_mut().facial_expression(id(ACTOR)), Some((10, 100)));
    assert_eq!(call(&mut game, ACTOR, "ClearExpressionOverride", &[]), Value::None);
    assert_eq!(game.world_mut().facial_expression(id(ACTOR)), Some((10, 100)));
}

#[test]
fn gift_menu_applies_ck_defaults_and_rejects_a_second_pending_request() {
    let mut game = game();
    assert_eq!(call(&mut game, ACTOR, "ShowGiftMenu", &[Value::Bool(false)]), Value::None);
    let request = game.gift_menu_request().unwrap();
    assert_eq!(request.giver, id(ACTOR));
    assert_eq!(request.source, id(ACTOR));
    assert_eq!(request.target, game.world_mut().player());
    assert_eq!(request.filter, None);
    assert!(!request.show_stolen);
    assert!(request.use_favor_points);

    assert_eq!(call(&mut game, ACTOR, "ShowGiftMenu", &[Value::Bool(true), Value::None, Value::Bool(true), Value::Bool(false)]), Value::Int(0));
    let still_pending = game.gift_menu_request().unwrap();
    assert_eq!(still_pending.latent, request.latent);
    assert_eq!(still_pending.source, id(ACTOR));
    assert!(game.close_gift_menu());
    assert!(!game.close_gift_menu());
}
