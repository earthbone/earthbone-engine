// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Inventory entries contain count deltas and independent item-stack extras.

use soe_save::{reference::Field, ChangeForm, RefId, Save};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn fixture() -> ChangeForm {
    let mut data = vec![8];
    reference(0x400800).write(&mut data);
    data.extend((-3_i32).to_le_bytes());
    data.push(8);
    data.extend([8, 22, 37]);
    data.extend(1.5_f32.to_le_bytes());
    data.extend([12, 23, 36]);
    data.extend(2_i16.to_le_bytes());
    data.push(40);
    data.extend(f32::from_bits(0x7FC00042).to_le_bytes());
    reference(0x400801).write(&mut data);
    data.extend(7_i32.to_le_bytes());
    data.push(0);
    ChangeForm {
        form: reference(0x400900),
        change_flags: 1 << 5,
        type_field: 0,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn stack_nesting_keeps_negative_deltas_separate_from_instance_counts() {
    let change = fixture();
    let decoded = change.reference_contents(78).unwrap().unwrap();
    assert_eq!(decoded.remaining_bytes, 0);
    assert_eq!(decoded.data_bytes().unwrap(), change.data);
    let inventory = decoded.inventory.unwrap();
    assert_eq!(inventory.entries.len(), 2);
    assert_eq!(inventory.entries[0].delta, -3);
    assert_eq!(inventory.entries[0].stacks.len(), 2);
    assert_eq!(inventory.entries[0].stacks[0].entries[0].kind, 22);
    assert!(matches!(
        inventory.entries[0].stacks[1].entries[1].fields[0],
        Field::Short(2)
    ));
    assert_eq!(inventory.entries[1].delta, 7);
    assert!(inventory.entries[1].stacks.is_empty());
}

#[test]
fn truncated_nested_arrays_and_edited_counts_fail() {
    let change = fixture();
    for cut in 0..change.data.len() {
        let mut truncated = change.clone();
        truncated.data.truncate(cut);
        assert!(truncated.reference_contents(78).is_err(), "{cut}");
    }
    let mut decoded = change.reference_contents(78).unwrap().unwrap();
    decoded.inventory.as_mut().unwrap().entries[0].stacks.pop();
    assert!(decoded.data_bytes().is_err());
    let mut huge = change;
    huge.data = vec![0xFE, 0xFF, 0xFF];
    assert!(huge.reference_contents(78).is_err());
}

#[test]
fn absent_inventory_and_unconsumed_actor_state_remain_explicit() {
    let mut change = fixture();
    change.type_field = 1;
    change.data.splice(0..0, [0; 8]);
    change.data.extend([0xAA, 0xBB]);
    let decoded = change.reference_contents(78).unwrap().unwrap();
    assert_eq!(decoded.start, 8);
    assert_eq!(decoded.remaining_bytes, 2);
    assert_eq!(
        decoded.data_bytes().unwrap(),
        change.data[decoded.start..decoded.end]
    );
    change.change_flags = 0;
    change.data = vec![0; 8];
    let decoded = change.reference_contents(78).unwrap().unwrap();
    assert!(decoded.inventory.is_none() && decoded.extra.is_none());
}

#[test]
fn leveled_choice_reader_stops_at_its_declared_extra_without_accepting_later_unknown_data() {
    let mut change = fixture();
    change.type_field = 1;
    change.change_flags = 1 << 18;
    change.data = vec![0; 8];
    change.data.extend([8, 45]);
    reference(0x400810).write(&mut change.data);
    reference(0x400811).write(&mut change.data);
    change.data.extend(0_u32.to_le_bytes());
    let end = change.data.len();
    change.data.extend([108, 18]);
    let selected = change.leveled_creature(78).unwrap().unwrap();
    assert_eq!(selected.original_base, reference(0x400810));
    assert_eq!(selected.template_base, reference(0x400811));
    assert_eq!(selected.bytes_consumed, end);
    assert!(selected.npc.data_bytes().unwrap().is_empty());
    assert!(change.reference_contents(78).is_err());
    for cut in 0..end {
        let mut truncated = change.clone();
        truncated.data.truncate(cut);
        assert!(truncated.leveled_creature(78).is_err());
    }
    change.data = vec![0; 9];
    assert!(change.leveled_creature(78).is_err());
    change.change_flags = 0;
    assert!(change.leveled_creature(78).unwrap().is_none());
}

#[test]
fn empty_custom_package_preserves_following_extra_and_inventory_boundaries() {
    for package in [vec![108, 255], vec![108, 18, 0, 0, 0]] {
        let mut change = fixture();
        change.change_flags |= 1 << 11;
        let inventory = change.data.clone();
        change.data = vec![8];
        change.data.extend(&package);
        change.data.push(133);
        reference(0x800900).write(&mut change.data);
        change.data.extend(&inventory);
        let decoded = change.reference_contents(78).unwrap().unwrap();
        assert_eq!(decoded.data_bytes().unwrap(), change.data);
        assert_eq!(decoded.remaining_bytes, 0);
        let extra = decoded.extra.unwrap();
        assert_eq!(extra.entries[0].kind, 108);
        assert_eq!(extra.entries[1].kind, 133);
        assert!(
            matches!(extra.entries[1].fields[0], Field::Reference(id) if id == reference(0x800900))
        );
        let items = decoded.inventory.unwrap().entries;
        assert_eq!(items[0].item, reference(0x400800));
        assert_eq!(items[0].delta, -3);
        assert_eq!(items[0].stacks.len(), 2);
        assert_eq!(items[1].item, reference(0x400801));
        assert_eq!(items[1].delta, 7);
        for cut in 0..change.data.len() {
            let mut truncated = change.clone();
            truncated.data.truncate(cut);
            assert!(
                truncated.reference_contents(78).is_err(),
                "{package:?} at {cut}"
            );
        }
    }
}

#[test]
fn nonempty_or_unknown_packages_cannot_be_mistaken_for_empty_state() {
    let mut change = fixture();
    change.change_flags = 1 << 11;
    for kind in [0, 17, 19, 254] {
        change.data = vec![4, 108, kind, 0, 0, 0];
        assert!(change.reference_contents(78).is_err());
    }
    for prefix in [1, 128, 255] {
        change.data = vec![4, 108, 18, prefix, 0, 0];
        assert!(change.reference_contents(78).is_err());
    }
    for name in ["Sequence", "Stacked", "Unknown"] {
        change.data = vec![4, 108, 18, 0];
        soe_save::WString::from_text(name).write(&mut change.data);
        change.data.extend([0; 8]);
        let error = change.reference_contents(78).unwrap_err().to_string();
        assert!(error.contains(name), "{error}");
    }
}

#[test]
fn sav_21_stacked_state_stays_rejected_until_its_boundary_is_known() {
    let mut change = fixture();
    change.change_flags = 1 << 11;
    change.data = vec![4, 108, 18, 0];
    soe_save::WString::from_text("Stacked").write(&mut change.data);
    change.data.extend([0x00, 0x01, 0x02, 0x03, 0x04, 0x05]);

    let error = change.reference_contents(78).unwrap_err().to_string();
    assert!(error.contains("Stacked"), "{error}");
}

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "ess") {
            paths.push(path);
        }
    }
}

