// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/magic_adversarial.rs"]
mod support;

use support::*;

const ITEM: u32 = 0x220;
const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x310;

fn game_with_telekinesis() -> soe_game::Game {
    let mut effect = effect(EFFECT, 20, 0, 0);
    put(effect.subrecords[0].data.as_mut_slice(), 80, 2);
    put(effect.subrecords[0].data.as_mut_slice(), 84, 0);
    let mut spell = spell(SPELL, EFFECT, 0, 1.0, 2);
    put(spell.subrecords[0].data.as_mut_slice(), 20, 0);
    let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
    plugin.push_top(
        soe_esm::Record::new(soe_esm::Tag(*b"NPC_"), BASE).sub(soe_esm::Tag(*b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(soe_esm::Record::new(soe_esm::Tag(*b"STAT"), ITEM));
    plugin.push_top(actor(CASTER, [0.0; 3]));
    plugin.push_top(effect);
    plugin.push_top(spell);
    let mut order = soe_esm::LoadOrder::new();
    order.add("TelekinesisExtraBreaker.esm", plugin).unwrap();
    let game = soe_game::Game::new(order);
    game.world_mut()
        .set_actor_value(id(CASTER), "magicka", 100.0)
        .unwrap();
    game
}

fn begin_telekinesis(game: &mut soe_game::Game) {
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
}

#[test]
fn mag_41_rejects_a_deleted_reference_after_the_cast_starts() {
    let mut game = game_with_telekinesis();
    begin_telekinesis(&mut game);
    let item = game.world_mut().place_at(id(ITEM), id(CASTER), false);
    game.world_mut().reference(item).unwrap().deleted = true;

    assert!(!game
        .world_mut()
        .grab_telekinesis_reference(id(CASTER), item));
    assert_eq!(game.world().telekinesis_reference(id(CASTER)), None);
}

#[test]
fn mag_41_wrong_caster_cannot_release_another_casters_handle() {
    let mut game = game_with_telekinesis();
    begin_telekinesis(&mut game);
    let item = game.world_mut().place_at(id(ITEM), id(CASTER), false);
    assert!(game
        .world_mut()
        .grab_telekinesis_reference(id(CASTER), item));

    assert!(!game.world_mut().release_telekinesis_reference(id(TARGET)));
    assert_eq!(game.world().telekinesis_reference(id(CASTER)), Some(item));
}
