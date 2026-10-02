// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::{
    Opcode::{CallMethod, Return},
    Value,
    asm::{FunctionBuilder, ScriptBuilder, ident, s},
};

const LOCATION: u32 = 10;
const REFTYPE: u32 = 20;
const ALIVE: u32 = 30;
const DEAD: u32 = 31;
const ACTOR_BASE: u32 = 32;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn entry(reference: u32) -> Vec<u8> {
    [
        REFTYPE.to_le_bytes(),
        reference.to_le_bytes(),
        40u32.to_le_bytes(),
        [0; 4],
    ]
    .concat()
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"LCTN"), LOCATION)
            .sub(Tag::new(b"ACSR"), entry(ALIVE))
            .sub(Tag::new(b"LCSR"), entry(DEAD)),
    );
    plugin.push_top(Record::new(Tag::new(b"LCRT"), REFTYPE));
    plugin.push_top(
        Record::new(Tag::new(b"CELL"), 40).sub(Tag::new(b"XLCN"), LOCATION.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag::new(b"NPC_"), ACTOR_BASE));
    for actor in [ALIVE, DEAD] {
        plugin.push_top(
            Record::new(Tag::new(b"ACHR"), actor)
                .sub(Tag::new(b"NAME"), ACTOR_BASE.to_le_bytes())
                .sub(Tag::new(b"DATA"), [0; 24]),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Scr35.esm", plugin).unwrap();
    Game::new(order)
}

static PROBE_ID: AtomicUsize = AtomicUsize::new(0);

fn call(game: &mut Game, target: u32, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr35Probe{}", PROBE_ID.fetch_add(1, Ordering::SeqCst));
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Location")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("target"), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let argument = format!("arg{index}");
        function = function.param(
            &argument,
            if matches!(value, Value::Int(_)) {
                "Int"
            } else {
                "Form"
            },
        );
        operands.push(ident(&argument));
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
    let mut values = vec![Value::Object(game.object_of_form(id(target)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn ref_type(game: &mut Game) -> Value {
    Value::Object(game.object_of_form(id(REFTYPE)))
}

#[test]
fn scr_35_location_native_bindings_dispatch_and_observe_record_and_actor_state() {
    let mut game = game();
    assert_eq!(
        call(&mut game, LOCATION, "IsLoaded", &[]),
        Value::Bool(false)
    );
    game.world_mut().load_cell(id(40));
    assert_eq!(
        call(&mut game, LOCATION, "IsLoaded", &[]),
        Value::Bool(true)
    );

    let typ = ref_type(&mut game);
    assert_eq!(
        call(
            &mut game,
            LOCATION,
            "HasRefType",
            std::slice::from_ref(&typ)
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut game, LOCATION, "HasRefType", &[Value::None]),
        Value::Bool(false)
    );
    assert_eq!(
        call(
            &mut game,
            LOCATION,
            "GetRefTypeAliveCount",
            std::slice::from_ref(&typ),
        ),
        Value::Int(2)
    );
    assert_eq!(
        call(
            &mut game,
            LOCATION,
            "GetRefTypeDeadCount",
            std::slice::from_ref(&typ),
        ),
        Value::Int(0)
    );
    game.world_mut().actor(id(DEAD)).unwrap().dead = true;
    assert_eq!(
        call(
            &mut game,
            LOCATION,
            "GetRefTypeAliveCount",
            std::slice::from_ref(&typ),
        ),
        Value::Int(1)
    );
    assert_eq!(
        call(&mut game, LOCATION, "GetRefTypeDeadCount", &[typ]),
        Value::Int(1)
    );
    assert_eq!(
        call(&mut game, LOCATION, "GetRefTypeAliveCount", &[Value::None]),
        Value::Int(0)
    );
    assert!(game.vm.log.messages().is_empty());
}
