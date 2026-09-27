// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for CRF-06 recipe listing and transaction confirmation.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::crafting_menu;

const PLAYER: u32 = 0x800;
const ACTOR: u32 = 0x801;
const BENCH: u32 = 0x802;
const KEYWORD: u32 = 0x803;
const INGREDIENT: u32 = 0x804;
const OUTPUT: u32 = 0x805;
const RECIPE: u32 = 0x806;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"MISC"), INGREDIENT));
    plugin.push_top(Record::new(Tag(*b"WEAP"), OUTPUT));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), BENCH)
            .sub(Tag(*b"WBDT"), [1, 0xff])
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), ACTOR + 10).sub(Tag(*b"NAME"), BENCH.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"COBJ"), RECIPE)
            .sub(Tag(*b"CNAM"), OUTPUT.to_le_bytes())
            .sub(Tag(*b"BNAM"), KEYWORD.to_le_bytes())
            .sub(Tag(*b"NAM1"), 1_u16.to_le_bytes())
            .sub(
                Tag(*b"CNTO"),
                [INGREDIENT.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Crf06Adversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn empty_recipe_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), BENCH)
            .sub(Tag(*b"WBDT"), [1, 0xff])
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), ACTOR + 10).sub(Tag(*b"NAME"), BENCH.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Crf06EmptyBench.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn crf_06_disabled_recipe_preview_rechecks_after_inventory_changes() {
    let mut game = game();
    let actor = id(PLAYER);
    let bench = id(ACTOR + 10);
    assert!(crafting_menu::is_creation_bench(&game, bench));

    let rows = crafting_menu::entries(&mut game, actor, bench);
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].available);
    assert!(crafting_menu::confirm(&mut game, actor, bench, id(RECIPE)).is_err());
    assert_eq!(game.world_mut().item_count(actor, id(OUTPUT)), 0);

    game.world_mut().add_item(actor, id(INGREDIENT), 2);
    let rows = crafting_menu::entries(&mut game, actor, bench);
    assert!(rows[0].available);
    assert_eq!(
        crafting_menu::confirm(&mut game, actor, bench, id(RECIPE))
            .unwrap()
            .created,
        id(OUTPUT)
    );
    assert_eq!(game.world_mut().item_count(actor, id(INGREDIENT)), 0);
    assert_eq!(game.world_mut().item_count(actor, id(OUTPUT)), 1);
}

#[test]
fn crf_06_wrong_bench_cannot_confirm_a_listed_recipe() {
    let mut game = game();
    let actor = id(PLAYER);
    let bench = id(ACTOR + 10);
    let wrong_bench = id(BENCH);
    game.world_mut().add_item(actor, id(INGREDIENT), 2);

    assert!(crafting_menu::is_creation_bench(&game, bench));
    assert!(!crafting_menu::is_creation_bench(&game, wrong_bench));
    assert!(crafting_menu::entries(&mut game, actor, wrong_bench).is_empty());
    assert!(crafting_menu::confirm(&mut game, actor, wrong_bench, id(RECIPE)).is_err());
    assert_eq!(game.world_mut().item_count(actor, id(INGREDIENT)), 2);
    assert_eq!(game.world_mut().item_count(actor, id(OUTPUT)), 0);
}

#[test]
fn crf_06_creation_bench_with_no_recipes_has_an_empty_presenter_list() {
    let mut game = empty_recipe_game();
    assert!(crafting_menu::is_creation_bench(&game, id(ACTOR + 10)));
    assert!(crafting_menu::entries(&mut game, id(PLAYER), id(ACTOR + 10)).is_empty());
}
