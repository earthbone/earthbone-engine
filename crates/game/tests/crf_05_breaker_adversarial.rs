// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, quest};

const PLAYER: u32 = 0x14;
const BASE: u32 = 0x800;
const BENCH: u32 = 0x801;
const FURNITURE: u32 = 0x802;
const KEYWORD: u32 = 0x803;
const INGREDIENT: u32 = 0x804;
const OUTPUT: u32 = 0x805;
const RECIPE: u32 = 0x806;
const QUEST: u32 = 0x807;

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
