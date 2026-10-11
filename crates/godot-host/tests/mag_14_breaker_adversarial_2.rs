// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::effect_shader::EffectShaderStack;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn mag_14_host_snapshot_tracks_replay_refresh_and_pair_scoped_stop() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 0x801));
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 0x802));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 0x803));
    let mut order = LoadOrder::new();
    order.add("Mag14Breaker.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let mut host = EffectShaderStack::default();

    game.world_mut()
        .play_effect_shader(id(0x801), id(0x803), 2.0)
        .unwrap();
    game.world_mut()
        .play_effect_shader(id(0x801), id(0x803), -1.0)
        .unwrap();
    game.world_mut()
        .play_effect_shader(id(0x802), id(0x803), 1.0)
        .unwrap();
    host.sync(&game.world());
    assert_eq!(host.playbacks().len(), 2);
    assert!(host.playbacks().iter().any(|playback| {
        playback.shader == id(0x801) && playback.target == id(0x803) && playback.remaining.is_none()
    }));

    game.world_mut()
        .stop_effect_shader(id(0x801), id(0x803))
        .unwrap();
    host.sync(&game.world());
    assert_eq!(host.playbacks().len(), 1);
    assert_eq!(host.playbacks()[0].shader, id(0x802));
}
