// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::CreatedObjects;
use std::path::{Path, PathBuf};

#[test]
fn sav_65_zero_count_widths_survive_independently_in_all_four_tables() {
    let bytes = [0, 1, 0, 2, 0, 0, 0, 0];
    let decoded = CreatedObjects::read(&bytes).unwrap();
    assert!(decoded.weapon_enchantments.is_empty());
    assert!(decoded.armor_enchantments.is_empty());
    assert!(decoded.potions.is_empty());
    assert!(decoded.poisons.is_empty());
    assert_eq!(decoded.data_bytes().unwrap(), bytes);
}

#[test]
fn sav_65_effect_payload_truncations_never_shift_the_next_table() {
    let mut object = vec![4, 0x40, 0x01, 0, 0, 0, 0, 0, 4];
    object.extend([0x40, 0x02, 0]);
    object.extend(1.25_f32.to_le_bytes());
    object.extend(0x1234_u32.to_le_bytes());
    object.extend(0x5678_u32.to_le_bytes());
    object.extend((-0.0_f32).to_le_bytes());
    let mut payload = object.clone();
    payload.extend([0, 0, 0]);
    assert_eq!(CreatedObjects::read(&payload).unwrap().data_bytes().unwrap(), payload);
    for cut in 0..object.len() {
        assert!(CreatedObjects::read(&object[..cut]).is_err(), "cut={cut}");
    }
}

#[test]
fn sav_65_unknown_variable_count_tag_and_trailing_data_are_rejected() {
    for bytes in [&[3, 0, 0, 0][..], &[0, 0, 0, 0, 0][..]] {
        assert!(CreatedObjects::read(bytes).is_err(), "{bytes:?}");
    }
}

fn collect_saves(root: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_65_external_corpus_accounts_for_every_created_objects_payload() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping external corpus");
        return;
    };
    let mut paths = Vec::new();
    collect_saves(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "found only {} saves", paths.len());

    let mut objects = 0;
    let mut effects = 0;
    let mut payload_bytes = 0;
    for path in &paths {
        let save = soe_save::Save::open(path).unwrap();
        let blocks: Vec<_> = save
            .table_1
            .iter()
            .filter(|block| block.kind == soe_save::GlobalData::CREATED_OBJECTS)
            .collect();
        assert_eq!(blocks.len(), 1, "one type-4 block required per save");
        let decoded = save.created_objects().unwrap().unwrap();
        assert_eq!(decoded.data_bytes().unwrap(), blocks[0].data);
        payload_bytes += blocks[0].data.len();
        for table in [
            &decoded.weapon_enchantments,
            &decoded.armor_enchantments,
            &decoded.potions,
            &decoded.poisons,
        ] {
            objects += table.len();
            effects += table.iter().map(|object| object.effects.len()).sum::<usize>();
        }
    }
    assert!(payload_bytes > 0, "no Created Objects payload bytes accounted for");
    eprintln!(
        "SAV-65 corpus: saves={} objects={objects} effects={effects} payload_bytes={payload_bytes}",
        paths.len()
    );
}
