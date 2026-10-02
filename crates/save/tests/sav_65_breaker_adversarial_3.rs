// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use soe_save::{CreatedObjects, GlobalData, Save};

fn count(value: u32) -> Vec<u8> {
    if value <= 0x3f {
        vec![(value << 2) as u8]
    } else {
        (((value << 2) | 1).to_le_bytes())[..2].to_vec()
    }
}

fn object() -> [u8; 8] {
    [0x40, 0, 1, 0, 0, 0, 0, 0]
}

#[test]
fn sav_65_table_counts_cross_the_one_to_two_byte_vsval_boundary() {
    let mut bytes = Vec::new();
    for amount in [63, 64, 0, 0] {
        bytes.extend(count(amount));
        for _ in 0..amount {
            bytes.extend(object());
        }
    }

    let decoded = CreatedObjects::read(&bytes).unwrap();
    assert_eq!(decoded.weapon_enchantments.len(), 63);
    assert_eq!(decoded.armor_enchantments.len(), 64);
    assert!(decoded.potions.is_empty());
    assert!(decoded.poisons.is_empty());
    assert_eq!(decoded.data_bytes().unwrap(), bytes);
}

fn collect_saves(path: &Path, saves: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, saves);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
        {
            saves.push(path);
        }
    }
}

#[test]
fn sav_65_external_corpus_accounts_for_populated_fields_in_each_table() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping Created Objects corpus");
        return;
    };
    let mut paths = Vec::new();
    collect_saves(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut table_objects = [0_usize; 4];
    for path in &paths {
        let save = Save::open(path).unwrap();
        let blocks: Vec<_> = save
            .table_1
            .iter()
            .filter(|block| block.kind == GlobalData::CREATED_OBJECTS)
            .collect();
        assert_eq!(blocks.len(), 1, "one type-4 block required in {path:?}");
        let objects = save.created_objects().unwrap().unwrap();
        assert_eq!(objects.data_bytes().unwrap(), blocks[0].data);
        table_objects[0] += objects.weapon_enchantments.len();
        table_objects[1] += objects.armor_enchantments.len();
        table_objects[2] += objects.potions.len();
        table_objects[3] += objects.poisons.len();
    }

    eprintln!(
        "SAV-65 corpus: saves={} object_counts={table_objects:?}",
        paths.len()
    );
}
