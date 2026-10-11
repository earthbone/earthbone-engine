// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! CMB-27 check for repeated hits with identical stagger state.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::Hand, Game};
use soe_godot_host::{combat::MeleeContact, godot_actor::{StaggerGraphEvent, StaggerTracker}};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_stagger_weapon() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for slot in [0xb00_u32, 0xb01] {
        plugin.push_top(Record::new(Tag(*b"EQUP"), slot).sub(Tag(*b"DATA"), 0_u32.to_le_bytes()));
    }
    let mut parents = 0xb00_u32.to_le_bytes().to_vec();
    parents.extend_from_slice(&0xb01_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"EQUP"), 0xb02)
            .sub(Tag(*b"DATA"), 0_u32.to_le_bytes())
            .sub(Tag(*b"PNAM"), parents),
    );
    let mut defaults = b"LHEQ".to_vec();
    defaults.extend_from_slice(&0xb00_u32.to_le_bytes());
    defaults.extend_from_slice(b"RHEQ");
    defaults.extend_from_slice(&0xb01_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"DOBJ"), 0x31).sub(Tag(*b"DNAM"), defaults));

    let mut data = vec![0; 10];
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1_f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&(-1_i32).to_le_bytes());
    dnam[88..92].copy_from_slice(&(-1_i32).to_le_bytes());
    dnam[96..100].copy_from_slice(&1_f32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0xa07)
            .sub(Tag(*b"DATA"), data)
            .sub(Tag(*b"DNAM"), dnam)
            .sub(Tag(*b"ETYP"), 0xb02_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("RepeatedHit.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert_eq!(game.player(), id(0x14));
    for actor in [id(0x14), id(0x900)] {
        game.world_mut().set_actor_value(actor, "health", 100.0).unwrap();
    }
    game.world_mut().add_item(id(0x14), id(0xa07), 1);
    game.world_mut().equip_hand_weapon(id(0x14), Hand::Right, id(0xa07)).unwrap();
    game
}

#[test]
fn cmb_27_identical_same_tick_hits_each_start_the_authored_reaction() {
    let attacker = id(0x14);
    let target = id(0x900);
    let mut game = game_with_stagger_weapon();
    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();

    let mut first = MeleeContact::default();
    first.begin(&game, attacker, Hand::Right).unwrap();
    let hit = first.event(&mut game, "HitFrame", |_| Ok(Some(target))).unwrap();
    assert!(hit.is_some());
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Start, 10.0))
    );

    let mut second = MeleeContact::default();
    second.begin(&game, attacker, Hand::Right).unwrap();
    let hit = second.event(&mut game, "HitFrame", |_| Ok(Some(target))).unwrap();
    assert!(hit.is_some());
    assert_eq!(game.world_mut().actor_value(target, "health"), 80.0);
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Start, 10.0)),
        "a second qualifying hit must notify the authored graph even when its deadline and magnitude match"
    );
}
