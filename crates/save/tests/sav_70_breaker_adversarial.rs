// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{
    reference::{flags, Initial}, ChangeForm, RefId, Save,
};
use std::path::{Path, PathBuf};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn placement(out: &mut Vec<u8>, cell: u32) {
    reference(cell).write(out);
    for value in [1.0_f32, -2.5, 3.0, 0.0, 0.5, 2.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn pcon(form: u32, change_flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: reference(form),
        change_flags,
        type_field: 42,
        version: 0,
        uncompressed_len: 0,
        data,
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
fn pcon_constructed_header_keeps_reference_flags_out_of_prefix() {
    let mut data = vec![];
    placement(&mut data, 0x400800);
    data.push(0xA5);
    reference(0x400900).write(&mut data);
    data.extend([0xFE, 0xFF, 0x7F]);
    let form = pcon(
        0x800ABC,
        flags::FORM | flags::BASE | flags::SCALE,
        data,
    );

    let header = form.reference_header(78).unwrap().unwrap();
    assert!(matches!(header.initial, Initial::Created { flags: 0xA5, .. }));
    assert!(!header.carries_reference_data);
    assert_eq!(header.bytes_consumed, 31);
    assert_eq!(header.data_bytes().unwrap(), form.data[..header.bytes_consumed]);
    assert_eq!(&form.data[header.bytes_consumed..], &[0xFE, 0xFF, 0x7F]);
    assert_eq!(header.bytes_consumed + header.remaining_bytes, form.data.len());
}

#[test]
fn pcon_promoted_header_keeps_reference_flags_out_of_prefix() {
    let mut data = vec![];
    placement(&mut data, 0x400800);
    reference(0x400801).write(&mut data);
    data.extend((-2_i16).to_le_bytes());
    data.extend(7_i16.to_le_bytes());
    data.extend([0xA5, 0x5A]);
    let form = pcon(
        0x400A00,
        flags::CELL | flags::FORM | flags::BASE | flags::SCALE,
        data,
    );

    let header = form.reference_header(78).unwrap().unwrap();
    assert!(matches!(header.initial, Initial::Promoted { coordinates: [-2, 7], .. }));
    assert!(!header.carries_reference_data);
    assert_eq!(header.bytes_consumed, 34);
    assert_eq!(header.data_bytes().unwrap(), form.data[..header.bytes_consumed]);
    assert_eq!(&form.data[header.bytes_consumed..], &[0xA5, 0x5A]);
}

#[test]
fn every_corpus_reference_header_reencodes_and_matches_player_position() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        panic!("SOE_TEST_SAVES must point to the user's save corpus");
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
            assert_eq!(header.data_bytes().unwrap(), payload[..header.bytes_consumed]);
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
