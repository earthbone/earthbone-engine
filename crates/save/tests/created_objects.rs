// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UESP/skymp type-4 fields, exact encodings and external corpus coverage.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use soe_save::{CreatedObjects, Error, GlobalData, Save};

fn fixture() -> Vec<u8> {
    let mut bytes = vec![4, 0x80, 1, 0];
    bytes.extend(7_u32.to_le_bytes());
    bytes.extend([5, 0, 0x40, 0x12, 0x34]);
    bytes.extend(0x8000_0000_u32.to_le_bytes());
    bytes.extend(0x1122_3344_u32.to_le_bytes());
    bytes.extend(0x5566_7788_u32.to_le_bytes());
    bytes.extend(0x7FC0_0123_u32.to_le_bytes());

    bytes.extend([5, 0, 0, 0, 1]);
    bytes.extend(u32::MAX.to_le_bytes());
    bytes.extend([6, 0, 0, 0, 0, 0, 2]);
    bytes.extend(0xFF80_0000_u32.to_le_bytes());
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend(3_u32.to_le_bytes());
    bytes.extend(0x7F80_0000_u32.to_le_bytes());

    bytes.extend([6, 0, 0, 0, 0x40, 0xFF, 0]);
    bytes.extend(11_u32.to_le_bytes());
    bytes.extend([4, 0x80, 0xFF, 2]);
    bytes.extend(3.5_f32.to_le_bytes());
    bytes.extend(17_u32.to_le_bytes());
    bytes.extend(5_u32.to_le_bytes());
    bytes.extend((-1.0_f32).to_le_bytes());

    bytes.extend([4, 0x80, 1, 3]);
    bytes.extend(0_u32.to_le_bytes());
    bytes.extend([1, 0]);
    bytes
}

#[test]
fn sav_65_four_tables_retain_field_order_count_widths_and_float_bits() {
    let bytes = fixture();
    let objects = CreatedObjects::read(&bytes).unwrap();
    let tables = [
        &objects.weapon_enchantments,
        &objects.armor_enchantments,
        &objects.potions,
        &objects.poisons,
    ];
    for (table, form, count) in [
        (0, 0x800100, 7),
        (1, 0x000001, u32::MAX),
        (2, 0x40FF00, 11),
        (3, 0x800103, 0),
    ] {
        assert_eq!(tables[table].len(), 1);
        assert_eq!(tables[table][0].form.raw(), form);
        assert_eq!(tables[table][0].times_used, count);
    }
    let weapon = &objects.weapon_enchantments[0].effects[0];
    assert_eq!(weapon.effect.raw(), 0x401234);
    assert_eq!(weapon.magnitude.to_bits(), 0x8000_0000);
    assert_eq!(weapon.duration, 0x1122_3344);
    assert_eq!(weapon.area, 0x5566_7788);
    assert_eq!(weapon.price.to_bits(), 0x7FC0_0123);
    let armor = &objects.armor_enchantments[0].effects[0];
    assert_eq!(armor.effect.raw(), 2);
    assert_eq!(armor.magnitude.to_bits(), 0xFF80_0000);
    assert_eq!((armor.duration, armor.area), (2, 3));
    assert_eq!(armor.price.to_bits(), 0x7F80_0000);
    let potion = &objects.potions[0].effects[0];
    assert_eq!(potion.effect.raw(), 0x80FF02);
    assert_eq!(potion.magnitude.to_bits(), 3.5_f32.to_bits());
    assert_eq!((potion.duration, potion.area), (17, 5));
    assert_eq!(potion.price.to_bits(), (-1.0_f32).to_bits());
    assert!(objects.poisons[0].effects.is_empty());
    assert_eq!(objects.data_bytes().unwrap(), bytes);
}

#[test]
fn sav_65_empty_tables_preserve_wider_zero_counts() {
    let bytes = [0, 1, 0, 2, 0, 0, 0, 0];
    let objects = CreatedObjects::read(&bytes).unwrap();
    assert!(objects.weapon_enchantments.is_empty());
    assert!(objects.armor_enchantments.is_empty());
    assert!(objects.potions.is_empty());
    assert!(objects.poisons.is_empty());
    assert_eq!(objects.data_bytes().unwrap(), bytes);
}

