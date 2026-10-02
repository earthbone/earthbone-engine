// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! ANM-58 adversarial checks for the host event delivery path.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"ACTI"), 0x100));
    let mut order = LoadOrder::new();
    order.add("Anm58.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn host_graph_event_reaches_game_once_across_signal_and_notice_paths() {
    let mut game = game();
    let source = id(0x100);

    // SkinnedActorNode queues each event and emits its signal. PlayerNode
    // reports that signal immediately, then drains the queued notice later.
    game.world_mut().queue_animation_report(source, "HitFrame");
    game.report_animation_event(source, "HitFrame");
    game.deliver_animation_reports();

    assert_eq!(game.world().animation_events, [(source, "HitFrame".into())]);
}
