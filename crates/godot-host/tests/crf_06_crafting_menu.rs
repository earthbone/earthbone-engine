// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CRF-06 recipe presentation and confirmation through the shared Game.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::crafting_menu;

const PLAYER: u32 = 0x800;
const ACTOR: u32 = 0x801;
const BENCH_BASE: u32 = 0x802;
const KEYWORD: u32 = 0x803;
const INGREDIENT: u32 = 0x804;
const OUTPUT: u32 = 0x805;
const RECIPE: u32 = 0x806;
const WRONG_RECIPE: u32 = 0x807;
const MISSING_RECIPE: u32 = 0x808;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn recipe(object: u32, keyword: u32) -> Record {
    Record::new(Tag(*b"COBJ"), object)
        .zsub(Tag(*b"FULL"), "Iron dagger")
        .sub(Tag(*b"CNAM"), OUTPUT.to_le_bytes())
        .sub(Tag(*b"BNAM"), keyword.to_le_bytes())
        .sub(Tag(*b"NAM1"), 1_u16.to_le_bytes())
        .sub(
            Tag(*b"CNTO"),
            [INGREDIENT.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
        )
}

fn missing_recipe() -> Record {
    recipe(MISSING_RECIPE, KEYWORD).sub(
        Tag(*b"CNTO"),
        [INGREDIENT.to_le_bytes(), 3_u32.to_le_bytes()].concat(),
    )
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"MISC"), INGREDIENT).zsub(Tag(*b"FULL"), "Iron ingot"));
    plugin.push_top(Record::new(Tag(*b"WEAP"), OUTPUT).zsub(Tag(*b"FULL"), "Iron dagger"));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), BENCH_BASE)
            .sub(Tag(*b"WBDT"), [1, 0xff])
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), ACTOR + 10).sub(Tag(*b"NAME"), BENCH_BASE.to_le_bytes()),
    );
    plugin.push_top(recipe(RECIPE, KEYWORD));
    plugin.push_top(recipe(WRONG_RECIPE, KEYWORD + 1));
    plugin.push_top(missing_recipe());
    let mut order = LoadOrder::new();
    order.add("Crafting.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn crf_06_creation_bench_activation_lists_recipes_and_marks_missing_inputs() {
    let mut game = game();
    let player = id(PLAYER);
    let bench = id(ACTOR + 10);
    assert!(crafting_menu::is_creation_bench(&game, bench));
    assert!(!crafting_menu::is_creation_bench(&game, id(RECIPE)));
    game.world_mut().add_item(player, id(INGREDIENT), 2);

    let rows = crafting_menu::entries(&mut game, player, bench);
    assert_eq!(
        rows.iter().map(|entry| entry.recipe).collect::<Vec<_>>(),
        [id(RECIPE), id(MISSING_RECIPE)]
    );
    assert_eq!(rows[0].name, "Iron dagger");
    assert_eq!(rows[0].output, id(OUTPUT));
    assert_eq!(rows[0].count, 1);
    assert!(rows[0].available);
    assert!(!rows[1].available);
}

#[test]
fn crf_06_confirm_calls_game_craft_and_rechecks_inventory() {
    let mut game = game();
    let player = id(PLAYER);
    let bench = id(ACTOR + 10);
    game.world_mut().add_item(player, id(INGREDIENT), 2);
    let outcome = crafting_menu::confirm(&mut game, player, bench, id(RECIPE)).unwrap();
    assert_eq!(outcome.created, id(OUTPUT));
    assert_eq!(game.world_mut().item_count(player, id(INGREDIENT)), 0);
    assert_eq!(game.world_mut().item_count(player, id(OUTPUT)), 1);
    assert!(crafting_menu::confirm(&mut game, player, bench, id(RECIPE)).is_err());
    assert_eq!(game.world_mut().item_count(player, id(OUTPUT)), 1);
}
