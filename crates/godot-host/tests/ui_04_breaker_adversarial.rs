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
fn ui_04_completed_displayed_objective_remains_a_live_journal_row() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut dnam = vec![0; 12];
    dnam[8..12].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(
        Record::new(tag(*b"QUST"), 0x100)
            .zsub(tag(*b"FULL"), "Quest")
            .sub(tag(*b"DNAM"), dnam)
            .sub(tag(*b"QOBJ"), 7_u16.to_le_bytes())
            .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
            .zsub(tag(*b"NNAM"), "Return to the Jarl"),
    );
    let mut order = LoadOrder::new();
    order.add("Journal.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        world.quest_state(id(0x100)).unwrap().active = true;
        let objective = world.objective(id(0x100), 7).unwrap();
        objective.displayed = true;
        objective.completed = true;
    }

    let entries = quest_journal::entries(&game);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].objectives.len(), 1);
    assert!(entries[0].objectives[0].completed);
}
