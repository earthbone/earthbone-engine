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

fn quest(form: u32, name: &str, quest_type: u32, objectives: &[(u16, &str)]) -> Record {
    let mut dnam = vec![0; 12];
    dnam[8..12].copy_from_slice(&quest_type.to_le_bytes());
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
    order.add("UiQuestJournal.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ui_04_lists_active_journal_quests_and_hides_miscellaneous_titles() {
    let game = game([
        quest(
            0x100,
            "Main Quest",
            1,
            &[(10, "Find the key"), (11, "Hidden objective")],
        ),
        quest(0x101, "Misc Quest", 6, &[(20, "Find the clue")]),
        quest(0x102, "No Journal Quest", 0, &[(30, "Hidden")]),
        quest(0x103, "Inactive Quest", 1, &[(40, "Not active")]),
    ]);
    {
        let mut world = game.world_mut();
        for form in [0x100, 0x101, 0x102] {
            world.quest_state(id(form)).unwrap().active = true;
        }
        for (form, objective) in [(0x100, 10), (0x101, 20), (0x102, 30)] {
            world.objective(id(form), objective).unwrap().displayed = true;
        }
    }

    let entries = quest_journal::entries(&game);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].title.as_deref(), Some("Main Quest"));
    assert_eq!(
        entries[0].objectives.len(),
        1,
        "only displayed objectives render"
    );
    assert_eq!(entries[0].objectives[0].text, "Find the key");
    assert_eq!(
        entries[1].title, None,
        "miscellaneous quest names are hidden"
    );
    assert_eq!(entries[1].objectives[0].text, "Find the clue");
}

#[test]
fn ui_04_journal_rows_follow_shared_objective_state_after_stage_changes() {
    let mut game = game([quest(
        0x200,
        "The Task",
        8,
        &[(10, "Open the door"), (20, "Speak to the guard")],
    )]);
    assert!(game.set_stage(id(0x200), 10));
    {
        let mut world = game.world_mut();
        world.objective(id(0x200), 10).unwrap().displayed = true;
    }
    let first = quest_journal::entries(&game);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].objectives.len(), 1);
    assert_eq!(first[0].objectives[0].text, "Open the door");

    assert!(game.set_stage(id(0x200), 20));
    {
        let mut world = game.world_mut();
        world.objective(id(0x200), 10).unwrap().displayed = false;
        let next = world.objective(id(0x200), 20).unwrap();
        next.displayed = true;
        next.completed = true;
    }
    let second = quest_journal::entries(&game);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].objectives.len(), 1);
    assert_eq!(second[0].objectives[0].text, "Speak to the guard");
    assert!(second[0].objectives[0].completed);
    assert!(!second[0].objectives[0].failed);
}

#[test]
fn ui_04_failed_objectives_keep_their_visible_status() {
    let game = game([quest(0x300, "Failed Quest", 1, &[(10, "Escape")])]);
    {
        let mut world = game.world_mut();
        world.quest_state(id(0x300)).unwrap().active = true;
        let objective = world.objective(id(0x300), 10).unwrap();
        objective.displayed = true;
        objective.failed = true;
    }

    let entries = quest_journal::entries(&game);

    assert!(entries[0].objectives[0].failed);
    assert!(!entries[0].objectives[0].completed);
}
