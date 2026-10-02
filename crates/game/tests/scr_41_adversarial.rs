// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-38 ObjectReference native calls backed by existing reference systems.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x210;
const TRIGGER: u32 = 0x320;
const ORDINARY: u32 = 0x321;
const BASE: u32 = 0x410;
const ACTOR_BASE: u32 = 0x420;
const SPELL: u32 = 0x430;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), CELL));
    plugin.push_top(Record::new(T(b"ACTI"), BASE));
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR_BASE));
    let mut spell = vec![0; 36];
    spell[16..20].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"SPEL"), SPELL).sub(T(b"SPIT"), spell));
    let mut trigger = Record::new(T(b"REFR"), TRIGGER)
        .sub(T(b"NAME"), BASE.to_le_bytes())
        .sub(
            T(b"DATA"),
            soe_esm::records::Transform {
                position: [0.0; 3],
                rotation: [0.0; 3],
            }
            .to_bytes(),
        )
        .sub(T(b"XPRM"), trigger_primitive());
    trigger.placement.cell = Some(CELL);
    plugin.push_top(trigger);
    let mut ordinary = Record::new(T(b"REFR"), ORDINARY).sub(T(b"NAME"), BASE.to_le_bytes());
    ordinary.placement.cell = Some(CELL);
    plugin.push_top(ordinary);
    plugin
}

fn trigger_primitive() -> Vec<u8> {
    let mut data = Vec::new();
    for value in [10.0_f32, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&1_u32.to_le_bytes());
    data
}

fn game() -> Game {
    let mut order = LoadOrder::new();
    order.add("SCR38.esm", plugin()).unwrap();
    Game::new(order)
}

static PROBES: AtomicUsize = AtomicUsize::new(0);

fn call(game: &mut Game, target: GlobalFormId, method: &str) -> Value {
    call_with_args(game, target, method, &[])
}

