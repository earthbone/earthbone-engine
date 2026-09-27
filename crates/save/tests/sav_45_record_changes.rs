// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CLAS, FACT and RELA change fixtures from xEdit save definitions and
//! FallrimTools ChangeFormRela, checked against external Special Edition saves.

use std::{collections::BTreeMap, io::Write, path::Path};

use soe_save::{ChangeForm, RefId, RefKind, Save};

const CREATED: [u8; 3] = [0x80, 0x01, 0x23];
const DEFAULT: [u8; 3] = [0x40, 0x01, 0x23];
const FORM_FLAGS: [u8; 6] = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
const CRIME: [u32; 4] = [0, 0x7FFF_FFFF, 0xFF00_1234, 0x8000_0000];

fn form(kind: u8, identity: [u8; 3], flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes(identity),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn fixture(kind: u8, identity: [u8; 3], flags: u32) -> Vec<u8> {
    let mut out = Vec::new();
    if flags & 1 != 0 {
        out.extend(FORM_FLAGS);
    }
    if kind == 31 {
        if flags & 4 != 0 {
            out.push(4);
            out.extend([0x42, 0x34, 0x56]);
            out.extend((-9i32).to_le_bytes());
            out.extend(0xDEAD_BEEFu32.to_le_bytes());
        }
        if flags & 2 != 0 {
            out.extend(0xA581_0001u32.to_le_bytes());
        }
        if flags & (1 << 31) != 0 {
            for word in CRIME {
                out.extend(word.to_le_bytes());
            }
        }
    }
    if kind == 39 {
        if RefId::from_bytes(identity).kind() == RefKind::Created {
            out.extend([0x40, 1, 2, 0x80, 3, 4, 0, 5, 6]);
        }
        if flags & 2 != 0 {
            out.extend((-3i32).to_le_bytes());
        }
    }
    out
}

fn decoded(form: &ChangeForm) -> soe_save::Result<(u32, Vec<u8>)> {
    match form.record_type() {
        "CLAS" => {
            let value = form.class_change()?.unwrap();
            Ok((value.change_flags(), value.data_bytes()))
        }
        "FACT" => {
            let value = form.faction_change()?.unwrap();
            Ok((value.change_flags(), value.data_bytes()?))
        }
        "RELA" => {
            let value = form.relationship_change()?.unwrap();
            Ok((value.change_flags(), value.data_bytes(form.form)?))
        }
        _ => unreachable!(),
    }
}

#[test]
fn sav_45_all_source_flag_combinations_preserve_exact_bytes() {
    for (kind, allowed) in [(30u8, 0b11u32), (31, 0x8000_0007), (39, 0b11)] {
        for identity in if kind == 39 {
            [DEFAULT, CREATED].as_slice()
        } else {
            [DEFAULT].as_slice()
        } {
            let bits: Vec<u32> = (0..32).filter(|bit| allowed & (1 << bit) != 0).collect();
            for subset in 0..1 << bits.len() {
                let flags = bits.iter().enumerate().fold(0, |flags, (index, bit)| {
                    flags
                        | if subset & (1 << index) != 0 {
                            1 << bit
                        } else {
                            0
                        }
                });
                let data = fixture(kind, *identity, flags);
                let change = form(kind, *identity, flags, data.clone());
                assert_eq!(
                    decoded(&change).unwrap(),
                    (flags, data),
                    "{kind} {flags:#x}"
                );
                for cut in 0..change.data.len() {
                    assert!(
                        decoded(&form(kind, *identity, flags, change.data[..cut].to_vec()))
                            .is_err(),
                        "kind {kind}, flags {flags:#x}, cut {cut}"
                    );
                }
            }
        }
    }
}

#[test]
fn sav_45_other_record_types_remain_opaque() {
    let other = form(8, DEFAULT, u32::MAX, vec![0xA5]);
    assert!(other.class_change().unwrap().is_none());
    assert!(other.faction_change().unwrap().is_none());
    assert!(other.relationship_change().unwrap().is_none());
}

#[test]
fn sav_45_fields_have_distinct_typed_values() {
    let clas = form(30, DEFAULT, 3, fixture(30, DEFAULT, 3))
        .class_change()
        .unwrap()
        .unwrap();
    assert!(clas.tag_skills_changed);
    assert_eq!(clas.form_flags.unwrap().flags, 0xB4A3_9281);
    assert_eq!(clas.form_flags.unwrap().unknown, 0xD6C5);

    let fact = form(31, DEFAULT, 0x8000_0007, fixture(31, DEFAULT, 0x8000_0007))
        .faction_change()
        .unwrap()
        .unwrap();
    assert_eq!(fact.faction_flags, Some(0xA581_0001));
    let reactions = fact.reactions.unwrap();
    assert_eq!(reactions.entries.len(), 1);
    assert_eq!(
        reactions.entries[0].faction,
        RefId::from_bytes([0x42, 0x34, 0x56])
    );
    assert_eq!(reactions.entries[0].modifier, -9);
    assert_eq!(reactions.entries[0].reaction, 0xDEAD_BEEF);
    assert_eq!(fact.crime_counts, Some(CRIME));

    let rela = form(39, CREATED, 3, fixture(39, CREATED, 3))
        .relationship_change()
        .unwrap()
        .unwrap();
    assert_eq!(rela.created.unwrap()[1], RefId::from_bytes([0x80, 3, 4]));
    assert_eq!(rela.rank, Some(-3));
}

#[test]
fn sav_45_rejects_each_truncated_field_and_unknown_bit() {
    for (kind, identity, flags) in [
        (30, DEFAULT, 3),
        (31, DEFAULT, 0x8000_0007),
        (39, CREATED, 3),
        (39, DEFAULT, 3),
    ] {
        let data = fixture(kind, identity, flags);
        for cut in 0..data.len() {
            assert!(
                decoded(&form(kind, identity, flags, data[..cut].to_vec())).is_err(),
                "kind {kind} cut {cut}"
            );
        }
        let mut trailing = data;
        trailing.push(0);
        assert!(decoded(&form(kind, identity, flags, trailing)).is_err());
        for bit in 0..32 {
            if flags & (1 << bit) == 0
                && !matches!((kind, bit), (31, 0 | 1 | 2 | 31))
                && !matches!((kind, bit), (30 | 39, 0 | 1))
            {
                assert!(decoded(&form(kind, identity, 1 << bit, vec![])).is_err());
            }
        }
    }
}

#[test]
fn sav_45_reaction_count_width_and_bounds_are_preserved() {
    for count in [[0x01, 0x00].as_slice(), [0x02, 0, 0, 0].as_slice()] {
        let bytes = count.to_vec();
        let change = form(31, DEFAULT, 4, bytes.clone());
        assert_eq!(decoded(&change).unwrap(), (4, bytes));
    }
    assert!(decoded(&form(31, DEFAULT, 4, vec![0xFE, 0xFF, 0xFF, 0xFF])).is_err());
    assert!(decoded(&form(31, DEFAULT, 4, vec![3])).is_err());

    let mut change = form(31, DEFAULT, 4, fixture(31, DEFAULT, 4))
        .faction_change()
        .unwrap()
        .unwrap();
    change.reactions.as_mut().unwrap().entries.clear();
    assert!(change.data_bytes().is_err());
}

#[test]
fn sav_45_created_relationship_identity_is_required() {
    let created = form(39, CREATED, 0, fixture(39, CREATED, 0))
        .relationship_change()
        .unwrap()
        .unwrap();
    assert!(created.data_bytes(RefId::from_bytes(DEFAULT)).is_err());
    let ordinary = form(39, DEFAULT, 0, vec![])
        .relationship_change()
        .unwrap()
        .unwrap();
    assert!(ordinary.data_bytes(RefId::from_bytes(CREATED)).is_err());
}

#[test]
fn sav_45_compressed_payloads_decode_exactly() {
    for (kind, identity, flags) in [
        (30, DEFAULT, 3),
        (31, DEFAULT, 0x8000_0007),
        (39, CREATED, 3),
    ] {
        let data = fixture(kind, identity, flags);
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&data).unwrap();
        let mut change = form(kind, identity, flags, encoder.finish().unwrap());
        change.uncompressed_len = data.len() as u32;
        assert_eq!(decoded(&change).unwrap(), (flags, data));
    }
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_45_external_corpus_accounts_for_every_matching_change() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping CLAS/FACT/RELA corpus");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut counts = BTreeMap::from([("CLAS", 0usize), ("FACT", 0), ("RELA", 0)]);
    let mut flags = BTreeMap::new();
    let mut bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let Some(count) = counts.get_mut(form.record_type()) else {
                continue;
            };
            let (decoded_flags, encoded) = decoded(form).unwrap_or_else(|error| {
                panic!(
                    "{} {} {:06x} flags {:08x} payload {:02x?}: {error}",
                    path.display(),
                    form.record_type(),
                    form.form.raw(),
                    form.change_flags,
                    form.payload().unwrap()
                )
            });
            assert_eq!(decoded_flags, form.change_flags);
            assert_eq!(encoded, form.payload().unwrap().as_ref());
            *count += 1;
            bytes += encoded.len();
            *flags
                .entry((
                    save.form_version,
                    form.version,
                    form.record_type(),
                    form.change_flags,
                ))
                .or_insert(0usize) += 1;
        }
    }
    eprintln!(
        "SAV-45 {} saves, {bytes} bytes, counts={counts:?}, flags={flags:x?}",
        paths.len()
    );
    assert!(counts["FACT"] > 0 && counts["RELA"] > 0);
}
