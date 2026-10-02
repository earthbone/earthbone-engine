// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial corpus check for SAV-12's Sequence boundary claim.

use soe_save::{reference::Field, Save};
use std::collections::BTreeSet;
use std::path::Path;

const LEDGER: &str = include_str!("../../../docs/save-package-variants-v1.tsv");

#[test]
fn sav_12_all_observed_sequence_entries_round_trip_before_the_next_extra() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SAV-12 corpus check");
        return;
    };

    let mut occurrences = BTreeSet::new();
    for row in LEDGER
        .lines()
        .filter(|row| row.ends_with("PD18-SEQUENCE\tOPAQUE-CONTINUATION"))
    {
        let columns: Vec<_> = row.split('\t').collect();
        occurrences.insert((columns[1].to_owned(), columns[2].to_owned()));
    }
    assert_eq!(occurrences.len(), 72);

    for (save_name, raw_form) in occurrences {
        let save = Save::open(Path::new(&root).join(&save_name)).unwrap();
        let raw_form = u32::from_str_radix(&raw_form, 16).unwrap();
        let change = save
            .change_forms
            .iter()
            .find(|change| change.form.raw() == raw_form)
            .unwrap_or_else(|| panic!("missing {raw_form:06x} in {save_name}"));
        let payload = change.payload().unwrap();
        let contents = change
            .reference_contents(save.form_version)
            .unwrap()
            .unwrap_or_else(|| panic!("missing reference contents for {raw_form:06x}"));
        let encoded = contents.data_bytes().unwrap();
        assert_eq!(
            encoded,
            payload[contents.start..contents.end],
            "{save_name}"
        );
        assert_eq!(
            contents.remaining_bytes,
            payload.len() - contents.end,
            "{save_name}"
        );

        let extras = contents
            .extra
            .unwrap_or_else(|| panic!("no extras in {save_name}"));
        let sequence_at = extras.entries.iter().position(|extra| {
            extra.kind == 108
                && matches!(extra.fields.first(), Some(Field::Byte(18)))
                && matches!(extra.fields.get(2), Some(Field::String(name)) if name.bytes() == b"Sequence")
        }).unwrap_or_else(|| panic!("no Sequence PackageData in {save_name}"));
        assert!(
            sequence_at < extras.entries.len(),
            "Sequence extra was lost in {save_name}"
        );
    }
}
