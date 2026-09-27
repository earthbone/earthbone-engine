// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::inventory_menu::{self, Category};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x100).zsub(Tag(*b"FULL"), "Coin"));
    plugin.push_top(
        Record::new(Tag(*b"CONT"), 0x101)
            .zsub(Tag(*b"FULL"), "Chest")
            .sub(Tag(*b"COCT"), 1_u32.to_le_bytes())
            .sub(
                Tag(*b"CNTO"),
                [0x100_u32.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x102).sub(Tag(*b"NAME"), 0x101_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Container.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn inv_04_stale_item_row_cannot_transfer_after_container_was_emptied() {
    let mut game = game();
    let player = game.player();
    let chest = id(0x102);
    let coin = id(0x100);
    assert!(game.activate(chest, player));
    let initial = inventory_menu::entries(&game, chest, Category::All);
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].count, 2);
    assert_eq!(game.transfer_item(chest, coin, 2, player), 2);
    assert!(inventory_menu::entries(&game, chest, Category::All).is_empty());
    assert_eq!(game.transfer_item(chest, coin, 1, player), 0);
    assert_eq!(
        inventory_menu::entries(&game, player, Category::All)[0].count,
        2
    );
}
