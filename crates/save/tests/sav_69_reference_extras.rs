// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-69 top-level list boundaries from xEdit and observed package framing.

use soe_save::{reference::Field, ChangeForm, RefId, WString};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn extra_change(tree: &str, actor: bool) -> (ChangeForm, Vec<u8>) {
    // CONTEXT: Use a wide count and marker-like bytes to expose boundary guesses.
    let mut extra = vec![0x15, 0, 108, 18, 0];
    WString::from_text(tree).write(&mut extra);
    WString::from_text("Sandbox").write(&mut extra);
    extra.extend([0x85; 74]);
    extra.extend([0, 0, 0]);
    WString::default().write(&mut extra);
    extra.extend([0x85; 5]);
    extra.extend(0_u32.to_le_bytes());
    extra.extend([0x85; 8]);
    extra.extend(8_u32.to_le_bytes());
    extra.push(33);
    reference(0x400811).write(&mut extra);
    extra.push(35);
    extra.extend((-7_i32).to_le_bytes());
    extra.push(34);
    reference(0x400812).write(&mut extra);
    extra.extend([42, 75, 5]);
    reference(0x400801).write(&mut extra);
    extra.extend(0xAABB_CCDD_u32.to_le_bytes());
    extra.extend(0x1234_5678_u32.to_le_bytes());
    let mut data = if actor { vec![0; 8] } else { vec![] };
    data.extend(&extra);
    (
        ChangeForm {
            form: reference(0x400100),
            change_flags: (1 << 31) | (1 << 5),
            type_field: u8::from(actor),
            version: 78,
            data,
            uncompressed_len: 0,
        },
        extra,
    )
}

#[test]
fn sav_69_named_package_lists_preserve_owner_lock_and_suffix_boundaries() {
    for tree in ["Sequence", "Stacked"] {
        for actor in [false, true] {
            let (mut change, expected) = extra_change(tree, actor);
            let boundary = change.data.len();
            // CONTEXT: Reserved inventory count and unknown actor bytes stay unread.
            change.data.extend([0xFF, 0xDE, 0xAD]);
            let extra = change.reference_extra_data(78).unwrap().unwrap();
            assert_eq!(extra.data_bytes().unwrap(), expected, "{tree}, {actor}");
            assert_eq!(
                extra.entries.iter().map(|e| e.kind).collect::<Vec<_>>(),
                [108, 33, 35, 34, 42]
            );
            assert!(
                matches!(extra.entries[2].fields.as_slice(), [Field::Word(rank)] if *rank as i32 == -7)
            );
            assert!(change.reference_contents(78).is_err());

            change.data.truncate(boundary);
            change.data.extend([4]);
            reference(0x400800).write(&mut change.data);
            change.data.extend((-3_i32).to_le_bytes());
            change.data.extend([0, 0xDE, 0xAD]);
            let contents = change.reference_contents(78).unwrap().unwrap();
            assert_eq!(contents.remaining_bytes, 2);
            assert_eq!(
                contents.data_bytes().unwrap(),
                change.data[contents.start..contents.end]
            );
            assert_eq!(contents.inventory.unwrap().entries[0].delta, -3);
        }
    }
}

#[test]
fn sav_69_truncated_top_level_lists_and_edited_counts_return_typed_errors() {
    for tree in ["Sequence", "Stacked"] {
        for actor in [false, true] {
            let (change, _) = extra_change(tree, actor);
            for cut in 0..change.data.len() {
                let mut truncated = change.clone();
                truncated.data.truncate(cut);
                assert!(
                    truncated.reference_extra_data(78).is_err(),
                    "{tree}, {actor}, cut {cut}"
                );
            }
            let mut extra = change.reference_extra_data(78).unwrap().unwrap();
            extra.entries.pop();
            assert!(extra
                .data_bytes()
                .unwrap_err()
                .to_string()
                .contains("count mismatch"));
        }
    }
}

#[test]
fn sav_69_absent_lists_and_unknown_types_do_not_accept_a_guessed_boundary() {
    let (mut change, _) = extra_change("Sequence", false);
    change.change_flags = 0;
    change.data.clear();
    assert!(change.reference_extra_data(78).unwrap().is_none());
    change.change_flags = 1 << 31;
    change.data.extend([4, 255, 33, 0, 0, 0, 42]);
    assert!(change
        .reference_extra_data(78)
        .unwrap_err()
        .to_string()
        .contains("unknown extra-data type 255"));
}
