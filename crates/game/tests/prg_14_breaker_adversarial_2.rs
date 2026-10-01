// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const fn tag(value: [u8; 4]) -> Tag {
    Tag::new(&value)
}

const KEYWORD: u32 = 0x800;
const QUEST: u32 = 0x801;
const EVENT: u32 = 0x802;
const QUEST_NODE: u32 = 0x803;
const LOCATION: u32 = 0x804;
const REF_ONE: u32 = 0x805;
const REF_TWO: u32 = 0x806;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(reset_hours: f32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(tag(*b"LCTN"), LOCATION));
    plugin.push_top(Record::new(tag(*b"REFR"), REF_ONE));
    plugin.push_top(Record::new(tag(*b"REFR"), REF_TWO));
    plugin.push_top(Record::new(tag(*b"QUST"), QUEST).sub(tag(*b"DNAM"), [0; 12]));
    plugin.push_top(
        Record::new(tag(*b"SMEN"), EVENT)
            .sub(tag(*b"DNAM"), 0u32.to_le_bytes())
            .sub(tag(*b"ENAM"), u32::from_le_bytes(*b"SCPT").to_le_bytes()),
    );
    plugin.push_top(
        Record::new(tag(*b"SMQN"), QUEST_NODE)
            .sub(tag(*b"PNAM"), EVENT.to_le_bytes())
            .sub(tag(*b"DNAM"), 0u32.to_le_bytes())
            .sub(tag(*b"XNAM"), 1u32.to_le_bytes())
            .sub(tag(*b"MNAM"), 1u32.to_le_bytes())
            .sub(tag(*b"NNAM"), QUEST.to_le_bytes())
            .sub(tag(*b"FNAM"), [0])
            .sub(tag(*b"RNAM"), reset_hours.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("StoryNativeBreaker.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("Prg14NativeBreaker", "Quest")
        .function(
            FunctionBuilder::new("Fire", "None")
                .global()
                .param("keyword", "Keyword")
                .param("location", "Location")
                .param("ref1", "ObjectReference")
                .param("ref2", "ObjectReference")
                .param("value1", "Int")
                .param("value2", "Int")
                .local("result", "Var")
                .ins(
                    CallMethod,
                    vec![
                        s("SendStoryEvent"),
                        ident("keyword"),
                        ident("result"),
                        ident("location"),
                        ident("ref1"),
                        ident("ref2"),
                        ident("value1"),
                        ident("value2"),
                    ],
                )
                .ins(Return, vec![])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    game
}

fn args(game: &mut Game, value1: i32, value2: i32) -> [Value; 6] {
    [
        Value::Object(game.object_of_form(id(KEYWORD))),
        Value::Object(game.object_of_form(id(LOCATION))),
        Value::Object(game.object_of_form(id(REF_ONE))),
        Value::Object(game.object_of_form(id(REF_TWO))),
        Value::Int(value1),
        Value::Int(value2),
    ]
}

fn fire(game: &mut Game, value1: i32, value2: i32) {
    let arguments = args(game, value1, value2);
    game.vm
        .call_global("Prg14NativeBreaker", "Fire", &arguments)
        .unwrap();
}

#[test]
fn prg_14_two_native_events_before_pump_start_the_quest_once_with_first_context() {
    let mut game = game(0.0);
    fire(&mut game, 17, -9);
    fire(&mut game, 31, 44);
    game.vm.run(64);

    assert!(game.world().quest(id(QUEST)).is_none_or(|quest| !quest.running));
    game.pump();

    let world = game.world();
    assert!(world.quest(id(QUEST)).unwrap().running);
    let event = world
        .quest(id(QUEST))
        .unwrap()
        .runtime
        .run_data
        .as_ref()
        .unwrap()
        .event
        .as_ref()
        .unwrap();
    assert_eq!(event.parameters.len(), 6);
    assert_eq!(event.parameters[4], soe_game::quest::state::StoryParameter::Integer(17));
    assert_eq!(event.parameters[5], soe_game::quest::state::StoryParameter::Integer((-9_i32) as u32));
}

#[test]
fn prg_14_native_reset_history_and_context_survive_reload() {
    let mut game = game(24.0);
    fire(&mut game, 17, -9);
    assert!(game.run_until_idle(64));
    game.world_mut().stop_quest(id(QUEST));

    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    game.world_mut().clock.game_days = 0.5;
    fire(&mut game, 31, 44);
    assert!(game.run_until_idle(64));
    assert!(!game.world().quest(id(QUEST)).unwrap().running);

    game.world_mut().clock.game_days = 1.0;
    fire(&mut game, 31, 44);
    assert!(game.run_until_idle(64));
    let world = game.world();
    let quest = world.quest(id(QUEST)).unwrap();
    assert!(quest.running);
    let event = quest
        .runtime
        .run_data
        .as_ref()
        .unwrap()
        .event
        .as_ref()
        .unwrap();
    assert_eq!(event.parameters[4], soe_game::quest::state::StoryParameter::Integer(31));
    assert_eq!(event.parameters[5], soe_game::quest::state::StoryParameter::Integer(44));
}
