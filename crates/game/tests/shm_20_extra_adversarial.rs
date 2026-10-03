// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{EventSink, NotifyControl, SpellsLearned};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

struct Capture(Rc<RefCell<Vec<SpellsLearned>>>);

impl EventSink<SpellsLearned> for Capture {
    fn process(&mut self, event: &SpellsLearned) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn learning_a_spell_for_an_npc_does_not_emit_the_player_event() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"SPEL"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Shm20Learner.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let npc = id(0x900);
    game.world_mut().actor(npc).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    game.world_mut()
        .commonlib_event_sources()
        .spells_learned()
        .add_sink(Box::new(Capture(seen.clone())));

    assert!(game.world_mut().add_actor_spell(npc, id(0x801)));
    assert!(seen.borrow().is_empty());
    assert!(game.world_mut().add_actor_spell(player, id(0x801)));
    assert_eq!(seen.borrow().len(), 1);
}
