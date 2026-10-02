// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Check the event argument contract documented by the Creation Kit.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Pending};
use soe_papyrus::{ObjectId, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn cmb_37_event_contains_old_and_current_stage() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut base = Record::new(T(b"ACTI"), 0x100).sub(
        T(b"DEST"),
        [100i32.to_le_bytes().as_slice(), &[1, 0, 0, 0]].concat(),
    );
    let mut stage = vec![50, 0, 0, 0];
    stage.extend_from_slice(&[0; 16]);
    base = base.sub(T(b"DSTD"), stage).sub(T(b"DSTF"), Vec::new());
    plugin.push_top(base);
    let mut order = LoadOrder::new();
    order.add("Cmb37Event.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let reference = game.world_mut().place_at(id(0x100), player, false);

    game.world_mut()
        .damage_object(reference, 60.0, &mut |_, _| ObjectId(1));

    let world = game.world();
    let event = world
        .pending
        .iter()
        .find_map(|pending| match pending {
            Pending::Event { name, args, .. } if name == "OnDestructionStageChanged" => Some(args),
            _ => None,
        })
        .expect("stage change event");
    assert_eq!(event, &[Value::Int(-1), Value::Int(0)]);
}
