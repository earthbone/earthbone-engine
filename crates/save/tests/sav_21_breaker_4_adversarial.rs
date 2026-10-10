// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-21 checks for exact observed frames and the documented terminator.

use soe_save::{reference::Field, ChangeForm, RefId, Save};
use std::path::Path;

const LEDGER: &str = include_str!("../../../docs/save-package-variants-v1.tsv");

fn has_stacked(fields: &[Field]) -> bool {
    fields.iter().any(|field| match field {
        Field::String(name) => name.bytes() == b"Stacked",
        Field::Group(children) => has_stacked(children),
        Field::Array(array) => array
            .entries
            .iter()
            .any(|children| has_stacked(children)),
        _ => false,
    })
}

#[test]
fn every_observed_stacked_frame_reencodes_without_moving_its_successor() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping observed Stacked frames");
        return;
    };
    let mut checked = 0;
    for row in LEDGER
        .lines()
        .filter(|row| row.ends_with("PD18-STACKED\tOPAQUE-CONTINUATION"))
    {
        let columns: Vec<_> = row.split('\t').collect();
        let save = Save::open(Path::new(&root).join(columns[1])).unwrap();
        let form_id = u32::from_str_radix(columns[2], 16).unwrap();
        let change = save
            .change_forms
            .iter()
            .find(|form| form.form.raw() == form_id)
            .unwrap();
        let payload = change.payload().unwrap();
        let contents = change
            .reference_contents(save.form_version)
            .unwrap()
            .unwrap();
        let extras = contents.extra.as_ref().unwrap();
        assert!(extras.entries.iter().any(|entry| has_stacked(&entry.fields)));
        let encoded = contents.data_bytes().unwrap();
        assert_eq!(encoded, payload[contents.start..contents.end]);
        assert_eq!(contents.start + encoded.len(), contents.end);
        assert_eq!(contents.remaining_bytes, payload.len() - contents.end);
        let at: usize = columns[5].parse().unwrap();
        let frame = &payload[at..at + 170];
        let mut isolated = ChangeForm {
            form: RefId::from_bytes([0x40, 0, 1]),
            change_flags: 1 << 11,
            type_field: 0,
            version: 0,
            uncompressed_len: 0,
            data: [vec![4], frame.to_vec()].concat(),
        };
        let isolated_contents = isolated
            .reference_contents(save.form_version)
            .unwrap()
            .unwrap();
        assert_eq!(isolated_contents.end, 171);
        assert_eq!(isolated_contents.remaining_bytes, 0);
        assert_eq!(&isolated_contents.data_bytes().unwrap()[1..], frame);
        isolated.data.push(0xA5);
        let with_suffix = isolated
            .reference_contents(save.form_version)
            .unwrap()
            .unwrap();
        assert_eq!(with_suffix.end, 171);
        assert_eq!(with_suffix.remaining_bytes, 1);
        checked += 1;
    }
    assert_eq!(checked, 9);
}

#[test]
fn package_data_255_ends_before_the_next_extra() {
    let change = ChangeForm {
        form: RefId::from_bytes([0x40, 0, 1]),
        change_flags: 1 << 11,
        type_field: 0,
        version: 0,
        uncompressed_len: 0,
        data: vec![8, 108, 255, 133, 0x80, 9, 0],
    };
    let contents = change.reference_contents(78).unwrap().unwrap();
    let extras = contents.extra.unwrap();
    assert_eq!(extras.entries.len(), 2);
    assert_eq!(extras.entries[0].kind, 108);
    assert!(matches!(extras.entries[0].fields.as_slice(), [Field::Byte(255)]));
    assert_eq!(extras.entries[1].kind, 133);
    assert!(matches!(extras.entries[1].fields.as_slice(), [Field::Reference(id)] if id.raw() == 0x800900));
}
