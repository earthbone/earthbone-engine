// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Independent wire fixtures and complete NPC payload checks over external saves.

use std::{collections::BTreeMap, io::Write, path::Path};

use soe_save::{npc::flags, npc::NpcChange, ChangeForm, RefId, Save};

fn change(flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 0, 7]),
        change_flags: flags,
        type_field: 9,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

fn sections() -> Vec<(u32, Vec<u8>)> {
    let mut face = vec![
        2, 0x40, 1, 2, 10, 20, 30, 255, 0, 0, 3,
        // Two head parts with a deliberately wide count.
        9, 0, 0x40, 2, 3, 0x80, 4, 5, 3,
    ];
    face.extend(2_u32.to_le_bytes());
    face.extend(0x80000000_u32.to_le_bytes());
    face.extend(0x7FC01234_u32.to_le_bytes());
    face.extend(1_u32.to_le_bytes());
    face.extend(0x12345678_u32.to_le_bytes());
    vec![
        (flags::FORM, vec![1, 2, 3, 4, 5, 6]),
        (flags::BASE, (0..24).collect()),
        (flags::FACTIONS, vec![9, 0, 0x40, 0, 1, 255, 0, 0, 2, 6]),
        (flags::SPELLS, vec![4, 0x40, 0, 3, 0, 4, 0, 0, 4]),
        (flags::AI, (10..30).collect()),
        (flags::NAME, vec![3, 0, b'A', 0x80, 0]),
        (flags::SKILLS, (20..72).collect()),
        (flags::CLASS, vec![0x40, 0, 5]),
        (flags::RACE, vec![0x40, 0, 6, 0, 0, 7]),
        (flags::FACE, face),
        (flags::GENDER, vec![1]),
        (flags::DEFAULT_OUTFIT, vec![0x40, 0, 8]),
        (flags::SLEEP_OUTFIT, vec![0, 0, 0]),
    ]
}

fn all_fields() -> ChangeForm {
    let mut flags = flags::ATTRIBUTES;
    let mut data = vec![];
    for (flag, bytes) in sections() {
        flags |= flag;
        data.extend(bytes);
    }
    change(flags, data)
}

#[test]
fn full_npc_decodes_named_fields_and_exact_wire_bits() {
    let form = all_fields();
    let npc = form.npc_change().unwrap().unwrap();
    assert_eq!(npc.data_bytes().unwrap(), form.data);
    assert_eq!(npc.form_flags, Some((0x04030201, 0x0605)));
    assert_eq!(npc.base.unwrap()[23], 23);
    assert_eq!(npc.ai.unwrap()[19], 29);
    assert_eq!(npc.skills.unwrap()[51], 71);
    assert_eq!(npc.factions.as_ref().unwrap().entries[0].rank, -1);
    assert_eq!(npc.factions.as_ref().unwrap().entries[1].form.raw(), 2);
    assert_eq!(
        npc.spells.as_ref().unwrap().spells.entries[0].raw(),
        0x400003
    );
    assert!(npc
        .spells
        .as_ref()
        .unwrap()
        .leveled_spells
        .entries
        .is_empty());
    assert_eq!(npc.spells.as_ref().unwrap().shouts.entries[0].raw(), 4);
    assert_eq!(npc.name.as_ref().unwrap().bytes(), [b'A', 0x80, 0]);
    assert_eq!(npc.class.unwrap().raw(), 0x400005);
    assert_eq!(npc.race.unwrap().0.raw(), 0x400006);
    assert_eq!(npc.race.unwrap().1.raw(), 7);
    let face = npc.face.as_ref().unwrap();
    assert_eq!(face.head.as_ref().unwrap().skin_color, [10, 20, 30, 255]);
    assert_eq!(face.head_parts.entries[1].raw(), 0x800405);
    assert_eq!(
        face.morph.as_ref().unwrap().options,
        [0x80000000, 0x7FC01234]
    );
    assert_eq!(face.morph.as_ref().unwrap().presets, [0x12345678]);
    assert_eq!(npc.gender, Some(1));
    assert_eq!(npc.default_outfit.unwrap().raw(), 0x400008);
    assert_eq!(npc.sleep_outfit.unwrap().raw(), 0);
}

