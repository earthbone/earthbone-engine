// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{
    CannotCastReason, MagicWorldState, ObjectEquipParams, UnsupportedState,
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{commonlib_magic::CommonLibMagic, World};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn runtime(world: &World, form: GlobalFormId) -> FormId {
    FormId(world.order().runtime_id(form))
}

fn fixture_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x10));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 1));
    let mut spell_data = vec![0; 36];
    spell_data[0..4].copy_from_slice(&500_u32.to_le_bytes());
    spell_data[4..8].copy_from_slice(&1_u32.to_le_bytes());
    spell_data[8..12].copy_from_slice(&2_u32.to_le_bytes());
    spell_data[16..20].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"SPEL"), 2).sub(Tag(*b"SPIT"), spell_data));
    plugin.push_top(Record::new(Tag(*b"MGEF"), 3));
    plugin.push_top(Record::new(Tag(*b"MGEF"), 4));
    plugin.push_top(Record::new(Tag(*b"MISC"), 5));
    let mut actor = Record::new(Tag(*b"ACHR"), 0x20).sub(Tag(*b"NAME"), 1_u32.to_le_bytes());
    actor.placement.cell = Some(0x10);
    plugin.push_top(actor);
    let mut order = LoadOrder::new();
    order.add("shm_13_breaker.esp", plugin).unwrap();
    World::new(order)
}

fn allocate(_name: &str, _class: &str) -> soe_papyrus::ObjectId {
    soe_papyrus::ObjectId(1)
}

#[test]
fn shm_13_favorites_toggle_idempotently_and_survive_save_load() {
    let actor = id(0x20);
    let spell = id(2);
    let mut world = fixture_world();
    world.actor(actor).unwrap();
    let (actor_form, spell_form) = (runtime(&world, actor), runtime(&world, spell));
    let mut allocator = allocate;
    {
        let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
        assert!(adapter.set_favorite(actor_form, spell_form, true).unwrap());
        assert!(!adapter.set_favorite(actor_form, spell_form, true).unwrap());
        assert!(adapter.is_favorite(actor_form, spell_form).unwrap());
        assert!(adapter.set_favorite(actor_form, spell_form, false).unwrap());
        assert!(!adapter.set_favorite(actor_form, spell_form, false).unwrap());
        assert!(adapter.set_favorite(actor_form, spell_form, true).unwrap());
    }
    let saved = world.save_state();
    let mut restored = fixture_world();
    restored.load_state(&saved).unwrap();
    let mut allocator = allocate;
    let mut adapter = CommonLibMagic::new(&mut restored, &mut allocator);
    assert!(adapter.is_favorite(actor_form, spell_form).unwrap());
    assert!(adapter.set_favorite(actor_form, spell_form, false).unwrap());
    assert!(!adapter.is_favorite(actor_form, spell_form).unwrap());
}

#[test]
fn shm_13_cast_checks_prioritize_live_restrictions_and_reject_non_spells() {
    let actor = id(0x20);
    let spell = id(2);
    let mut world = fixture_world();
    world.actor(actor).unwrap();
    let (actor_form, spell_form) = (runtime(&world, actor), runtime(&world, spell));
    let effect_form = runtime(&world, id(3));
    world.set_actor_value(actor, "magicka", 0.0).unwrap();
    let mut allocator = allocate;
    {
        let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
        assert_eq!(
            adapter.cannot_cast(actor_form, spell_form).unwrap(),
            CannotCastReason::Magicka
        );
        assert!(matches!(
            adapter.cannot_cast(actor_form, effect_form),
            Err(UnsupportedState::Hook(
                "MagicSystem::CannotCast invalid spell"
            ))
        ));
    }
    assert!(world.set_voice_recovery_time(actor, 5.0));
    {
        let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
        // PowerUsed applies only to power spell types, and this fixture is a power.
        assert_eq!(
            adapter.cannot_cast(actor_form, spell_form).unwrap(),
            CannotCastReason::PowerUsed
        );
    }
    assert!(world.set_voice_recovery_time(actor, 0.0));
    world.begin_spell_cast(actor, spell, None).unwrap();
    let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
    assert_eq!(
        adapter.cannot_cast(actor_form, spell_form).unwrap(),
        CannotCastReason::MultipleCast
    );
    assert_eq!(CannotCastReason::CustomReasonNoStart as u8, 100);
}

