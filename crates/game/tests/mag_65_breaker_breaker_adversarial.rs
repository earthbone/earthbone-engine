// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CASTER: u32 = 0x900;
const TARGET: u32 = 0x901;
const EFFECT: u32 = 0xA00;
const SPELL: u32 = 0xA01;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture(associated_item: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x800));
    for actor in [CASTER, TARGET] {
        plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    }
    let mut effect_data = vec![0; 152];
    effect_data[8..12].copy_from_slice(&associated_item.to_le_bytes());
    effect_data[64..68].copy_from_slice(&44_u32.to_le_bytes());
    effect_data[80..84].copy_from_slice(&1_u32.to_le_bytes());
    effect_data[84..88].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), effect_data));
    let mut spell_data = vec![0; 36];
    spell_data[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spell_data[20..24].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    20.0_f32.to_le_bytes(),
                    0_u32.to_le_bytes(),
                    3_u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Disguise.esm", plugin).expect("valid fixture");
    Game::new(order)
}

fn cast(
    game: &mut Game,
) -> Result<Vec<soe_game::world::magic::EffectId>, soe_game::world::magic::MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(CASTER),
        Some(id(CASTER)),
        Some(id(TARGET)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result
}

#[test]
fn mag_65_repeated_unknown_disguise_casts_never_add_active_effects() {
    let mut game = fixture(0);
    for _ in 0..2 {
        assert!(matches!(
            cast(&mut game),
            Err(soe_game::world::magic::MagicError::Unsupported(
                "effect archetype"
            ))
        ));
        assert_eq!(game.world().active_effects().count(), 0);
    }
}

#[test]
fn mag_65_unknown_disguise_rejection_preserves_factions_and_sneak() {
    let mut game = fixture(0);
    let (sneak_before, factions_before) = {
        let mut world = game.world_mut();
        world.set_actor_value(id(TARGET), "sneak", 47.0).unwrap();
        let sneak = world.actor_value(id(TARGET), "sneak");
        let target = world.actor(id(TARGET)).expect("target is an actor");
        (sneak, target.factions.clone())
    };

    assert!(matches!(
        cast(&mut game),
        Err(soe_game::world::magic::MagicError::Unsupported(
            "effect archetype"
        ))
    ));

    assert_eq!(
        game.world_mut().actor_value(id(TARGET), "sneak"),
        sneak_before
    );
    assert_eq!(
        game.world_mut()
            .actor(id(TARGET))
            .expect("target remains an actor")
            .factions,
        factions_before
    );
}

#[test]
fn mag_65_associated_npc_does_not_invent_disguise_behavior() {
    let mut game = fixture(0x800);
    assert!(matches!(
        cast(&mut game),
        Err(soe_game::world::magic::MagicError::Unsupported(
            "effect archetype"
        ))
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert!(game
        .world_mut()
        .actor(id(TARGET))
        .unwrap()
        .factions
        .is_empty());
}
