// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{EventSink, LocationDiscovery, NotifyControl};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

struct Capture(Rc<RefCell<Vec<LocationDiscovery>>>);

impl EventSink<LocationDiscovery> for Capture {
    fn process(&mut self, event: &LocationDiscovery) -> NotifyControl {
        self.0.borrow_mut().push(event.clone());
        NotifyControl::Continue
    }
}

#[test]
fn discovery_is_once_even_when_fast_travel_is_enabled_later() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x800));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x801).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Shm19Map.esp", plugin).unwrap();
    let game = Game::new(order);
    let seen = Rc::new(RefCell::new(Vec::new()));
    game.world_mut()
        .commonlib_event_sources()
        .location_discovery()
        .add_sink(Box::new(Capture(seen.clone())));

    game.world_mut().add_to_map(id(0x801), false);
    game.world_mut().add_to_map(id(0x801), true);

    assert_eq!(seen.borrow().len(), 1);
    assert!(game.world().reference_ref(id(0x801)).unwrap().fast_travel);
}
