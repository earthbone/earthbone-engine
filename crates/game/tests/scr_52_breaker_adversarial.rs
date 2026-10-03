// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::VisualEffectError, Game};

#[test]
fn play_rejects_rfct_without_required_effect_data_without_creating_an_instance() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"RFCT"), 1));
    plugin.push_top(Record::new(Tag::new(b"STAT"), 2));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 3).sub(Tag::new(b"NAME"), 2_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr52MalformedRfct.esp", plugin).unwrap();
    let game = Game::new(order);
    let effect = GlobalFormId { file: 0, object: 1 };
    let target = GlobalFormId { file: 0, object: 3 };

    assert_eq!(
        game.world_mut()
            .play_visual_effect(effect, target, -1.0, None),
        Err(VisualEffectError::Effect(effect))
    );
    assert_eq!(game.world().visual_effect_instances().count(), 0);
}
