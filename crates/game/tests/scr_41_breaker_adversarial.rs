// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-41 adversarial checks for ObjectReference native bindings.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x210;
const CART: u32 = 0x320;
const HORSE: u32 = 0x321;
const BASE: u32 = 0x410;
const ACTOR_BASE: u32 = 0x420;
const SPELL: u32 = 0x430;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), CELL));
    plugin.push_top(Record::new(T(b"ACTI"), BASE));
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR_BASE));
    let mut spell = vec![0; 36];
    spell[16..20].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"SPEL"), SPELL).sub(T(b"SPIT"), spell));
    for form in [CART, HORSE] {
        let mut reference = Record::new(T(b"REFR"), form).sub(T(b"NAME"), BASE.to_le_bytes());
        reference.placement.cell = Some(CELL);
        plugin.push_top(reference);
    }
    let mut order = LoadOrder::new();
    order.add("Scr41Breaker.esm", plugin).unwrap();
    Game::new(order)
}

static PROBES: AtomicUsize = AtomicUsize::new(0);

fn call(game: &mut Game, target: GlobalFormId, method: &str, args: &[Value]) -> Value {
    let script = format!("Scr41Breaker{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "ObjectReference")
        .local("result", "Var");
    let mut operands = vec![s(method), ident("target"), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let name = format!("arg{index}");
        let value_type = match value {
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::None | Value::Object(_) => "ObjectReference",
            _ => panic!("unsupported probe argument"),
        };
        function = function.param(&name, value_type);
        operands.push(ident(&name));
    }
    let function = function
        .ins(CallMethod, operands)
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let target = Value::Object(game.object_of_form(target));
    let mut call_args = vec![target];
    call_args.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &call_args).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap()
}

fn wait_thread(game: &mut Game, target: GlobalFormId, event: &str) -> soe_papyrus::vm::ThreadId {
    let script = format!("Scr41Wait{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let function = FunctionBuilder::new("Run", "Bool")
        .global()
        .param("target", "ObjectReference")
        .param("eventName", "String")
        .local("result", "Bool")
        .ins(
            CallMethod,
            vec![
                s("WaitForAnimationEvent"),
                ident("target"),
                ident("result"),
                ident("eventName"),
            ],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let target = Value::Object(game.object_of_form(target));
    let thread = game
        .vm
        .call_global(&script, "Run", &[target, Value::string(event)])
        .unwrap();
    game.run_until_idle(64);
    thread
}

#[test]
fn scr_41_wait_uses_called_reference_and_resolves_a_matching_event_only_once() {
    let mut game = game();
    let cart = wait_thread(&mut game, id(CART), "done");
    let horse = wait_thread(&mut game, id(HORSE), "done");
    game.report_animation_event(id(HORSE), "done");
    assert_eq!(game.vm.thread_result(cart), None);
    assert_eq!(game.vm.thread_result(horse), Some(Value::Bool(true)));
    game.report_animation_event(id(CART), "other");
    assert_eq!(game.vm.thread_result(cart), None);
    game.report_animation_event(id(CART), "DONE");
    assert_eq!(game.vm.thread_result(cart), Some(Value::Bool(true)));
    game.report_animation_event(id(CART), "done");
    assert_eq!(game.vm.thread_result(cart), Some(Value::Bool(true)));
}

#[test]
fn scr_41_wait_returns_false_if_graph_unloads_or_reference_is_disabled() {
    let mut game = game();
    let unloaded = wait_thread(&mut game, id(CART), "finished");
    game.report_animation_unloaded(id(CART));
    assert_eq!(game.vm.thread_result(unloaded), Some(Value::Bool(false)));
    let disabled = wait_thread(&mut game, id(HORSE), "finished");
    call(&mut game, id(HORSE), "DisableNoWait", &[]);
    assert_eq!(game.vm.thread_result(disabled), Some(Value::Bool(false)));
}

#[test]
fn scr_41_interrupt_cast_only_cancels_the_called_reference_and_repeats_safely() {
    let mut game = game();
    game.world_mut()
        .begin_spell_cast(id(CART), id(SPELL), None)
        .unwrap();
    game.world_mut()
        .begin_spell_cast(id(HORSE), id(SPELL), None)
        .unwrap();
    assert_eq!(call(&mut game, id(CART), "InterruptCast", &[]), Value::None);
    assert_eq!(call(&mut game, id(CART), "InterruptCast", &[]), Value::None);
    assert!(game.world().pending_spell_cast(id(CART)).is_none());
    assert!(game.world().pending_spell_cast(id(HORSE)).is_some());
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn scr_41_ragdoll_native_ignores_non_actor_and_keeps_unconscious_state() {
    let mut game = game();
    assert_eq!(
        call(&mut game, id(CART), "ForceAddRagdollToWorld", &[]),
        Value::None
    );
    assert_eq!(
        call(&mut game, id(CART), "ForceRemoveRagdollFromWorld", &[]),
        Value::None
    );
    let actor = game.world_mut().place_at(id(ACTOR_BASE), id(CART), false);
    game.world_mut().actor(actor).unwrap().unconscious = true;
    call(&mut game, actor, "ForceAddRagdollToWorld", &[]);
    call(&mut game, actor, "ForceRemoveRagdollFromWorld", &[]);
    call(&mut game, actor, "ForceRemoveRagdollFromWorld", &[]);
    let mut world = game.world_mut();
    let state = world.actor(actor).unwrap();
    assert_eq!(state.ragdoll_forced, Some(false));
    assert!(state.unconscious);
}

#[test]
fn scr_41_ragdoll_override_survives_world_save_and_load() {
    let mut instance = game();
    let actor = instance
        .world_mut()
        .place_at(id(ACTOR_BASE), id(CART), false);
    call(&mut instance, actor, "ForceAddRagdollToWorld", &[]);
    let saved = instance.world().save_state();
    let restored = game();
    restored.world_mut().load_state(&saved).unwrap();
    assert_eq!(
        restored.world_mut().actor(actor).unwrap().ragdoll_forced,
        Some(true)
    );
}

#[test]
fn scr_41_tether_missing_reference_is_atomic_and_valid_tether_copies_cell() {
    let mut game = game();
    let horse_before = game.world_mut().reference(id(HORSE)).unwrap().clone();
    let spell_object = Value::Object(game.object_of_form(id(SPELL)));
    assert_eq!(
        call(&mut game, id(CART), "TetherToHorse", &[spell_object],),
        Value::None
    );
    assert_eq!(
        game.world_mut().reference(id(HORSE)).unwrap(),
        &horse_before
    );
    assert!(game.world().reference_physics_requests.is_empty());

    game.world_mut().reference(id(CART)).unwrap().cell = None;
    let horse = Value::Object(game.object_of_form(id(HORSE)));
    assert_eq!(
        call(&mut game, id(CART), "TetherToHorse", &[horse]),
        Value::None
    );
    assert_eq!(game.world_mut().reference(id(HORSE)).unwrap().cell, None);
}
