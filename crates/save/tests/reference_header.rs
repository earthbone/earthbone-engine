// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Placement prefixes must retain exact bytes while identifying the unparsed
//! suffix. Corpus checks compare player headers with the independent type-1 block.

use soe_save::{
    reference::{flags, Initial, PlayerLocation, Scale},
    ChangeForm, RefId, Save,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn placement(out: &mut Vec<u8>, cell: u32) {
    reference(cell).write(out);
    for value in [1_f32, -2.5, 3.0, -0.0, 0.5, 2.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn change(form: u32, actor: bool, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: reference(form),
        change_flags: flags,
        type_field: u8::from(actor),
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn created_identity_controls_initial_data_even_without_movement_flags() {
    let mut data = vec![];
    placement(&mut data, 0x400800);
    data.push(0xA5);
    reference(0x400900).write(&mut data);
    data.extend([9, 8, 7]);
    let form = change(0x800ABC, false, 0, data);
    let header = form.reference_header(78).unwrap().unwrap();
    assert!(
        matches!(header.initial, Initial::Created { flags: 0xA5, base, .. } if base.raw() == 0x400900)
    );
    assert_eq!(header.bytes_consumed, 31);
    assert_eq!(header.remaining_bytes, 3);
    assert_eq!(header.data_bytes().unwrap(), form.data[..31]);
}

#[test]
fn promoted_actor_fields_follow_havok_and_precede_optional_reference_fields() {
    let mut data = vec![];
    placement(&mut data, 0x400800);
    reference(0x400801).write(&mut data);
    data.extend((-2_i16).to_le_bytes());
    data.extend(7_i16.to_le_bytes());
    data.extend([9, 0, 1, 2]);
    data.extend(0x12345678_u32.to_le_bytes());
    data.extend(0xABCDEFAA_u32.to_le_bytes());
    data.extend(0x800_u32.to_le_bytes());
    data.extend(0x123_u16.to_le_bytes());
    reference(0x400900).write(&mut data);
    data.extend(1.5_f32.to_le_bytes());
    let form = change(
        0x400A00,
        true,
        flags::PROMOTED | flags::HAVOK | flags::FORM | flags::BASE | flags::SCALE,
        data,
    );
    let header = form.reference_header(78).unwrap().unwrap();
    assert!(
        matches!(header.initial, Initial::Promoted { secondary_cell, coordinates: [-2, 7], .. } if secondary_cell.raw() == 0x400801)
    );
    assert_eq!(header.havok.as_ref().unwrap().data, [1, 2]);
    assert_eq!(header.actor, Some([0x12345678, 0xABCDEFAA]));
    assert_eq!(header.form_flags, Some((0x800, 0x123)));
    assert_eq!(header.base.unwrap().raw(), 0x400900);
    assert_eq!(header.scale, Some(Scale::Float(1.5)));
    assert_eq!(header.remaining_bytes, 0);
    assert_eq!(header.data_bytes().unwrap(), form.data);
    for cut in 0..form.data.len() {
        let mut truncated = form.clone();
        truncated.data.truncate(cut);
        assert!(truncated.reference_header(78).is_err(), "{cut}");
    }
}

#[test]
fn scale_version_55_exception_is_not_applied_to_special_edition() {
    let form = change(
        0x400A00,
        false,
        flags::SCALE,
        125_u16.to_le_bytes().to_vec(),
    );
    let old = form.reference_header(55).unwrap().unwrap();
    assert_eq!(old.scale.unwrap().factor(), 1.25);
    assert!(form.reference_header(78).is_err());
    let mut modern = form;
    modern.data = f32::from_bits(0x7FC00023).to_le_bytes().to_vec();
    let header = modern.reference_header(78).unwrap().unwrap();
    assert_eq!(header.data_bytes().unwrap(), modern.data);
}

#[test]
fn unchanged_reference_and_actor_prefixes_remain_distinct() {
    let mut form = change(0x400A00, false, 0, vec![]);
    let header = form.reference_header(78).unwrap().unwrap();
    assert_eq!(header.initial, Initial::Unchanged);
    assert_eq!(header.bytes_consumed, 0);
    form.type_field = 1;
    assert!(form.reference_header(78).is_err());
    form.data = vec![0; 8];
    assert_eq!(
        form.reference_header(78).unwrap().unwrap().actor,
        Some([0, 0])
    );
    form.type_field = 8;
    assert!(form.reference_header(78).unwrap().is_none());
}

#[test]
fn sav_26_pcon_decodes_placement_and_preserves_opaque_tail() {
    let mut data = vec![];
    placement(&mut data, 0x400800);
    data.extend([0xAA, 0xBB]);
    let mut form = change(0x400A00, false, flags::MOVE, data);
    form.type_field = 42;
    let header = form.reference_header(78).unwrap().unwrap();
    assert!(matches!(header.initial, Initial::Moved(_)));
    assert_eq!(header.bytes_consumed, 27);
    assert_eq!(header.remaining_bytes, 2);
    assert_eq!(&form.data[header.bytes_consumed..], &[0xAA, 0xBB]);
}

#[test]
fn sav_70_pcon_and_parw_unmodeled_reference_flags_round_trip() {
    let unmodeled_flags = flags::FORM | flags::BASE | flags::SCALE;

    let mut pcon_data = vec![];
    placement(&mut pcon_data, 0x400800);
    pcon_data.extend([0xAA, 0xBB]);
    let mut pcon = change(0x400A00, false, flags::MOVE | unmodeled_flags, pcon_data);
    pcon.type_field = 42;
    let pcon_header = pcon.reference_header(78).unwrap().unwrap();
    assert!(!pcon_header.carries_reference_data);
    assert!(pcon_header.carries_havok);
    let pcon_prefix = pcon_header.data_bytes().unwrap();
    assert_eq!(pcon_prefix, pcon.data[..pcon_header.bytes_consumed]);
    assert_eq!(&pcon.data[pcon_header.bytes_consumed..], &[0xAA, 0xBB]);
    assert_eq!(
        pcon_header.bytes_consumed + pcon_header.remaining_bytes,
        pcon.data.len()
    );
    assert_eq!(
        [
            pcon_prefix,
            pcon.data[pcon_header.bytes_consumed..].to_vec()
        ]
        .concat(),
        pcon.data
    );

    let mut parw = change(
        0x400A00,
        false,
        flags::HAVOK | unmodeled_flags,
        vec![0x11, 0x22, 0x33, 0x44],
    );
    parw.type_field = 47;
    assert!(parw.reference_header(78).unwrap().is_none());
    let envelope = parw.parw_change().unwrap();
    assert_eq!(envelope.change_flags, parw.change_flags);
    assert_eq!(envelope.data, parw.data);
}

#[test]
fn malformed_havok_counts_and_changed_header_presence_are_rejected() {
    let mut data = vec![];
    placement(&mut data, 0x400800);
    data.extend([0xFE, 0xFF, 0xFF]);
    let mut form = change(0x400A00, false, flags::HAVOK, data);
    assert!(form.reference_header(78).is_err());
    form.data.truncate(27);
    form.data.push(0);
    let mut header = form.reference_header(78).unwrap().unwrap();
    header.havok.as_mut().unwrap().data.push(0);
    assert!(header.data_bytes().is_err());
    header.havok = None;
    assert!(header.data_bytes().is_err());
}

#[test]
fn player_location_retains_counter_coordinates_and_float_bits() {
    let location = PlayerLocation {
        next_object_id: 0x1234,
        world: reference(0x40003C),
        coordinates: [-2, 10],
        cell_or_world: reference(0x40003C),
        position: [-0.0, f32::from_bits(0x7FC00031), 1024.0],
        unknown: 17,
    };
    let bytes = location.data_bytes();
    assert_eq!(bytes.len(), 31);
    assert_eq!(PlayerLocation::read(&bytes).unwrap().data_bytes(), bytes);
    for cut in 0..bytes.len() {
        assert!(PlayerLocation::read(&bytes[..cut]).is_err());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(PlayerLocation::read(&trailing).is_err());
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
fn sav_70_all_external_reference_headers_reencode_and_compare_player_locations() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping reference header corpus");
        return;
    };
    let mut paths = vec![];
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let (mut total, mut created, mut with_suffix, mut nonfinite) = (0, 0, 0, 0);
    let mut layouts = BTreeMap::new();
    let mut player_pairs = BTreeMap::new();
    let mut actor_words = BTreeMap::new();
    let mut mask_versions = BTreeMap::new();
    let mut mask_bits = BTreeMap::new();
    let mut second_words = BTreeMap::new();
    let mut relevant_masks = [0_usize; 3];
    let mut next_range = (u32::MAX, 0);
    for path in &paths {
        let save = Save::open(path).unwrap();
        let player = save.player_location().unwrap().unwrap();
        next_range.0 = next_range.0.min(player.next_object_id);
        next_range.1 = next_range.1.max(player.next_object_id);
        for form in &save.change_forms {
            let Some(header) = form
                .reference_header(save.form_version)
                .unwrap_or_else(|error| panic!("{} {:?}: {error}", path.display(), form.form))
            else {
                continue;
            };
            let bytes = header.data_bytes().unwrap();
            let payload = form.payload().unwrap();
            assert_eq!(bytes, payload[..header.bytes_consumed]);
            assert_eq!(
                header.bytes_consumed + header.remaining_bytes,
                payload.len()
            );
            let kind = match header.initial {
                Initial::Unchanged => 0,
                Initial::Moved(_) => 4,
                Initial::Created { .. } => 5,
                Initial::Promoted { .. } => 6,
            };
            *layouts.entry(kind).or_insert(0) += 1;
            if kind == 5 {
                created += 1;
            }
            if header.remaining_bytes > 0 {
                with_suffix += 1;
            }
            if header
                .initial
                .placement()
                .is_some_and(|p| p.position.iter().chain(&p.rotation).any(|v| !v.is_finite()))
            {
                nonfinite += 1;
            }
            if let Some((mask, unknown)) = header.form_flags {
                *mask_versions
                    .entry((
                        save.header.version,
                        save.form_version,
                        form.version,
                        form.type_index(),
                    ))
                    .or_insert(0_usize) += 1;
                *mask_bits.entry(mask & 0xC00).or_insert(0_usize) += 1;
                *second_words.entry(unknown).or_insert(0_usize) += 1;
                relevant_masks[0] += usize::from(mask & 0x800 != 0);
                relevant_masks[1] += usize::from(mask & 0x400 != 0);
                relevant_masks[2] += usize::from(unknown != 0);
            }
            if let Some(words) = header.actor {
                *actor_words.entry(words[0]).or_insert(0) += 1;
            }
            if save.resolve(form.form) == Some(0x14) {
                let initial = header.initial.placement().unwrap();
                let first = save.resolve(initial.cell_or_world);
                let second = match header.initial {
                    Initial::Promoted { secondary_cell, .. } => save.resolve(secondary_cell),
                    _ => None,
                };
                let current = save.resolve(player.cell_or_world);
                *player_pairs
                    .entry((first == current, second == current))
                    .or_insert(0) += 1;
                assert_eq!(
                    initial.position.map(f32::to_bits),
                    player.position.map(f32::to_bits),
                    "{}",
                    path.display()
                );
            }
            total += 1;
        }
    }
    assert!(total > 100000);
    eprintln!("{} saves: {total} reference headers, {created} created, {with_suffix} nonempty suffixes, {nonfinite} nonfinite transforms", paths.len());
    eprintln!("mask versions (file, form, change, type)={mask_versions:?}");
    for (bits, count) in mask_bits {
        eprintln!("mask bits 0x{bits:03X}: {count}");
    }
    for (word, count) in second_words {
        eprintln!("second word 0x{word:04X}: {count}");
    }
    eprintln!("bit11/bit10/nonzero second word={relevant_masks:?}");
    eprintln!("layouts={layouts:?}, player first/second match={player_pairs:?}, actor first word={actor_words:?}, next object range={next_range:x?}");
}
