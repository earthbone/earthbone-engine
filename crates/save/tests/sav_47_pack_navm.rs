// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PACK and NAVM change fields from xEdit's save definitions, checked against
//! the UESP type table, FallrimTools type indices and external saves.

use std::{collections::BTreeMap, path::Path};

use soe_save::quest::FormFlags;
use soe_save::{ChangeForm, RefId, Save};

const IDENTITY: [u8; 3] = [0x40, 0x01, 0x23];
const FORM_FLAGS: [u8; 6] = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];

fn form(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes(IDENTITY),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn fixture(flags: u32) -> Vec<u8> {
    if flags & 1 != 0 {
        FORM_FLAGS.to_vec()
    } else {
        vec![]
    }
}

#[test]
fn sav_47_all_source_flag_combinations_preserve_typed_fields_and_bytes() {
    for (kind, masks) in [(32u8, [1, 1 << 30, 1 << 31]), (33, [1, 2, 4])] {
        for subset in 0..1 << masks.len() {
            let flags = masks.iter().enumerate().fold(0, |flags, (index, mask)| {
                flags | if subset & (1 << index) != 0 { *mask } else { 0 }
            });
            let data = fixture(flags);
            let change = form(kind, flags, data.clone());
            let (decoded_flags, decoded_data) = if kind == 32 {
                let decoded = change.pack_change().unwrap().unwrap();
                assert_eq!(decoded.form_flags.is_some(), flags & 1 != 0);
                assert_eq!(decoded.waiting, flags & (1 << 30) != 0);
                assert_eq!(decoded.never_run, flags & (1 << 31) != 0);
                (decoded.change_flags(), decoded.data_bytes())
            } else {
                let decoded = change.navmesh_change().unwrap().unwrap();
                assert_eq!(decoded.form_flags.is_some(), flags & 1 != 0);
                assert_eq!(decoded.value_changed, flags & 2 != 0);
                assert_eq!(decoded.full_name_changed, flags & 4 != 0);
                (decoded.change_flags(), decoded.data_bytes())
            };
            assert_eq!(decoded_flags, flags, "kind {kind}, flags {flags:#x}");
            assert_eq!(decoded_data, data, "kind {kind}, flags {flags:#x}");
        }
    }
}

#[test]
fn sav_47_form_flag_values_are_typed_and_retained() {
    let flags = FormFlags {
        flags: 0xB4A3_9281,
        unknown: 0xD6C5,
    };
    let pack = form(32, 1, fixture(1)).pack_change().unwrap().unwrap();
    let navm = form(33, 1, fixture(1)).navmesh_change().unwrap().unwrap();
    assert_eq!(pack.form_flags, Some(flags));
    assert_eq!(navm.form_flags, Some(flags));
}

#[test]
fn sav_47_truncation_unknown_flags_and_trailing_bytes_are_rejected() {
    for (kind, allowed, all_flags) in [(32u8, 0xC000_0001u32, 0xC000_0001u32), (33, 0b111, 0b111)] {
        let data = fixture(all_flags);
        for cut in 0..data.len() {
            let truncated = form(kind, all_flags, data[..cut].to_vec());
            let result = if kind == 32 {
                truncated.pack_change().map(|_| ())
            } else {
                truncated.navmesh_change().map(|_| ())
            };
            assert!(result.is_err(), "kind {kind}, cut {cut}");
        }
        let mut trailing = data;
        trailing.push(0);
        let change = form(kind, all_flags, trailing);
        let result = if kind == 32 {
            change.pack_change().map(|_| ())
        } else {
            change.navmesh_change().map(|_| ())
        };
        assert!(result.is_err(), "kind {kind} trailing byte");

        for bit in 0..32 {
            if allowed & (1 << bit) == 0 {
                let change = form(kind, 1 << bit, vec![]);
                let result = if kind == 32 {
                    change.pack_change().map(|_| ())
                } else {
                    change.navmesh_change().map(|_| ())
                };
                assert!(result.is_err(), "kind {kind}, unknown bit {bit}");
            }
        }
    }
}

#[test]
fn sav_47_compressed_payloads_retain_source_fields() {
    for (kind, flags) in [(32u8, 0xC000_0001), (33, 7)] {
        let data = fixture(flags);
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &data).unwrap();
        let mut change = form(kind, flags, encoder.finish().unwrap());
        change.uncompressed_len = data.len() as u32;
        let result = if kind == 32 {
            let decoded = change.pack_change().unwrap().unwrap();
            (decoded.change_flags(), decoded.data_bytes())
        } else {
            let decoded = change.navmesh_change().unwrap().unwrap();
            (decoded.change_flags(), decoded.data_bytes())
        };
        assert_eq!(result, (flags, data));
    }
}

#[test]
fn sav_47_other_record_types_remain_opaque() {
    let other = form(8, u32::MAX, vec![0xA5]);
    assert!(other.pack_change().unwrap().is_none());
    assert!(other.navmesh_change().unwrap().is_none());
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_47_external_corpus_accounts_for_every_pack_and_navm_change() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping PACK/NAVM corpus");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut counts = BTreeMap::from([("PACK", 0usize), ("NAVM", 0usize)]);
    let mut flag_counts = BTreeMap::new();
    let mut payload_bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let Some(count) = counts.get_mut(change.record_type()) else {
                continue;
            };
            let payload = change.payload().unwrap();
            let (flags, bytes) = if change.record_type() == "PACK" {
                let decoded = change.pack_change().unwrap().unwrap();
                (decoded.change_flags(), decoded.data_bytes())
            } else {
                let decoded = change.navmesh_change().unwrap().unwrap();
                (decoded.change_flags(), decoded.data_bytes())
            };
            assert_eq!(flags, change.change_flags);
            assert_eq!(bytes, payload.as_ref());
            *count += 1;
            payload_bytes += bytes.len();
            *flag_counts
                .entry((
                    save.form_version,
                    change.version,
                    change.record_type(),
                    flags,
                ))
                .or_insert(0usize) += 1;
        }
    }
    eprintln!(
        "SAV-47 {} saves, {payload_bytes} bytes, counts={counts:?}, flags={flag_counts:x?}",
        paths.len()
    );
}

#[test]
fn sav_47_installed_skyrim_defines_pack_and_navm_records() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping installed PACK/NAVM record check");
        return;
    };
    let plugin =
        soe_esm::Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap())
            .unwrap();
    let pack = plugin
        .records
        .iter()
        .filter(|record| record.tag == soe_esm::Tag(*b"PACK"))
        .count();
    let navm = plugin
        .records
        .iter()
        .filter(|record| record.tag == soe_esm::Tag(*b"NAVM"))
        .count();
    assert!(pack > 0, "Skyrim.esm should define PACK records");
    assert!(navm > 0, "Skyrim.esm should define NAVM records");
}
