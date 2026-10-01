// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::quest::state::{StoryEvent, StoryParameter};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const fn tag(value: [u8; 4]) -> Tag {
    Tag::new(&value)
}

const QUEST: u32 = 0x800;
const QUEST_TWO: u32 = 0x807;
const EVENT: u32 = 0x801;
const QUEST_NODE: u32 = 0x802;
const KEYWORD: u32 = 0x803;
const LOCATION: u32 = 0x804;
const REF_ONE: u32 = 0x805;
const REF_TWO: u32 = 0x806;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(event_code: [u8; 4]) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(tag(*b"LCTN"), LOCATION));
    plugin.push_top(Record::new(tag(*b"REFR"), REF_ONE));
    plugin.push_top(Record::new(tag(*b"REFR"), REF_TWO));
    plugin.push_top(Record::new(tag(*b"QUST"), QUEST).sub(tag(*b"DNAM"), [0; 12]));
    plugin.push_top(
        Record::new(tag(*b"SMEN"), EVENT)
            .sub(tag(*b"DNAM"), 0u32.to_le_bytes())
            .sub(tag(*b"ENAM"), u32::from_le_bytes(event_code).to_le_bytes()),
    );
    plugin.push_top(
        Record::new(tag(*b"SMQN"), QUEST_NODE)
            .sub(tag(*b"PNAM"), EVENT.to_le_bytes())
            .sub(tag(*b"DNAM"), 0u32.to_le_bytes())
            .sub(tag(*b"XNAM"), 1u32.to_le_bytes())
            .sub(tag(*b"MNAM"), 1u32.to_le_bytes())
            .sub(tag(*b"NNAM"), QUEST.to_le_bytes())
            .sub(tag(*b"FNAM"), [0])
            .sub(tag(*b"RNAM"), 24.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("StoryBreaker.esm", plugin).unwrap();
    Game::new(order)
}

fn selection_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(*b"QUST"), QUEST).sub(tag(*b"DNAM"), [0; 12]));
    plugin.push_top(Record::new(tag(*b"QUST"), QUEST_TWO).sub(tag(*b"DNAM"), [0; 12]));
    plugin.push_top(
        Record::new(tag(*b"SMEN"), EVENT)
            .sub(tag(*b"DNAM"), 0u32.to_le_bytes())
            .sub(tag(*b"ENAM"), u32::from_le_bytes(*b"SCPT").to_le_bytes()),
    );
    plugin.push_top(
        Record::new(tag(*b"SMQN"), QUEST_NODE)
            .sub(tag(*b"PNAM"), EVENT.to_le_bytes())
            .sub(tag(*b"DNAM"), ((1_u32 << 16) | (1 << 18)).to_le_bytes())
            .sub(tag(*b"XNAM"), 1u32.to_le_bytes())
            .sub(tag(*b"MNAM"), 1u32.to_le_bytes())
            .sub(tag(*b"NNAM"), QUEST.to_le_bytes())
            .sub(tag(*b"FNAM"), [0])
            .sub(tag(*b"RNAM"), 0.0f32.to_le_bytes())
            .sub(tag(*b"NNAM"), QUEST_TWO.to_le_bytes())
            .sub(tag(*b"FNAM"), [0])
            .sub(tag(*b"RNAM"), 0.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("StorySelectionBreaker.esm", plugin).unwrap();
    Game::new(order)
}

fn script_event() -> StoryEvent {
    StoryEvent {
        id: u32::from_le_bytes(*b"SCPT"),
        code: *b"SCPT",
        parameters: vec![
            StoryParameter::Reference {
                kind: 1,
                reference: Some(id(KEYWORD)),
            },
            StoryParameter::Reference {
                kind: 2,
                reference: Some(id(LOCATION)),
            },
            StoryParameter::Reference {
                kind: 0,
                reference: Some(id(REF_ONE)),
            },
            StoryParameter::Reference {
                kind: 0,
                reference: Some(id(REF_TWO)),
            },
            StoryParameter::Integer(17),
            StoryParameter::Integer((-9_i32) as u32),
        ],
    }
}

#[test]
fn prg_14_send_story_event_and_wait_stub_does_not_route_a_quest() {
    let mut game = game(*b"SCPT");
    let script = ScriptBuilder::new("Prg14WaitProbe", "Quest")
        .function(
            FunctionBuilder::new("Run", "Var")
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
                        s("SendStoryEventAndWait"),
                        ident("keyword"),
                        ident("result"),
                        ident("location"),
                        ident("ref1"),
                        ident("ref2"),
                        ident("value1"),
                        ident("value2"),
                    ],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let args = [
        Value::Object(game.object_of_form(id(KEYWORD))),
        Value::Object(game.object_of_form(id(LOCATION))),
        Value::Object(game.object_of_form(id(REF_ONE))),
        Value::Object(game.object_of_form(id(REF_TWO))),
        Value::Int(17),
        Value::Int(-9),
    ];
    let thread = game.vm.call_global("Prg14WaitProbe", "Run", &args).unwrap();
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Bool(false)));
    assert!(game.world().quest(id(QUEST)).is_none());
}

#[test]
fn prg_14_script_event_arguments_survive_engine_state_roundtrip() {
    let mut game = game(*b"SCPT");
    assert_eq!(
        game.send_story_event(&script_event()).unwrap(),
        vec![id(QUEST)]
    );
    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    let world = game.world();
    let stored = world
        .quest(id(QUEST))
        .unwrap()
        .runtime
        .run_data
        .as_ref()
        .unwrap()
        .event
        .as_ref()
        .unwrap();
    assert_eq!(stored, &script_event());
}

#[test]
fn prg_14_do_all_selection_cursor_survives_engine_state_roundtrip() {
    let mut game = selection_game();
    assert_eq!(
        game.send_story_event(&script_event()).unwrap(),
        vec![id(QUEST)]
    );
    game.world_mut().stop_quest(id(QUEST));
    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    assert_eq!(
        game.send_story_event(&script_event()).unwrap(),
        vec![id(QUEST_TWO)]
    );
}

#[test]
fn prg_14_reset_deadline_survives_engine_state_roundtrip() {
    let mut game = game(*b"SCPT");
    assert_eq!(
        game.send_story_event(&script_event()).unwrap(),
        vec![id(QUEST)]
    );
    game.world_mut().stop_quest(id(QUEST));
    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    game.world_mut().clock.game_days = 0.5;
    assert!(game.send_story_event(&script_event()).unwrap().is_empty());
    game.world_mut().clock.game_days = 1.0;
    assert_eq!(
        game.send_story_event(&script_event()).unwrap(),
        vec![id(QUEST)]
    );
}

#[test]
fn prg_14_unmatched_event_does_not_start_or_record_a_quest() {
    let mut game = game(*b"SCPT");
    let mut event = script_event();
    event.code = *b"SKIL";
    assert!(game.send_story_event(&event).unwrap().is_empty());
    let world = game.world();
    assert!(world.quest(id(QUEST)).is_none());
}
