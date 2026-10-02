// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, quest};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, i, ident};

const PLAYER: u32 = 0x14;
const BASE: u32 = 0x800;
const BENCH: u32 = 0x801;
const FURNITURE: u32 = 0x802;
const KEYWORD: u32 = 0x803;
const INGREDIENT: u32 = 0x804;
const OUTPUT: u32 = 0x805;
const RECIPE: u32 = 0x806;
const QUEST: u32 = 0x807;
const EVENT_NODE: u32 = 0x808;
const QUEST_NODE: u32 = 0x809;

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"MISC"), INGREDIENT));
    plugin.push_top(Record::new(Tag(*b"MISC"), OUTPUT));
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), FURNITURE)
            .sub(Tag(*b"WBDT"), [1, 0xff])
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), BENCH).sub(Tag(*b"NAME"), FURNITURE.to_le_bytes()));
    plugin.push_top(quest::build_record(QUEST, "CraftQuest", 0));
    plugin.push_top(
        Record::new(Tag(*b"SMEN"), EVENT_NODE)
            .sub(Tag(*b"PNAM"), 0u32.to_le_bytes())
            .sub(Tag(*b"DNAM"), 0u32.to_le_bytes())
            .sub(Tag(*b"ENAM"), u32::from_le_bytes(*b"CRFT").to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"SMQN"), QUEST_NODE)
            .sub(Tag(*b"PNAM"), EVENT_NODE.to_le_bytes())
            .sub(Tag(*b"DNAM"), 0u32.to_le_bytes())
            .sub(Tag(*b"XNAM"), 1u32.to_le_bytes())
            .sub(Tag(*b"MNAM"), 1u32.to_le_bytes())
            .sub(Tag(*b"NNAM"), QUEST.to_le_bytes())
            .sub(Tag(*b"FNAM"), [0])
            .sub(Tag(*b"RNAM"), 0.0f32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"COBJ"), RECIPE)
            .sub(Tag(*b"CNAM"), OUTPUT.to_le_bytes())
            .sub(Tag(*b"BNAM"), KEYWORD.to_le_bytes())
            .sub(Tag(*b"NAM1"), 1u16.to_le_bytes())
            .sub(
                Tag(*b"CNTO"),
                [
                    INGREDIENT.to_le_bytes().as_slice(),
                    1i32.to_le_bytes().as_slice(),
                ]
                .concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Craft.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn crf_05_successful_craft_routes_authored_event_once_with_ck_arguments() {
    let mut game = game();
    let player = id(&game, PLAYER);
    let bench = id(&game, BENCH);
    let quest_id = id(&game, QUEST);
    let ingredient = id(&game, INGREDIENT);
    let output = id(&game, OUTPUT);
    let recipe = id(&game, RECIPE);
    let story_script = ScriptBuilder::new("CraftStoryReceiver", "Quest")
        .variable("events", "Int", i(0))
        .function(
            FunctionBuilder::new("OnStoryCraftItem", "None")
                .param("bench", "ObjectReference")
                .param("location", "Location")
                .param("item", "Form")
                .ins(IAdd, vec![ident("events"), ident("events"), i(1)])
                .build(),
        )
        .bytes();
    game.load_script(&story_script).unwrap();
    let quest_object = game.object_of_form(quest_id);
    game.vm.attach(quest_object, "CraftStoryReceiver").unwrap();
    game.world_mut().add_item(player, ingredient, 1);

    game.craft(player, bench, recipe).unwrap();
    game.run_until_idle(64);
    assert_eq!(
        game.vm
            .get_var(quest_object, "CraftStoryReceiver", "events"),
        Some(Value::Int(1))
    );

    let world = game.world();
    let quest = world.quest(quest_id).unwrap();
    assert!(quest.running);
    let event = quest
        .runtime
        .run_data
        .as_ref()
        .unwrap()
        .event
        .as_ref()
        .unwrap();
    assert_eq!(event.code, *b"CRFT");
    assert_eq!(event.parameters.len(), 3);
    assert!(matches!(
        event.parameters[0],
        soe_game::quest::state::StoryParameter::Reference {
            kind: 0,
            reference: Some(reference)
        } if reference == bench
    ));
    assert!(matches!(
        event.parameters[1],
        soe_game::quest::state::StoryParameter::Reference {
            kind: 2,
            reference: None
        }
    ));
    assert!(matches!(
        event.parameters[2],
        soe_game::quest::state::StoryParameter::Reference {
            kind: 1,
            reference: Some(reference)
        } if reference == output
    ));
}

#[test]
fn crf_05_rejected_craft_does_not_route_authored_event() {
    let mut game = game();
    let player = id(&game, PLAYER);
    let bench = id(&game, BENCH);
    let quest_id = id(&game, QUEST);
    let recipe = id(&game, RECIPE);

    assert!(game.craft(player, bench, recipe).is_err());
    game.pump();

    assert!(
        game.world_mut()
            .quest_state(quest_id)
            .is_some_and(|quest| !quest.running && quest.runtime.run_data.is_none())
    );
    assert!(!game.world().pending.iter().any(|pending| matches!(
        pending,
        soe_game::Pending::Event { name, .. } if name == "OnStoryCraftItem"
    )));
}

fn id(game: &Game, raw: u32) -> GlobalFormId {
    game.world().order().resolve(0, raw)
}

#[test]
fn crf_05_breaker_each_success_dispatches_once_and_later_rejection_adds_none() {
    let mut game = game();
    let player = id(&game, PLAYER);
    let bench = id(&game, BENCH);
    let recipe = id(&game, RECIPE);
    let ingredient = id(&game, INGREDIENT);
    game.world_mut().add_item(player, ingredient, 2);

    game.craft(player, bench, recipe).unwrap();
    game.craft(player, bench, recipe).unwrap();
    let events = game.world_mut().take_story_manager_events();
    assert_eq!(events.len(), 2);
    for event in &events {
        assert_eq!(
            event.kind,
            soe_game::quest::state::StoryManagerEventKind::CraftItem
        );
        assert_eq!(
            event.parameters,
            vec![Some(bench), None, Some(id(&game, OUTPUT))]
        );
    }
    assert!(game.craft(player, bench, recipe).is_err());
    assert!(game.world_mut().take_story_manager_events().is_empty());
}
