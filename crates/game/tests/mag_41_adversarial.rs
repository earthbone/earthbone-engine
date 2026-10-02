// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::support::magic_adversarial as support;

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use support::*;

#[test]
fn mag_41_replacing_telekinesis_concentration_releases_the_grabbed_reference() {
    let mut tk_effect = effect(0x300, 20, 0, 0);
    put(tk_effect.subrecords[0].data.as_mut_slice(), 80, 2);
    put(tk_effect.subrecords[0].data.as_mut_slice(), 84, 0);
    let mut other_effect = effect(0x301, 0, 0, 24);
    put(other_effect.subrecords[0].data.as_mut_slice(), 80, 2);
    put(other_effect.subrecords[0].data.as_mut_slice(), 84, 0);
    let mut tk_spell = spell(0x310, 0x300, 0, 1.0, 2);
    put(tk_spell.subrecords[0].data.as_mut_slice(), 20, 0);
    let mut replacement = spell(0x311, 0x301, 0, 1.0, 2);
    put(replacement.subrecords[0].data.as_mut_slice(), 20, 0);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x220));
    plugin.push_top(actor(CASTER, [0.0; 3]));
    plugin.push_top(tk_effect);
    plugin.push_top(other_effect);
    plugin.push_top(tk_spell);
    plugin.push_top(replacement);
    let mut order = LoadOrder::new();
    order.add("TelekinesisBreak.esm", plugin).unwrap();
    let mut game = soe_game::Game::new(order);
    game.world_mut()
        .set_actor_value(id(CASTER), "magicka", 100.0)
        .unwrap();
    {
        let shared = game.shared_world();
        shared
            .borrow_mut()
            .begin_concentration_cast(
                id(0x310),
                id(CASTER),
                id(CASTER),
                None,
                &mut |name, class| game.vm.create_object_of(name, class),
            )
            .unwrap();
    }
    let held = game.world_mut().place_at(id(0x220), id(CASTER), false);
    assert!(
        game.world_mut()
            .grab_telekinesis_reference(id(CASTER), held)
    );
    {
        let shared = game.shared_world();
        shared
            .borrow_mut()
            .begin_concentration_cast(
                id(0x311),
                id(CASTER),
                id(CASTER),
                None,
                &mut |name, class| game.vm.create_object_of(name, class),
            )
            .unwrap();
    }
    assert_eq!(game.world().telekinesis_reference(id(CASTER)), None);
}
