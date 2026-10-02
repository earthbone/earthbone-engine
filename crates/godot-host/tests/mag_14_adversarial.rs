// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::effect_shader::EffectShaderStack;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn mag_14_host_snapshot_keeps_stop_scoped_to_one_shader_target_pair() {
    let mut plugin = Plugin::new(FileHeader::default());
    for shader in [0x801, 0x802] {
        plugin.push_top(Record::new(Tag::new(b"EFSH"), shader));
    }
    for target in [0x803, 0x804] {
        plugin.push_top(Record::new(Tag::new(b"REFR"), target));
    }
    let mut order = LoadOrder::new();
    order.add("EffectLifetime.esm", plugin).unwrap();
    let game = Game::new(order);
    let mut host = EffectShaderStack::default();

    game.world_mut()
        .play_effect_shader(id(0x801), id(0x803), -1.0)
        .unwrap();
    game.world_mut()
        .play_effect_shader(id(0x801), id(0x804), 8.0)
        .unwrap();
    game.world_mut()
        .play_effect_shader(id(0x802), id(0x803), 3.0)
        .unwrap();
    host.sync(&game.world());
    assert_eq!(host.playbacks().len(), 3);

    game.world_mut()
        .stop_effect_shader(id(0x801), id(0x803))
        .unwrap();
    host.sync(&game.world());
    assert_eq!(host.playbacks().len(), 2);
    assert!(host
        .playbacks()
        .iter()
        .any(|playback| playback.shader == id(0x801)
            && playback.target == id(0x804)
            && playback.remaining == Some(8.0)));
    assert!(host
        .playbacks()
        .iter()
        .any(|playback| playback.shader == id(0x802)
            && playback.target == id(0x803)
            && playback.remaining == Some(3.0)));
}

#[test]
fn mag_14_host_snapshot_replay_refreshes_without_duplicate_effects() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 0x801));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 0x803));
    let mut order = LoadOrder::new();
    order.add("EffectRefresh.esm", plugin).unwrap();
    let game = Game::new(order);
    let mut host = EffectShaderStack::default();

    game.world_mut()
        .play_effect_shader(id(0x801), id(0x803), 2.0)
        .unwrap();
    game.world_mut()
        .play_effect_shader(id(0x801), id(0x803), -1.0)
        .unwrap();
    host.sync(&game.world());

    assert_eq!(host.playbacks().len(), 1);
    assert_eq!(host.playbacks()[0].remaining, None);
}
