// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Check the observed Stacked boundary with a known following extra.

use soe_save::{reference::Field, ChangeForm, RefId, Save};
use std::path::Path;

const LEDGER: &str = include_str!("../../../docs/save-package-variants-v1.tsv");

fn with_successor(frame: &[u8]) -> ChangeForm {
    let mut data = vec![8];
    data.extend_from_slice(frame);
    data.extend([133, 0x80, 9, 0]);
    data.extend([4, 0x40, 8, 0]);
    data.extend((-3_i32).to_le_bytes());
    data.extend([0, 0xDE, 0xAD]);
    ChangeForm {
        form: RefId::from_bytes([0x40, 0, 1]),
        change_flags: (1 << 11) | (1 << 5),
        type_field: 0,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

fn observed_frames() -> Vec<(u8, Vec<u8>)> {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        return Vec::new();
    };
    let root = Path::new(&root);
    let mut frames = Vec::new();
    for row in LEDGER
        .lines()
        .filter(|row| row.ends_with("PD18-STACKED\tOPAQUE-CONTINUATION"))
    {
        let columns: Vec<_> = row.split('\t').collect();
        let save = Save::open(root.join(columns[1])).unwrap();
        let raw_form = u32::from_str_radix(columns[2], 16).unwrap();
        let change = save
            .change_forms
            .iter()
            .find(|form| form.form.raw() == raw_form)
            .unwrap();
        let payload = change.payload().unwrap();
        let at: usize = columns[5].parse().unwrap();
        frames.push((save.form_version, payload[at..at + 170].to_vec()));
    }
    frames
}

#[test]
fn observed_stacked_frames_keep_a_real_successor_extra_and_inventory_boundary() {
    let frames = observed_frames();
    if frames.is_empty() {
        eprintln!("SOE_TEST_SAVES unset, skipping observed Stacked successor check");
        return;
    }
    assert_eq!(frames.len(), 9);
    for (form_version, frame) in frames {
        let change = with_successor(&frame);
        let contents = change.reference_contents(form_version).unwrap().unwrap();
        let extra = contents.extra.as_ref().unwrap();
        assert_eq!(extra.entries.len(), 2);
        assert_eq!(extra.entries[0].kind, 108);
        assert!(extra.entries[0]
            .fields
            .iter()
            .any(|field| matches!(field, Field::String(name) if name.bytes() == b"Stacked")));
        assert_eq!(extra.entries[1].kind, 133);
        assert!(
            matches!(extra.entries[1].fields.as_slice(), [Field::Reference(id)] if id.raw() == 0x800900)
        );
        assert_eq!(contents.inventory.as_ref().unwrap().entries[0].delta, -3);
        assert_eq!(contents.end, change.data.len() - 2);
        assert_eq!(contents.remaining_bytes, 2);
        assert_eq!(contents.data_bytes().unwrap(), change.data[..contents.end]);
    }
}

#[test]
fn every_observed_stacked_truncation_fails_without_successor_bytes() {
    let frames = observed_frames();
    if frames.is_empty() {
        eprintln!("SOE_TEST_SAVES unset, skipping observed Stacked truncation check");
        return;
    }
    assert_eq!(frames.len(), 9);
    for (form_version, frame) in frames {
        for cut in 1..frame.len() {
            let mut change = with_successor(&frame);
            change.data.truncate(1 + cut);
            assert!(
                change.reference_contents(form_version).is_err(),
                "accepted Stacked frame prefix of {cut} bytes"
            );
        }
    }
}
