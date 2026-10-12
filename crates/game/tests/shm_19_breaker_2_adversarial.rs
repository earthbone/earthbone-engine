// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{EventSink, LevelIncrease, NotifyControl};
use soe_esm::LoadOrder;
use soe_game::World;

struct Capture(Rc<RefCell<Vec<LevelIncrease>>>);

impl EventSink<LevelIncrease> for Capture {
    fn process(&mut self, event: &LevelIncrease) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn shm_19_level_events_follow_each_current_live_player_level() {
    let mut world = World::new(LoadOrder::new());
    let seen = Rc::new(RefCell::new(Vec::new()));
    world
        .commonlib_event_sources()
        .level_increase()
        .add_sink(Box::new(Capture(seen.clone())));

    world.player_level = 17;
    assert_eq!(world.commonlib_level_increase(), Ok(1));
    world.player_level = 18;
    assert_eq!(world.commonlib_level_increase(), Ok(1));

    assert_eq!(seen.borrow().len(), 2);
    assert_eq!(seen.borrow()[0].player, seen.borrow()[1].player);
    assert_eq!(seen.borrow()[0].new_level, 17);
    assert_eq!(seen.borrow()[1].new_level, 18);
}
