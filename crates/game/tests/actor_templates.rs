// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Actor-level template selection and group ownership, including persistence.

#[path = "common/legacy.rs"]
mod legacy;

#[path = "common/installed.rs"]
mod installed;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, template, Config, Stats},
    World,
};
use std::collections::BTreeSet;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const BASE: u32 = 0x200;
const LIST: u32 = 0x300;
const LOW: u32 = 0x400;
const HIGH: u32 = 0x401;
const GROUPS: u16 = template::STATS
    | template::TRAITS
    | template::SPELLS
    | template::FACTIONS
    | template::INVENTORY
    | template::AI_PACKAGES;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn npc(form: u32, flags: u32, groups: u16, health: u16) -> Record {
    npc::build_record(
        form,
        "Actor",
        "Actor",
        Config {
            flags,
            template_flags: groups,
            level: 10,
            ..Config::default()
        },
        Stats {
            health,
            ..Stats::default()
        },
    )
}

fn list(form: u32, flags: u8, chance: u8, entries: &[(u16, u32)]) -> Record {
    let mut record = Record::new(T(b"LVLN"), form)
        .sub(T(b"LVLF"), [flags])
        .sub(T(b"LVLD"), [chance])
        .sub(T(b"LLCT"), [entries.len() as u8]);
    for &(level, form) in entries {
        let mut data = vec![0; 12];
        data[..2].copy_from_slice(&level.to_le_bytes());
        data[4..8].copy_from_slice(&form.to_le_bytes());
        data[8..10].copy_from_slice(&1u16.to_le_bytes());
        record = record.sub(T(b"LVLO"), data);
    }
    record
}

fn plugin(list: Record) -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        npc(
            BASE,
            npc::flags::ESSENTIAL | npc::flags::PROTECTED,
            GROUPS,
            1,
        )
        .sub(T(b"TPLT"), LIST.to_le_bytes()),
    );
    for (form, health, item) in [(LOW, 100, 0x600u32), (HIGH, 200, 0x601u32)] {
        let actor = npc(
            form,
            npc::flags::AUTO_CALC_STATS | npc::flags::FEMALE,
            0,
            health,
        )
        .sub(T(b"SPLO"), item.to_le_bytes())
        .sub(T(b"PKID"), item.to_le_bytes());
        plugin.push_top(npc::with_item(actor, item, 3));
        plugin.push_top(Record::new(T(b"MISC"), item));
    }
    for actor in ACTOR..ACTOR + 64 {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(list);
    plugin
}

