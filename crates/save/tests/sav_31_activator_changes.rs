// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ACTI, TACT and DOOR saved change fields from xEdit save definitions.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use soe_save::{ActivatorChange, ChangeForm, RefId, Save};

const TYPES: [(u8, &str, u32); 3] = [
    (10, "ACTI", 0b111),
    (11, "TACT", 0x0080_0007),
    (15, "DOOR", 0b111),
];

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

fn decode(form: &ChangeForm) -> soe_save::Result<(u32, Vec<u8>, ActivatorChange)> {
    let value = form.activator_change()?.expect("target record type");
    Ok((
        value.change_flags(form.record_type())?,
        value.data_bytes(),
        value,
    ))
}

fn fixture(flags: u32) -> Vec<u8> {
    if flags & 1 != 0 {
        vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6]
    } else {
        vec![]
    }
}

#[test]
fn sav_31_every_source_flag_combination_round_trips() {
    for (kind, name, known) in TYPES {
        let bits: Vec<u32> = (0..32).filter(|bit| known & (1 << bit) != 0).collect();
        for subset in 0..1 << bits.len() {
            let flags = bits.iter().enumerate().fold(0, |flags, (index, bit)| {
                flags
                    | if subset & (1 << index) != 0 {
                        1 << bit
                    } else {
                        0
                    }
            });
            let data = fixture(flags);
            let decoded = decode(&change(kind, flags, data.clone())).unwrap();
            assert_eq!(decoded.0, flags, "{name} {flags:#010x}");
            assert_eq!(decoded.1, data, "{name} {flags:#010x}");
            assert_eq!(decoded.2.value_changed, flags & 2 != 0);
            assert_eq!(decoded.2.full_name_changed, flags & 4 != 0);
            assert_eq!(
                decoded.2.speaker_changed,
                name == "TACT" && flags & (1 << 23) != 0
            );
            assert_eq!(
                decoded
                    .2
                    .form_flags
                    .map(|value| (value.flags, value.unknown)),
                (flags & 1 != 0).then_some((0xB4A3_9281, 0xD6C5))
            );
        }
    }
}

#[test]
fn sav_31_rejects_truncated_flags_unknown_bits_and_trailing_bytes() {
    for (kind, name, known) in TYPES {
        for bit in 0..32 {
            let flags = 1u32 << bit;
            if flags & !known != 0 {
                assert!(
                    change(kind, flags, vec![]).activator_change().is_err(),
                    "{name} unknown flags {flags:#010x}"
                );
            }
        }
        let flagged = change(kind, 1, fixture(1));
        for cut in 0..6 {
            assert!(
                change(kind, 1, flagged.data[..cut].to_vec())
                    .activator_change()
                    .is_err(),
                "{name} cut {cut}"
            );
        }
        let mut trailing = fixture(0);
        trailing.push(0xA5);
        assert!(change(kind, 0, trailing).activator_change().is_err());
    }
    let door = change(15, 0, vec![]).activator_change().unwrap().unwrap();
    assert!(door.change_flags("ACTI").is_ok());
    assert!(ActivatorChange {
        speaker_changed: true,
        ..door
    }
    .change_flags("DOOR")
    .is_err());
}

#[test]
fn sav_31_compressed_payloads_retain_typed_markers_and_bytes() {
    for (kind, name, known) in TYPES {
        let data = fixture(known);
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &data).unwrap();
        let mut form = change(kind, known, encoder.finish().unwrap());
        form.uncompressed_len = data.len() as u32;
        let decoded = decode(&form).unwrap();
        assert_eq!(decoded.0, known, "{name}");
        assert_eq!(decoded.1, data, "{name}");
    }
}

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
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
fn sav_31_external_corpus_accounts_for_each_target_change() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping ACTI/TACT/DOOR corpus");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut counts = BTreeMap::from([("ACTI", 0usize), ("TACT", 0), ("DOOR", 0)]);
    let mut variants = BTreeMap::new();
    let mut payload_bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let Some(count) = counts.get_mut(form.record_type()) else {
                continue;
            };
            let (flags, encoded, _) = decode(form).unwrap_or_else(|error| {
                panic!(
                    "{} {} {:06x}: {error}",
                    path.display(),
                    form.record_type(),
                    form.form.raw()
                )
            });
            assert_eq!(flags, form.change_flags);
            assert_eq!(encoded, form.payload().unwrap().as_ref());
            *count += 1;
            payload_bytes += encoded.len();
            *variants
                .entry((save.form_version, form.version, form.record_type(), flags))
                .or_insert(0usize) += 1;
        }
    }
    eprintln!(
        "SAV-31 {} saves, {payload_bytes} payload bytes, counts={counts:?}, variants={variants:?}",
        paths.len()
    );
    assert_eq!(
        variants.values().sum::<usize>(),
        counts.values().sum::<usize>()
    );
}

#[test]
fn sav_31_installed_skyrim_has_the_named_record_types() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping vanilla record check");
        return;
    };
    let plugin =
        soe_esm::Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap())
            .unwrap();
    let mut counts = BTreeMap::from([("ACTI", 0usize), ("TACT", 0), ("DOOR", 0)]);
    for record in &plugin.records {
        let name = match &record.tag.0 {
            b"ACTI" => "ACTI",
            b"TACT" => "TACT",
            b"DOOR" => "DOOR",
            _ => continue,
        };
        if let Some(count) = counts.get_mut(name) {
            *count += 1;
        }
    }
    eprintln!("SAV-31 Skyrim.esm base records={counts:?}");
    assert!(counts.values().all(|count| *count > 0));
}
