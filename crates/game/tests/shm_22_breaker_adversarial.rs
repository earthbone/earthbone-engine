// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_commonlib_shim::events::{EventSink, HitFlags, NotifyControl, TesHitEvent};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"RNAM"), 0x700_u32.to_le_bytes()));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, 6);
    put(&mut dnam, 88, u32::MAX);
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0xA00)
            .sub(Tag(*b"DATA"), data)
            .sub(Tag(*b"DNAM"), dnam),
    );
    let mut attack = vec![0; 44];
    put(&mut attack, 0, 1_f32.to_bits());
    put(&mut attack, 12, 4);
    put(&mut attack, 40, 1_f32.to_bits());
    plugin.push_top(
        Record::new(Tag(*b"RACE"), 0x700)
            .sub(Tag(*b"DATA"), vec![0; 124])
            .sub(Tag(*b"ATKD"), attack),
    );
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 3.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Shm22.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [0x900, 0x901] {
        let mut world = game.world_mut();
        world.set_actor_value(id(actor), "health", 100.).unwrap();
        world.set_actor_value(id(actor), "onehanded", 100.).unwrap();
        world.set_actor_value(id(actor), "stamina", 200.).unwrap();
        world.add_item(id(actor), id(0xA00), 1);
        world.actor(id(actor)).unwrap().equipped.insert(id(0xA00));
    }
    game
}

struct Capture(Rc<RefCell<Vec<TesHitEvent>>>);

impl EventSink<TesHitEvent> for Capture {
    fn process(&mut self, event: &TesHitEvent) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn committed_power_hit_preserves_commonlib_flag_and_live_identities() {
    let mut game = fixture();
    let seen = Rc::new(RefCell::new(Vec::new()));
    game.world_mut()
        .commonlib_event_sources()
        .hit()
        .add_sink(Box::new(Capture(seen.clone())));

    game.melee_hit(MeleeHit {
        attacker: id(0x900),
        target: id(0x901),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: true,
    })
    .unwrap();

    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0].target.0, 0x901);
    assert_eq!(seen.borrow()[0].cause.0, 0x900);
    assert_eq!(seen.borrow()[0].source.0, 0xA00);
    assert_eq!(seen.borrow()[0].projectile.0, 0);
    assert_eq!(seen.borrow()[0].flags, HitFlags::POWER_ATTACK);
}
