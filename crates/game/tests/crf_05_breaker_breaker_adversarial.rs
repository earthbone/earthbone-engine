// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game};
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

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
const LOCATION: u32 = 0x80a;

fn game(with_created_object: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"MISC"), INGREDIENT));
    plugin.push_top(Record::new(Tag(*b"MISC"), OUTPUT));
    plugin.push_top(Record::new(Tag(*b"LCTN"), LOCATION));
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
    let mut recipe = Record::new(Tag(*b"COBJ"), RECIPE)
        .sub(Tag(*b"BNAM"), KEYWORD.to_le_bytes())
        .sub(Tag(*b"NAM1"), 1u16.to_le_bytes())
        .sub(
            Tag(*b"CNTO"),
            [INGREDIENT.to_le_bytes().as_slice(), 1i32.to_le_bytes().as_slice()].concat(),
        );
    if with_created_object {
        recipe = recipe.sub(Tag(*b"CNAM"), OUTPUT.to_le_bytes());
    }
    plugin.push_top(recipe);
    let mut order = LoadOrder::new();
    order.add("Craft.esm", plugin).unwrap();
    Game::new(order)
}

fn id(game: &Game, raw: u32) -> GlobalFormId {
    game.world().order().resolve(0, raw)
}

#[test]
fn crf_05_breaker_location_argument_and_callback_survive_repeated_pumps_once() {
    let mut game = game(true);
    let player = id(&game, PLAYER);
    let bench = id(&game, BENCH);
    let location = id(&game, LOCATION);
    let quest_id = id(&game, QUEST);
    let ingredient = id(&game, INGREDIENT);
    let recipe = id(&game, RECIPE);
    game.world_mut().reference(player).unwrap().location = Some(location);

    let receiver = ScriptBuilder::new("CraftStoryReceiver", "Quest")
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
    game.load_script(&receiver).unwrap();
    let quest_object = game.object_of_form(quest_id);
    game.vm.attach(quest_object, "CraftStoryReceiver").unwrap();
    game.world_mut().add_item(player, ingredient, 1);

    game.craft(player, bench, recipe).unwrap();
    game.pump();
    game.run_until_idle(64);
    game.pump();
    game.run_until_idle(64);

    assert_eq!(
        game.vm.get_var(quest_object, "CraftStoryReceiver", "events"),
        Some(Value::Int(1))
    );
    let world = game.world();
    let run = world
        .quest(quest_id)
        .unwrap()
        .runtime
        .run_data
        .as_ref()
        .unwrap();
    assert_eq!(run.event.as_ref().unwrap().code, *b"CRFT");
    assert_eq!(
        run.event.as_ref().unwrap().parameters,
        vec![
            soe_game::quest::state::StoryParameter::Reference {
                kind: 0,
                reference: Some(bench),
            },
            soe_game::quest::state::StoryParameter::Reference {
                kind: 2,
                reference: Some(location),
            },
            soe_game::quest::state::StoryParameter::Reference {
                kind: 1,
                reference: Some(id(&game, OUTPUT)),
            },
        ]
    );
}

#[test]
fn crf_05_breaker_missing_required_created_object_rejects_without_event() {
    let mut game = game(false);
    let player = id(&game, PLAYER);
    let ingredient = id(&game, INGREDIENT);
    game.world_mut().add_item(player, ingredient, 1);

    assert!(game.craft(player, id(&game, BENCH), id(&game, RECIPE)).is_err());
    assert!(game.world_mut().take_story_manager_events().is_empty());
    assert_eq!(
        game.world_mut()
            .reference(player)
            .unwrap()
            .inventory
            .count(ingredient),
        1
    );
}
