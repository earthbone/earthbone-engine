// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Ordinary LVLI quantities, deterministic choices and atomic rejection.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use std::collections::BTreeMap;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const LIST: u32 = 0x100;
const CHILD: u32 = 0x101;
const A: u32 = 0x200;
const B: u32 = 0x201;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn list(object: u32, flags: u8, chance: u8, entries: &[(u16, u32, u16)]) -> Record {
    let mut record = Record::new(T(b"LVLI"), object)
        .sub(T(b"LVLF"), [flags])
        .sub(T(b"LVLD"), [chance])
        .sub(T(b"LLCT"), [entries.len() as u8]);
    for &(level, form, count) in entries {
        let mut bytes = vec![0; 12];
        bytes[..2].copy_from_slice(&level.to_le_bytes());
        bytes[4..8].copy_from_slice(&form.to_le_bytes());
        bytes[8..10].copy_from_slice(&count.to_le_bytes());
        record = record.sub(T(b"LVLO"), bytes);
    }
    record
}

fn world(records: Vec<Record>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    for item in [A, B] {
        plugin.push_top(Record::new(T(b"MISC"), item));
    }
    let mut order = LoadOrder::new();
    order.add("Lists.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn prg_05_nested_counts_multiply_at_every_level() {
    let mut world = world(vec![
        list(LIST, 0, 0, &[(1, CHILD, 3)]),
        list(CHILD, 0, 0, &[(1, A, 2)]),
    ]);
    assert_eq!(
        world.roll_leveled_items(id(LIST), 1, 4).unwrap(),
        BTreeMap::from([(id(A), 24)])
    );
}

#[test]
fn highest_eligible_level_excludes_lower_and_future_entries() {
    let mut world = world(vec![list(LIST, 0, 0, &[(1, A, 1), (10, B, 2), (20, A, 3)])]);
    assert_eq!(
        world.roll_leveled_items(id(LIST), 15, 1).unwrap(),
        BTreeMap::from([(id(B), 2)])
    );
}

#[test]
fn use_all_ignores_entry_levels_and_combines_duplicate_counts() {
    let mut world = world(vec![list(LIST, 4, 0, &[(1, A, 2), (50, B, 3), (99, A, 4)])]);
    assert_eq!(
        world.roll_leveled_items(id(LIST), 1, 2).unwrap(),
        BTreeMap::from([(id(A), 12), (id(B), 6)])
    );
}

#[test]
fn per_count_rolls_retain_duplicate_entry_weight_and_lower_levels() {
    let mut world = world(vec![list(LIST, 3, 0, &[(1, A, 1), (1, A, 1), (10, B, 1)])]);
    let items = world.roll_leveled_items(id(LIST), 10, 3000).unwrap();
    assert_eq!(items.values().sum::<i32>(), 3000);
    assert!((1800..2200).contains(&items[&id(A)]));
    assert!((800..1200).contains(&items[&id(B)]));
}

#[test]
fn without_per_count_flag_one_selection_is_multiplied() {
    let mut world = world(vec![list(LIST, 1, 0, &[(1, A, 1), (10, B, 1)])]);
    let items = world.roll_leveled_items(id(LIST), 10, 1000).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items.values().sum::<i32>(), 1000);
}

#[test]
fn live_global_chance_overrides_authored_and_script_chance() {
    let global = Record::new(T(b"GLOB"), 0x300)
        .sub(T(b"FNAM"), [b'f'])
        .sub(T(b"FLTV"), 100_f32.to_le_bytes());
    let mut world = world(vec![
        list(LIST, 0, 0, &[(1, A, 1)]).sub(T(b"LVLG"), 0x300_u32.to_le_bytes()),
        global,
    ]);
    assert!(world.roll_leveled_items(id(LIST), 1, 1).unwrap().is_empty());
    world.set_global(id(0x300), 0.0);
    world.leveled_list(id(LIST)).unwrap().chance_none = 100;
    assert_eq!(
        world.roll_leveled_items(id(LIST), 1, 1).unwrap(),
        BTreeMap::from([(id(A), 1)])
    );
}