fn world(plugin: Plugin) -> World {
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn a_spawn_shares_one_choice_across_groups_without_inheriting_stats_flags() {
    let mut world = world(plugin(list(LIST, 0, 0, &[(1, LOW), (10, HIGH)])));
    world.player_level = 10;
    assert_eq!(world.actor_value(id(ACTOR), "health"), 200.0);
    assert!(world.actor(id(ACTOR)).unwrap().essential);
    assert!(world.actor(id(ACTOR)).unwrap().protected);
    assert!(world.actor_base(id(BASE)).unwrap().essential);
    assert_eq!(
        world
            .actor_component_source(id(ACTOR), template::STATS)
            .unwrap(),
        id(HIGH)
    );
    assert_eq!(
        world
            .actor_component_source(id(ACTOR), template::TRAITS)
            .unwrap(),
        id(HIGH)
    );
    assert_eq!(
        world
            .actor_component_source(id(ACTOR), template::BASE_DATA)
            .unwrap(),
        id(BASE)
    );
    assert!(world.actor(id(ACTOR)).unwrap().spells.contains(&id(0x601)));
    assert_eq!(world.item_count(id(ACTOR), id(0x601)), 3);
    assert_eq!(
        soe_game::ai::candidates(&mut world, id(ACTOR))[0].package,
        id(0x601)
    );
    world.player_level = 1;
    world.leveled_list(id(LIST)).unwrap().entries.clear();
    assert_eq!(
        world
            .actor_component_source(id(ACTOR), template::STATS)
            .unwrap(),
        id(HIGH)
    );
    assert_eq!(world.actor_value(id(ACTOR), "health"), 200.0);
}

#[test]
fn all_levels_and_duplicate_weighted_entries_are_not_collapsed() {
    let mut world = world(plugin(list(LIST, 1, 0, &[(1, LOW), (1, LOW), (10, HIGH)])));
    world.player_level = 10;
    let mut counts = std::collections::BTreeMap::new();
    for reference in ACTOR..ACTOR + 64 {
        let source = world
            .actor_component_source(id(reference), template::STATS)
            .unwrap();
        *counts.entry(source).or_insert(0) += 1;
    }
    assert_eq!(counts.len(), 2);
    assert!(counts[&id(LOW)] > counts[&id(HIGH)]);
}

#[test]
fn chance_none_and_no_eligible_level_remain_absent_after_reload() {
    for (chance, minimum) in [(100, 1), (0, 20)] {
        let fixture = || plugin(list(LIST, 0, chance, &[(minimum, LOW)]));
        let mut original = world(fixture());
        assert!(!original.reference(id(ACTOR)).unwrap().enabled);
        assert!(original.actor(id(ACTOR)).is_none());
        let bytes = original.save_state();
        let hash = original.state_hash();
        let mut restored = world(fixture());
        restored.load_state(&bytes).unwrap();
        assert_eq!(restored.state_hash(), hash);
        restored.player_level = 100;
        restored.leveled_list(id(LIST)).unwrap().chance_none = 0;
        assert!(restored.actor(id(ACTOR)).is_none());
        assert!(!restored.reference(id(ACTOR)).unwrap().enabled);
    }
}

#[test]
fn pending_actor_initialization_reloads_the_same_choice_and_inventory() {
    let fixture = || plugin(list(LIST, 1, 0, &[(1, LOW), (1, HIGH)]));
    let mut original = world(fixture());
    original.reference(id(ACTOR)).unwrap();
    assert!(original.actor_ref(id(ACTOR)).is_none());
    let bytes = original.save_state();
    let expected_rng = original.random_u64();
    let expected = original
        .actor_component_source(id(ACTOR), template::STATS)
        .unwrap();
    let mut restored = world(fixture());
    restored.load_state(&bytes).unwrap();
    assert_eq!(restored.random_u64(), expected_rng);
    assert_eq!(
        restored
            .actor_component_source(id(ACTOR), template::STATS)
            .unwrap(),
        expected
    );
    assert_eq!(
        restored.actor_value(id(ACTOR), "health"),
        original.actor_value(id(ACTOR), "health")
    );
    assert_eq!(
        restored.reference(id(ACTOR)).unwrap().inventory,
        original.reference(id(ACTOR)).unwrap().inventory
    );
}

#[test]
fn nested_lists_and_template_cycles_have_bounded_evaluation() {
    let mut valid = plugin(list(LIST, 0, 0, &[(1, 0x301)]));
    valid.push_top(list(0x301, 0, 0, &[(1, HIGH)]));
    let mut good = world(valid);
    assert_eq!(good.actor_value(id(ACTOR), "health"), 200.0);
    let mut invalid = plugin(list(LIST, 0, 0, &[(1, 0x301)]));
    invalid.push_top(list(0x301, 0, 0, &[(1, LIST)]));
    let mut broken = world(invalid.clone());
    let next = world(invalid).random_u64();
    assert!(broken
        .prepare_actor_template(id(ACTOR), id(BASE), 10)
        .is_err());
    assert_eq!(broken.random_u64(), next);
}

#[test]
fn live_chance_global_overrides_the_list_byte() {
    let mut data = plugin(list(LIST, 0, 0, &[(1, LOW)]).sub(T(b"LVLG"), 0x700u32.to_le_bytes()));
    data.push_top(Record::new(T(b"GLOB"), 0x700).sub(T(b"FLTV"), 100.0f32.to_le_bytes()));
    let mut world = world(data);
    assert!(world.actor(id(ACTOR)).is_none());
    world.set_global(id(0x700), 0.0);
    assert!(world.actor(id(ACTOR + 1)).is_some());
    assert!(world.actor(id(ACTOR)).is_none());
}

#[test]
fn malformed_selected_lists_restore_the_rng_and_reject_partial_spawns() {
    for record in [
        list(LIST, 0, 0, &[(1, LIST), (1, LIST)]),
        list(LIST, 4, 0, &[(1, LOW)]),
        list(LIST, 0, 101, &[(1, LOW)]),
        Record::new(T(b"LVLN"), LIST).sub(T(b"LVLO"), vec![0; 8]),
    ] {
        let fixture = plugin(record);
        let mut original = world(fixture.clone());
        let expected = world(fixture).random_u64();
        assert!(original
            .prepare_actor_template(id(ACTOR), id(BASE), 10)
            .is_err());
        assert_eq!(original.random_u64(), expected);
        assert!(original.actor_ref(id(ACTOR)).is_none());
    }
}

#[test]
fn corrupt_saved_template_does_not_replace_existing_state() {
    let mut world = world(plugin(list(LIST, 0, 0, &[(1, LOW)])));
    world.actor(id(ACTOR)).unwrap();
    let hash = world.state_hash();
    let mut bytes = world.save_state();
    let offset = bytes.len() - 49;
    bytes[offset..offset + 4].copy_from_slice(&0x600u32.to_le_bytes());
    assert!(world.load_state(&bytes).is_err());
    assert_eq!(world.state_hash(), hash);
}

#[test]
fn version_nine_loads_static_actors_without_a_selection_table() {
    let mut fixture = plugin(list(LIST, 0, 0, &[(1, LOW)]));
    fixture
        .records
        .iter_mut()
        .find(|record| record.form_id == BASE)
        .unwrap()
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"TPLT"))
        .unwrap()
        .data = LOW.to_le_bytes().to_vec();
    let mut world = world(fixture);
    world.actor(id(ACTOR)).unwrap();
    let hash = world.state_hash();
    let mut bytes = legacy::without_exclusive_branches(&mut world, &[id(ACTOR)]);
    // Remove the version 10 to 36 blocks, detection's empty pool count, and
    // the version 37 world sections that this version 9 save cannot carry.
    bytes.truncate(bytes.len() - 60);
    bytes[4..6].copy_from_slice(&9u16.to_le_bytes());
    world.load_state(&bytes).unwrap();
    assert_eq!(world.state_hash(), hash);
}

