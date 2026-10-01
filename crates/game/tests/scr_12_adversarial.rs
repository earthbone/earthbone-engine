// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-12 head-tracking state round-trip regression.

use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn game_with_actor() -> Game {
    let actor = npc::build_record(
        0x700,
        "GuardBase",
        "Guard",
        npc::Config::default(),
        npc::Stats::default(),
    );
    let cell = Record::new(T(b"CELL"), 0x701)
        .zsub(T(b"EDID"), "Cell")
        .sub(T(b"DATA"), [1u8, 0]);
    let reference = Record::new(T(b"ACHR"), 0x702)
        .zsub(T(b"EDID"), "GuardRef")
        .sub(T(b"NAME"), 0x700u32.to_le_bytes())
        .sub(T(b"DATA"), vec![0; 24]);

    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(actor);
    let cell_index = plugin.push_record(cell);
    let reference_index = plugin.push_record(reference);
    let mut temporary = Group::new(GroupKind::CellTemporary(0x701));
    temporary.entries.push(Entry::Record(reference_index));
    let mut children = Group::new(GroupKind::CellChildren(0x701));
    children.entries.push(Entry::Group(temporary));
    let mut subblock = Group::new(GroupKind::InteriorSubBlock(0));
    subblock.entries.push(Entry::Record(cell_index));
    subblock.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(subblock));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);

    let mut load_order = LoadOrder::new();
    load_order
        .add("SCR12.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(load_order)
}

#[test]
fn scr_12_unset_and_explicit_head_tracking_survive_save_load() {
    let game = game_with_actor();
    let actor = game.form("GuardRef").unwrap();
    assert_eq!(game.world_mut().head_tracking(actor), Some(true));
    game.world_mut().set_head_tracking(actor, false).unwrap();
    assert_eq!(game.world_mut().head_tracking(actor), Some(false));
    let saved = game.world_mut().save_state();
    let loaded = game_with_actor();
    loaded.world_mut().load_state(&saved).unwrap();
    let loaded_actor = loaded.form("GuardRef").unwrap();
    assert_eq!(loaded.world_mut().head_tracking(loaded_actor), Some(false));
}
