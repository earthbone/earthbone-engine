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
const FURNITURE: u32 = 0x322;
const FURN_BASE: u32 = 0x411;
const ACTOR_BASE: u32 = 0x412;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), CELL));
    plugin.push_top(Record::new(T(b"ACTI"), BASE));
    plugin.push_top(
        Record::new(T(b"FURN"), FURN_BASE)
            .sub(T(b"MNAM"), 1_u32.to_le_bytes())
            .sub(T(b"FNPR"), vec![1, 0, 0, 0]),
    );
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR_BASE));
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
    let mut furniture = Record::new(T(b"REFR"), FURNITURE).sub(T(b"NAME"), FURN_BASE.to_le_bytes());
    furniture.placement.cell = Some(CELL);
    plugin.push_top(furniture);
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

#[test]
fn scr_38_furniture_queries_report_hst_09_marker_reservations() {
    let mut game = game();
    let actor = game
        .world_mut()
        .place_at(id(ACTOR_BASE), id(FURNITURE), false);
    assert_eq!(
        game.world_mut()
            .reserve_furniture_marker(id(FURNITURE), actor),
        Some(0)
    );

    assert_eq!(
        call_with_args(
            &mut game,
            id(FURNITURE),
            "IsFurnitureInUse",
            &[Value::Bool(false)]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_args(
            &mut game,
            id(FURNITURE),
            "IsFurnitureInUse",
            &[Value::Bool(true)]
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call_with_args(
            &mut game,
            id(FURNITURE),
            "IsFurnitureMarkerInUse",
            &[Value::Int(0), Value::Bool(false)]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call_with_args(
            &mut game,
            id(FURNITURE),
            "IsFurnitureMarkerInUse",
            &[Value::Int(1), Value::Bool(false)]
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
fn scr_38_trigger_count_reads_actor_occupancy() {
    let mut game = game();
    let actor = game.world_mut().place_at(id(ACTOR_BASE), id(CELL), false);
    game.update_trigger_actor_position(actor, Some(id(CELL)), [20.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    assert_eq!(
        call(&mut game, id(TRIGGER), "GetTriggerObjectCount"),
        Value::Int(1)
    );
}

#[test]
fn scr_38_destruction_stage_reports_undamaged_baseline() {
    let mut game = game();
    assert_eq!(
        call(&mut game, id(ORDINARY), "GetCurrentDestructionStage"),
        Value::Int(-1)
    );
}
