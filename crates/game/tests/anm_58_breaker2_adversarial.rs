// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Check a wait's false completion after an unrelated event and graph unload.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::vm::LatentId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn anm_58_unrelated_event_does_not_win_over_graph_unload() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"ACTI"), 0x100));
    let mut order = LoadOrder::new();
    order.add("Anm58Order.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let source = game.world_mut().place_at(id(0x100), player, false);
    game.world_mut()
        .wait_for_animation_event(source, "done", LatentId(42));

    game.world_mut()
        .animation_graph_event(source, "other", &mut |_, _| unreachable!());
    game.world_mut().animation_graph_unloaded(source);

    assert_eq!(game.world().completed_animation_waits, [(LatentId(42), false)]);
    assert!(game.world().animation_waiters.is_empty());
}
