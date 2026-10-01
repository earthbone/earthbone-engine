// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

#[test]
fn scr_35_loaded_parent_includes_its_descendant_location() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), 1).sub(T(b"XLCN"), 11u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"LCTN"), 10));
    plugin.push_top(Record::new(T(b"LCTN"), 11).sub(T(b"PNAM"), 10u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr35.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().load_cell(id(1));
    assert!(game.world().location_is_loaded(id(10)));
    assert!(game.world().location_is_loaded(id(11)));
}