#[test]
fn shm_13_magic_target_empty_queries_and_repeated_dispels_are_stable() {
    let actor = id(0x20);
    let spell = id(2);
    let effect = id(3);
    let mut world = fixture_world();
    world.actor(actor).unwrap();
    let (actor_form, spell_form, effect_form) = (
        runtime(&world, actor),
        runtime(&world, spell),
        runtime(&world, effect),
    );
    let mut allocator = allocate;
    let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
    assert_eq!(adapter.active_effect_count(actor_form).unwrap(), 0);
    assert!(!adapter.has_magic_effect(actor_form, effect_form).unwrap());
    assert!(!adapter.dispel_spell(actor_form, spell_form).unwrap());
    assert!(!adapter.dispel_spell(actor_form, spell_form).unwrap());
    assert_eq!(adapter.active_effect_count(actor_form).unwrap(), 0);
}

#[test]
fn shm_13_marker_flag_matrix_roundtrips_and_unknown_bits_are_atomic() {
    let reference = id(0x20);
    let mut world = fixture_world();
    world.reference(reference).unwrap();
    let reference_form = runtime(&world, reference);
    let mut allocator = allocate;
    let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
    for flags in 0..=3 {
        adapter.set_map_marker_flags(reference_form, flags).unwrap();
        assert_eq!(adapter.map_marker_flags(reference_form).unwrap(), flags);
    }
    for flags in [4, 8, 128, 255] {
        assert!(matches!(
            adapter.set_map_marker_flags(reference_form, flags),
            Err(UnsupportedState::Hook(
                "MapMarkerData::ShowAllHidden has no runtime state"
            ))
        ));
        assert_eq!(adapter.map_marker_flags(reference_form).unwrap(), 3);
    }
}

#[test]
fn shm_13_equip_rejects_each_unmodeled_option_without_equipping() {
    let actor = id(0x20);
    let item = id(5);
    let mut world = fixture_world();
    world.actor(actor).unwrap();
    let (actor_form, item_form) = (runtime(&world, actor), runtime(&world, item));
    {
        let mut allocator = allocate;
        let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
        for params in [
            ObjectEquipParams {
                count: 0,
                ..Default::default()
            },
            ObjectEquipParams {
                count: -1,
                ..Default::default()
            },
            ObjectEquipParams {
                count: 2,
                ..Default::default()
            },
            ObjectEquipParams {
                play_equip_sounds: false,
                ..Default::default()
            },
            ObjectEquipParams {
                force_equip: true,
                ..Default::default()
            },
            ObjectEquipParams {
                show_message: true,
                ..Default::default()
            },
            ObjectEquipParams {
                has_extra_data: true,
                ..Default::default()
            },
            ObjectEquipParams {
                has_equip_slot: true,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                adapter.equip_object(actor_form, item_form, params),
                Err(UnsupportedState::Hook(
                    "ActorEquipManager::EquipObject unsupported ObjectEquipParams"
                ))
            ));
        }
    }
    assert!(!world.actor_ref(actor).unwrap().equipped.contains(&item));
}

#[test]
fn shm_13_missing_forms_return_explicit_errors_across_interfaces() {
    let actor = id(0x20);
    let mut world = fixture_world();
    world.actor(actor).unwrap();
    let actor_form = runtime(&world, actor);
    let missing = FormId(0xdead_beef);
    let mut allocator = allocate;
    let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
    assert_eq!(
        adapter.is_favorite(missing, missing),
        Err(UnsupportedState::MissingReference)
    );
    assert_eq!(
        adapter.cannot_cast(actor_form, missing),
        Err(UnsupportedState::MissingReference)
    );
    assert_eq!(
        adapter.active_effect_count(missing),
        Err(UnsupportedState::MissingReference)
    );
    assert_eq!(
        adapter.has_magic_effect(missing, actor_form),
        Err(UnsupportedState::MissingReference)
    );
    assert_eq!(
        adapter.dispel_spell(missing, actor_form),
        Err(UnsupportedState::MissingReference)
    );
    assert_eq!(
        adapter.map_marker_flags(missing),
        Err(UnsupportedState::MissingReference)
    );
    assert_eq!(
        adapter.set_map_marker_flags(missing, 0),
        Err(UnsupportedState::MissingReference)
    );
    assert_eq!(
        adapter.equip_object(missing, actor_form, ObjectEquipParams::default()),
        Err(UnsupportedState::MissingReference)
    );
}
