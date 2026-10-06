// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{
    reference::{flags, Initial},
    ChangeForm, RefId, Save,
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

fn change(form: u32, flags: u32, ty: u8, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: reference(form),
        change_flags: flags,
        type_field: ty,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn pcon_ignores_reference_flag_payloads_and_keeps_the_tail() {
    let mut data = vec![];
    placement(&mut data, 0x400800);
    data.extend([0xA5, 0xFE, 0xFF, 0xFF, 0x7F, 0x11, 0x22]);
    let form = change(
        0x400A00,
        flags::MOVE | flags::FORM | flags::BASE | flags::SCALE,
        42,
        data,
    );
    let header = form.reference_header(78).unwrap().unwrap();
    assert!(matches!(header.initial, Initial::Moved(_)));
    assert!(!header.carries_reference_data);
    assert_eq!(header.data_bytes().unwrap(), form.data[..27]);
    assert_eq!(
        &form.data[header.bytes_consumed..],
        &[0xA5, 0xFE, 0xFF, 0xFF, 0x7F, 0x11, 0x22]
    );
    assert_eq!(
        header.bytes_consumed + header.remaining_bytes,
        form.data.len()
    );
}

#[test]
fn pcon_only_consumes_havok_when_flagged_and_rejects_truncation() {
    let mut ordinary = vec![];
    placement(&mut ordinary, 0x400800);
    ordinary.extend([0x03, 0xAA, 0xBB]);
    let form = change(0x400A00, flags::MOVE, 42, ordinary.clone());
    let header = form.reference_header(78).unwrap().unwrap();
    assert_eq!(header.bytes_consumed, 27);
    assert_eq!(header.data_bytes().unwrap(), ordinary[..27]);

    let mut havok = vec![];
    placement(&mut havok, 0x400800);
    havok.extend([0x03, 0xAA, 0xBB]);
    let form = change(0x400A00, flags::MOVE | flags::HAVOK, 42, havok);
    assert!(form.reference_header(78).is_err());
}

#[test]
fn parw_remains_fully_opaque_for_created_and_movement_flags() {
    let data = [0x00, 0xFF, 0x80, 0x01, 0xAA];
    let form = change(
        0x800ABC,
        flags::MOVE | flags::HAVOK | flags::CELL | flags::FORM | flags::BASE | flags::SCALE,
        47,
        data.to_vec(),
    );
    assert!(form.reference_header(78).unwrap().is_none());
    let envelope = form.parw_change().unwrap();
    assert_eq!(envelope.change_flags, form.change_flags);
    assert_eq!(envelope.data, data);
}

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path.extension().is_some_and(|extension| extension == "ess") {
            paths.push(path);
        }
    }
}

#[test]
fn all_external_reference_headers_reencode_and_player_positions_match() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping reference header corpus");
        return;
    };
    let mut paths = vec![];
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let mut total = 0;
    for path in paths {
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
            assert_eq!(
                header.bytes_consumed + header.remaining_bytes,
                payload.len()
            );
            if save.resolve(form.form) == Some(0x14) {
                let initial = header.initial.placement().unwrap();
                assert_eq!(
                    initial.position.map(f32::to_bits),
                    player.position.map(f32::to_bits)
                );
            }
            total += 1;
        }
    }
    assert!(total > 100_000);
}
