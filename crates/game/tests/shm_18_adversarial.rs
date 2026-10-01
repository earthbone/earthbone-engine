// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{ActorKill, EventFormId, EventSink, NotifyControl};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use std::{cell::RefCell, rc::Rc};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn game() -> Game {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for object in [0x900, 0x901, 0x902] {
        p.push_top(Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()));
    }
    let mut order = LoadOrder::new();
    order.add("EventEdge.esm", p).unwrap();
    Game::new(order)
}
struct Capture(Rc<RefCell<Vec<ActorKill>>>);
impl EventSink<ActorKill> for Capture {
    fn process(&mut self, e: &ActorKill) -> NotifyControl {
        self.0.borrow_mut().push(*e);
        NotifyControl::Continue
    }
}

#[test]
fn repeated_death_does_not_duplicate_event_and_non_actor_killer_is_suppressed() {
    let game = game();
    let calls = Rc::new(RefCell::new(Vec::new()));
    game.world_mut()
        .actor_kill_events()
        .add_sink(Box::new(Capture(calls.clone())));
    assert!(game.world_mut().kill_actor(id(0x900), Some(id(0x901))));
    assert!(!game.world_mut().kill_actor(id(0x900), Some(id(0x900))));
    assert!(game.world_mut().kill_actor(id(0x902), Some(id(0x800))));
    assert_eq!(
        calls.borrow().as_slice(),
        &[ActorKill {
            killer: EventFormId(0x901),
            victim: EventFormId(0x900)
        }]
    );
}
