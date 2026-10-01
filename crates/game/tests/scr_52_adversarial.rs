// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, world::VisualEffectError};

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn visual_effect_nonfinite_facing_and_duration_are_rejected_without_partial_instances() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"RFCT"), 1));
    plugin.push_top(Record::new(Tag::new(b"STAT"), 2));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 3).sub(Tag::new(b"NAME"), 2_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("scr52-breaker.esp", plugin).expect("plugin loads");
    let mut game = Game::new(order);

    assert_eq!(
        game.world_mut().play_visual_effect(id(1), id(3), f64::INFINITY, None),
        Ok(soe_game::world::VisualEffectId(1))
    );
    assert_eq!(
        game.world_mut().play_visual_effect(id(1), id(3), 2.0, Some(id(2))),
        Err(VisualEffectError::Target(id(2)))
    );
    assert_eq!(game.world().visual_effect_instances().count(), 1);
    assert_eq!(game.world().visual_effect_instances().next().unwrap().remaining, None);
}
