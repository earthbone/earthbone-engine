// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const PLAYER: u32 = 0x14;
const ACTOR: u32 = 0x800;
const BENCH_KEYWORD: u32 = 0x801;
const BENCH_BASE: u32 = 0x802;
const BENCH: u32 = 0x803;
const ARMOR: u32 = 0x804;
const MATERIAL: u32 = 0x805;
const RECIPE: u32 = 0x806;

fn id(raw: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: raw,
    }
}

#[test]
fn armor_tempering_accepts_the_armor_workbench_type() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"KYWD"), BENCH_KEYWORD)
            .sub(Tag(*b"EDID"), b"CraftingSmithingArmorTable\0"),
    );
    plugin.push_top(Record::new(Tag(*b"ARMO"), ARMOR));
    plugin.push_top(Record::new(Tag(*b"MISC"), MATERIAL));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), BENCH_BASE)
            .sub(Tag(*b"WBDT"), [7, 4])
            .sub(Tag(*b"KWDA"), BENCH_KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), BENCH).sub(Tag(*b"NAME"), BENCH_BASE.to_le_bytes()));
    let mut armor_input = ARMOR.to_le_bytes().to_vec();
    armor_input.extend(1_i32.to_le_bytes());
    let mut material_input = MATERIAL.to_le_bytes().to_vec();
    material_input.extend(1_i32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"COBJ"), RECIPE)
            .sub(Tag(*b"CNAM"), ARMOR.to_le_bytes())
            .sub(Tag(*b"BNAM"), BENCH_KEYWORD.to_le_bytes())
            .sub(Tag(*b"NAM1"), 1_u16.to_le_bytes())
            .sub(Tag(*b"CNTO"), armor_input)
            .sub(Tag(*b"CNTO"), material_input),
    );
    let mut order = LoadOrder::new();
    order.add("ArmorTemper.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut().add_item(id(PLAYER), id(ARMOR), 1);
    game.world_mut().add_item(id(PLAYER), id(MATERIAL), 1);

    let result = game.temper(id(PLAYER), id(BENCH), id(RECIPE));
    assert!(
        result.is_ok(),
        "armor workbench should temper armor: {result:?}"
    );
}
