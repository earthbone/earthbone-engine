// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! LVSP actor initialization, record ownership and saved concrete selections.

use crate::common::installed;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    World,
    npc::{self, Config, Stats},
};
use std::collections::BTreeSet;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const BASE: u32 = 0x200;
const ROOT: u32 = 0x300;
const CHILD: u32 = 0x301;
const LOW: u32 = 0x400;
const MID: u32 = 0x401;
const HIGH: u32 = 0x402;
const SHOUT: u32 = 0x403;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn ids(objects: &[u32]) -> BTreeSet<GlobalFormId> {
    objects.iter().copied().map(id).collect()
}

fn list(form: u32, flags: u8, chance: u8, entries: &[(u16, u32, u16)]) -> Record {
    let mut record = Record::new(T(b"LVSP"), form)
        .sub(T(b"LVLF"), [flags])
        .sub(T(b"LVLD"), [chance])
        .sub(T(b"LLCT"), [entries.len() as u8]);
    for &(level, spell, count) in entries {
        let mut bytes = vec![0; 12];
        bytes[..2].copy_from_slice(&level.to_le_bytes());
        bytes[4..8].copy_from_slice(&spell.to_le_bytes());
        bytes[8..10].copy_from_slice(&count.to_le_bytes());
        record = record.sub(T(b"LVLO"), bytes);
    }
    record
}

fn actor_base(form: u32, spells: &[u32], template_flags: u16) -> Record {
    let mut record = npc::build_record(
        form,
        "Caster",
        "Caster",
        Config {
            level: 10,
            template_flags,
            ..Config::default()
        },
        Stats::default(),
    );
    for spell in spells {
        record = record.sub(T(b"SPLO"), spell.to_le_bytes());
    }
    record
}

fn plugin(records: Vec<Record>, spells: &[u32]) -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(actor_base(BASE, spells, 0));
    for actor in [ACTOR, ACTOR + 1] {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    for spell in [LOW, MID, HIGH] {
        plugin.push_top(Record::new(T(b"SPEL"), spell));
    }
    plugin.push_top(Record::new(T(b"SHOU"), SHOUT));
    for record in records {
        plugin.push_top(record);
    }
    plugin
}

fn world(records: Vec<Record>, spells: &[u32]) -> World {
    let mut order = LoadOrder::new();
    order.add("Spells.esm", plugin(records, spells)).unwrap();
    World::new(order)
}

#[test]
fn nested_lists_use_actor_level_and_preserve_direct_spells_and_shouts() {
    let mut world = world(
        vec![
            list(ROOT, 0, 0, &[(1, LOW, 1), (10, CHILD, 1), (20, HIGH, 1)]),
            list(CHILD, 0, 0, &[(1, MID, 1), (15, HIGH, 1)]),
        ],
        &[ROOT, LOW, SHOUT],
    );
    world.player_level = 50;
    let actor = world.actor(id(ACTOR)).unwrap();
    assert_eq!(actor.level, 10);
    assert_eq!(actor.spells, ids(&[LOW, MID]));
    assert_eq!(actor.shouts, ids(&[SHOUT]));
}

#[test]
fn use_all_respects_highest_level_or_all_levels_flag() {
    let mut world = world(
        vec![
            list(ROOT, 4, 0, &[(1, LOW, 1), (10, MID, 1), (20, HIGH, 1)]),
            list(CHILD, 5, 0, &[(1, LOW, 1), (10, MID, 1), (20, HIGH, 1)]),
        ],
        &[],
    );
    assert_eq!(
        world.select_leveled_spells(id(ROOT), 10).unwrap(),
        ids(&[MID])
    );
    assert_eq!(
        world.select_leveled_spells(id(CHILD), 10).unwrap(),
        ids(&[LOW, MID])
    );
    assert!(world.select_leveled_spells(id(CHILD), 0).is_err());
}

#[test]
fn duplicate_entries_retain_weight_and_lower_levels_remain_candidates() {
    let mut world = world(
        vec![list(
            ROOT,
            1,
            0,
            &[(1, LOW, 1), (1, LOW, 1), (1, LOW, 1), (10, MID, 1)],
        )],
        &[],
    );
    let mut low = 0;
    // Set before the first run: 512 draws must show both candidates and the
    // three repeated entries must receive between 65 and 85 percent of draws.
    for _ in 0..512 {
        let selected = world.select_leveled_spells(id(ROOT), 10).unwrap();
        assert_eq!(selected.len(), 1);
        low += usize::from(selected.contains(&id(LOW)));
    }
    assert!((333..=435).contains(&low), "low selected {low} times");
}