#[test]
fn fractional_global_chance_is_truncated_to_the_runtime_byte() {
    let global = Record::new(T(b"GLOB"), 0x300)
        .sub(T(b"FNAM"), [b'f'])
        .sub(T(b"FLTV"), 0.99_f32.to_le_bytes());
    let mut world = world(vec![
        list(LIST, 2, 100, &[(1, A, 1)]).sub(T(b"LVLG"), 0x300_u32.to_le_bytes()),
        global,
    ]);
    assert_eq!(
        world.roll_leveled_items(id(LIST), 1, 1000).unwrap(),
        BTreeMap::from([(id(A), 1000)])
    );
    world.set_global(id(0x300), 25.99);
    let before = world.save_state();
    let fractional = world.roll_leveled_items(id(LIST), 1, 1000).unwrap();
    world.load_state(&before).unwrap();
    world.set_global(id(0x300), 25.0);
    assert_eq!(
        world.roll_leveled_items(id(LIST), 1, 1000).unwrap(),
        fractional
    );
}

#[test]
fn chance_none_applies_separately_to_each_count() {
    let mut world = world(vec![list(LIST, 2, 25, &[(1, A, 1)])]);
    let count = world.roll_leveled_items(id(LIST), 1, 4000).unwrap()[&id(A)];
    assert!((2800..3200).contains(&count));
}

#[test]
fn prg_05_nested_cycles_are_rejected_even_beyond_the_current_level() {
    let mut world = world(vec![
        list(LIST, 0, 100, &[(1, A, 1), (99, CHILD, 1)]),
        list(CHILD, 0, 0, &[(1, LIST, 1)]),
    ]);
    let before = world.save_state();
    let expected_rng = world.random_u64();
    world.load_state(&before).unwrap();
    assert!(world
        .roll_leveled_items(id(LIST), 1, 1)
        .unwrap_err()
        .to_string()
        .contains("cyclic"));
    assert_eq!(world.random_u64(), expected_rng);
}

#[test]
fn overflow_after_a_random_choice_restores_rng() {
    let mut world = world(vec![list(LIST, 1, 0, &[(1, A, 2), (1, B, 2)])]);
    let before = world.save_state();
    let expected_rng = world.random_u64();
    world.load_state(&before).unwrap();
    assert!(world.roll_leveled_items(id(LIST), 1, i32::MAX).is_err());
    assert_eq!(world.random_u64(), expected_rng);
}

#[test]
fn saved_rng_and_runtime_list_edits_reproduce_future_rolls() {
    let mut world = world(vec![list(LIST, 3, 20, &[(1, A, 1), (10, B, 1)])]);
    world.leveled_list(id(LIST)).unwrap().entries[0].count = 7;
    world.roll_leveled_items(id(LIST), 10, 3).unwrap();
    let bytes = world.save_state();
    let expected = world.roll_leveled_items(id(LIST), 10, 30).unwrap();
    world.load_state(&bytes).unwrap();
    assert_eq!(
        world.roll_leveled_items(id(LIST), 10, 30).unwrap(),
        expected
    );
}

#[test]
fn malformed_lists_and_unmodeled_metadata_are_explicit_errors() {
    for record in [
        list(LIST, 16, 0, &[(1, A, 1)]),
        list(LIST, 0, 101, &[(1, A, 1)]),
        list(LIST, 0, 0, &[(1, A, 0)]),
        list(LIST, 0, 0, &[(1, 0xdead, 1)]),
        list(LIST, 0, 0, &[(1, A, 1)]).sub(T(b"COED"), [0; 12]),
        list(LIST, 0, 0, &[]).sub(T(b"LVLO"), [0; 8]),
    ] {
        assert!(world(vec![record])
            .roll_leveled_items(id(LIST), 1, 1)
            .is_err());
    }
}

#[test]
fn expansion_budget_prevents_unbounded_per_count_work_and_restores_rng() {
    let mut world = world(vec![list(LIST, 2, 50, &[(1, A, 1)])]);
    let before = world.save_state();
    let expected_rng = world.random_u64();
    world.load_state(&before).unwrap();
    assert!(world
        .roll_leveled_items(id(LIST), 1, i32::MAX)
        .unwrap_err()
        .to_string()
        .contains("expansion"));
    assert_eq!(world.random_u64(), expected_rng);
}

