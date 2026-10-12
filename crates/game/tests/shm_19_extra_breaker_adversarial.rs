// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{
    DisarmedEvent, EventSink, FormIdentity, LevelIncrease, LocationDiscovery, NotifyControl,
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

struct StopSink(Rc<RefCell<Vec<&'static str>>>);

impl EventSink<DisarmedEvent> for StopSink {
    fn process(&mut self, _: &DisarmedEvent) -> NotifyControl {
        self.0.borrow_mut().push("stop");
        NotifyControl::Stop
    }
}

struct ContinueSink(Rc<RefCell<Vec<&'static str>>>);

impl EventSink<DisarmedEvent> for ContinueSink {
    fn process(&mut self, _: &DisarmedEvent) -> NotifyControl {
        self.0.borrow_mut().push("continue");
        NotifyControl::Continue
    }
}

#[test]
fn shm_19_disarmed_stop_prevents_later_sink_from_receiving_event() {
    let mut world = World::new(LoadOrder::new());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let source = world.commonlib_event_sources().disarmed();
    source.add_sink(Box::new(StopSink(calls.clone())));
    source.add_sink(Box::new(ContinueSink(calls.clone())));

    let seen = world.commonlib_disarmed(id(0x123), id(0x456));

    assert_eq!(seen, 1);
    assert_eq!(*calls.borrow(), ["stop"]);
}

struct LevelCapture(Rc<RefCell<Vec<LevelIncrease>>>);

impl EventSink<LevelIncrease> for LevelCapture {
    fn process(&mut self, event: &LevelIncrease) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn shm_19_rejected_level_does_not_dispatch_or_change_live_level() {
    let mut world = World::new(LoadOrder::new());
    world.player_level = 22;
    let seen = Rc::new(RefCell::new(Vec::new()));
    world
        .commonlib_event_sources()
        .level_increase()
        .add_sink(Box::new(LevelCapture(seen.clone())));

    assert!(world
        .commonlib_level_increase_at(u32::from(u16::MAX) + 1)
        .is_err());

    assert_eq!(world.player_level, 22);
    assert!(seen.borrow().is_empty());
}

struct DiscoveryCapture(Rc<RefCell<Vec<LocationDiscovery>>>);

impl EventSink<LocationDiscovery> for DiscoveryCapture {
    fn process(&mut self, event: &LocationDiscovery) -> NotifyControl {
        self.0.borrow_mut().push(event.clone());
        NotifyControl::Continue
    }
}

#[test]
fn shm_19_discovery_preserves_absent_worldspace_and_is_once_only() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x800));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x801).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x802));
    let mut order = LoadOrder::new();
    order.add("Shm19NoWorldspace.esp", plugin).unwrap();
    let mut world = World::new(order);
    world.reference(id(0x801)).unwrap().cell = Some(id(0x802));

    let seen = Rc::new(RefCell::new(Vec::new()));
    world
        .commonlib_event_sources()
        .location_discovery()
        .add_sink(Box::new(DiscoveryCapture(seen.clone())));

    world.add_to_map(id(0x801), false);
    world.add_to_map(id(0x801), true);

    assert_eq!(
        *seen.borrow(),
        [LocationDiscovery {
            map_marker: FormIdentity(0x801),
            worldspace_id: None,
        }]
    );
    assert!(world.reference_ref(id(0x801)).unwrap().fast_travel);
}