#[test]
fn nested_count_rolls_each_only_when_the_child_requests_it() {
    let mut world = world(
        vec![
            list(ROOT, 0, 0, &[(1, CHILD, 32)]),
            list(CHILD, 0, 0, &[(1, LOW, 1), (1, MID, 1)]),
        ],
        &[],
    );
    assert_eq!(world.select_leveled_spells(id(ROOT), 10).unwrap().len(), 1);
    world.leveled_list(id(CHILD)).unwrap().flags = 2;
    assert_eq!(
        world.select_leveled_spells(id(ROOT), 10).unwrap(),
        ids(&[LOW, MID])
    );
}

#[test]
fn saved_actor_keeps_its_selected_spells_after_list_changes() {
    let fixture = || vec![list(ROOT, 0, 0, &[(1, LOW, 1), (1, MID, 1)])];
    let mut original = world(fixture(), &[ROOT]);
    let selected = original.actor(id(ACTOR)).unwrap().spells.clone();
    let saved = original.save_state();
    let expected_rng = original.random_u64();
    let mut restored = world(fixture(), &[ROOT]);
    restored.load_state(&saved).unwrap();
    restored.player_level = 50;
    restored.leveled_list(id(ROOT)).unwrap().entries.clear();
    assert_eq!(restored.actor(id(ACTOR)).unwrap().spells, selected);
    assert_eq!(restored.random_u64(), expected_rng);
    assert!(restored.actor(id(ACTOR + 1)).unwrap().spells.is_empty());
}

#[test]
fn legacy_saved_list_placeholders_expand_once_and_preserve_granted_spells() {
    let fixture = || vec![list(ROOT, 0, 0, &[(1, MID, 1), (1, HIGH, 1)])];
    let mut legacy = world(fixture(), &[ROOT]);
    legacy.actor(id(ACTOR)).unwrap().spells = ids(&[ROOT, LOW]);
    let saved = legacy.save_state();
    let mut expected = world(fixture(), &[ROOT]);
    expected.load_state(&saved).unwrap();
    let mut selected = expected.select_leveled_spells(id(ROOT), 10).unwrap();
    selected.insert(id(LOW));
    let expected_rng = expected.random_u64();
    let mut restored = world(fixture(), &[ROOT]);
    restored.load_state(&saved).unwrap();
    assert!(
        restored
            .actor_ref(id(ACTOR))
            .unwrap()
            .spells
            .contains(&id(ROOT))
    );
    assert_eq!(restored.actor(id(ACTOR)).unwrap().spells, selected);
    assert_eq!(restored.actor(id(ACTOR)).unwrap().spells, selected);
    let saved = restored.save_state();
    let mut continued = world(fixture(), &[ROOT]);
    continued.load_state(&saved).unwrap();
    assert_eq!(continued.actor(id(ACTOR)).unwrap().spells, selected);
    assert_eq!(continued.random_u64(), expected_rng);
}

#[test]
fn chance_none_is_saved_without_repeating_initialization() {
    let fixture = || vec![list(ROOT, 0, 100, &[(1, LOW, 1)])];
    let mut original = world(fixture(), &[ROOT]);
    assert!(original.actor(id(ACTOR)).unwrap().spells.is_empty());
    let saved = original.save_state();
    let mut restored = world(fixture(), &[ROOT]);
    restored.load_state(&saved).unwrap();
    restored.leveled_list(id(ROOT)).unwrap().chance_none = 0;
    assert!(restored.actor(id(ACTOR)).unwrap().spells.is_empty());
    assert_eq!(restored.actor(id(ACTOR + 1)).unwrap().spells, ids(&[LOW]));
}

#[test]
fn malformed_or_cyclic_selection_restores_rng_and_installs_no_actor() {
    let cases = vec![
        vec![list(ROOT, 8, 0, &[(1, LOW, 1)])],
        vec![list(ROOT, 0, 101, &[(1, LOW, 1)])],
        vec![list(ROOT, 0, 0, &[(1, LOW, 0)])],
        vec![list(ROOT, 0, 0, &[(1, 0x999, 1)])],
        vec![list(ROOT, 0, 0, &[(1, SHOUT, 1)])],
        vec![
            list(ROOT, 0, 0, &[(1, CHILD, 1)]),
            list(CHILD, 0, 0, &[(1, ROOT, 1)]),
        ],
        vec![Record::new(T(b"LVSP"), ROOT).sub(T(b"LVLO"), [0; 4])],
        vec![list(ROOT, 0, 0, &[(1, LOW, 1)]).sub(T(b"LVLG"), 0u32.to_le_bytes())],
    ];
    for records in cases {
        let mut untouched = world(records.clone(), &[ROOT]);
        let expected_rng = untouched.random_u64();
        let mut broken = world(records, &[ROOT]);
        assert!(broken.select_leveled_spells(id(ROOT), 10).is_err());
        assert!(broken.actor(id(ACTOR)).is_none());
        assert!(broken.actor_ref(id(ACTOR)).is_none());
        assert_eq!(broken.random_u64(), expected_rng);
    }
}