#[test]
fn winning_override_uses_its_own_namespace_for_items_and_chance_global() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(list(LIST, 0, 0, &[(1, A, 1)]));
    base.push_top(Record::new(T(b"MISC"), A));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(
        list(LIST, 0, 100, &[(1, 0x0100_0200, 3)]).sub(T(b"LVLG"), 0x0100_0300_u32.to_le_bytes()),
    );
    patch.push_top(Record::new(T(b"APPA"), 0x0100_0200));
    patch.push_top(
        Record::new(T(b"GLOB"), 0x0100_0300)
            .sub(T(b"FNAM"), [b'f'])
            .sub(T(b"FLTV"), 0_f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut world = World::new(order);
    assert_eq!(
        world.roll_leveled_items(id(LIST), 1, 2).unwrap(),
        BTreeMap::from([(GlobalFormId { file: 1, object: A }, 6)])
    );
}

#[test]
fn shared_nested_lists_are_not_cycles_and_each_call_rolls_separately() {
    let mut world = world(vec![
        list(LIST, 4, 0, &[(1, CHILD, 2), (1, CHILD, 3)]),
        list(CHILD, 0, 0, &[(1, A, 4)]),
    ]);
    assert_eq!(
        world.roll_leveled_items(id(LIST), 1, 1).unwrap(),
        BTreeMap::from([(id(A), 20)])
    );
}

#[test]
fn no_eligible_items_and_zero_count_return_empty_without_random_draws() {
    let mut world = world(vec![list(LIST, 0, 50, &[(10, A, 1)])]);
    let before = world.save_state();
    let expected_rng = world.random_u64();
    world.load_state(&before).unwrap();
    assert!(world.roll_leveled_items(id(LIST), 1, 1).unwrap().is_empty());
    assert!(world
        .roll_leveled_items(id(LIST), 10, 0)
        .unwrap()
        .is_empty());
    assert_eq!(world.random_u64(), expected_rng);
}

#[test]
fn nonfinite_global_and_invalid_request_are_rejected() {
    let global = Record::new(T(b"GLOB"), 0x300)
        .sub(T(b"FNAM"), [b'f'])
        .sub(T(b"FLTV"), f32::NAN.to_le_bytes());
    let mut world = world(vec![
        list(LIST, 0, 0, &[(1, A, 1)]).sub(T(b"LVLG"), 0x300_u32.to_le_bytes()),
        global,
    ]);
    assert!(world.roll_leveled_items(id(LIST), 1, 1).is_err());
    assert!(world.roll_leveled_items(id(LIST), 0, 1).is_err());
    assert!(world.roll_leveled_items(id(LIST), 1, -1).is_err());
}

#[test]
fn mixed_use_all_flags_are_rejected_until_precedence_is_established() {
    for flags in [5, 6, 7] {
        let mut world = world(vec![list(LIST, flags, 0, &[(1, A, 1)])]);
        assert!(world
            .roll_leveled_items(id(LIST), 1, 1)
            .unwrap_err()
            .to_string()
            .contains("unsupported LVLI flags"));
    }
}

#[test]
fn installed_ordinary_lists_resolve_or_report_declared_metadata_boundaries() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping leveled item survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let lists: Vec<_> = order
        .winners_of(T(b"LVLI"))
        .filter(|(_, record)| !record.is_deleted())
        .map(|(id, _)| id)
        .collect();
    assert!(lists.len() > 1000, "expected five-master LVLI survey");
    let special_lists: std::collections::BTreeSet<_> = order
        .winners_of(T(b"LVLI"))
        .filter(|(_, record)| {
            !record.is_deleted()
                && record
                    .find(T(b"LVLF"))
                    .is_some_and(|flags| flags[0] & 8 != 0)
        })
        .map(|(id, _)| id)
        .collect();
    assert!(
        special_lists.len() > 50,
        "expected authored Special Loot coverage"
    );
    let mut special_resolved = 0;
    let mut world = World::new(order);
    let mut resolved = 0;
    let mut boundaries = BTreeMap::new();
    for id in &lists {
        for level in [1, 20, 50] {
            world.player_level = level;
            match world.roll_leveled_items(*id, level, 1) {
                Ok(items) => {
                    assert!(items.values().all(|count| *count > 0));
                    resolved += 1;
                    special_resolved += usize::from(special_lists.contains(id));
                }
                Err(error) => {
                    let reason = error.to_string();
                    assert!(
                        reason.contains("LVLI ownership metadata is unsupported")
                            || reason.contains("unsupported LVLI flags"),
                        "{id} at {level}: {error}"
                    );
                    *boundaries.entry(reason).or_insert(0) += 1;
                }
            }
        }
    }
    assert!(
        resolved > lists.len(),
        "at least one third of list/level cases must resolve"
    );
    assert!(
        special_resolved > 150,
        "Special Loot must resolve at all three survey levels"
    );
    eprintln!(
        "LVLI survey: {} lists, {resolved} resolved cases, {special_resolved} Special Loot cases, boundaries {boundaries:?}",
        lists.len()
    );
}