#[test]
fn corpus_reference_contents_decode_with_exact_typed_reencoding() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping reference contents corpus");
        return;
    };
    let mut paths = vec![];
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let (mut forms, mut inventories, mut entries, mut stacks, mut tails) = (0, 0, 0, 0, 0);
    let mut kinds = BTreeMap::new();
    let mut errors = BTreeMap::<String, usize>::new();
    let mut examples = vec![];
    let (mut negatives, mut zeroes) = (0, 0);
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let contents = match form.reference_contents(save.form_version) {
                Ok(Some(value)) => value,
                Ok(None) => continue,
                Err(error) => {
                    *errors.entry(error.to_string()).or_default() += 1;
                    if examples.len() < 20 {
                        examples.push((
                            path.file_name().unwrap().to_string_lossy().into_owned(),
                            form.form,
                            form.change_flags,
                            error.to_string(),
                        ));
                    }
                    continue;
                }
            };
            let data = form.payload().unwrap();
            assert_eq!(
                contents.data_bytes().unwrap(),
                data[contents.start..contents.end]
            );
            if let Some(extra) = contents.extra {
                for value in extra.entries {
                    *kinds.entry(value.kind).or_insert(0) += 1;
                }
            }
            if let Some(inventory) = contents.inventory {
                inventories += 1;
                for entry in inventory.entries {
                    entries += 1;
                    stacks += entry.stacks.len();
                    if entry.delta < 0 {
                        negatives += 1;
                    }
                    if entry.delta == 0 {
                        zeroes += 1;
                    }
                    for stack in entry.stacks {
                        for value in stack.entries {
                            *kinds.entry(value.kind).or_insert(0) += 1;
                        }
                    }
                }
            }
            if contents.remaining_bytes != 0 {
                tails += 1;
            }
            forms += 1;
        }
    }
    eprintln!("{} saves: {forms} reference contents, {inventories} inventories, {entries} entries, {stacks} stacks, {negatives} negative and {zeroes} zero deltas, {tails} remaining tails", paths.len());
    eprintln!("Extra types: {kinds:?}");
    eprintln!("Errors: {errors:?}\nExamples: {examples:?}");
    assert!(forms > 100000);
    assert!(errors.is_empty(), "every reference inventory must decode");
}
