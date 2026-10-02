// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-21 corpus check for typed Stacked state and following boundaries.

use soe_save::{reference::Field, ChangeForm, RefId, Save, WString};
use std::path::Path;

const LEDGER: &str = include_str!("../../../docs/save-package-variants-v1.tsv");

fn framed(package: &[u8]) -> ChangeForm {
    let mut data = vec![8];
    data.extend(package);
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

fn check_successors(change: &ChangeForm) {
    let contents = change.reference_contents(78).unwrap().unwrap();
    assert_eq!(contents.end, change.data.len() - 2);
    assert_eq!(contents.remaining_bytes, 2);
    assert_eq!(contents.data_bytes().unwrap(), change.data[..contents.end]);
    let extras = contents.extra.unwrap();
    assert_eq!(extras.entries[1].kind, 133);
    assert!(matches!(extras.entries[1].fields[0], Field::Reference(id) if id.raw() == 0x800900));
    let inventory = contents.inventory.unwrap();
    assert_eq!(inventory.entries[0].item.raw(), 0x400800);
    assert_eq!(inventory.entries[0].delta, -3);
}

#[test]
fn absent_sandbox_request_preserves_the_branch_index_and_successors() {
    for root in ["Sequence", "Stacked"] {
        let mut package = vec![108, 18, 0];
        WString::from_text(root).write(&mut package);
        WString::from_text("Sandbox").write(&mut package);
        package.extend([0xFF; 74]);
        package.extend([4]);
        package.extend([0x85; 15]);
        package.push(1);
        WString::from_text("LocPt").write(&mut package);
        package.extend([0xFF; 12]);
        package.extend([0x40, 0, 1]);
        package.extend([0x85; 12]);
        package.push(0);
        WString::default().write(&mut package);
        package.extend([0x85; 5]);
        package.extend(0_u32.to_le_bytes());
        package.extend([0xFF; 8]);
        package.extend(8_u32.to_le_bytes());
        let change = framed(&package);
        check_successors(&change);
        let contents = change.reference_contents(78).unwrap().unwrap();
        let extras = contents.extra.unwrap();
        let Field::Group(state) = &extras.entries[0].fields[3] else {
            panic!("missing branch state")
        };
        assert!(matches!(state[1], Field::Word(8)));
        for cut in 0..change.data.len() - 2 {
            let mut truncated = change.clone();
            truncated.data.truncate(cut);
            assert!(truncated.reference_contents(78).is_err(), "{root} at {cut}");
        }
    }
}

#[test]
fn sav_21_all_observed_stacked_entries_reencode_and_keep_their_successors() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping Stacked corpus");
        return;
    };
    let mut checked = 0;
    for row in LEDGER
        .lines()
        .filter(|row| row.ends_with("PD18-STACKED\tOPAQUE-CONTINUATION"))
    {
        let columns: Vec<_> = row.split('\t').collect();
        let path = Path::new(&root).join(columns[1]);
        let save = Save::open(path).unwrap();
        let raw_form = u32::from_str_radix(columns[2], 16).unwrap();
        let change = save
            .change_forms
            .iter()
            .find(|form| form.form.raw() == raw_form)
            .unwrap();
        let payload = change.payload().unwrap();
        let contents = change
            .reference_contents(save.form_version)
            .unwrap()
            .unwrap();
        let stacked = contents
            .extra
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .find(|extra| {
                extra.kind == 108
                    && extra.fields.iter().any(
                        |field| matches!(field, Field::String(name) if name.bytes() == b"Stacked"),
                    )
            })
            .unwrap_or_else(|| panic!("Stacked missing in {}", columns[1]));
        assert!(matches!(stacked.fields.last(), Some(Field::Group(_))));
        assert_eq!(
            contents.data_bytes().unwrap(),
            payload[contents.start..contents.end]
        );
        assert_eq!(contents.remaining_bytes, payload.len() - contents.end);
        let at: usize = columns[5].parse().unwrap();
        // CONTEXT: All nine observed frames have this boundary, without their suffixes.
        let package = &payload[at..at + 170];
        check_successors(&framed(package));
        let mut isolated = framed(package);
        isolated.change_flags = 1 << 11;
        isolated.data.truncate(171);
        isolated.data[0] = 4;
        let decoded = isolated
            .reference_contents(save.form_version)
            .unwrap()
            .unwrap();
        assert_eq!(decoded.end, 171);
        assert_eq!(decoded.remaining_bytes, 0);
        assert_eq!(&decoded.data_bytes().unwrap()[1..], package);
        checked += 1;
    }
    assert_eq!(checked, 9);
}