#[test]
fn every_optional_field_can_stand_alone_and_truncation_never_succeeds() {
    for (flags, bytes) in sections() {
        let form = change(flags, bytes);
        assert_eq!(
            form.npc_change().unwrap().unwrap().data_bytes().unwrap(),
            form.data
        );
        for end in 0..form.data.len() {
            let mut short = form.clone();
            short.data.truncate(end);
            assert!(short.npc_change().is_err(), "flag={flags:#x}, cut={end}");
        }
    }
    let form = all_fields();
    for end in 0..form.data.len() {
        let mut short = form.clone();
        short.data.truncate(end);
        assert!(short.npc_change().is_err(), "all fields cut={end}");
    }
}

#[test]
fn head_parts_and_morph_are_independent_of_head_data_presence() {
    // xEdit, unlike FallrimTools, places both sections outside the head-data union.
    let form = change(flags::FACE, vec![0, 4, 0x40, 0, 1, 0]);
    let npc = form.npc_change().unwrap().unwrap();
    let face = npc.face.as_ref().unwrap();
    assert!(face.head.is_none());
    assert_eq!(face.head_parts.entries[0].raw(), 0x400001);
    assert!(face.morph.is_none());
    assert_eq!(npc.data_bytes().unwrap(), form.data);
    let mut morph = vec![0, 0, 1];
    morph.extend(0_u32.to_le_bytes());
    morph.extend(1_u32.to_le_bytes());
    morph.extend(99_u32.to_le_bytes());
    let form = change(flags::FACE, morph);
    assert_eq!(
        form.npc_change().unwrap().unwrap().data_bytes().unwrap(),
        form.data
    );
    assert!(change(flags::FACE, vec![0]).npc_change().is_err());
}

#[test]
fn prefix_and_complete_payloads_enforce_different_boundaries() {
    let mut form = all_fields();
    let size = form.data.len();
    form.data.extend([0xCD, 0xAB]);
    let (npc, used) = NpcChange::read_prefix(&form.data, form.change_flags).unwrap();
    assert_eq!(used, size);
    assert_eq!(npc.data_bytes().unwrap(), form.data[..size]);
    assert!(form.npc_change().is_err());
    form.type_field = 8;
    assert!(form.npc_change().unwrap().is_none());
    assert!(change(0, vec![]).npc_change().unwrap().is_some());
    assert!(change(flags::ATTRIBUTES, vec![])
        .npc_change()
        .unwrap()
        .is_some());
    assert!(change(1 << 7, vec![]).npc_change().is_err());
    assert!(change(1 << 31, vec![0; 16]).npc_change().is_err());
}

#[test]
fn oversized_lists_reserved_counts_and_inconsistent_edits_are_rejected() {
    for bytes in [vec![0xFE, 0xFF, 0xFF, 0xFF], vec![3], vec![4]] {
        assert!(change(flags::FACTIONS, bytes).npc_change().is_err());
    }
    assert!(change(flags::FACE, vec![0, 0, 1, 255, 255, 255, 255])
        .npc_change()
        .is_err());
    let form = all_fields();
    let mut npc = form.npc_change().unwrap().unwrap();
    npc.base = None;
    assert!(npc.data_bytes().is_err());
    let mut npc = form.npc_change().unwrap().unwrap();
    npc.face.as_mut().unwrap().head = None;
    assert!(npc.data_bytes().is_err());
    let mut npc = form.npc_change().unwrap().unwrap();
    npc.factions.as_mut().unwrap().entries.pop();
    assert!(npc.data_bytes().is_err());
    // An over-wide zero must retain all four count bytes.
    let form = change(flags::FACTIONS, vec![2, 0, 0, 0]);
    assert_eq!(
        form.npc_change().unwrap().unwrap().data_bytes().unwrap(),
        form.data
    );
}