fn special_settings(minimum: [f32; 4], maximum: [f32; 4], weighting: f32) -> Vec<Record> {
    [
        ("fSpecialLootMinPCLevelMult", minimum[0]),
        ("fSpecialLootMinPCLevelBase", minimum[1]),
        ("fSpecialLootMinZoneLevelBase", minimum[2]),
        ("fSpecialLootMinZoneLevelMult", minimum[3]),
        ("fSpecialLootMaxPCLevelMult", maximum[0]),
        ("fSpecialLootMaxPCLevelBase", maximum[1]),
        ("fSpecialLootMaxZoneLevelBase", maximum[2]),
        ("fSpecialLootMaxZoneLevelMult", maximum[3]),
        ("fSpecialLootWeighting", weighting),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (name, value))| {
        Record::new(T(b"GMST"), 0x400 + index as u32)
            .sub(T(b"EDID"), format!("{name}\0").into_bytes())
            .sub(T(b"DATA"), value.to_le_bytes())
    })
    .collect()
}

#[test]
fn prg_14_special_loot_combines_player_input_and_all_eight_bound_settings() {
    let bound = [2.0, 3.0, 5.0, 7.0];
    let mut records = special_settings(bound, bound, 4.0);
    records.push(list(LIST, 8, 0, &[(55, A, 1), (56, B, 2), (57, A, 3)]));
    let mut world = world(records);
    world.player_level = 10;
    assert_eq!(
        world.roll_leveled_items(id(LIST), 4, 1).unwrap(),
        BTreeMap::from([(id(B), 2)])
    );
}

#[test]
fn special_loot_per_count_recalculates_the_selection_level() {
    let mut records = special_settings([0.0, 10.0, 0.0, 0.0], [0.0, 30.0, 0.0, 0.0], 1.0);
    records.push(list(LIST, 10, 0, &[(1, A, 1), (20, B, 1)]));
    let mut world = world(records);
    let items = world.roll_leveled_items(id(LIST), 1, 1000).unwrap();
    assert_eq!(items.values().sum::<i32>(), 1000);
    assert!((400..600).contains(&items[&id(A)]));
    assert!((400..600).contains(&items[&id(B)]));
}

#[test]
fn special_loot_without_per_count_multiplies_one_level_draw() {
    let mut records = special_settings([0.0, 10.0, 0.0, 0.0], [0.0, 30.0, 0.0, 0.0], 1.0);
    records.push(list(LIST, 8, 0, &[(1, A, 1), (20, B, 1)]));
    let mut world = world(records);
    let before = world.save_state();
    world.random_u64();
    let expected_next = world.random_u64();
    world.load_state(&before).unwrap();
    let items = world.roll_leveled_items(id(LIST), 1, 1000).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items.values().sum::<i32>(), 1000);
    assert_eq!(world.random_u64(), expected_next);
}

#[test]
fn special_loot_nested_lists_receive_original_input_level() {
    let bound = [0.0, 0.0, 0.0, 2.0];
    for (child_flags, expected) in [(0, A), (8, B)] {
        let mut records = special_settings(bound, bound, 4.0);
        records.extend([
            list(LIST, 8, 0, &[(20, CHILD, 2)]),
            list(CHILD, child_flags, 0, &[(10, A, 1), (20, B, 1), (40, A, 1)]),
        ]);
        let mut world = world(records);
        assert_eq!(
            world.roll_leveled_items(id(LIST), 10, 3).unwrap(),
            BTreeMap::from([(id(expected), 6)])
        );
    }
}

