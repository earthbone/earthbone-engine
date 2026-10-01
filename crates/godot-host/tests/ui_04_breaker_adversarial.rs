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

fn quest(form: u32, quest_type: u32, name: &str) -> Record {
    let mut data = vec![0; 12];
    data[8..12].copy_from_slice(&quest_type.to_le_bytes());
    Record::new(tag(*b"QUST"), form)
        .zsub(tag(*b"FULL"), name)
        .sub(tag(*b"DNAM"), data)
        .sub(tag(*b"QOBJ"), 4_u16.to_le_bytes())
        .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
        .zsub(tag(*b"NNAM"), "Return to the Jarl")
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(quest(0x100, 0, "Hidden Quest"));
    plugin.push_top(quest(0x101, 6, "Miscellaneous Quest"));
    let mut order = LoadOrder::new();
    order.add("Ui04Breaker.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        for form in [0x100, 0x101] {
            world.quest_state(id(form)).unwrap().active = true;
            world.objective(id(form), 4).unwrap().displayed = true;
        }
    }
    game
}

#[test]
fn ui_04_miscellaneous_quest_hides_title_but_keeps_displayed_objective() {
    let game = game();

    let entries = quest_journal::entries(&game);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].quest, id(0x101));
    assert_eq!(entries[0].title, None);
    assert_eq!(entries[0].objectives[0].text, "Return to the Jarl");
}

#[test]
fn ui_04_type_zero_quest_stays_out_of_journal_after_state_update() {
    let game = game();
    assert_eq!(quest_journal::entries(&game).len(), 1);

    game.world_mut().quest_state(id(0x100)).unwrap().active = false;
    game.world_mut().quest_state(id(0x100)).unwrap().active = true;

    assert_eq!(quest_journal::entries(&game).len(), 1);
    assert_eq!(quest_journal::entries(&game)[0].quest, id(0x101));
}
