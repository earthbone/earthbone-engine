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

fn placement(out: &mut Vec<u8>) {
    reference(0x400800).write(out);
    for value in [1.25_f32, -2.0, 3.5, 0.0, 0.5, 2.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn pcon(identity: u32, change_flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: reference(identity),
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
fn pcon_each_initial_layout_keeps_unmodeled_fields_in_opaque_suffix() {
    let ignored = flags::FORM | flags::BASE | flags::SCALE;
    let mut moved = vec![];
    placement(&mut moved);
    moved.extend([0xA1, 0xB2, 0xC3]);
    let moved_form = pcon(0x400A00, flags::MOVE | ignored, moved);

    let mut created = vec![];
    placement(&mut created);
    created.push(0x5A);
    reference(0x400900).write(&mut created);
    created.extend([0xD4, 0xE5]);
    let created_form = pcon(0x800ABC, ignored, created);

    let mut promoted = vec![];
    placement(&mut promoted);
    reference(0x400801).write(&mut promoted);
    promoted.extend((-2_i16).to_le_bytes());
    promoted.extend(7_i16.to_le_bytes());
    promoted.extend([0xF6, 0x07]);
    let promoted_form = pcon(0x400A00, flags::CELL | ignored, promoted);

    for (form, expected, tail) in [
        (&moved_form, Initial::Moved(soe_save::reference::Placement {
            cell_or_world: reference(0x400800),
            position: [1.25, -2.0, 3.5],
            rotation: [0.0, 0.5, 2.0],
        }), &[0xA1, 0xB2, 0xC3][..]),
        (&created_form, Initial::Created {
            placement: soe_save::reference::Placement {
                cell_or_world: reference(0x400800),
                position: [1.25, -2.0, 3.5],
                rotation: [0.0, 0.5, 2.0],
            },
            flags: 0x5A,
            base: reference(0x400900),
        }, &[0xD4, 0xE5][..]),
        (&promoted_form, Initial::Promoted {
            placement: soe_save::reference::Placement {
                cell_or_world: reference(0x400800),
                position: [1.25, -2.0, 3.5],
                rotation: [0.0, 0.5, 2.0],
            },
            secondary_cell: reference(0x400801),
            coordinates: [-2, 7],
        }, &[0xF6, 0x07][..]),
    ] {
        let header = form.reference_header(78).unwrap().unwrap();
        assert_eq!(header.initial, expected);
        assert!(!header.carries_reference_data);
        assert_eq!(header.data_bytes().unwrap(), form.data[..header.bytes_consumed]);
        assert_eq!(&form.data[header.bytes_consumed..], tail);
        assert_eq!(header.bytes_consumed + header.remaining_bytes, form.data.len());
    }
}

#[test]
fn pcon_all_unmodeled_reference_flag_subsets_round_trip_without_consumption() {
    let bits = [flags::FORM, flags::BASE, flags::SCALE];
    for subset in 0..8 {
        let ignored = bits
            .iter()
            .enumerate()
            .filter(|(index, _)| subset & (1 << index) != 0)
            .fold(0, |mask, (_, bit)| mask | bit);
        let mut data = vec![];
        placement(&mut data);
        let prefix_len = data.len();
        data.extend([0xFE, 0xED, subset]);
        let form = pcon(0x400A00, flags::MOVE | ignored, data);
        let header = form.reference_header(78).unwrap().unwrap();
        assert_eq!(header.bytes_consumed, prefix_len);
        assert_eq!(header.data_bytes().unwrap(), form.data[..prefix_len]);
        assert_eq!(&form.data[prefix_len..], &[0xFE, 0xED, subset as u8]);
    }
}

#[test]
fn pcon_havok_count_width_and_zero_length_are_preserved() {
    let mut data = vec![];
    placement(&mut data);
    data.extend([0x00, 0xA5]);
    let empty = pcon(0x400A00, flags::MOVE | flags::HAVOK, data);
    let header = empty.reference_header(78).unwrap().unwrap();
    assert_eq!(header.havok.as_ref().unwrap().data, []);
    assert_eq!(header.data_bytes().unwrap(), empty.data[..header.bytes_consumed]);
    assert_eq!(&empty.data[header.bytes_consumed..], &[0xA5]);

    let mut data = vec![];
    placement(&mut data);
    data.extend([0x02, 0x00, 0x00, 0x00, 0x22, 0x33]);
    let wide_zero = pcon(0x400A00, flags::MOVE | flags::HAVOK, data);
    let header = wide_zero.reference_header(78).unwrap().unwrap();
    assert_eq!(header.havok.as_ref().unwrap().data, []);
    assert_eq!(header.data_bytes().unwrap(), wide_zero.data[..header.bytes_consumed]);
    assert_eq!(
        &wide_zero.data[header.bytes_consumed..],
        &[0x22, 0x33]
    );
}

#[test]
fn external_save_reference_headers_reencode_and_player_positions_match() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping reference header corpus");
        return;
    };
    let mut saves = vec![];
    collect_saves(Path::new(&root), &mut saves);
    saves.sort();
    assert!(saves.len() >= 50);

    let mut reference_count = 0;
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
            assert_eq!(
                header.bytes_consumed + header.remaining_bytes,
                payload.len()
            );
            if save.resolve(form.form) == Some(0x14) {
                let placement = header.initial.placement().unwrap();
                assert_eq!(
                    placement.position.map(f32::to_bits),
                    player.position.map(f32::to_bits)
                );
            }
            reference_count += 1;
        }
    }
    assert!(reference_count > 100_000);
}
