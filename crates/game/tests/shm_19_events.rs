// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{
    DisarmedEvent, DragonSoulsGained, EventSink, FormIdentity, LevelIncrease, LocationDiscovery,
    NotifyControl,
};
use soe_esm::{GlobalFormId, LoadOrder};
use soe_game::World;
use std::cell::RefCell;
use std::rc::Rc;

struct Capture<T>(Rc<RefCell<Vec<T>>>);

impl<T: Clone> EventSink<T> for Capture<T> {
    fn process(&mut self, event: &T) -> NotifyControl {
        self.0.borrow_mut().push(event.clone());
        NotifyControl::Continue
    }
}

#[test]
fn shm_19_disarmed_source_receives_load_order_actor_identities() {
    let mut world = World::new(LoadOrder::new());
    let seen = Rc::new(RefCell::new(Vec::<DisarmedEvent>::new()));
    world
        .commonlib_event_sources()
        .disarmed()
        .add_sink(Box::new(Capture(seen.clone())));
    assert_eq!(
        world.commonlib_disarmed(
            GlobalFormId {
                file: 2,
                object: 0x123
            },
            GlobalFormId {
                file: 3,
                object: 0x456
            },
        ),
        1
    );
    assert_eq!(
        *seen.borrow(),
        [DisarmedEvent {
            source: FormIdentity((2_u64 << 32) | 0x123),
            target: FormIdentity((3_u64 << 32) | 0x456),
        }]
    );
}

#[test]
fn shm_19_dragon_souls_source_receives_awarded_amount() {
    let mut world = World::new(LoadOrder::new());
    let seen = Rc::new(RefCell::new(Vec::<DragonSoulsGained>::new()));
    world
        .commonlib_event_sources()
        .dragon_souls_gained()
        .add_sink(Box::new(Capture(seen.clone())));
    assert_eq!(world.commonlib_dragon_souls_gained(2.5), 1);
    assert_eq!(*seen.borrow(), [DragonSoulsGained { souls: 2.5 }]);
}

#[test]
fn shm_19_level_source_reads_current_world_player_and_level() {
    let mut world = World::new(LoadOrder::new());
    world.player_level = 17;
    let seen = Rc::new(RefCell::new(Vec::<LevelIncrease>::new()));
    world
        .commonlib_event_sources()
        .level_increase()
        .add_sink(Box::new(Capture(seen.clone())));
    assert_eq!(world.commonlib_level_increase(), Ok(1));
    assert_eq!(
        *seen.borrow(),
        [LevelIncrease {
            player: FormIdentity((u64::from(0_u16) << 32) | 0x14),
            new_level: 17,
        }]
    );
}

#[test]
fn shm_19_location_source_receives_marker_and_worldspace() {
    let mut world = World::new(LoadOrder::new());
    let seen = Rc::new(RefCell::new(Vec::<LocationDiscovery>::new()));
    world
        .commonlib_event_sources()
        .location_discovery()
        .add_sink(Box::new(Capture(seen.clone())));
    assert_eq!(
        world.commonlib_location_discovery(
            GlobalFormId {
                file: 4,
                object: 0x10
            },
            Some(GlobalFormId {
                file: 1,
                object: 0x20
            }),
        ),
        1
    );
    assert_eq!(
        *seen.borrow(),
        [LocationDiscovery {
            map_marker: FormIdentity((4_u64 << 32) | 0x10),
            worldspace_id: Some(((1_u64 << 32) | 0x20).to_string()),
        }]
    );
}

#[test]
fn shm_19_level_overflow_rejects_without_dispatch() {
    let mut world = World::new(LoadOrder::new());
    world.player_level = u32::from(u16::MAX) + 1;
    let seen = Rc::new(RefCell::new(Vec::<LevelIncrease>::new()));
    world
        .commonlib_event_sources()
        .level_increase()
        .add_sink(Box::new(Capture(seen.clone())));
    assert!(world.commonlib_level_increase().is_err());
    assert!(seen.borrow().is_empty());
}

#[test]
fn shm_19_world_rejects_unsupported_binary_hooks_explicitly() {
    let world = World::new(LoadOrder::new());
    assert_eq!(
        world.reject_commonlib_hook("write_vfunc").unwrap_err().name,
        "write_vfunc"
    );
}

#[test]
fn shm_19_player_dragon_soul_modification_emits_only_accepted_gains() {
    let mut world = World::new(LoadOrder::new());
    let player = world.player();
    let seen = Rc::new(RefCell::new(Vec::<DragonSoulsGained>::new()));
    world
        .commonlib_event_sources()
        .dragon_souls_gained()
        .add_sink(Box::new(Capture(seen.clone())));

    world.mod_actor_value(player, "dragonsouls", 2.5).unwrap();
    world.mod_actor_value(player, "dragonsouls", -1.0).unwrap();

    assert_eq!(world.actor_value(player, "dragonsouls"), 1.5);
    assert_eq!(*seen.borrow(), [DragonSoulsGained { souls: 2.5 }]);
}

#[test]
fn shm_19_add_to_map_emits_once_after_reference_state_changes() {
    let mut world = World::new(LoadOrder::new());
    let marker = world.player();
    let seen = Rc::new(RefCell::new(Vec::<LocationDiscovery>::new()));
    world
        .commonlib_event_sources()
        .location_discovery()
        .add_sink(Box::new(Capture(seen.clone())));

    world.add_to_map(marker, true);
    world.add_to_map(marker, false);

    let state = world.reference_ref(marker).unwrap();
    assert!(state.on_map);
    assert!(!state.fast_travel);
    assert_eq!(
        *seen.borrow(),
        [LocationDiscovery {
            map_marker: FormIdentity(0x14),
            worldspace_id: None,
        }]
    );
}
