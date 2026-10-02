// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, crafting::Error};

const PLAYER: u32 = 0x14;
const ACTOR: u32 = 0x800;
const KEYWORD: u32 = 0x801;
const BENCH_BASE: u32 = 0x802;
const BENCH: u32 = 0x803;
const INGREDIENT_A: u32 = 0x804;
const INGREDIENT_B: u32 = 0x805;
const EFFECT: u32 = 0x806;

fn id(raw: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: raw,
    }
}

fn ingredient(raw: u32) -> Record {
    Record::new(Tag(*b"INGR"), raw)
        .sub(Tag(*b"ENIT"), [0; 8])
        .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
        .sub(Tag(*b"EFIT"), [0; 12])
}

fn game(bench_type: u8) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD).sub(Tag(*b"EDID"), b"CraftingAlchemy\0"));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), BENCH_BASE)
            .sub(Tag(*b"WBDT"), [bench_type, 0xff])
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), BENCH).sub(Tag(*b"NAME"), BENCH_BASE.to_le_bytes()));
    plugin.push_top(ingredient(INGREDIENT_A));
    plugin.push_top(ingredient(INGREDIENT_B));
    plugin.push_top(Record::new(Tag(*b"MGEF"), EFFECT));
    let mut order = LoadOrder::new();
    order.add("AlchemyWrongBench.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().add_item(id(PLAYER), id(INGREDIENT_A), 1);
    game.world_mut().add_item(id(PLAYER), id(INGREDIENT_B), 1);
    game
}

#[test]
fn alchemy_rejects_a_non_alchemy_furniture_type_atomically() {
    let mut game = game(4);
    assert_eq!(
        game.brew_potion(id(PLAYER), id(BENCH), &[id(INGREDIENT_A), id(INGREDIENT_B)],),
        Err(Error::BenchType(4))
    );
    assert_eq!(game.world_mut().item_count(id(PLAYER), id(INGREDIENT_A)), 1);
    assert_eq!(game.world_mut().item_count(id(PLAYER), id(INGREDIENT_B)), 1);
}

#[test]
fn alchemy_accepts_the_alchemy_furniture_type() {
    let mut game = game(5);
    let result = game.brew_potion(id(PLAYER), id(BENCH), &[id(INGREDIENT_A), id(INGREDIENT_B)]);
    assert!(
        result.is_ok(),
        "WBDT=5 alchemy station was rejected: {result:?}"
    );
}
