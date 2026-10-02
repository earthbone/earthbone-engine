// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::quest::state::{StoryEvent, StoryParameter};
use soe_game::{Game, Pending, quest};
use soe_papyrus::Opcode::{CallMethod, CallStatic, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, i, ident, s};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const KEYWORD: u32 = 0x800;
const QUEST: u32 = 0x801;
const EVENT_NODE: u32 = 0x802;
const QUEST_NODE: u32 = 0x803;
const BENCH: u32 = 0x804;
const LOCATION: u32 = 0x805;
const ITEM: u32 = 0x806;

fn id(form: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: form,
    }
}

fn fixture_for(code: [u8; 4]) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"KYWD"), KEYWORD).zsub(T(b"EDID"), "StoryCraftItem"));
    plugin.push_top(Record::new(T(b"LCTN"), LOCATION));
    plugin.push_top(Record::new(T(b"FURN"), BENCH));
    plugin.push_top(Record::new(T(b"MISC"), ITEM));
    plugin.push_top(quest::build_record(QUEST, "StoryCraftQuest", 0));
    plugin.push_top(
        Record::new(T(b"SMEN"), EVENT_NODE)
            .sub(T(b"PNAM"), 0x5Bu32.to_le_bytes())
            .sub(T(b"CITC"), 0u32.to_le_bytes())
            .sub(T(b"DNAM"), 0u32.to_le_bytes())
            .sub(T(b"XNAM"), 0u32.to_le_bytes())
            .sub(T(b"ENAM"), code),
    );
    plugin.push_top(
        Record::new(T(b"SMQN"), QUEST_NODE)
            .sub(T(b"PNAM"), EVENT_NODE.to_le_bytes())
            .sub(T(b"SNAM"), 0u32.to_le_bytes())
            .sub(T(b"CITC"), 0u32.to_le_bytes())
            .sub(T(b"DNAM"), 0u32.to_le_bytes())
            .sub(T(b"XNAM"), 0u32.to_le_bytes())
            .sub(T(b"QNAM"), 1u32.to_le_bytes())
            .sub(T(b"NNAM"), QUEST.to_le_bytes())
            .sub(T(b"RNAM"), 0.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Story.esm", plugin).unwrap();
    Game::new(order)
}

fn fixture() -> Game {
    fixture_for(*b"CRFT")
}

fn send_craft_event(game: &mut Game) -> bool {
    let event = StoryEvent {
        id: KEYWORD,
        code: *b"CRFT",
        parameters: vec![
            StoryParameter::Reference {
                kind: 0,
                reference: Some(id(BENCH)),
            },
            StoryParameter::Reference {
                kind: 2,
                reference: Some(id(LOCATION)),
            },
            StoryParameter::Reference {
                kind: 1,
                reference: Some(id(ITEM)),
            },
        ],
    };
    game.send_story_event(&event).unwrap().contains(&id(QUEST))
}

#[test]
fn prg_14_matching_authored_event_starts_quest_once_with_ordered_arguments() {
    let mut game = fixture();
    let expected_args = [
        Value::Object(game.object_of_form(id(BENCH))),
        Value::Object(game.object_of_form(id(LOCATION))),
        Value::Object(game.object_of_form(id(ITEM))),
    ];
    assert!(send_craft_event(&mut game));
    let world = game.world();
    let quest = world.quest(id(QUEST)).unwrap();
    assert!(quest.running);
    let events: Vec<_> = world
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { name, args, .. } if name == "OnStoryCraftItem" => Some(args.clone()),
            _ => None,
        })
        .collect();
    drop(world);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].len(), 3);
    assert_eq!(events[0], expected_args);
    assert!(!send_craft_event(&mut game));
    assert_eq!(
        game.world()
            .pending
            .iter()
            .filter(|pending| matches!(pending, Pending::Event { name, .. } if name == "OnStoryCraftItem"))
            .count(),
        1
    );
}

#[test]
fn prg_14_keyword_send_story_event_starts_script_quest_with_all_arguments() {
    let mut game = fixture_for(*b"SCPT");
    let script = ScriptBuilder::new("Prg14SendStory", "Quest")
        .function(
            FunctionBuilder::new("Run", "None")
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
        .function(
            FunctionBuilder::new("OnStoryScript", "None")
                .param("keyword", "Keyword")
                .param("location", "Location")
                .param("ref1", "ObjectReference")
                .param("ref2", "ObjectReference")
                .param("value1", "Int")
                .param("value2", "Int")
                .ins(
                    CallStatic,
                    vec![
                        s("Debug"),
                        s("Trace"),
                        ident("::nonevar"),
                        ident("value1"),
                        i(0),
                    ],
                )
                .ins(
                    CallStatic,
                    vec![
                        s("Debug"),
                        s("Trace"),
                        ident("::nonevar"),
                        ident("value2"),
                        i(0),
                    ],
                )
                .ins(Return, vec![])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let quest_object = game.object_of_form(id(QUEST));
    game.vm.attach(quest_object, "Prg14SendStory").unwrap();
    let values = [
        Value::Object(game.object_of_form(id(KEYWORD))),
        Value::Object(game.object_of_form(id(LOCATION))),
        Value::Object(game.object_of_form(id(BENCH))),
        Value::Object(game.object_of_form(id(ITEM))),
        Value::Int(17),
        Value::Int(-9),
    ];
    game.vm
        .call_global("Prg14SendStory", "Run", &values)
        .unwrap();
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
    assert_eq!(event.code, *b"SCPT");
    assert_eq!(event.parameters.len(), 6);
    assert_eq!(
        event.parameters[0],
        StoryParameter::Reference {
            kind: 1,
            reference: Some(id(KEYWORD)),
        }
    );
    assert_eq!(
        event.parameters[1],
        StoryParameter::Reference {
            kind: 2,
            reference: Some(id(LOCATION)),
        }
    );
    assert_eq!(
        event.parameters[2],
        StoryParameter::Reference {
            kind: 0,
            reference: Some(id(BENCH)),
        }
    );
    assert_eq!(
        event.parameters[3],
        StoryParameter::Reference {
            kind: 0,
            reference: Some(id(ITEM)),
        }
    );
    assert_eq!(event.parameters[4], StoryParameter::Integer(17));
    assert_eq!(
        event.parameters[5],
        StoryParameter::Integer((-9_i32) as u32)
    );
    drop(world);
    let traces = game.traces();
    assert!(
        traces.iter().any(|trace| trace.contains("17")),
        "{traces:?}"
    );
    assert!(
        traces.iter().any(|trace| trace.contains("-9")),
        "{traces:?}"
    );
}