#[test]
fn static_group_links_keep_the_winning_template_namespace() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(npc(BASE, npc::flags::ESSENTIAL, GROUPS, 1).sub(T(b"TPLT"), LOW.to_le_bytes()));
    base.push_top(npc(LOW, 0, GROUPS, 1).sub(T(b"TPLT"), HIGH.to_le_bytes()));
    base.push_top(npc(HIGH, 0, 0, 100));
    base.push_top(Record::new(T(b"ACHR"), ACTOR).sub(T(b"NAME"), BASE.to_le_bytes()));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Actors.esm".into()],
        ..FileHeader::default()
    });
    let source = npc(
        HIGH,
        npc::flags::AUTO_CALC_STATS | npc::flags::FEMALE,
        0,
        250,
    )
    .sub(T(b"SPLO"), 0x01000600u32.to_le_bytes())
    .sub(T(b"PRKR"), [0, 6, 0, 1, 1, 0, 0, 0])
    .sub(T(b"PKID"), 0x01000601u32.to_le_bytes())
    .sub(T(b"DOFT"), 0x01000602u32.to_le_bytes())
    .sub(T(b"SOFT"), 0x01000603u32.to_le_bytes())
    .sub(T(b"CRIF"), 0x01000604u32.to_le_bytes());
    patch.push_top(npc::with_item(
        npc::with_faction(source, 0x01000604, 2),
        0x01000605,
        4,
    ));
    for (form, tag) in [
        (0x600, b"SPEL"),
        (0x601, b"PACK"),
        (0x602, b"OTFT"),
        (0x603, b"OTFT"),
        (0x604, b"FACT"),
        (0x605, b"MISC"),
    ] {
        patch.push_top(Record::new(T(tag), 0x01000000 | form));
    }
    let mut order = LoadOrder::new();
    order.add("Actors.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut world = World::new(order);
    let patch_id = |object| GlobalFormId { file: 1, object };
    let actor = world.actor(id(ACTOR)).unwrap();
    assert_eq!(actor.values.get("health").base(), 250.0);
    assert!(actor.essential);
    assert_eq!(actor.spells, BTreeSet::from([patch_id(0x600)]));
    assert_eq!(actor.factions[&patch_id(0x604)], 2);
    assert_eq!(actor.crime_faction, Some(patch_id(0x604)));
    assert_eq!(actor.outfit, Some(patch_id(0x602)));
    assert_eq!(actor.sleep_outfit, Some(patch_id(0x603)));
    assert_eq!(world.item_count(id(ACTOR), patch_id(0x605)), 4);
    assert_eq!(
        soe_game::ai::candidates(&mut world, id(ACTOR))[0].package,
        patch_id(0x601)
    );
    assert!(world.actor_base(id(BASE)).unwrap().female);
}

#[test]
fn installed_npc_templates_initialize_at_multiple_selection_levels() {
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping actor template survey");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let bases: Vec<_> = order
        .winners_of(T(b"NPC_"))
        .filter(|(_, record)| !record.is_deleted())
        .map(|(id, _)| id)
        .collect();
    assert!(
        bases.len() > 6000,
        "expected the full five-master NPC survey"
    );
    let mut world = World::from_shared_order(order);
    let player = world.player();
    for level in [1, 20, 50] {
        world.player_level = level;
        let mut initialized = 0;
        let mut suppressed = 0;
        let mut errors = std::collections::BTreeMap::<String, Vec<GlobalFormId>>::new();
        for &base in &bases {
            let actor = world.place_at(base, player, false);
            match world.prepare_actor_template(actor, base, level) {
                Ok(false) => {
                    suppressed += 1;
                    continue;
                }
                Err(error) => {
                    errors.entry(error.to_string()).or_default().push(base);
                    continue;
                }
                Ok(true) => {}
            }
            if let Some(state) = world.actor(actor) {
                assert!(
                    ["health", "magicka", "stamina"].iter().all(|name| state
                        .values
                        .get(name)
                        .current()
                        .is_finite()),
                    "{base}"
                );
                initialized += 1;
            } else {
                errors
                    .entry("actor initialization".into())
                    .or_default()
                    .push(base);
            }
        }
        eprintln!("level={level}, bases={}, initialized={initialized}, suppressed={suppressed}, errors={errors:?}", bases.len());
        // Threshold fixed before the first installed-data run.
        assert!(initialized > 6000, "too few actors initialized");
        assert!(errors.is_empty(), "unexpected actor source errors");
    }
    let bytes = world.save_state();
    let hash = world.state_hash();
    world.load_state(&bytes).unwrap();
    assert_eq!(world.state_hash(), hash);
}