#[test]
fn actor_initialization_rolls_back_earlier_roots_after_a_later_failure() {
    let fixture = || {
        vec![
            list(ROOT, 0, 0, &[(1, LOW, 1), (1, MID, 1)]),
            list(CHILD, 0, 0, &[(1, CHILD, 1)]),
        ]
    };
    let mut untouched = world(fixture(), &[ROOT, CHILD]);
    let expected_rng = untouched.random_u64();
    let mut broken = world(fixture(), &[ROOT, CHILD]);
    assert!(broken.actor(id(ACTOR)).is_none());
    assert_eq!(broken.random_u64(), expected_rng);
    let mut public = world(
        vec![
            list(ROOT, 4, 0, &[(1, CHILD, 1), (1, 0x999, 1)]),
            list(CHILD, 0, 0, &[(1, LOW, 1), (1, MID, 1)]),
        ],
        &[],
    );
    assert!(public.select_leveled_spells(id(ROOT), 10).is_err());
    assert_eq!(public.random_u64(), expected_rng);
}

#[test]
fn excessive_depth_and_expansion_return_errors() {
    let deep: Vec<_> = (0..129)
        .map(|offset| {
            list(
                ROOT + offset,
                0,
                0,
                &[(
                    1,
                    if offset == 128 {
                        LOW
                    } else {
                        ROOT + offset + 1
                    },
                    1,
                )],
            )
        })
        .collect();
    assert!(
        world(deep, &[])
            .select_leveled_spells(id(ROOT), 10)
            .is_err()
    );
    let wide: Vec<_> = (0..17)
        .map(|offset| {
            let next = if offset == 16 { LOW } else { ROOT + offset + 1 };
            list(ROOT + offset, 4, 0, &[(1, next, 1), (1, next, 1)])
        })
        .collect();
    assert!(
        world(wide, &[])
            .select_leveled_spells(id(ROOT), 10)
            .is_err()
    );
}

#[test]
fn installed_leveled_spell_lists_resolve_only_concrete_spells() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping leveled spell survey");
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
    let lists: Vec<_> = order
        .winners_of(T(b"LVSP"))
        .filter(|(_, record)| !record.is_deleted())
        .map(|(id, _)| id)
        .collect();
    assert!(!lists.is_empty(), "installed load order contains no LVSP");
    let mut world = World::from_shared_order(order);
    let mut selected = 0;
    let mut flags = std::collections::BTreeMap::<u8, usize>::new();
    for list in &lists {
        *flags
            .entry(world.leveled_list(*list).unwrap().flags)
            .or_default() += 1;
        for level in [1, 10, 50] {
            let spells = world
                .select_leveled_spells(*list, level)
                .unwrap_or_else(|error| panic!("{list} at level {level}: {error}"));
            for spell in &spells {
                assert_eq!(world.tag_of(*spell), Some(T(b"SPEL")));
            }
            selected += spells.len();
        }
    }
    eprintln!(
        "LVSP survey: {} lists, {} selections, {selected} concrete results, flags {flags:?}",
        lists.len(),
        lists.len() * 3
    );
}

#[test]
fn inherited_spell_list_uses_winning_plugin_links() {
    let mut base = plugin(vec![list(ROOT, 0, 0, &[(1, LOW, 1)])], &[]);
    base.push_top(actor_base(BASE + 1, &[ROOT], 0));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Spells.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(
        actor_base(BASE, &[], npc::template::SPELLS).sub(T(b"TPLT"), (BASE + 1).to_le_bytes()),
    );
    patch.push_top(list(ROOT, 0, 0, &[(1, 0x01000420, 1)]));
    patch.push_top(Record::new(T(b"SPEL"), 0x01000420));
    let mut order = LoadOrder::new();
    order.add("Spells.esm", base).unwrap();
    order.add("Override.esp", patch).unwrap();
    let mut world = World::new(order);
    assert_eq!(
        world.actor(id(ACTOR)).unwrap().spells,
        BTreeSet::from([GlobalFormId {
            file: 1,
            object: 0x420
        }])
    );
}
