// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for CRF-06 workbench and recipe record boundaries.

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

fn recipe(object: u32, keyword: u32, output: u32, ingredient: u32) -> Record {
    Record::new(Tag(*b"COBJ"), object)
        .zsub(Tag(*b"FULL"), "Crafted item")
        .sub(Tag(*b"CNAM"), output.to_le_bytes())
        .sub(Tag(*b"BNAM"), keyword.to_le_bytes())
        .sub(Tag(*b"NAM1"), 1_u16.to_le_bytes())
        .sub(
            Tag(*b"CNTO"),
            [ingredient.to_le_bytes(), 1_u32.to_le_bytes()].concat(),
        )
}

fn game(workbench_type: u8, keywords: Option<u32>, recipes: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"MISC"), INGREDIENT));
    plugin.push_top(Record::new(Tag(*b"WEAP"), OUTPUT));
    let mut bench = Record::new(Tag(*b"FURN"), BENCH).sub(Tag(*b"WBDT"), [workbench_type, 0xff]);
    if let Some(keyword) = keywords {
        bench = bench.sub(Tag(*b"KWDA"), keyword.to_le_bytes());
    }
    plugin.push_top(bench);
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), ACTOR + 10).sub(Tag(*b"NAME"), BENCH.to_le_bytes()));
    for record in recipes {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Crf06Breaker.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn crf_06_only_create_object_workbenches_are_presented_as_creation_benches() {
    for workbench_type in [0, 2, 3, 4, 5, 6, 7] {
        let game = game(workbench_type, Some(KEYWORD), vec![]);
        assert!(
            !crafting_menu::is_creation_bench(&game, id(ACTOR + 10)),
            "WBDT type {workbench_type} must not open the create-object presenter"
        );
    }
    let game = game(1, Some(KEYWORD), vec![]);
    assert!(crafting_menu::is_creation_bench(&game, id(ACTOR + 10)));
}

#[test]
fn crf_06_recipe_requires_its_resolved_output_and_ingredient_records() {
    let malformed_output = recipe(RECIPE + 1, KEYWORD, 0x900, INGREDIENT);
    let malformed_ingredient = recipe(RECIPE + 2, KEYWORD, OUTPUT, 0x901);
    let mut game = game(
        1,
        Some(KEYWORD),
        vec![recipe(RECIPE, KEYWORD, OUTPUT, INGREDIENT), malformed_output, malformed_ingredient],
    );

    let rows = crafting_menu::entries(&mut game, id(PLAYER), id(ACTOR + 10));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].recipe, id(RECIPE));
    assert!(!rows[0].available);
    assert!(crafting_menu::confirm(&mut game, id(PLAYER), id(ACTOR + 10), id(RECIPE + 1)).is_err());
    assert!(crafting_menu::confirm(&mut game, id(PLAYER), id(ACTOR + 10), id(RECIPE + 2)).is_err());
}

#[test]
fn crf_06_create_object_bench_must_carry_the_recipe_bench_keyword() {
    let mut game = game(
        1,
        None,
        vec![recipe(RECIPE, KEYWORD, OUTPUT, INGREDIENT)],
    );
    game.world_mut().add_item(id(PLAYER), id(INGREDIENT), 1);

    assert!(crafting_menu::is_creation_bench(&game, id(ACTOR + 10)));
    assert!(crafting_menu::entries(&mut game, id(PLAYER), id(ACTOR + 10)).is_empty());
    assert!(crafting_menu::confirm(&mut game, id(PLAYER), id(ACTOR + 10), id(RECIPE)).is_err());
    assert_eq!(game.world_mut().item_count(id(PLAYER), id(INGREDIENT)), 1);
    assert_eq!(game.world_mut().item_count(id(PLAYER), id(OUTPUT)), 0);
}

#[test]
fn crf_06_bench_detection_rejects_missing_or_truncated_workbench_data() {
    for data in [None, Some(vec![]), Some(vec![1]), Some(vec![1, 0, 0])] {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
        let mut bench = Record::new(Tag(*b"FURN"), BENCH);
        if let Some(data) = &data {
            bench = bench.sub(Tag(*b"WBDT"), data.clone());
        }
        plugin.push_top(bench);
        plugin.push_top(
            Record::new(Tag(*b"REFR"), ACTOR + 10).sub(Tag(*b"NAME"), BENCH.to_le_bytes()),
        );
        let mut order = LoadOrder::new();
        order.add("Crf06MalformedBench.esm", plugin).unwrap();
        let game = Game::new(order);

        assert!(
            !crafting_menu::is_creation_bench(&game, id(ACTOR + 10)),
            "malformed WBDT {data:?} must not open the creation menu"
        );
    }
}
