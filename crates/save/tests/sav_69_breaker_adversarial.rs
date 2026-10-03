// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-69 adversarial top-level extra-list boundary checks.

use std::path::{Path, PathBuf};

use soe_save::{reference::Field, ChangeForm, RefId, Save, WString};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn change(data: Vec<u8>, flags: u32) -> ChangeForm {
    ChangeForm {
        form: reference(0x400100),
        change_flags: flags,
        type_field: 0,
        version: 0,
        data,
        uncompressed_len: 0,
    }
}

fn named_package(tree: &str, child: &str) -> Vec<u8> {
    let mut bytes = vec![12, 108, 18, 0];
    WString::from_text(tree).write(&mut bytes);
    WString::from_text(child).write(&mut bytes);
    bytes.extend([0xA5; 8]);
    bytes.push(33);
    reference(0x400811).write(&mut bytes);
    bytes.push(42);
    bytes.extend([255, 5]);
    reference(0xFE0001).write(&mut bytes);
    bytes.extend(0xAABB_CCDDu32.to_le_bytes());
    bytes.extend(0x1234_5678u32.to_le_bytes());
    bytes
}

fn collect_saves(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
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
fn sav_69_sequence_and_stacked_frames_stop_at_following_owner_and_lock() {
    for tree in ["Sequence", "Stacked"] {
        let data = named_package(tree, "Wait");
        let decoded = change(data.clone(), 1 << 31)
            .reference_extra_data(78)
            .unwrap()
            .unwrap();
        assert_eq!(decoded.data_bytes().unwrap(), data);
        assert_eq!(
            decoded
                .entries
                .iter()
                .map(|entry| entry.kind)
                .collect::<Vec<_>>(),
            [108, 33, 42]
        );
        assert!(matches!(
            decoded.entries[1].fields.as_slice(),
            [Field::Reference(owner)] if *owner == reference(0x400811)
        ));
        assert!(matches!(
            decoded.entries[2].fields.as_slice(),
            [Field::Byte(255), Field::Byte(5), Field::Reference(key), Field::Word(0xAABB_CCDD), Field::Word(0x1234_5678)] if *key == reference(0xFE0001)
        ));
    }
}

#[test]
fn sav_69_each_truncated_observed_package_frame_is_rejected() {
    for tree in ["Sequence", "Stacked"] {
        let complete = named_package(tree, "Wait");
        for cut in 0..complete.len() {
            let parsed = change(complete[..cut].to_vec(), 1 << 31).reference_extra_data(78);
            assert!(parsed.is_err(), "{tree} frame cut at {cut}");
        }
    }
}

#[test]
fn sav_69_extra_flag_gates_list_read_and_does_not_parse_actor_suffix() {
    let malformed = change(vec![0xFF, 0xFF], 0);
    assert!(malformed.reference_extra_data(78).unwrap().is_none());

    let mut actor_data = vec![0xA5; 8];
    actor_data.extend(named_package("Sequence", "Wait"));
    actor_data.extend([0xDE, 0xAD]);
    let mut actor = change(actor_data, 1 << 18);
    actor.type_field = 1;
    let decoded = actor.reference_extra_data(78).unwrap().unwrap();
    assert_eq!(decoded.entries.len(), 3);
    assert_eq!(decoded.entries[1].kind, 33);
    assert_eq!(decoded.entries[2].kind, 42);
}

#[test]
fn sav_69_all_60_external_saves_decode_and_reencode_each_top_level_list() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SAV-69 save corpus");
        return;
    };
    let mut paths = Vec::new();
    collect_saves(Path::new(&root), &mut paths);
    paths.sort();
    assert_eq!(paths.len(), 60, "SAV-69 requires the complete save corpus");

    let mut references = 0usize;
    let mut lists = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let Some(header) = change.reference_header(save.form_version).unwrap() else {
                continue;
            };
            references += 1;
            let extra = change
                .reference_extra_data(save.form_version)
                .unwrap_or_else(|error| panic!("{} {:?}: {error}", path.display(), change.form));
            let Some(extra) = extra else {
                continue;
            };
            lists += 1;
            let payload = change.payload().unwrap();
            let end = header.bytes_consumed + extra.data_bytes().unwrap().len();
            assert!(end <= payload.len(), "{} {:?}", path.display(), change.form);
            assert_eq!(
                extra.data_bytes().unwrap(),
                payload[header.bytes_consumed..end],
                "{} {:?}",
                path.display(),
                change.form
            );
        }
    }
    assert!(references >= 1_000_000);
    assert!(lists >= 1000);
}
