// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Reject malformed DEST stages using the xEdit stage layout.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::ObjectId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture(dstd: Vec<u8>, include_end_marker: bool) -> Game {
    let dest = [100i32.to_le_bytes().as_slice(), &[1, 0, 0, 0]].concat();
    let mut base = Record::new(T(b"ACTI"), BASE)
        .sub(T(b"DEST"), dest)
        .sub(T(b"DSTD"), dstd)
        .sub(T(b"DMDL"), b"meshes/stage.nif\0".to_vec());
    if include_end_marker {
        base = base.sub(T(b"DSTF"), Vec::new());
    }
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(base);
    let mut order = LoadOrder::new();
    order.add("Cmb37Malformed.esm", plugin).unwrap();
    Game::new(order)
}

fn reference(game: &mut Game) -> GlobalFormId {
    let player = game.player();
    game.world_mut().place_at(id(BASE), player, false)
}

fn damage(game: &mut Game, reference: GlobalFormId) {
    game.world_mut()
        .damage_object(reference, 100.0, &mut |_, _| ObjectId(1));
}

fn stage() -> Vec<u8> {
    let mut data = vec![50, 0, 1, 0];
    data.extend_from_slice(&[0; 16]);
    data
}

#[test]
fn cmb_37_missing_stage_end_marker_is_rejected_without_state_change() {
    let mut game = fixture(stage(), false);
    let reference = reference(&mut game);

    damage(&mut game, reference);

    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert_eq!(
        game.world_mut()
            .reference(reference)
            .unwrap()
            .destruction_health,
        None
    );
    assert!(game.world().pending.is_empty());
}

#[test]
fn cmb_37_wrong_sized_stage_data_is_rejected_without_state_change() {
    let mut malformed = stage();
    malformed.pop();
    let mut game = fixture(malformed, true);
    let reference = reference(&mut game);

    damage(&mut game, reference);

    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert_eq!(
        game.world_mut()
            .reference(reference)
            .unwrap()
            .destruction_health,
        None
    );
    assert!(game.world().pending.is_empty());
}
