// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::support::magic_adversarial as support;

use soe_esm::{Record, Tag};
use support::*;

#[test]
fn mag_33_recasting_command_does_not_lose_original_owner_to_second_caster() {
    let mut plugin_effect = effect(0x300, 18, 0, 0);
    put(plugin_effect.subrecords[0].data.as_mut_slice(), 8, 0x202);
    let command = effect(0x301, 10, 0, 0);
    let summon_spell = spell(0x310, 0x300, 30, 0.0, 1);
    let command_spell = spell(0x311, 0x301, 10, 0.0, 1);
    let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    let mut acbs = vec![0; 24];
    put(&mut acbs, 0, 0x4000);
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x202).sub(Tag(*b"ACBS"), acbs));
    for actor in [CASTER, TARGET, OTHER] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor)
                .sub(Tag(*b"NAME"), BASE.to_le_bytes())
                .sub(Tag(*b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(plugin_effect);
    plugin.push_top(command);
    plugin.push_top(summon_spell);
    plugin.push_top(command_spell);
    let mut order = soe_esm::LoadOrder::new();
    order.add("CommandBreak.esm", plugin).unwrap();
    let mut game = soe_game::Game::new(order);
    let summon = {
        let shared = game.shared_world();
        let result = shared
            .borrow_mut()
            .cast_spell(
                id(0x310),
                id(CASTER),
                Some(id(CASTER)),
                Some(id(CASTER)),
                &mut |name, class| game.vm.create_object_of(name, class),
            )
            .unwrap();
        result[0]
    };
    let summoned = game
        .world()
        .active_effects()
        .find(|active| active.id == summon)
        .unwrap()
        .summoned
        .unwrap();
    let first = {
        let shared = game.shared_world();
        let result = shared
            .borrow_mut()
            .cast_spell(
                id(0x311),
                id(CASTER),
                Some(id(CASTER)),
                Some(summoned),
                &mut |name, class| game.vm.create_object_of(name, class),
            )
            .unwrap();
        result[0]
    };
    let second_caster = {
        let shared = game.shared_world();
        let result = shared
            .borrow_mut()
            .cast_spell(
                id(0x311),
                id(OTHER),
                Some(id(OTHER)),
                Some(summoned),
                &mut |name, class| game.vm.create_object_of(name, class),
            )
            .unwrap();
        result
    };
    assert!(second_caster.is_empty());
    let shared = game.shared_world();
    assert!(
        shared
            .borrow_mut()
            .dispel_effect(first, &mut |name, class| game
                .vm
                .create_object_of(name, class))
    );
    assert_eq!(
        game.world().reference_ref(summoned).unwrap().owner,
        Some(id(CASTER))
    );
}
