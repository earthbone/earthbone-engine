// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::common::mag_adversarial as support;
use soe_esm::Record;
use support::*;

#[test]
fn equipping_unenchanted_armor_into_an_overlapping_slot_removes_only_replaced_source_effect() {
    let mut mgef = effect(2, -1);
    mgef.subrecords[0].data[68..72].copy_from_slice(&24u32.to_le_bytes());
    let mut enit = vec![0; 36];
    put(&mut enit, 20, 6);
    let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x500u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(mgef);
    plugin.push_top(
        Record::new(T(b"ENCH"), 0x700)
            .sub(T(b"ENIT"), enit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    10.0f32.to_le_bytes().as_slice(),
                    &0u32.to_le_bytes(),
                    &0u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    let armor = |form: u32, enchant: u32| {
        Record::new(T(b"ARMO"), form)
            .sub(
                T(b"DATA"),
                [0u32.to_le_bytes().as_slice(), &1.0f32.to_le_bytes()].concat(),
            )
            .sub(T(b"DNAM"), 100u32.to_le_bytes())
            .sub(
                T(b"BOD2"),
                [1u32.to_le_bytes().as_slice(), &2u32.to_le_bytes()].concat(),
            )
            .sub(T(b"EITM"), enchant.to_le_bytes())
    };
    plugin.push_top(armor(0x701, 0x700));
    plugin.push_top(armor(0x702, 0));
    let mut order = soe_esm::LoadOrder::new();
    order.add("Armor.esm", plugin).unwrap();
    let mut game = soe_game::Game::new(order);
    game.world_mut()
        .set_actor_value(id(ACTOR), "health", 100.0)
        .unwrap();
    let world = game.shared_world();
    let vm = &mut game.vm;
    world
        .borrow_mut()
        .equip_item_with_effects(id(ACTOR), id(0x701), &mut |name, class| {
            vm.create_object_of(name, class)
        })
        .unwrap();
    world
        .borrow_mut()
        .equip_item_with_effects(id(ACTOR), id(0x702), &mut |name, class| {
            vm.create_object_of(name, class)
        })
        .unwrap();
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
}
