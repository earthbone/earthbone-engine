// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for the live SHM-19 event producers.

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{
    DisarmedEvent, DragonSoulsGained, EventSink, FormIdentity, LevelIncrease, LocationDiscovery,
    NotifyControl,
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

use crate::support::magic_adversarial::{
    actor, cast, effect, id, put, spell, BASE, CASTER as ACTOR, TARGET as OTHER,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EFFECT: u32 = 0x400;
const SPELL: u32 = 0x300;

fn game(records: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"ACBS"), vec![0; 24]));
    for form in [ACTOR, OTHER] {
        plugin.push_top(actor(form, [0.0; 3]));
    }
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Events.esm", plugin).unwrap();
    let game = Game::new(order);
    for form in [ACTOR, OTHER] {
        for name in ["health", "magicka"] {
            game.world_mut()
                .set_actor_value(id(form), name, 100.0)
                .unwrap();
        }
    }
    game
}

fn disarm_fixture() -> Game {
    const RIGHT_SLOT: u32 = 0x701;
    const LEFT_SLOT: u32 = 0x703;
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&12_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 40, 0x2000);
    put(&mut dnam, 76, 6);
    put(&mut dnam, 88, u32::MAX);
    let mut slots = u32::from_le_bytes(*b"LHEQ").to_le_bytes().to_vec();
    slots.extend_from_slice(&LEFT_SLOT.to_le_bytes());
    slots.extend_from_slice(&u32::from_le_bytes(*b"RHEQ").to_le_bytes());
    slots.extend_from_slice(&RIGHT_SLOT.to_le_bytes());
    game(vec![
        effect(EFFECT, 9, 0, 24),
        spell(SPELL, EFFECT, 10, 5.0, 1),
        Record::new(T(b"WEAP"), 0x700)
            .sub(T(b"DATA"), data)
            .sub(T(b"DNAM"), dnam)
            .sub(T(b"ETYP"), RIGHT_SLOT.to_le_bytes()),
        Record::new(T(b"DOBJ"), 0x702).sub(T(b"DNAM"), slots),
        Record::new(T(b"EQUP"), RIGHT_SLOT).sub(T(b"DATA"), 0_u32.to_le_bytes()),
        Record::new(T(b"EQUP"), LEFT_SLOT).sub(T(b"DATA"), 0_u32.to_le_bytes()),
    ])
}

struct Capture<T>(Rc<RefCell<Vec<T>>>);

impl<T: Clone> EventSink<T> for Capture<T> {
    fn process(&mut self, event: &T) -> NotifyControl {
        self.0.borrow_mut().push(event.clone());
        NotifyControl::Continue
    }
}

fn identity(form: GlobalFormId) -> FormIdentity {
    FormIdentity((u64::from(form.file) << 32) | u64::from(form.object))
}

#[test]
fn shm_19_disarm_reports_distinct_live_caster_and_target() {
    use soe_game::combat::Hand;

    const WEAPON: u32 = 0x700;
    let mut game = disarm_fixture();
    game.world_mut().actor(id(OTHER)).unwrap().level = 5;
    game.world_mut().add_item(id(OTHER), id(WEAPON), 1);
    game.world_mut()
        .equip_hand_weapon(id(OTHER), Hand::Right, id(WEAPON))
        .unwrap();
    let seen = Rc::new(RefCell::new(Vec::<DisarmedEvent>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .disarmed()
        .add_sink(Box::new(Capture(seen.clone())));

    cast(&mut game, SPELL, OTHER);

    assert_eq!(game.world_mut().item_count(id(OTHER), id(WEAPON)), 0);
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(WEAPON)), 0);
    assert_eq!(
        *seen.borrow(),
        [DisarmedEvent {
            source: identity(id(ACTOR)),
            target: identity(id(OTHER)),
        }]
    );
}

