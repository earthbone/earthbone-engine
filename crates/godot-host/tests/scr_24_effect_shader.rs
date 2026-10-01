// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::effect_shader::EffectShaderStack;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn scr_24_host_snapshot_tracks_game_play_stop_expiry_and_world_replacement() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 1));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 10));
    let mut order = LoadOrder::new();
    order.add("EffectShaders.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let mut host = EffectShaderStack::default();
    game.world_mut()
        .play_effect_shader(id(1), id(10), 2.0)
        .unwrap();
    host.sync(&game.world());
    assert_eq!(host.playbacks().len(), 1);
    assert_eq!(host.playbacks()[0].remaining, Some(2.0));
    game.tick(1.0);
    host.sync(&game.world());
    assert_eq!(host.playbacks()[0].remaining, Some(1.0));
    game.world_mut().stop_effect_shader(id(1), id(10)).unwrap();
    host.sync(&game.world());
    assert!(host.playbacks().is_empty());
    game.world_mut()
        .play_effect_shader(id(1), id(10), 0.5)
        .unwrap();
    game.tick(0.5);
    host.sync(&game.world());
    assert!(host.playbacks().is_empty());
    host.sync(&Game::new(LoadOrder::new()).world());
    assert!(host.playbacks().is_empty());
}