#[test]
fn compressed_npc_payload_uses_the_same_complete_decoder() {
    let mut form = all_fields();
    let original = form.data.clone();
    let mut encoder = flate2::write::ZlibEncoder::new(vec![], flate2::Compression::default());
    encoder.write_all(&original).unwrap();
    form.data = encoder.finish().unwrap();
    form.uncompressed_len = original.len() as u32;
    assert_eq!(
        form.npc_change().unwrap().unwrap().data_bytes().unwrap(),
        original
    );
    form.uncompressed_len += 1;
    assert!(form.npc_change().is_err());
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
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
fn all_corpus_npc_changes_decode_completely_and_reencode_from_fields() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping NPC change corpus");
        return;
    };
    let mut paths = vec![];
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let (mut count, mut players, mut created, mut faces_without_head) = (0, 0, 0, 0);
    let mut flags = BTreeMap::new();
    let mut failures = BTreeMap::new();
    for path in &paths {
        let save = Save::open(path).unwrap();
        let before = count;
        for form in &save.change_forms {
            if form.type_index() != 9 {
                continue;
            }
            *flags.entry(form.change_flags).or_insert(0) += 1;
            match form.npc_change() {
                Ok(Some(npc)) => {
                    assert_eq!(
                        npc.data_bytes().unwrap(),
                        form.payload().unwrap().as_ref(),
                        "{} {:?}",
                        path.display(),
                        form.form
                    );
                    count += 1;
                    players += usize::from(save.resolve(form.form) == Some(7));
                    created += usize::from(form.form.kind() == soe_save::RefKind::Created);
                    faces_without_head +=
                        usize::from(npc.face.as_ref().is_some_and(|f| f.head.is_none()));
                }
                Err(error) => {
                    *failures.entry(error.to_string()).or_insert(0) += 1;
                }
                Ok(None) => panic!("NPC change was not recognized"),
            }
        }
        assert!(count > before, "no decoded NPCs in {}", path.display());
    }
    eprintln!("{} saves: {count} NPC changes, {players} player bases, {created} created, {faces_without_head} faces without head data", paths.len());
    eprintln!("flags={flags:x?}, failures={failures:?}");
    assert!(count > 100);
    assert_eq!(players, paths.len());
    assert!(failures.is_empty());
}

#[test]
fn inline_npc_fields_use_the_complete_decoder_and_stop_before_later_extras() {
    let npc = all_fields();
    let mut actor = change(1 << 18, vec![0; 8]);
    actor.type_field = 1;
    // ExtraCount precedes ExtraLeveledCreature. Unsupported PackageData follows it.
    actor.data.extend([12, 36, 7, 0, 45]);
    actor.data.extend([0x40, 8, 0x10, 0x40, 8, 0x12]);
    actor.data.extend(npc.change_flags.to_le_bytes());
    actor.data.extend(&npc.data);
    let end = actor.data.len();
    actor.data.extend([108, 18]);
    let decoded = actor.leveled_creature(78).unwrap().unwrap();
    assert_eq!(decoded.npc.data_bytes().unwrap(), npc.data);
    assert_eq!(decoded.npc.change_flags, npc.change_flags);
    assert_eq!(decoded.bytes_consumed, end);
    assert!(actor.reference_contents(78).is_err());
    for cut in 0..end {
        let mut short = actor.clone();
        short.data.truncate(cut);
        assert!(short.leveled_creature(78).is_err(), "cut={cut}");
    }
    actor.data.truncate(end);
    actor.data.push(22); // ExtraWorn has no payload.
    let contents = actor.reference_contents(78).unwrap().unwrap();
    assert_eq!(contents.remaining_bytes, 0);
    assert_eq!(contents.data_bytes().unwrap(), actor.data[8..]);
}

#[test]
fn all_corpus_inline_npcs_reencode_exactly_with_named_fields() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        return;
    };
    let mut paths = vec![];
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let (mut actors, mut changed) = (0, 0);
    let mut flags = BTreeMap::new();
    let mut configs = BTreeMap::new();
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let Some(saved) = form.leveled_creature(save.form_version).unwrap() else {
                continue;
            };
            let bytes = saved.npc.data_bytes().unwrap();
            let start = saved.bytes_consumed - bytes.len();
            assert_eq!(bytes, form.payload().unwrap()[start..saved.bytes_consumed]);
            actors += 1;
            changed += usize::from(saved.npc.change_flags != 0);
            *flags.entry(saved.npc.change_flags).or_insert(0) += 1;
            if let Some(base) = saved.npc.base {
                let template = u16::from_le_bytes([base[18], base[19]]);
                *configs.entry(template).or_insert(0) += 1;
            }
        }
    }
    eprintln!(
        "{} saves: {actors} leveled actors, {changed} inline NPC changes",
        paths.len()
    );
    eprintln!("flags={flags:x?}, saved template flags={configs:x?}");
    assert!(actors > 100_000);
    assert!(changed > 4_000);
}