#[test]
fn sav_65_encoding_uses_edited_fields_and_rejects_resized_lists() {
    let mut objects = CreatedObjects::read(&fixture()).unwrap();
    objects.potions[0].effects[0].duration = 901;
    let again = CreatedObjects::read(&objects.data_bytes().unwrap()).unwrap();
    assert_eq!(again.potions[0].effects[0].duration, 901);
    objects.potions[0].effects.clear();
    assert!(matches!(
        objects.data_bytes(),
        Err(Error::CreatedObjects { .. })
    ));
    let mut objects = CreatedObjects::read(&fixture()).unwrap();
    objects.weapon_enchantments.clear();
    assert!(matches!(
        objects.data_bytes(),
        Err(Error::CreatedObjects { .. })
    ));
}

#[test]
fn sav_65_truncation_bad_widths_impossible_counts_and_trailing_bytes_fail() {
    let bytes = fixture();
    for cut in 0..bytes.len() {
        assert!(CreatedObjects::read(&bytes[..cut]).is_err(), "cut={cut}");
    }
    for bytes in [
        vec![3],
        vec![0, 0, 0, 3],
        vec![0xFE, 0xFF, 0xFF, 0xFF, 0, 0, 0],
        vec![4, 0x80, 0, 1, 0, 0, 0, 0, 0xFE, 0xFF, 0xFF, 0xFF, 0, 0, 0],
        vec![4, 0x80, 0, 1, 0, 0, 0, 0, 3, 0, 0, 0],
        vec![0, 0, 0, 0, 0],
    ] {
        assert!(CreatedObjects::read(&bytes).is_err(), "{bytes:?}");
    }
}

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_65_external_created_objects_reencode_with_complete_accounting() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping Created Objects corpus");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "coverage requires at least 50 saves");
    let mut object_counts = [0; 4];
    let mut effect_counts = [0; 4];
    let mut object_kinds = BTreeMap::new();
    let mut versions = BTreeMap::new();
    let mut changes = BTreeMap::new();
    let mut matches = BTreeMap::new();
    let mut duplicate_ids = 0;
    let mut unresolved_ids = 0;
    let mut unresolved_effects = 0;
    let mut payload_bytes = 0;
    for path in &paths {
        let save = Save::open(path).unwrap();
        let blocks: Vec<_> = save
            .table_1
            .iter()
            .filter(|block| block.kind == GlobalData::CREATED_OBJECTS)
            .collect();
        assert_eq!(blocks.len(), 1, "one type-4 block required per save");
        let objects = save.created_objects().unwrap().unwrap();
        assert_eq!(objects.data_bytes().unwrap(), blocks[0].data);
        payload_bytes += blocks[0].data.len();
        *versions.entry(save.form_version).or_insert(0) += 1;
        let mut ids = BTreeSet::new();
        for (table, objects) in [
            objects.weapon_enchantments,
            objects.armor_enchantments,
            objects.potions,
            objects.poisons,
        ]
        .into_iter()
        .enumerate()
        {
            object_counts[table] += objects.len();
            for object in objects {
                effect_counts[table] += object.effects.len();
                *object_kinds
                    .entry((table, object.form.raw() >> 22))
                    .or_insert(0) += 1;
                if let Some(id) = save.resolve(object.form) {
                    duplicate_ids += usize::from(!ids.insert(id));
                } else {
                    unresolved_ids += 1;
                }
                for effect in object.effects {
                    unresolved_effects += usize::from(save.resolve(effect.effect).is_none());
                }
            }
        }
        for form in &save.change_forms {
            if let Some(id) = save.resolve(form.form) {
                if id >> 24 == 0xFF {
                    *changes
                        .entry((form.record_type(), form.change_flags))
                        .or_insert(0) += 1;
                }
                if ids.contains(&id) {
                    *matches
                        .entry((form.record_type(), form.change_flags))
                        .or_insert(0) += 1;
                }
            }
        }
    }
    eprintln!(
        "SAV-65: saves={} payload_bytes={payload_bytes} versions={versions:?}",
        paths.len()
    );
    eprintln!("table order: weapon enchantments, armor enchantments, potions, poisons");
    eprintln!("objects={object_counts:?} effects={effect_counts:?} object_reference_kinds={object_kinds:?}");
    eprintln!("duplicate_ids={duplicate_ids} unresolved_ids={unresolved_ids} unresolved_effects={unresolved_effects}");
    eprintln!("created_changeforms={changes:?} table_identity_changeforms={matches:?}");
}
