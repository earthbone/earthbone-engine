// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::effect_shader::EffectShaderStack;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 0x801));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 0x802));
    let mut order = LoadOrder::new();
    order.add("Mag14Lifetime.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn finite_effect_shader_leaves_the_host_snapshot_at_its_expiry() {
    let mut game = game();
    game.world_mut()
        .play_effect_shader(id(0x801), id(0x802), 0.5)
        .unwrap();
    let mut host = EffectShaderStack::default();

    host.sync(&game.world());
    assert_eq!(host.playbacks().len(), 1);
    game.tick(0.25);
    host.sync(&game.world());
    assert_eq!(host.playbacks()[0].remaining, Some(0.25));
    game.tick(0.25);
    host.sync(&game.world());
    assert!(host.playbacks().is_empty());
}

#[test]
fn zero_duration_is_absent_and_negative_duration_survives_until_stop() {
    let mut game = game();
    let mut host = EffectShaderStack::default();
    game.world_mut()
        .play_effect_shader(id(0x801), id(0x802), 0.0)
        .unwrap();
    host.sync(&game.world());
    assert!(host.playbacks().is_empty());

    game.world_mut()
        .play_effect_shader(id(0x801), id(0x802), -1.0)
        .unwrap();
    game.tick(60.0);
    host.sync(&game.world());
    assert_eq!(host.playbacks().len(), 1);
    assert_eq!(host.playbacks()[0].remaining, None);

    game.world_mut()
        .stop_effect_shader(id(0x801), id(0x802))
        .unwrap();
    host.sync(&game.world());
    assert!(host.playbacks().is_empty());
}
