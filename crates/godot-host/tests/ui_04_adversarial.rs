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

fn quest(form: u32, name: &str, objectives: &[(u16, &str)]) -> Record {
    let mut dnam = vec![0; 12];
    dnam[8..12].copy_from_slice(&1_u32.to_le_bytes());
    let mut record = Record::new(tag(*b"QUST"), form)
        .zsub(tag(*b"FULL"), name)
        .sub(tag(*b"DNAM"), dnam);
    for (stage, _) in objectives {
        let mut index = stage.to_le_bytes().to_vec();
        index.push(0);
        record = record.sub(tag(*b"INDX"), index);
    }
    for (index, text) in objectives {
        record = record
            .sub(tag(*b"QOBJ"), index.to_le_bytes())
            .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
            .zsub(tag(*b"NNAM"), text);
    }
    record
}

fn game(records: impl IntoIterator<Item = Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("UiQuestJournalAdversarial.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ui_04_journal_reads_current_active_state_on_every_snapshot() {
    let game = game([quest(0x100, "Tracked Quest", &[(10, "Find the key")])]);
    {
        let mut world = game.world_mut();
        world.quest_state(id(0x100)).unwrap().active = true;
        world.objective(id(0x100), 10).unwrap().displayed = true;
    }
    let first = quest_journal::entries(&game);
    assert_eq!(first.len(), 1);

    game.world_mut().quest_state(id(0x100)).unwrap().active = false;

    assert!(quest_journal::entries(&game).is_empty());
    assert_eq!(first[0].title.as_deref(), Some("Tracked Quest"));
}

#[test]
fn ui_04_stage_changes_replace_rows_without_reusing_prior_objective_state() {
    let mut game = game([quest(
        0x200,
        "Changing Quest",
        &[(10, "Open the gate"), (20, "Meet the steward")],
    )]);
    assert!(game.set_stage(id(0x200), 10));
    {
        let mut world = game.world_mut();
        world.quest_state(id(0x200)).unwrap().active = true;
        let old = world.objective(id(0x200), 10).unwrap();
        old.displayed = true;
        old.completed = true;
    }
    let first = quest_journal::entries(&game);
    assert_eq!(first[0].objectives[0].text, "Open the gate");
    assert!(first[0].objectives[0].completed);

    assert!(game.set_stage(id(0x200), 20));
    {
        let mut world = game.world_mut();
        world.objective(id(0x200), 10).unwrap().displayed = false;
        let next = world.objective(id(0x200), 20).unwrap();
        next.displayed = true;
    }
    let second = quest_journal::entries(&game);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].objectives.len(), 1);
    assert_eq!(second[0].objectives[0].index, 20);
    assert_eq!(second[0].objectives[0].text, "Meet the steward");
    assert!(!second[0].objectives[0].completed);
    assert!(!second[0].objectives[0].failed);
}

#[test]
fn ui_04_visible_objective_reports_completion_and_failure_state_changes() {
    let game = game([quest(0x300, "Mutable Objective", &[(7, "Return")])]);
    {
        let mut world = game.world_mut();
        world.quest_state(id(0x300)).unwrap().active = true;
        world.objective(id(0x300), 7).unwrap().displayed = true;
    }
    assert!(!quest_journal::entries(&game)[0].objectives[0].completed);

    {
        let mut world = game.world_mut();
        let objective = world.objective(id(0x300), 7).unwrap();
        objective.completed = true;
        objective.failed = true;
    }
    let updated = quest_journal::entries(&game);
    assert!(updated[0].objectives[0].completed);
    assert!(updated[0].objectives[0].failed);
}