#[test]
fn shm_19_level_increase_reports_each_crossed_level_in_order() {
    let mut game = game(vec![
        Record::new(T(b"GMST"), 0x800)
            .sub(T(b"EDID"), b"fXPLevelUpBase\0".to_vec())
            .sub(T(b"DATA"), 10.0f32.to_le_bytes()),
        Record::new(T(b"GMST"), 0x801)
            .sub(T(b"EDID"), b"fXPLevelUpMult\0".to_vec())
            .sub(T(b"DATA"), 5.0f32.to_le_bytes()),
    ]);
    let player = game.player();
    game.world_mut().actor(player).unwrap();
    let seen = Rc::new(RefCell::new(Vec::<LevelIncrease>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .level_increase()
        .add_sink(Box::new(Capture(seen.clone())));

    game.world_mut()
        .advance_character_experience(player, 40.0)
        .unwrap();

    assert_eq!(game.world().actor_ref(player).unwrap().level, 3);
    assert_eq!(
        *seen.borrow(),
        [
            LevelIncrease {
                player: identity(player),
                new_level: 2,
            },
            LevelIncrease {
                player: identity(player),
                new_level: 3,
            },
        ]
    );
}

#[test]
fn shm_19_soul_awards_ignore_non_player_changes_and_negative_deltas() {
    let mut game = game(vec![]);
    let player = game.player();
    game.world_mut().actor(id(OTHER)).unwrap();
    let seen = Rc::new(RefCell::new(Vec::<DragonSoulsGained>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .dragon_souls_gained()
        .add_sink(Box::new(Capture(seen.clone())));

    game.world_mut()
        .mod_actor_value(id(OTHER), "dragonsouls", 4.0)
        .unwrap();
    game.world_mut()
        .mod_actor_value(player, "dragonsouls", 2.5)
        .unwrap();
    game.world_mut()
        .mod_actor_value(player, "dragonsouls", -1.0)
        .unwrap();

    assert_eq!(game.world_mut().actor_value(id(OTHER), "dragonsouls"), 4.0);
    assert_eq!(game.world_mut().actor_value(player, "dragonsouls"), 1.5);
    assert_eq!(*seen.borrow(), [DragonSoulsGained { souls: 2.5 }]);
}

#[test]
fn shm_19_map_discovery_resolves_the_reference_cell_worldspace() {
    const CELL: u32 = 0x800;
    const WORLD: u32 = 0x900;
    let mut cell = Record::new(T(b"CELL"), CELL);
    cell.placement.world = Some(WORLD);
    let mut game = game(vec![cell, Record::new(T(b"WRLD"), WORLD)]);
    let marker = game.player();
    game.world_mut().reference(marker).unwrap().cell = Some(id(CELL));
    let seen = Rc::new(RefCell::new(Vec::<LocationDiscovery>::new()));
    game.world_mut()
        .commonlib_event_sources()
        .location_discovery()
        .add_sink(Box::new(Capture(seen.clone())));

    game.world_mut().add_to_map(marker, true);

    assert_eq!(
        *seen.borrow(),
        [LocationDiscovery {
            map_marker: identity(marker),
            worldspace_id: Some(identity(id(WORLD)).0.to_string()),
        }]
    );
}

#[test]
fn shm_19_map_discovery_of_missing_reference_is_silent() {
    let mut world = soe_game::World::new(LoadOrder::new());
    let seen = Rc::new(RefCell::new(Vec::<LocationDiscovery>::new()));
    world
        .commonlib_event_sources()
        .location_discovery()
        .add_sink(Box::new(Capture(seen.clone())));

    world.add_to_map(
        GlobalFormId {
            file: 7,
            object: 0xdead,
        },
        true,
    );

    assert!(seen.borrow().is_empty());
}

#[test]
fn shm_19_unsupported_hook_rejection_preserves_the_hook_name() {
    let world = soe_game::World::new(LoadOrder::new());
    for name in ["write_vfunc", "patch_callsite", "patch_branch"] {
        assert_eq!(world.reject_commonlib_hook(name).unwrap_err().name, name);
    }
}
