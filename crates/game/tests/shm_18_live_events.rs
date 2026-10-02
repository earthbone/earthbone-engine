// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_commonlib_shim::events::{ActorKill, CriticalHit, EventFormId, EventSink, NotifyControl};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fresh() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for object in [0x900, 0x901, 0x902] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"WEAP"), 0xA00));
    let mut order = LoadOrder::new();
    order.add("Events.esm", plugin).unwrap();
    Game::new(order)
}

struct Capture<T>(Rc<RefCell<Vec<T>>>);

impl<T: Clone> EventSink<T> for Capture<T> {
    fn process(&mut self, event: &T) -> NotifyControl {
        self.0.borrow_mut().push(event.clone());
        NotifyControl::Continue
    }
}

#[test]
fn shm_18_actor_kill_channel_follows_live_death_and_suppresses_silent_kills() {
    let game = fresh();
    let observed = Rc::new(RefCell::new(Vec::<ActorKill>::new()));
    game.world_mut()
        .actor_kill_events()
        .add_sink(Box::new(Capture(observed.clone())));

    assert!(game.world_mut().kill_actor(id(0x900), Some(id(0x901))));
    assert_eq!(
        &*observed.borrow(),
        &[ActorKill {
            killer: EventFormId(0x901),
            victim: EventFormId(0x900),
        }]
    );

    assert!(
        game.world_mut()
            .kill_actor_silent(id(0x901), Some(id(0x900)))
    );
    assert!(game.world_mut().kill_actor(id(0x902), None));
    assert_eq!(observed.borrow().len(), 1);
}

#[test]
fn shm_18_critical_hit_channel_validates_live_actor_and_weapon_forms() {
    let game = fresh();
    let observed = Rc::new(RefCell::new(Vec::<CriticalHit>::new()));
    game.world_mut()
        .critical_hit_events()
        .add_sink(Box::new(Capture(observed.clone())));

    game.world_mut()
        .report_critical_hit(id(0x900), id(0xA00), true)
        .unwrap();
    assert_eq!(
        &*observed.borrow(),
        &[CriticalHit {
            aggressor: EventFormId(0x900),
            weapon: EventFormId(0xA00),
            sneak_hit: true,
        }]
    );
    assert!(
        game.world_mut()
            .report_critical_hit(id(0x900), id(0x901), false)
            .is_err()
    );
    assert_eq!(observed.borrow().len(), 1);
}
