// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn weapon_fire_reports_valid_request_as_unsupported_until_projectile_execution_exists() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"WEAP"), 1)
            .sub(Tag::new(b"DATA"), [0; 10])
            .sub(Tag::new(b"DNAM"), [0; 100]),
    );
    plugin.push_top(Record::new(Tag::new(b"AMMO"), 2));
    plugin.push_top(Record::new(Tag::new(b"STAT"), 3));
    plugin.push_top(Record::new(Tag::new(b"ACHR"), 4).sub(Tag::new(b"NAME"), 3_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order
        .add("scr53-breaker.esp", plugin)
        .expect("plugin loads");
    let game = Game::new(order);
    let result = game.world_mut().fire_weapon(id(1), id(4), Some(id(2)));
    assert!(matches!(
        result,
        Err(soe_game::combat::FireError::Unsupported(
            "ranged projectile execution"
        ))
    ));
}
