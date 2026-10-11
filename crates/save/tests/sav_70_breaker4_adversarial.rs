// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{reference::flags, ChangeForm, RefId, Save};
use std::path::{Path, PathBuf};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn placement(out: &mut Vec<u8>) {
    reference(0x400800).write(out);
    for value in [1.25_f32, -2.0, 3.5, 0.0, 0.5, 2.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn collect_saves(path: &Path, saves: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, saves);
        } else if path.extension().is_some_and(|extension| extension == "ess") {
            saves.push(path);
        }
    }
}

#[test]
fn sav_70_pcon_havok_precedes_opaque_reference_flag_suffix() {
    let mut data = vec![];
    placement(&mut data);
    data.extend([0x00, 0xA1, 0xB2, 0xC3]);
    let form = ChangeForm {
        form: reference(0x400A00),
        change_flags: flags::MOVE
            | flags::HAVOK
            | flags::FORM
            | flags::BASE
            | flags::SCALE,
        type_field: 42,
        version: 0,
        uncompressed_len: 0,
        data,
    };

    let header = form.reference_header(78).unwrap().unwrap();
    assert_eq!(header.havok.as_ref().unwrap().data, []);
    assert!(!header.carries_reference_data);
    assert_eq!(header.bytes_consumed, 28);
    assert_eq!(
        header.data_bytes().unwrap(),
        form.data[..header.bytes_consumed]
    );
    assert_eq!(&form.data[header.bytes_consumed..], &[0xA1, 0xB2, 0xC3]);
    assert_eq!(header.bytes_consumed + header.remaining_bytes, form.data.len());
}

#[test]
fn sav_70_parw_payload_stays_opaque_even_with_reference_and_havok_flags() {
    let payload = [0xFF, 0x00, 0x80, 0x01, 0xAA, 0x55];
    let form = ChangeForm {
        form: reference(0x800ABC),
        change_flags: flags::MOVE
            | flags::HAVOK
            | flags::CELL
            | flags::FORM
            | flags::BASE
            | flags::SCALE,
        type_field: 47,
        version: 0,
        uncompressed_len: u32::MAX,
        data: payload.to_vec(),
    };

    assert!(form.reference_header(78).unwrap().is_none());
    let envelope = form.parw_change().unwrap();
    assert_eq!(envelope.change_flags, form.change_flags);
    assert_eq!(envelope.data, payload);
}

#[test]
fn sav_70_external_reference_prefixes_and_player_positions_round_trip() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping reference header corpus");
        return;
    };
    let mut saves = vec![];
    collect_saves(Path::new(&root), &mut saves);
    saves.sort();
    assert!(saves.len() >= 50);

    let mut references = 0;
    for path in saves {
        let save = Save::open(&path).unwrap();
        let player = save.player_location().unwrap().unwrap();
        for form in &save.change_forms {
            let Some(header) = form.reference_header(save.form_version).unwrap() else {
                continue;
            };
            let payload = form.payload().unwrap();
            assert_eq!(
                header.data_bytes().unwrap(),
                payload[..header.bytes_consumed]
            );
            assert_eq!(header.bytes_consumed + header.remaining_bytes, payload.len());
            if save.resolve(form.form) == Some(0x14) {
                let placement = header.initial.placement().unwrap();
                assert_eq!(
                    placement.position.map(f32::to_bits),
                    player.position.map(f32::to_bits)
                );
            }
            references += 1;
        }
    }
    assert!(references > 100_000);
}