fn call_with_args(game: &mut Game, target: GlobalFormId, method: &str, args: &[Value]) -> Value {
    let script = format!("Scr38Probe{}", PROBES.fetch_add(1, Ordering::SeqCst));
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
            _ => panic!("unsupported SCR-38 probe argument"),
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
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn scr_38_furniture_queries_accept_ck_arguments_for_reference_without_markers() {
    let mut game = game();
    assert_eq!(
        call_with_args(
            &mut game,
            id(ORDINARY),
            "IsFurnitureInUse",
            &[Value::Bool(false)]
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_with_args(
            &mut game,
            id(ORDINARY),
            "IsFurnitureInUse",
            &[Value::Bool(true)]
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_with_args(
            &mut game,
            id(ORDINARY),
            "IsFurnitureMarkerInUse",
            &[Value::Int(0), Value::Bool(false)]
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_with_args(
            &mut game,
            id(ORDINARY),
            "IsFurnitureMarkerInUse",
            &[Value::Int(0), Value::Bool(true)]
        ),
        Value::Bool(false)
    );
}

fn resident_game() -> Game {
    let game = game();
    game.world_mut()
        .attach_streaming_cells(&[id(CELL)])
        .unwrap();
    game
}

#[test]
fn scr_38_is_3d_loaded_reads_streamed_reference_residency() {
    let mut game = resident_game();
    assert_eq!(
        call(&mut game, id(ORDINARY), "Is3DLoaded"),
        Value::Bool(true)
    );
    game.world_mut().reference(id(ORDINARY)).unwrap().enabled = false;
    assert_eq!(
        call(&mut game, id(ORDINARY), "Is3DLoaded"),
        Value::Bool(false)
    );
}

#[test]
fn scr_38_trigger_count_returns_occupancy_and_diagnoses_non_triggers() {
    let mut game = game();
    assert_eq!(
        call(&mut game, id(TRIGGER), "GetTriggerObjectCount"),
        Value::Int(0)
    );
    assert_eq!(
        call(&mut game, id(ORDINARY), "GetTriggerObjectCount"),
        Value::Int(0)
    );
    assert!(game
        .traces()
        .iter()
        .any(|line| line.contains("GetTriggerObjectCount called on a non-trigger")));
}

#[test]
fn scr_41_reference_probe_has_undamaged_stage_baseline() {
    let mut game = game();
    assert_eq!(
        call(&mut game, id(ORDINARY), "GetCurrentDestructionStage"),
        Value::Int(-1)
    );
}

#[test]
fn scr_41_tether_places_horse_at_cart_relative_position() {
    let mut game = game();
    game.world_mut().reference(id(TRIGGER)).unwrap().position = [10.0, 20.0, 30.0];
    let horse = Value::Object(game.object_of_form(id(ORDINARY)));
    assert_eq!(
        call_with_args(&mut game, id(TRIGGER), "TetherToHorse", &[horse]),
        Value::None
    );
    let horse = game.world_mut().reference(id(ORDINARY)).unwrap().position;
    assert_eq!(horse, [82.0, 320.0, 30.0]);
    assert!(matches!(
        game.world_mut().take_reference_physics_requests().as_slice(),
        [soe_game::world::ReferencePhysicsRequest::TetherToHorse { cart, horse }]
            if *cart == id(TRIGGER) && *horse == id(ORDINARY)
    ));
}

#[test]
fn scr_41_tether_rotates_authored_offset_with_cart_yaw() {
    let mut game = game();
    {
        let mut world = game.world_mut();
        let cart = world.reference(id(TRIGGER)).unwrap();
        cart.position = [10.0, 20.0, 30.0];
        cart.rotation[2] = std::f32::consts::FRAC_PI_2;
    }
    let horse_arg = Value::Object(game.object_of_form(id(ORDINARY)));
    assert_eq!(
        call_with_args(&mut game, id(TRIGGER), "TetherToHorse", &[horse_arg]),
        Value::None
    );
    let mut world = game.world_mut();
    let horse = world.reference(id(ORDINARY)).unwrap();
    assert!((horse.position[0] + 290.0).abs() < 0.001);
    assert!((horse.position[1] - 92.0).abs() < 0.001);
    assert_eq!(horse.rotation[2], std::f32::consts::FRAC_PI_2);
}

#[test]
fn scr_41_tether_ignores_a_non_reference_argument_without_mutation() {
    let mut game = game();
    let before = game.world_mut().reference(id(ORDINARY)).unwrap().clone();
    assert_eq!(
        call_with_args(&mut game, id(TRIGGER), "TetherToHorse", &[Value::None]),
        Value::None
    );
    assert_eq!(game.world_mut().reference(id(ORDINARY)).unwrap(), &before);
    assert!(game.world().reference_physics_requests.is_empty());
}

#[test]
fn scr_41_interrupt_cast_clears_pending_cast_without_applying_effects() {
    let mut game = game();
    game.world_mut()
        .begin_spell_cast(id(TRIGGER), id(SPELL), None)
        .unwrap();
    assert!(game.world().pending_spell_cast(id(TRIGGER)).is_some());

    assert_eq!(call(&mut game, id(TRIGGER), "InterruptCast"), Value::None);

    assert!(game.world().pending_spell_cast(id(TRIGGER)).is_none());
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn scr_41_ragdoll_methods_set_and_clear_actor_override() {
    let mut game = game();
    let actor = game
        .world_mut()
        .place_at(id(ACTOR_BASE), id(TRIGGER), false);

    assert_eq!(
        call(&mut game, actor, "ForceAddRagdollToWorld"),
        Value::None
    );
    assert_eq!(
        game.world_mut().actor(actor).unwrap().ragdoll_forced,
        Some(true)
    );
    assert_eq!(
        call(&mut game, actor, "ForceRemoveRagdollFromWorld"),
        Value::None
    );
    assert_eq!(
        game.world_mut().actor(actor).unwrap().ragdoll_forced,
        Some(false)
    );
}
