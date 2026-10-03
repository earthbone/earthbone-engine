// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial SHM-20 live event binding checks.

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{
    EventSink, NotifyControl, SkillIncrease, SpellsLearned, TESCellFullyLoadedEvent,
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

struct Capture<T>(Rc<RefCell<Vec<T>>>);

impl<T: Clone> EventSink<T> for Capture<T> {
    fn process(&mut self, event: &T) -> NotifyControl {
        self.0.borrow_mut().push(event.clone());
        NotifyControl::Continue
    }
}

fn actor_plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x800,
        "Learner",
        "Learner",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin
}

#[test]
fn shm_20_add_spell_rejects_missing_or_wrong_type_forms_without_notification() {
    let mut plugin = actor_plugin();
    plugin.push_top(Record::new(T(b"MGEF"), 0x801));
    let mut order = LoadOrder::new();
    order.add("Shm20InvalidSpell.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let seen = Rc::new(RefCell::new(Vec::<SpellsLearned>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .spells_learned()
        .add_sink(Box::new(Capture(seen.clone())));

    let missing_added = game.world_mut().add_actor_spell(player, id(0x802));
    let wrong_type_added = game.world_mut().add_actor_spell(player, id(0x801));

    assert_eq!(
        (missing_added, wrong_type_added),
        (false, false),
        "only a valid SpellItem can be learned"
    );
    assert!(seen.borrow().is_empty());
}

#[test]
fn shm_20_fully_loaded_cell_notifies_once_until_it_leaves_residency() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), 0x900));
    let mut order = LoadOrder::new();
    order.add("Shm20CellArrival.esp", plugin).unwrap();
    let game = Game::new(order);
    let seen = Rc::new(RefCell::new(Vec::<TESCellFullyLoadedEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .cell_fully_loaded()
        .add_sink(Box::new(Capture(seen.clone())));

    game.world_mut().attach_streaming_cells(&[id(0x900)]).unwrap();
    game.world_mut().attach_streaming_cells(&[id(0x900)]).unwrap();
    let repeated_attach_count = seen.borrow().len();

    game.world_mut().detach_streaming_cells(&[id(0x900)]).unwrap();
    game.world_mut().attach_streaming_cells(&[id(0x900)]).unwrap();
    assert_eq!(
        (repeated_attach_count, seen.borrow().len()),
        (1, 2),
        "resident attaches do not notify, later arrivals do"
    );
}

#[test]
fn shm_20_skill_increase_requires_a_player_rank_gain() {
    let mut plugin = actor_plugin();
    plugin.push_top(
        Record::new(T(b"AVIF"), 0x802)
            .sub(T(b"EDID"), b"AVOneHanded\0".to_vec())
            .sub(
                T(b"AVSK"),
                [2.0f32, 1.0, 0.2, 2.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(
        Record::new(T(b"GMST"), 0x803)
            .sub(T(b"EDID"), b"fSkillUseCurve\0".to_vec())
            .sub(T(b"DATA"), 2.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Shm20SkillEdges.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let seen = Rc::new(RefCell::new(Vec::<SkillIncrease>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .skill_increase()
        .add_sink(Box::new(Capture(seen.clone())));

    game.world_mut()
        .advance_skill(player, "onehanded", 0.01)
        .unwrap();
    assert!(game.world().skill_experience(player, "onehanded").unwrap() > 0.0);
    assert!(seen.borrow().is_empty(), "partial experience is not a rank gain");

}
