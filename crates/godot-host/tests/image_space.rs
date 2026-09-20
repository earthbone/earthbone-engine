// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::image_space::ImageSpaceStack;

#[test]
fn image_space_host_snapshot_follows_game_fades_and_replacement() {
    let mut plugin = Plugin::new(FileHeader::default());
    for id in [1, 2] {
        plugin.push_top(Record::new(Tag::new(b"IMAD"), id).sub(Tag::new(b"DNAM"), vec![0; 244]));
    }
    let mut order = LoadOrder::new();
    order.add("HostImages.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let first = GlobalFormId { file: 0, object: 1 };
    let second = GlobalFormId { file: 0, object: 2 };
    let mut host = ImageSpaceStack::default();
    game.world_mut().apply_image_space(first, 2.0).unwrap();
    game.world_mut()
        .crossfade_image_space(Some(second), 2.0)
        .unwrap();
    game.tick(1.0);
    host.sync(&game.world());
    assert_eq!(
        host.layers()
            .iter()
            .map(|layer| (layer.modifier, layer.strength))
            .collect::<Vec<_>>(),
        vec![(first, 2.0), (second, 0.5)]
    );
    game.world_mut().remove_image_space(first);
    game.world_mut().crossfade_image_space(None, 1.0).unwrap();
    game.tick(1.0);
    host.sync(&game.world());
    assert!(host.layers().is_empty());
    game.world_mut().apply_image_space(first, 1.0).unwrap();
    host.sync(&game.world());
    host.sync(&Game::new(LoadOrder::new()).world());
    assert!(
        host.layers().is_empty(),
        "a replacement world cannot retain the previous stack"
    );
}
