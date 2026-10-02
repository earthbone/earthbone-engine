// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-12 corpus round trips and Sequence successor boundaries.

use soe_save::{reference::Field, Save};
use std::path::Path;

const LEDGER: &str = include_str!("../../../docs/save-package-variants-v1.tsv");

#[test]
fn sav_12_every_observed_sequence_reencodes_and_keeps_its_successor() {
    let root = std::env::var_os("SOE_TEST_SAVES").expect("SOE_TEST_SAVES must name the corpus");
    let mut checked = 0;
    for row in LEDGER.lines().filter(|row| row.ends_with("PD18-SEQUENCE\tOPAQUE-CONTINUATION")) {
        let columns: Vec<_> = row.split('\t').collect();
        let path = Path::new(&root).join(columns[1]);
        let save = Save::open(path).unwrap();
        let raw_form = u32::from_str_radix(columns[2], 16).unwrap();
        let change = save.change_forms.iter().find(|form| form.form.raw() == raw_form).unwrap();
        let payload = change.payload().unwrap();
        let contents = change.reference_contents(save.form_version).unwrap().unwrap();
        let sequence = contents.extra.as_ref().unwrap().entries.iter().find(|extra| {
            extra.kind == 108 && extra.fields.iter().any(|field| {
                matches!(field, Field::String(name) if name.bytes() == b"Sequence")
            })
        }).unwrap_or_else(|| panic!("Sequence missing in {}", columns[1]));
        assert!(matches!(sequence.fields.last(), Some(Field::Group(_))));
        assert_eq!(contents.data_bytes().unwrap(), payload[contents.start..contents.end]);
        assert_eq!(contents.remaining_bytes, payload.len() - contents.end);
        checked += 1;
    }
    assert_eq!(checked, 72);
}
