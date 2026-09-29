// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const PLAYER: u32 = 0x14;
const ACTOR: u32 = 0x800;
const KEYWORD: u32 = 0x801;
const BENCH_BASE: u32 = 0x802;
const BENCH: u32 = 0x803;
const INGREDIENT_A: u32 = 0x804;
const INGREDIENT_B: u32 = 0x805;
const SHARED: u32 = 0x806;
const UNIQUE_A: u32 = 0x807;
const UNIQUE_B: u32 = 0x808;

fn ingredient(id: u32, effects: &[(u32, f32)]) -> Record {
    let mut record = Record::new(Tag(*b"INGR"), id).sub(Tag(*b"ENIT"), [0; 8]);
    for (effect, magnitude) in effects {
        record = record
            .sub(Tag(*b"EFID"), effect.to_le_bytes())
            .sub(Tag(*b"EFIT"), {
                let mut bytes = magnitude.to_le_bytes().to_vec();
                bytes.extend(0_u32.to_le_bytes());
                bytes.extend(0_u32.to_le_bytes());
                bytes
            });
    }
    record
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD).sub(Tag(*b"EDID"), b"CraftingAlchemy\0"));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), BENCH_BASE)
            .sub(Tag(*b"WBDT"), [4, 0xff])
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), BENCH).sub(Tag(*b"NAME"), BENCH_BASE.to_le_bytes()));
    plugin.push_top(ingredient(INGREDIENT_A, &[(SHARED, 12.0), (UNIQUE_A, 2.0)]));
    plugin.push_top(ingredient(INGREDIENT_B, &[(UNIQUE_B, 3.0), (SHARED, 9.0)]));
    for id in [SHARED, UNIQUE_A, UNIQUE_B] {
        plugin.push_top(Record::new(Tag(*b"MGEF"), id));
    }
    let mut order = LoadOrder::new();
    order.add("Alchemy.esm", plugin).unwrap();
    let game = Game::new(order);
    let player = GlobalFormId {
        file: 0,
        object: PLAYER,
    };
    let ingredient_a = id(&game, INGREDIENT_A);
    let ingredient_b = id(&game, INGREDIENT_B);
    game.world_mut().add_item(player, ingredient_a, 1);
    game.world_mut().add_item(player, ingredient_b, 1);
    game
}

fn id(game: &Game, object: u32) -> GlobalFormId {
    game.world().order().resolve(0, object)
}

#[test]
fn crf_02_brew_uses_shared_effect_and_consumes_both_ingredients() {
    let mut game = game();
    let actor = id(&game, PLAYER);
    let bench = id(&game, BENCH);
    let ingredient_a = id(&game, INGREDIENT_A);
    let ingredient_b = id(&game, INGREDIENT_B);
    let result = game
        .brew_potion(actor, bench, &[ingredient_a, ingredient_b])
        .unwrap();
    assert_eq!(result.consumed.len(), 2);
    assert!(result.consumed.values().all(|count| *count == 1));
    assert_eq!(result.effects, [id(&game, SHARED)]);
    assert_eq!(game.world_mut().item_count(actor, ingredient_a), 0);
    assert_eq!(game.world_mut().item_count(actor, ingredient_b), 0);
    assert_eq!(game.world().tag_of(result.created), Some(Tag(*b"ALCH")));
    assert_eq!(
        game.world().name_of(result.created).as_deref(),
        Some("Potion")
    );
    let potion = game.world().ingestible(result.created).unwrap();
    assert_eq!(potion.effects.len(), 1);
    assert_eq!(potion.effects[0].data.magnitude, 12.0);
    assert_eq!(game.world_mut().item_count(actor, result.created), 1);
}

#[test]
fn crf_02_failed_combination_preserves_inputs_and_saved_potion_survives_reload() {
    let mut game = game();
    let actor = id(&game, PLAYER);
    let bench = id(&game, BENCH);
    let ingredient_a = id(&game, INGREDIENT_A);
    let ingredient_b = id(&game, INGREDIENT_B);
    assert!(game
        .brew_potion(actor, bench, &[ingredient_a, ingredient_a])
        .is_err());
    assert_eq!(game.world_mut().item_count(actor, ingredient_a), 1);
    assert_eq!(game.world_mut().item_count(actor, ingredient_b), 1);

    let brewed = game
        .brew_potion(actor, bench, &[ingredient_a, ingredient_b])
        .unwrap();
    let hash = game.world().state_hash();
    let state = game.save_state().unwrap();
    game.load_state(&state).unwrap();
    assert_eq!(game.world().tag_of(brewed.created), Some(Tag(*b"ALCH")));
    assert_eq!(
        game.world()
            .ingestible(brewed.created)
            .unwrap()
            .effects
            .len(),
        1
    );
    assert_eq!(game.world().state_hash(), hash);
}
