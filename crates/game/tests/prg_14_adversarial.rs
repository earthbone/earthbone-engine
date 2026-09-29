// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game, Pending};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const KEYWORD: u32 = 0x800;
const QUEST: u32 = 0x801;
const EVENT_NODE: u32 = 0x802;
const QUEST_NODE: u32 = 0x803;
const BENCH: u32 = 0x804;
const LOCATION: u32 = 0x805;
const ITEM: u32 = 0x806;

fn id(form: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object: form }
}

fn fixture() -> Game {
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
            .sub(T(b"ENAM"), *b"CRFT"),
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

fn send_craft_event(game: &mut Game) -> Value {
    let script = ScriptBuilder::new("StoryProbe", "Form")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("keyword", "Keyword")
                .param("location", "Location")
                .param("bench", "ObjectReference")
                .param("item", "Form")
                .local("result", "Var")
                .ins(
                    CallMethod,
                    vec![
                        s("SendStoryEventAndWait"),
                        ident("keyword"),
                        ident("result"),
                        ident("location"),
                        ident("bench"),
                        ident("item"),
                        soe_papyrus::asm::i(0),
                        soe_papyrus::asm::i(0),
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
        Value::Object(game.object_of_form(id(BENCH))),
        Value::Object(game.object_of_form(id(ITEM))),
    ];
    let thread = game.vm.call_global("StoryProbe", "Run", &args).unwrap();
    game.vm.run(game.budget);
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn prg_14_matching_authored_event_starts_quest_once_with_ck_arguments() {
    let mut game = fixture();
    let expected_args = [
        Value::Object(game.object_of_form(id(BENCH))),
        Value::Object(game.object_of_form(id(LOCATION))),
        Value::Object(game.object_of_form(id(ITEM))),
    ];
    assert_eq!(send_craft_event(&mut game), Value::Bool(true));
    let world = game.world();
    let quest = world.quest(id(QUEST)).unwrap();
    assert!(quest.running);
    let events: Vec<_> = world
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { name, args, .. } if name == "OnStoryCraftItem" => {
                Some(args.clone())
            }
            _ => None,
        })
        .collect();
    drop(world);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].len(), 3);
    assert_eq!(events[0], expected_args);
    assert_eq!(send_craft_event(&mut game), Value::Bool(false));
    assert_eq!(
        game.world()
            .pending
            .iter()
            .filter(|pending| matches!(pending, Pending::Event { name, .. } if name == "OnStoryCraftItem"))
            .count(),
        1
    );
}
