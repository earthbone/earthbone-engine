// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-08 Actor queries that have an owning World state field.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::npc::{self, Config, Stats};
use soe_papyrus::{
    Opcode::{CallMethod, Return},
    Value,
    asm::{FunctionBuilder, ScriptBuilder, ident, s},
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x800,
        "Probe",
        "Probe",
        Config {
            flags: npc::flags::BLEEDOUT_OVERRIDE | npc::flags::PROTECTED,
            bleedout: 20,
            ..Config::default()
        },
        Stats::default(),
    ));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr08.esm", plugin).unwrap();
    Game::new(order)
}

fn actor_query(game: &mut Game, actor: GlobalFormId, method: &str) -> Value {
    let class = format!("Scr08{method}Probe");
    let function = FunctionBuilder::new("Run", "Bool")
        .local("result", "Bool")
        .ins(CallMethod, vec![s(method), ident("self"), ident("result")])
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&class, "Actor")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(actor);
    game.vm.attach(object, &class).unwrap();
    let thread = game.vm.call(object, "Run", &[]).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_08_is_alarmed_reads_world_alert_state() {
    let mut game = game();
    assert_eq!(
        actor_query(&mut game, id(0x900), "IsAlarmed"),
        Value::Bool(false)
    );
    game.world_mut().actor(id(0x900)).unwrap().alerted = true;
    assert_eq!(
        actor_query(&mut game, id(0x900), "IsAlarmed"),
        Value::Bool(true)
    );
}

#[test]
fn scr_08_is_bleeding_out_reads_world_down_state() {
    let mut game = game();
    assert_eq!(
        actor_query(&mut game, id(0x900), "IsBleedingOut"),
        Value::Bool(false)
    );
    game.world_mut()
        .set_actor_value(id(0x900), "health", 100.0)
        .unwrap();
    game.world_mut().actor(id(0x900)).unwrap().unconscious = true;
    assert_eq!(
        actor_query(&mut game, id(0x900), "IsBleedingOut"),
        Value::Bool(false)
    );
    game.world_mut().actor(id(0x900)).unwrap().unconscious = false;
    game.world_mut()
        .damage_actor_value(id(0x900), "health", 100.0)
        .unwrap();
    assert_eq!(
        actor_query(&mut game, id(0x900), "IsBleedingOut"),
        Value::Bool(true)
    );
}
