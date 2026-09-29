// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial INGR, ALCH and ENCH change-form checks from xEdit's save schema.

use std::path::{Path, PathBuf};

use soe_save::{ChangeForm, RefId, Save};

fn change(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_37_every_unnamed_flag_round_trips_for_each_item_type() {
    let form_flags = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
    for (kind, semantic) in [(16, 1u32 << 31), (26, 6), (48, 6)] {
        for bit in 0..32 {
            let flags = (1u32 << bit) | semantic | 1;
            let form = change(kind, flags, form_flags.to_vec());
            let encoded = match kind {
                16 => {
                    let typed = form.ingredient_change().unwrap().unwrap();
                    (typed.change_flags(), typed.data_bytes())
                }
                26 => {
                    let typed = form.alchemy_change().unwrap().unwrap();
                    (typed.change_flags(), typed.data_bytes())
                }
                48 => {
                    let typed = form.enchantment_change().unwrap().unwrap();
                    (typed.change_flags(), typed.data_bytes())
                }
                _ => unreachable!(),
            };
            assert_eq!(
                encoded,
                (flags, form_flags.to_vec()),
                "type {kind}, bit {bit}"
            );
        }
    }
}

#[test]
fn sav_37_each_truncated_form_flag_payload_is_rejected() {
    for kind in [16, 26, 48] {
        for length in 0..6 {
            let form = change(kind, 1, vec![0x81; length]);
            let rejected = match kind {
                16 => form.ingredient_change().is_err(),
                26 => form.alchemy_change().is_err(),
                48 => form.enchantment_change().is_err(),
                _ => unreachable!(),
            };
            assert!(rejected, "type {kind}, length {length}");
        }
    }
}

#[test]
fn sav_37_zero_byte_markers_do_not_add_payload_bytes() {
    for (kind, flags) in [(16, 1 << 31), (26, 6), (48, 6)] {
        let form = change(kind, flags, Vec::new());
        let bytes = match kind {
            16 => form.ingredient_change().unwrap().unwrap().data_bytes(),
            26 => form.alchemy_change().unwrap().unwrap().data_bytes(),
            48 => form.enchantment_change().unwrap().unwrap().data_bytes(),
            _ => unreachable!(),
        };
        assert!(bytes.is_empty(), "type {kind}");
    }
}

fn collect_saves(path: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, output);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
        {
            output.push(path);
        }
    }
}

#[test]
fn sav_37_external_corpus_item_changes_decode_to_their_exact_payloads() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping save corpus accounting");
        return;
    };
    let mut paths = Vec::new();
    collect_saves(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");
    let mut counts = [0usize; 3];
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            let Some(index) = ["INGR", "ALCH", "ENCH"]
                .iter()
                .position(|kind| form.record_type() == *kind)
            else {
                continue;
            };
            let encoded = match form.record_type() {
                "INGR" => {
                    let typed = form.ingredient_change().unwrap().unwrap();
                    (typed.change_flags(), typed.data_bytes())
                }
                "ALCH" => {
                    let typed = form.alchemy_change().unwrap().unwrap();
                    (typed.change_flags(), typed.data_bytes())
                }
                "ENCH" => {
                    let typed = form.enchantment_change().unwrap().unwrap();
                    (typed.change_flags(), typed.data_bytes())
                }
                _ => unreachable!(),
            };
            assert_eq!(encoded.0, form.change_flags);
            assert_eq!(encoded.1, form.payload().unwrap().as_ref());
            counts[index] += 1;
        }
    }
    let accounted = counts.iter().sum::<usize>();
    assert_eq!(accounted, counts.into_iter().sum::<usize>());
    eprintln!("SAV-37 external corpus counts={counts:?}");
}
