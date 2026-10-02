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

fn quest(form: u32) -> Record {
    let mut dnam = vec![0; 12];
    dnam[8..12].copy_from_slice(&1_u32.to_le_bytes());
    Record::new(tag(*b"QUST"), form)
        .zsub(tag(*b"FULL"), "Split Stage and Objective")
        .sub(tag(*b"DNAM"), dnam)
        .sub(tag(*b"INDX"), [10_u8, 0, 0])
        .sub(tag(*b"QOBJ"), 42_u16.to_le_bytes())
        .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
        .zsub(tag(*b"NNAM"), "Return to the Jarl")
}

fn game_with_quest() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(quest(0x100));
    let mut order = LoadOrder::new();
    order.add("Ui04BreakerBreaker.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ui_04_objective_index_is_independent_of_stage_and_display_state() {
    let mut game = game_with_quest();

    assert!(game.set_stage(id(0x100), 10));
    let before_display = quest_journal::entries(&game);
    assert_eq!(before_display.len(), 1);
    assert!(before_display[0].objectives.is_empty());

    let mut world = game.world_mut();
    let objective = world.objective(id(0x100), 42).unwrap();
    objective.displayed = true;
    objective.failed = true;
    drop(world);

    let rows = quest_journal::entries(&game);
    assert_eq!(rows[0].objectives.len(), 1);
    assert_eq!(rows[0].objectives[0].index, 42);
    assert_eq!(rows[0].objectives[0].text, "Return to the Jarl");
    assert!(rows[0].objectives[0].failed);
}

#[test]
fn ui_04_invalid_or_missing_stage_does_not_activate_a_quest() {
    let mut game = game_with_quest();

    assert!(!game.set_stage(id(0x100), 20));
    assert!(!game.set_stage(id(0x999), 10));
    assert!(quest_journal::entries(&game).is_empty());
}

#[test]
fn ui_04_hiding_objective_keeps_active_quest_and_removes_only_its_row() {
    let mut game = game_with_quest();
    assert!(game.set_stage(id(0x100), 10));
    {
        let mut world = game.world_mut();
        world.objective(id(0x100), 42).unwrap().displayed = true;
    }
    assert_eq!(quest_journal::entries(&game)[0].objectives.len(), 1);

    game.world_mut().objective(id(0x100), 42).unwrap().displayed = false;

    let rows = quest_journal::entries(&game);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title.as_deref(), Some("Split Stage and Objective"));
    assert!(rows[0].objectives.is_empty());
}
