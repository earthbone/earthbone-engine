// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-42 ObjectReference native contracts available without HST-16.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode::{CallMethod, Return},
    Value,
};

const CELL: u32 = 1;
const SOURCE: u32 = 2;
const TARGET: u32 = 3;
const OWNER_BASE: u32 = 4;
const OWNER: u32 = 5;
const BASE: u32 = 6;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL));
    plugin.push_top(Record::new(Tag(*b"STAT"), BASE));
    plugin.push_top(Record::new(Tag(*b"NPC_"), OWNER_BASE));
    let mut source = Record::new(Tag(*b"REFR"), SOURCE)
        .sub(b"NAME", BASE.to_le_bytes())
        .sub(b"DATA", transform([10.0, 20.0, 30.0]))
        .sub(
            b"VMAD",
            Vmad {
                scripts: vec![ScriptEntry::new("TranslationListener")],
                ..Vmad::default()
            }
            .write(Tag(*b"REFR")),
        );
    source.placement.cell = Some(CELL);
    plugin.push_top(source);
    let mut target = Record::new(Tag(*b"REFR"), TARGET)
        .sub(b"NAME", BASE.to_le_bytes())
        .sub(b"DATA", transform([100.0, 200.0, 300.0]));
    target.placement.cell = Some(CELL);
    plugin.push_top(target);
    let mut owner = Record::new(Tag(*b"ACHR"), OWNER).sub(b"NAME", OWNER_BASE.to_le_bytes());
    owner.placement.cell = Some(CELL);
    plugin.push_top(owner);
    let mut order = LoadOrder::new();
    order.add("Scr42.esm", plugin).unwrap();
    order
}

fn transform(position: [f32; 3]) -> Vec<u8> {
    position
        .into_iter()
        .chain([0.0_f32; 3])
        .flat_map(f32::to_le_bytes)
        .collect()
}

static PROBES: AtomicUsize = AtomicUsize::new(0);

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Bool(_) => "Bool",
        Value::Str(_) => "String",
        _ => "Form",
    }
}

fn call(game: &mut Game, reference: u32, name: &str, args: &[Value]) {
    let script_name = format!("Scr42Probe{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("target", "Form")
        .local("result", "None");
    let mut operands = vec![s(name), ident("target"), ident("result")];
    let mut values = vec![Value::Object(game.object_of_form(id(reference)))];
    for (index, arg) in args.iter().enumerate() {
        let parameter = format!("a{index}");
        function = function.param(&parameter, value_type(arg));
        operands.push(ident(&parameter));
        values.push(arg.clone());
    }
    let function = function
        .ins(CallMethod, operands)
        .ins(Return, vec![])
        .build();
    let bytes = ScriptBuilder::new(&script_name, "Form")
        .function(function)
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    game.run_until_idle(64);
    assert!(game.vm.thread_result(thread).is_some());
}

#[test]
fn scr_42_create_detection_event_uses_source_location_and_owner_argument() {
    let mut game = Game::new(fixture());
    let owner = Value::Object(game.object_of_form(id(OWNER)));
    call(
        &mut game,
        SOURCE,
        "CreateDetectionEvent",
        &[owner, Value::Int(125)],
    );

    let events = game.world_mut().take_detection_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].owner, id(OWNER));
    assert_eq!(events[0].cell, id(CELL));
    assert_eq!(events[0].position, [10.0, 20.0, 30.0]);
    assert_eq!(events[0].sound_level, 125.0);
}

#[test]
fn scr_42_create_detection_event_rejects_invalid_owner_and_negative_sound() {
    let mut game = Game::new(fixture());
    let not_actor = Value::Object(game.object_of_form(id(TARGET)));
    call(
        &mut game,
        SOURCE,
        "CreateDetectionEvent",
        &[not_actor, Value::Int(5)],
    );
    let owner = Value::Object(game.object_of_form(id(OWNER)));
    call(
        &mut game,
        SOURCE,
        "CreateDetectionEvent",
        &[owner, Value::Int(-1)],
    );

    assert!(game.world_mut().take_detection_events().is_empty());
}

#[test]
fn scr_42_reset_native_restores_runtime_reference_state() {
    let mut game = Game::new(fixture());
    game.world_mut().reference(id(SOURCE)).unwrap().opened = true;
    game.world_mut().reference(id(SOURCE)).unwrap().position = [1.0, 2.0, 3.0];
    call(&mut game, SOURCE, "Reset", &[]);

    let world = game.world();
    let reset = world.reference_ref(id(SOURCE)).unwrap();
    assert!(!reset.opened);
    assert_eq!(reset.position, [10.0, 20.0, 30.0]);
}

#[test]
fn scr_42_reset_native_honors_optional_target() {
    let mut game = Game::new(fixture());
    game.world_mut()
        .update_streaming_cells(id(CELL), &[id(CELL)], &[])
        .unwrap();
    let target = Value::Object(game.object_of_form(id(TARGET)));
    call(&mut game, SOURCE, "Reset", &[target]);

    assert_eq!(
        game.world().reference_ref(id(SOURCE)).unwrap().position,
        [100.0, 200.0, 300.0]
    );
}

#[test]
fn scr_42_translation_natives_move_and_stop_with_failure_event() {
    let mut game = Game::new(fixture());
    let listener = ScriptBuilder::new("TranslationListener", "ObjectReference")
        .function(
            FunctionBuilder::new("OnTranslationFailed", "None")
                .ins(
                    soe_papyrus::Opcode::CallStatic,
                    vec![
                        s("Debug"),
                        s("Trace"),
                        ident("::nonevar"),
                        s("translation failed"),
                    ],
                )
                .build(),
        )
        .bytes();
    game.load_script(&listener).unwrap();
    game.start();
    game.load_cell(id(CELL));
    let linear = [
        Value::Float(110.0),
        Value::Float(20.0),
        Value::Float(30.0),
        Value::Float(0.0),
        Value::Float(0.0),
        Value::Float(0.0),
        Value::Float(10.0),
    ];
    call(&mut game, SOURCE, "TranslateTo", &linear);
    game.world_mut()
        .tick_translations(2.0, &mut |_, _| soe_papyrus::ObjectId(1));
    assert_eq!(
        game.world().reference_ref(id(SOURCE)).unwrap().position,
        [30.0, 20.0, 30.0]
    );
    call(&mut game, SOURCE, "StopTranslation", &[]);

    assert_eq!(
        game.world().reference_ref(id(SOURCE)).unwrap().position,
        [30.0, 20.0, 30.0]
    );
    assert!(game.traces().contains(&"translation failed".to_string()));
}
