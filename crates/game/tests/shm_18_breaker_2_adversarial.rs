// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{ActorKill, EventSink, NotifyControl};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use std::{cell::RefCell, rc::Rc};

struct Capture(Rc<RefCell<Vec<ActorKill>>>);
impl EventSink<ActorKill> for Capture {
    fn process(&mut self, event: &ActorKill) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn shm_18_repeated_kill_requests_emit_only_the_single_death_transition() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x100));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x200).sub(Tag(*b"NAME"), 0x100_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x201).sub(Tag(*b"NAME"), 0x100_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Events.esm", plugin).unwrap();
    let game = Game::new(order);
    let seen = Rc::new(RefCell::new(Vec::new()));
    game.world_mut()
        .actor_kill_events()
        .add_sink(Box::new(Capture(seen.clone())));
    let actor = GlobalFormId {
        file: 0,
        object: 0x200,
    };
    let killer = GlobalFormId {
        file: 0,
        object: 0x201,
    };
    assert!(game.world_mut().kill_actor(actor, Some(killer)));
    assert!(!game.world_mut().kill_actor(actor, Some(killer)));
    assert_eq!(seen.borrow().len(), 1);
}
