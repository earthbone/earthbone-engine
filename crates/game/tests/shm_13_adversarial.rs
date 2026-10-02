// SPDX-License-Identifier: GPL-3.0-or-later

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{MagicWorldState, ObjectEquipParams, UnsupportedState};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{commonlib_magic::CommonLibMagic, World};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x10));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 1));
    plugin.push_top(Record::new(Tag(*b"SPEL"), 2).sub(Tag(*b"SPIT"), vec![0; 36]));
    plugin.push_top(Record::new(Tag(*b"MGEF"), 3));
    plugin.push_top(Record::new(Tag(*b"MGEF"), 4));
    let mut actor = Record::new(Tag(*b"ACHR"), 0x20).sub(Tag(*b"NAME"), 1_u32.to_le_bytes());
    actor.placement.cell = Some(0x10);
    plugin.push_top(actor);
    for marker in [0x30, 0x31] {
        plugin.push_top(
            Record::new(Tag(*b"REFR"), marker)
                .sub(Tag(*b"NAME"), 3_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("shm_13_fixture.esp", plugin).unwrap();
    World::new(order)
}

fn allocate(_name: &str, _class: &str) -> soe_papyrus::ObjectId {
    soe_papyrus::ObjectId(1)
}

fn runtime(world: &World, form: GlobalFormId) -> FormId {
    FormId(world.order().runtime_id(form))
}

#[test]
fn shm_13_favorite_repeats_are_idempotent_and_missing_actor_is_rejected() {
    let actor = id(0x20);
    let spell = id(2);
    let mut world = fixture_world();
    world.actor(actor).unwrap();
    let (actor_form, spell_form) = (runtime(&world, actor), runtime(&world, spell));
    let mut allocator = allocate;
    let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);

    assert!(adapter.set_favorite(actor_form, spell_form, true).unwrap());
    assert!(!adapter.set_favorite(actor_form, spell_form, true).unwrap());
    assert!(adapter.is_favorite(actor_form, spell_form).unwrap());
    assert!(matches!(
        adapter.set_favorite(FormId(0xdead_beef), spell_form, true),
        Err(UnsupportedState::MissingReference)
    ));
}

#[test]
fn shm_13_empty_magic_target_and_missing_target_follow_live_reference_state() {
    let actor = id(0x20);
    let spell = id(2);
    let mut world = fixture_world();
    world.actor(actor).unwrap();
    let actor_form = runtime(&world, actor);
    let spell_form = runtime(&world, spell);
    let mut allocator = allocate;
    let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);

    assert_eq!(adapter.active_effect_count(actor_form).unwrap(), 0);
    assert!(!adapter.dispel_spell(actor_form, spell_form).unwrap());
    assert!(!adapter.has_magic_effect(actor_form, spell_form).unwrap());
    assert!(matches!(
        adapter.active_effect_count(FormId(0xdead_beef)),
        Err(UnsupportedState::MissingReference)
    ));
}

#[test]
fn shm_13_marker_show_all_hidden_rejection_preserves_supported_bits() {
    let marker = id(0x30);
    let mut world = fixture_world();
    world.reference(marker).unwrap().on_map = true;
    let marker_form = runtime(&world, marker);
    let mut allocator = allocate;
    let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);

    assert_eq!(adapter.map_marker_flags(marker_form).unwrap(), 1);
    assert!(matches!(
        adapter.set_map_marker_flags(marker_form, 5),
        Err(UnsupportedState::Hook(_))
    ));
    assert_eq!(adapter.map_marker_flags(marker_form).unwrap(), 1);
    assert!(matches!(
        adapter.map_marker_flags(FormId(0xdead_beef)),
        Err(UnsupportedState::MissingReference)
    ));
}

#[test]
fn shm_13_unsupported_equip_options_do_not_mutate_equipment() {
    let actor = id(0x20);
    let item = id(3);
    let mut world = fixture_world();
    world.actor(actor).unwrap();
    let (actor_form, item_form) = (runtime(&world, actor), runtime(&world, item));
    let mut allocator = allocate;
    let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);

    for params in [
        ObjectEquipParams { count: 2, ..Default::default() },
        ObjectEquipParams { play_equip_sounds: false, ..Default::default() },
        ObjectEquipParams { force_equip: true, ..Default::default() },
        ObjectEquipParams { show_message: true, ..Default::default() },
        ObjectEquipParams { has_extra_data: true, ..Default::default() },
        ObjectEquipParams { has_equip_slot: true, ..Default::default() },
    ] {
        assert!(matches!(
            adapter.equip_object(actor_form, item_form, params),
            Err(UnsupportedState::Hook(_))
        ));
    }
    assert!(matches!(
        adapter.equip_object(FormId(0xdead_beef), item_form, ObjectEquipParams::default()),
        Err(UnsupportedState::MissingReference)
    ));
    drop(adapter);
    assert!(!world.actor_ref(actor).unwrap().equipped.contains(&item));
}