#[test]
fn special_loot_all_levels_caps_formula_input_at_highest_entry() {
    let bound = [0.0, 0.0, 0.0, 0.5];
    let mut records = special_settings(bound, bound, 4.0);
    records.push(list(LIST, 11, 0, &[(10, A, 1), (40, B, 1)]));
    assert_eq!(
        world(records)
            .roll_leveled_items(id(LIST), 100, 100)
            .unwrap(),
        BTreeMap::from([(id(A), 100)])
    );
}

#[test]
fn special_loot_all_levels_cap_does_not_propagate_to_children() {
    let bound = [0.0, 0.0, 0.0, 2.0];
    let mut records = special_settings(bound, bound, 4.0);
    records.extend([
        list(LIST, 9, 0, &[(10, CHILD, 1)]),
        list(CHILD, 0, 0, &[(40, A, 1)]),
    ]);
    assert_eq!(
        world(records).roll_leveled_items(id(LIST), 50, 1).unwrap(),
        BTreeMap::from([(id(A), 1)])
    );
}

#[test]
fn special_loot_chance_none_and_use_all_bypass_level_draws() {
    for (flags, chance, expected) in [
        (8, 100, BTreeMap::new()),
        (12, 0, BTreeMap::from([(id(A), 1), (id(B), 1)])),
    ] {
        let mut world = world(vec![list(LIST, flags, chance, &[(1, A, 1), (100, B, 1)])]);
        let before = world.save_state();
        let expected_next = world.random_u64();
        world.load_state(&before).unwrap();
        assert_eq!(world.roll_leveled_items(id(LIST), 1, 1).unwrap(), expected);
        assert_eq!(world.random_u64(), expected_next);
    }
}

#[test]
fn special_loot_live_settings_and_saved_rng_reproduce_future_rolls() {
    let mut world = world(vec![list(LIST, 10, 0, &[(1, A, 1), (20, B, 1)])]);
    world.player_level = 20;
    world.settings.insert(
        "fspeciallootweighting".into(),
        soe_papyrus::Value::Float(0.5),
    );
    let before = world.save_state();
    let expected = world.roll_leveled_items(id(LIST), 10, 100).unwrap();
    world.load_state(&before).unwrap();
    assert_eq!(
        world.roll_leveled_items(id(LIST), 10, 100).unwrap(),
        expected
    );
    world.settings.insert(
        "fspeciallootweighting".into(),
        soe_papyrus::Value::Float(0.0),
    );
    assert_eq!(
        world.roll_leveled_items(id(LIST), 10, 100).unwrap(),
        BTreeMap::from([(id(B), 100)])
    );
}

#[test]
fn special_loot_invalid_settings_and_late_overflow_preserve_rng() {
    let mut world = world(vec![list(LIST, 8, 0, &[(1, A, 2)])]);
    world.settings.insert(
        "fspeciallootweighting".into(),
        soe_papyrus::Value::Float(f32::NAN),
    );
    let before = world.save_state();
    let expected_next = world.random_u64();
    world.load_state(&before).unwrap();
    assert!(world
        .roll_leveled_items(id(LIST), 10, 1)
        .unwrap_err()
        .to_string()
        .contains("Special Loot setting"));
    assert_eq!(world.random_u64(), expected_next);
    world.settings.clear();
    let before = world.save_state();
    let expected_next = world.random_u64();
    world.load_state(&before).unwrap();
    assert!(world.roll_leveled_items(id(LIST), 10, i32::MAX).is_err());
    assert_eq!(world.random_u64(), expected_next);
}

#[test]
fn special_loot_signed_truncation_and_invalid_conversion_control_eligibility() {
    for (bound, expected) in [
        (1.9, BTreeMap::from([(id(B), 1)])),
        (-0.5, BTreeMap::from([(id(A), 1)])),
        (-1.5, BTreeMap::new()),
        (f32::MAX, BTreeMap::new()),
    ] {
        let bound = [0.0, bound, 0.0, 0.0];
        let mut records = special_settings(bound, bound, 1.0);
        records.push(list(LIST, 8, 0, &[(0, A, 1), (1, B, 1), (2, A, 1)]));
        assert_eq!(
            world(records).roll_leveled_items(id(LIST), 1, 1).unwrap(),
            expected
        );
    }
}
