// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CRF-08 Smithing skill use from successful COBJ transactions.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use std::path::PathBuf;

const PLAYER: u32 = 0x14;
const BASE: u32 = 0x800;
const BENCH: u32 = 0x801;
const FORGE: u32 = 0x802;
const COOKPOT: u32 = 0x803;
const INPUT: u32 = 0x804;
const OUTPUT: u32 = 0x805;
const RECIPE: u32 = 0x806;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(workbench: &str, count: u16, output_value: i32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        BASE,
        "Player",
        "Player",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin.push_top(
        Record::new(T(b"AVIF"), 0x807)
            .sub(T(b"EDID"), b"AVSmithing\0".to_vec())
            .sub(
                T(b"AVSK"),
                [2.0f32, 1.0, 10000.0, 100000.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(
        Record::new(T(b"GMST"), 0x808)
            .sub(T(b"EDID"), b"fSkillUseCurve\0".to_vec())
            .sub(T(b"DATA"), 1.0f32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"KYWD"), FORGE).sub(T(b"EDID"), b"CraftingSmithingForge\0".to_vec()),
    );
    plugin
        .push_top(Record::new(T(b"KYWD"), COOKPOT).sub(T(b"EDID"), b"CraftingCookpot\0".to_vec()));
    plugin.push_top(Record::new(T(b"FURN"), 0x809).sub(T(b"WBDT"), [1, 0]).sub(
        T(b"KWDA"),
        if workbench == "forge" {
            FORGE.to_le_bytes()
        } else {
            COOKPOT.to_le_bytes()
        },
    ));
    plugin.push_top(Record::new(T(b"REFR"), BENCH).sub(T(b"NAME"), 0x809u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"MISC"), INPUT).sub(T(b"DATA"), [0u8; 8]));
    plugin.push_top(Record::new(T(b"MISC"), OUTPUT).sub(
        T(b"DATA"),
        [output_value.to_le_bytes(), 0.0f32.to_le_bytes()].concat(),
    ));
    plugin.push_top(
        Record::new(T(b"COBJ"), RECIPE)
            .sub(
                T(b"CNTO"),
                [INPUT.to_le_bytes(), 1i32.to_le_bytes()].concat(),
            )
            .sub(T(b"CNAM"), OUTPUT.to_le_bytes())
            .sub(
                T(b"BNAM"),
                if workbench == "forge" { FORGE } else { COOKPOT }.to_le_bytes(),
            )
            .sub(T(b"NAM1"), count.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"ACHR"), PLAYER).sub(T(b"NAME"), BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Crf08.esm", plugin).unwrap();
    Game::new(order)
}

fn craft(game: &mut Game) -> Result<soe_game::crafting::Outcome, soe_game::crafting::Error> {
    game.craft(id(PLAYER), id(BENCH), id(RECIPE))
}

#[test]
fn crf_08_successful_craft_awards_value_and_count_scaled_skill_use_once() {
    let mut game = game("forge", 3, 20);
    let player = id(PLAYER);
    game.world_mut().add_item(player, id(INPUT), 1);

    craft(&mut game).unwrap();

    let expected = (3.0 * 20f32.powf(0.65) + 25.0) * 3.0;
    let expected = expected * 2.0 + 1.0;
    let actual = game.world().skill_experience(player, "smithing").unwrap();
    assert!(
        (actual - expected).abs() < 0.001,
        "actual {actual}, expected {expected}"
    );
    assert_eq!(game.world_mut().item_count(player, id(OUTPUT)), 3);
}

#[test]
fn crf_08_rejected_and_non_smithing_crafts_award_no_skill_use() {
    let mut rejected = game("forge", 3, 20);
    assert!(craft(&mut rejected).is_err());
    assert_eq!(
        rejected.world().skill_experience(id(PLAYER), "smithing"),
        Some(0.0)
    );

    let mut cooking = game("cookpot", 3, 20);
    cooking.world_mut().add_item(id(PLAYER), id(INPUT), 1);
    craft(&mut cooking).unwrap();
    assert_eq!(
        cooking.world().skill_experience(id(PLAYER), "smithing"),
        Some(0.0)
    );
}

#[test]
fn crf_08_installed_forge_recipe_awards_smithing_skill_use() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed CRF-08 crafting check");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let forge = GlobalFormId {
        file: 0,
        object: 0x1ad6e,
    };
    let dagger = GlobalFormId {
        file: 0,
        object: 0x1397e,
    };
    let mut game = Game::new(order);
    let player = game.world_mut().player();
    let recipe = game
        .world()
        .order()
        .winners_of(T(b"COBJ"))
        .find_map(|(recipe_id, record)| {
            let view = soe_esm::records::gameplay::Recipe::from_record(record).ok()?;
            (view
                .created_object
                .and_then(|raw| game.world().resolve_from(recipe_id, raw))
                == Some(dagger)
                && view
                    .workbench_keyword
                    .and_then(|raw| game.world().resolve_from(recipe_id, raw))
                    == game.world().form("CraftingSmithingForge"))
            .then_some((recipe_id, view))
        })
        .expect("vanilla iron dagger forge recipe");
    for ingredient in &recipe.1.ingredients {
        let form = game
            .world()
            .resolve_from(recipe.0, ingredient.item)
            .unwrap();
        game.world_mut().add_item(player, form, ingredient.count);
    }
    game.craft(player, forge, recipe.0).unwrap();
    assert!(game.world().skill_experience(player, "smithing").unwrap() > 0.0);
}

#[test]
fn crf_08_partial_skill_use_survives_engine_save_and_load() {
    let mut original = game("forge", 1, 20);
    original.world_mut().add_item(id(PLAYER), id(INPUT), 1);
    craft(&mut original).unwrap();
    let experience = original
        .world()
        .skill_experience(id(PLAYER), "smithing")
        .unwrap();
    let saved = original.save_state().unwrap();

    let mut loaded = game("forge", 1, 20);
    loaded.load_state(&saved).unwrap();
    assert_eq!(
        loaded.world().skill_experience(id(PLAYER), "smithing"),
        Some(experience)
    );
}
