// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Reanimate uses CK Magic Effect's dead-target and caster-control contract.
//! xEdit identifies MGEF archetype 22, with duration on the SPEL EFIT entry.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::world::magic::{EffectId, MagicError};

const BASE: u32 = 0x100;
const CASTER: u32 = 0x101;
const CORPSE: u32 = 0x102;
const PRIOR_OWNER: u32 = 0x103;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(duration: u32) -> Game {
    fixture_with_flags(duration, 1)
}

fn fixture_with_flags(duration: u32, flags: u32) -> Game {
    let tag = Tag::new;
    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 0, flags);
    put(&mut effect_data, 16, u32::MAX);
    put(&mut effect_data, 64, 22);
    put(&mut effect_data, 80, 1);
    put(&mut effect_data, 84, 3);

    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, 3);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(b"NPC_"), BASE).sub(tag(b"ACBS"), vec![0; 24]));
    for actor in [CASTER, CORPSE, PRIOR_OWNER] {
        let mut reference = Record::new(tag(b"ACHR"), actor)
            .sub(tag(b"NAME"), BASE.to_le_bytes())
            .sub(tag(b"DATA"), vec![0; 24]);
        if actor == CORPSE {
            reference = reference.sub(tag(b"XOWN"), PRIOR_OWNER.to_le_bytes());
        }
        plugin.push_top(reference);
    }
    plugin.push_top(Record::new(tag(b"FACT"), 0x400).sub(tag(b"DATA"), 0_u32.to_le_bytes()));
    plugin.push_top(Record::new(tag(b"MGEF"), EFFECT).sub(tag(b"DATA"), effect_data));
    plugin.push_top(
        Record::new(tag(b"SPEL"), SPELL)
            .sub(tag(b"SPIT"), spell_data)
            .sub(tag(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                tag(b"EFIT"),
                [
                    10.0f32.to_le_bytes(),
                    0u32.to_le_bytes(),
                    duration.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Reanimate.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [CASTER, CORPSE] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game
}

fn cast(game: &mut Game, caster: Option<GlobalFormId>) -> Result<Vec<EffectId>, MagicError> {
    game.shared_world().borrow_mut().cast_spell(
        id(SPELL),
        id(CASTER),
        caster,
        Some(id(CORPSE)),
        &mut |name, class| game.vm.create_object_of(name, class),
    )
}

#[test]
fn mag_43_revives_a_dead_actor_under_the_credited_caster() {
    let mut game = fixture(10);
    assert!(game.world_mut().kill_actor(id(CORPSE), None));

    let applied = cast(&mut game, Some(id(CASTER))).unwrap();

    assert_eq!(applied.len(), 1);
    assert!(!game.world().actor_ref(id(CORPSE)).unwrap().dead);
    assert_eq!(
        game.world().reference_ref(id(CORPSE)).unwrap().owner,
        Some(id(CASTER))
    );
}

#[test]
fn mag_43_rejects_living_and_ineligible_corpses_without_applying_an_effect() {
    let mut game = fixture(10);
    assert!(matches!(
        cast(&mut game, Some(id(CASTER))),
        Err(MagicError::Target)
    ));
    assert!(game.world_mut().kill_actor(id(CORPSE), None));
    game.world_mut().actor(id(CORPSE)).unwrap().ghost = true;
    assert!(matches!(
        cast(&mut game, Some(id(CASTER))),
        Err(MagicError::Target)
    ));
    game.world_mut().actor(id(CORPSE)).unwrap().ghost = false;
    game.world_mut().actor(id(CORPSE)).unwrap().level = 11;
    assert!(matches!(
        cast(&mut game, Some(id(CASTER))),
        Err(MagicError::Target)
    ));
    game.world_mut().actor(id(CORPSE)).unwrap().level = 1;
    assert!(matches!(cast(&mut game, None), Err(MagicError::Target)));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(
        game.world().reference_ref(id(CORPSE)).unwrap().owner,
        Some(id(PRIOR_OWNER))
    );
}

#[test]
fn mag_43_authored_duration_ends_control_and_returns_the_actor_to_death() {
    let mut game = fixture(2);
    assert!(game.world_mut().kill_actor(id(CORPSE), None));
    cast(&mut game, Some(id(CASTER))).unwrap();

    game.tick(1.0);
    assert!(!game.world().actor_ref(id(CORPSE)).unwrap().dead);
    game.tick(1.0);

    assert!(game.world().actor_ref(id(CORPSE)).unwrap().dead);
    assert_eq!(
        game.world().reference_ref(id(CORPSE)).unwrap().owner,
        Some(id(PRIOR_OWNER))
    );
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_43_dispel_ends_control_and_restores_the_prior_owner() {
    let mut game = fixture(30);
    assert!(game.world_mut().kill_actor(id(CORPSE), None));
    let effect = cast(&mut game, Some(id(CASTER))).unwrap()[0];

    let ended = game
        .shared_world()
        .borrow_mut()
        .dispel_effect(effect, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });

    assert!(ended);
    assert!(game.world().actor_ref(id(CORPSE)).unwrap().dead);
    assert_eq!(
        game.world().reference_ref(id(CORPSE)).unwrap().owner,
        Some(id(PRIOR_OWNER))
    );
}

#[test]
fn mag_43_editor_display_flags_keep_the_authored_duration_and_magnitude() {
    let mut game = fixture_with_flags(2, 1 | (1 << 9) | (1 << 10) | (1 << 11));
    assert!(game.world_mut().kill_actor(id(CORPSE), None));
    game.world_mut().actor(id(CORPSE)).unwrap().level = 11;
    assert!(matches!(
        cast(&mut game, Some(id(CASTER))),
        Err(MagicError::Target)
    ));
    game.world_mut().actor(id(CORPSE)).unwrap().level = 1;

    cast(&mut game, Some(id(CASTER))).unwrap();
    game.tick(1.0);

    assert!(!game.world().actor_ref(id(CORPSE)).unwrap().dead);
}

#[test]
fn mag_43_saved_reanimation_resumes_its_remaining_lifetime() {
    let mut game = fixture(2);
    assert!(game.world_mut().kill_actor(id(CORPSE), None));
    cast(&mut game, Some(id(CASTER))).unwrap();
    game.tick(1.0);
    let saved = game.save_state().unwrap();
    let mut loaded = fixture(2);
    loaded.load_state(&saved).unwrap();

    loaded.tick(1.0);

    assert!(loaded.world().actor_ref(id(CORPSE)).unwrap().dead);
    assert_eq!(
        loaded.world().reference_ref(id(CORPSE)).unwrap().owner,
        Some(id(PRIOR_OWNER))
    );
}

#[test]
fn frozen_version_37_layout_loads_factions_and_magic_without_extra_fields() {
    assert_frozen_layout(37);
}

#[test]
fn frozen_version_36_layout_loads_factions_and_magic_without_reused_fields() {
    assert_frozen_layout(36);
}

fn assert_frozen_layout(version: u16) {
    let mut game = fixture(10);
    assert!(game.world_mut().kill_actor(id(CORPSE), None));
    let effect = cast(&mut game, Some(id(CASTER))).unwrap()[0];
    let faction = id(0x400);
    let actor = id(CORPSE);
    {
        let mut world = game.world_mut();
        let state = world.faction(faction).unwrap();
        state.player_attacked = true;
        state.crime_gold_violent = 123;
        world.actor(actor).unwrap().swimming = true;
    }
    let bytes = match version {
        36 => {
            crate::common::legacy::version_36_at_freeze(&mut game.world_mut(), &[id(CASTER), actor])
        }
        37 => {
            crate::common::legacy::version_37_at_freeze(&mut game.world_mut(), &[id(CASTER), actor])
        }
        _ => unreachable!("fixture version"),
    };
    assert_eq!(&bytes[4..6], &version.to_le_bytes());
    // CONTEXT: Encode the active-effect block exactly as the frozen writer did.
    let mut magic = soe_papyrus::vm::state::Writer::new();
    magic.u64(effect.0 + 1);
    magic.len(1);
    magic.u64(effect.0);
    for form in [SPELL, EFFECT] {
        magic.u16(0);
        magic.u32(form);
    }
    magic.u32(0);
    magic.u16(0);
    magic.u32(CASTER);
    magic.bool(true);
    magic.u16(0);
    magic.u32(CASTER);
    magic.u16(0);
    magic.u32(CORPSE);
    magic.f32(10.0);
    magic.bool(true);
    magic.f64(10.0);
    magic.f64(0.0);
    magic.bool(false);
    magic.bool(false);
    magic.len(0);
    crate::common::legacy::unique_offset(&bytes, &magic.out);

    let restored = fixture(10);
    restored
        .world_mut()
        .faction(faction)
        .unwrap()
        .player_attacked = true;
    restored.world_mut().load_state(&bytes).unwrap();
    let mut world = restored.world_mut();
    assert_eq!(world.faction(faction).unwrap().crime_gold_violent, 123);
    assert!(!world.faction(faction).unwrap().player_attacked);
    assert!(!world.actor(actor).unwrap().swimming);
    let loaded = world
        .active_effects()
        .find(|loaded| loaded.id == effect)
        .unwrap();
    assert_eq!(loaded.target, actor);
    assert_eq!(loaded.reanimated_previous_owner, None);
    assert_eq!(loaded.elapsed, 0.0);
}
