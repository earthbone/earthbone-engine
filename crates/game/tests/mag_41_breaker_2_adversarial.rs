// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/mod.rs"]
mod support;

use support::magic_adversarial as fixture;

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use fixture::*;

const ITEM: u32 = 0x220;
const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x310;

#[test]
fn mag_41_magicka_exhaustion_clears_the_transient_reference_hold() {
    let mut effect = effect(EFFECT, 20, 0, 0);
    put(effect.subrecords[0].data.as_mut_slice(), 80, 2);
    put(effect.subrecords[0].data.as_mut_slice(), 84, 0);
    let mut spell = spell(SPELL, EFFECT, 0, 1.0, 2);
    put(spell.subrecords[0].data.as_mut_slice(), 0, 100);
    put(spell.subrecords[0].data.as_mut_slice(), 4, 1);
    put(spell.subrecords[0].data.as_mut_slice(), 20, 0);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(Tag(*b"STAT"), ITEM));
    plugin.push_top(actor(CASTER, [0.0; 3]));
    plugin.push_top(effect);
    plugin.push_top(spell);
    let mut order = LoadOrder::new();
    order.add("TelekinesisExhaustionBreaker.esm", plugin).unwrap();
    let mut game = soe_game::Game::new(order);
    game.world_mut()
        .set_actor_value(id(CASTER), "magicka", 5.0)
        .unwrap();

    let shared = game.shared_world();
    shared
        .borrow_mut()
        .begin_concentration_cast(
            id(SPELL),
            id(CASTER),
            id(CASTER),
            None,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    let held = game.world_mut().place_at(id(ITEM), id(CASTER), false);
    assert!(game
        .world_mut()
        .grab_telekinesis_reference(id(CASTER), held));

    assert!(shared
        .borrow_mut()
        .tick_concentration(1.0, &mut |name, class| game.vm.create_object_of(name, class))
        .is_empty());

    assert!(!game.world().is_concentration_casting(id(CASTER)));
    assert_eq!(game.world().telekinesis_reference(id(CASTER)), None);
}
