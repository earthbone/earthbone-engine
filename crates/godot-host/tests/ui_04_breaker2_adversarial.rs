// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::quest_journal;

const fn tag(bytes: [u8; 4]) -> Tag {
    Tag::new(&bytes)
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn ui_04_duplicate_objective_indexes_keep_both_authored_rows() {
    let mut dnam = vec![0; 12];
    dnam[8..12].copy_from_slice(&1_u32.to_le_bytes());
    let quest = Record::new(tag(*b"QUST"), 0x100)
        .zsub(tag(*b"FULL"), "Repeated Objective")
        .sub(tag(*b"DNAM"), dnam)
        .sub(tag(*b"INDX"), [10_u8, 0, 0])
        .sub(tag(*b"QOBJ"), 42_u16.to_le_bytes())
        .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
        .zsub(tag(*b"NNAM"), "First route")
        .sub(tag(*b"QOBJ"), 42_u16.to_le_bytes())
        .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
        .zsub(tag(*b"NNAM"), "Second route");
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(quest);
    let mut order = LoadOrder::new();
    order.add("Ui04RepeatedObjective.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.set_stage(id(0x100), 10);
    game.world_mut().objective(id(0x100), 42).unwrap().displayed = true;

    let entries = quest_journal::entries(&game);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].objectives.len(), 2);
    assert_eq!(entries[0].objectives[0].text, "First route");
    assert_eq!(entries[0].objectives[1].text, "Second route");
}

#[test]
fn ui_04_journal_projection_tracks_objective_visibility_and_completion_each_read() {
    let mut dnam = vec![0; 12];
    dnam[8..12].copy_from_slice(&1_u32.to_le_bytes());
    let quest = Record::new(tag(*b"QUST"), 0x101)
        .zsub(tag(*b"FULL"), "Refresh Objective")
        .sub(tag(*b"DNAM"), dnam)
        .sub(tag(*b"INDX"), [10_u8, 0, 0])
        .sub(tag(*b"QOBJ"), 10_u16.to_le_bytes())
        .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
        .zsub(tag(*b"NNAM"), "Open the gate");
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(quest);
    let mut order = LoadOrder::new();
    order.add("Ui04Refresh.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert!(game.set_stage(id(0x101), 10));

    assert!(quest_journal::entries(&game)[0].objectives.is_empty());
    {
        let mut world = game.world_mut();
        let objective = world.objective(id(0x101), 10).unwrap();
        objective.displayed = true;
        objective.completed = true;
    }
    let displayed = quest_journal::entries(&game);
    assert_eq!(displayed[0].objectives[0].text, "Open the gate");
    assert!(displayed[0].objectives[0].completed);
}
