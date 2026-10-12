// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Player-location state checks at the host adapter's input boundary.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn load_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x1234).sub(Tag(*b"DATA"), [1]));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

#[test]
fn sav_19_restored_player_cell_and_pose_survive_native_save_load() {
    let cell = GlobalFormId {
        file: 0,
        object: 0x1234,
    };
    let mut restored = Game::new(load_order());
    let player = restored.player();
    {
        let mut world = restored.world_mut();
        let reference = world.reference(player).unwrap();
        reference.cell = Some(cell);
        reference.position = [4096.0, -8192.0, 128.5];
        reference.rotation = [0.25, 1.5, -2.0];
    }

    let bytes = restored.save_state().unwrap();
    let mut resumed = Game::new(load_order());
    resumed.load_state(&bytes).unwrap();

    let player = resumed.player();
    let world = resumed.world();
    let reference = world.reference_ref(player).unwrap();
    assert_eq!(reference.cell, Some(cell));
    assert_eq!(reference.position, [4096.0, -8192.0, 128.5]);
    assert_eq!(reference.rotation, [0.25, 1.5, -2.0]);
}

#[test]
fn sav_19_missing_player_cell_remains_missing_after_native_save_load() {
    let mut restored = Game::new(load_order());
    let player = restored.player();
    {
        let mut world = restored.world_mut();
        let reference = world.reference(player).unwrap();
        reference.position = [1.0, 2.0, 3.0];
        reference.rotation = [0.0; 3];
    }

    let bytes = restored.save_state().unwrap();
    let mut resumed = Game::new(load_order());
    resumed.load_state(&bytes).unwrap();

    let player = resumed.player();
    assert_eq!(resumed.world().reference_ref(player).unwrap().cell, None);
}
