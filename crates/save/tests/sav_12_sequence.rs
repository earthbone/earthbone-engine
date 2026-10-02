// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-12 frames, corruption checks and the original 72-entry corpus ledger.

use soe_esm::{GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_save::{reference::Field, ChangeForm, RefId, Save, WString};
use std::collections::BTreeMap;
use std::path::Path;

const LEDGER: &str = include_str!("../../../docs/save-package-variants-v1.tsv");

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn named(name: &str, out: &mut Vec<u8>) {
    WString::from_text(name).write(out);
}

fn point(out: &mut Vec<u8>, exterior: bool, prefix: usize) {
    out.extend(vec![0xFF; prefix]);
    reference(if exterior { 0x40003C } else { 0 }).write(out);
    if exterior {
        out.extend([0xFF, 0xFF, 7, 0]);
    } else {
        reference(0x4133C6).write(out);
    }
    reference(0x4539F3).write(out);
    out.extend([0xFF, 0xFF, 1]);
}

fn request(out: &mut Vec<u8>, kind: u32, exterior: bool, extended: bool) {
    out.extend([0x19, 0x85, 0xFF, 0x6C]);
    out.extend(2_u16.to_le_bytes());
    out.push(0);
    out.extend(kind.to_le_bytes());
    point(out, exterior, 16);
    point(out, exterior, 21);
    out.extend([0xFF; 29]);
    out.extend(u32::from(extended).to_le_bytes());
    if extended {
        out.extend([0x85; 36]);
    }
    out.extend(vec![0xFF; if kind == 0x330E_B0E3 { 76 } else { 71 }]);
    out.extend(2_u32.to_le_bytes());
    for id in [0, 0x800009] {
        reference(id).write(out);
        out.push(0xFF);
    }
}

fn follow(out: &mut Vec<u8>, populated: bool, exterior: bool, extended: bool) {
    named("Follow", out);
    reference(0x413485).write(out);
    out.extend([0xFF; 16]);
    if populated {
        request(out, 0x330E_B0E3, exterior, extended);
    } else {
        out.extend([0; 4]);
    }
    out.extend([0x85; 8]);
}

fn location(out: &mut Vec<u8>, point: bool) {
    out.push(1);
    named(if point { "LocPt" } else { "LocRef" }, out);
    if point {
        out.extend([0xFF; 12]);
    }
    reference(0x800009).write(out);
    out.extend([0x85; 12]);
}

fn sandbox(out: &mut Vec<u8>, wander: bool, close_point: bool) {
    named("Sandbox", out);
    out.extend([0xFF; 74]);
    // CONTEXT: A wide count exercises preservation independently of its value.
    out.extend([0x09, 0x00]);
    out.extend([0x85; 30]);
    location(out, true);
    out.push(0);
    named(if wander { "Wander" } else { "" }, out);
    if wander {
        location(out, false);
        out.extend([0x85; 11]);
        let kind = if close_point {
            0xEB5E_D874
        } else {
            0x330E_B0E3
        };
        request(out, kind, close_point, !close_point);
        out.extend([0xFF; 4]);
        if close_point {
            out.extend(0x4348_0000_u32.to_le_bytes());
            out.extend([0xFF; 5]);
            point(out, true, 16);
            out.extend([0xFF; 3]);
        }
        out.extend([0xFF; 17]);
    } else {
        out.extend([0xFF; 5]);
        request(out, 0x330E_B0E3, false, false);
        out.extend([0x85; 9]);
    }
}

fn form(child: &[u8], index: u32) -> ChangeForm {
    let mut data = vec![8, 108, 18, 0];
    named("Sequence", &mut data);
    data.extend(child);
    data.extend(index.to_le_bytes());
    data.push(133);
    reference(0x800900).write(&mut data);
    data.push(4);
    reference(0x400800).write(&mut data);
    data.extend((-3_i32).to_le_bytes());
    data.push(0);
    data.extend([0xDE, 0xAD]);
    ChangeForm {
        form: reference(0x400001),
        change_flags: (1 << 11) | (1 << 5),
        type_field: 0,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

fn check_boundary(change: &ChangeForm) {
    let contents = change.reference_contents(78).unwrap().unwrap();
    assert_eq!(contents.end, change.data.len() - 2);
    assert_eq!(contents.remaining_bytes, 2);
    assert_eq!(contents.data_bytes().unwrap(), change.data[..contents.end]);
    let extra = contents.extra.unwrap();
    assert_eq!(extra.entries[1].kind, 133);
    assert!(
        matches!(extra.entries[1].fields[0], Field::Reference(id) if id == reference(0x800900))
    );
    let inventory = contents.inventory.unwrap();
    assert_eq!(inventory.entries[0].item, reference(0x400800));
    assert_eq!(inventory.entries[0].delta, -3);
}

#[test]
fn sav_12_follow_frames_preserve_raw_bits_and_the_next_extra_and_inventory() {
    for populated in [false, true] {
        for exterior in [false, true] {
            for extended in [false, true] {
                let mut child = vec![];
                follow(&mut child, populated, exterior, extended);
                check_boundary(&form(&child, 0));
            }
        }
    }
}

#[test]
fn sav_12_sandbox_frames_preserve_lists_locations_and_nested_wander() {
    for (wander, close_point) in [(false, false), (true, false), (true, true)] {
        let mut child = vec![];
        sandbox(&mut child, wander, close_point);
        check_boundary(&form(&child, 2));
    }
}

#[test]
fn sav_12_simultaneous_keeps_count_width_child_indices_and_sequence_index() {
    let mut child = vec![];
    named("Simultaneous", &mut child);
    child.extend([0x09, 0x00]);
    child.extend(0_u32.to_le_bytes());
    named("SitSleep", &mut child);
    child.extend([0x10, 0xFF, 0x85]);
    child.extend(1_u32.to_le_bytes());
    named("Wait", &mut child);
    child.extend([0xFF; 4]);
    check_boundary(&form(&child, 1));
}

#[test]
fn sav_12_every_truncated_frame_fails_before_accepting_a_boundary() {
    let mut children = vec![];
    let mut child = vec![];
    follow(&mut child, true, true, true);
    children.push(child);
    for close_point in [false, true] {
        let mut child = vec![];
        sandbox(&mut child, true, close_point);
        children.push(child);
    }
    for child in children {
        let complete = form(&child, 2);
        for cut in 0..complete.data.len() - 2 {
            let mut truncated = complete.clone();
            truncated.data.truncate(cut);
            assert!(truncated.reference_contents(78).is_err(), "cut {cut}");
        }
    }
}

#[test]
fn sav_12_unknown_children_requests_extensions_and_counts_return_errors() {
    let mut child = vec![];
    named("Other", &mut child);
    assert!(form(&child, 0).reference_contents(78).is_err());
    child.clear();
    follow(&mut child, true, false, false);
    for (at, bytes) in [
        (31, vec![3, 0]),
        (34, vec![0xFF; 4]),
        (129, vec![2, 0, 0, 0]),
        (209, vec![0xFF; 4]),
    ] {
        let mut invalid = child.clone();
        invalid[at..at + bytes.len()].copy_from_slice(&bytes);
        assert!(
            form(&invalid, 0).reference_contents(78).is_err(),
            "field {at}"
        );
    }
}

#[test]
fn sav_12_nesting_is_bounded_and_does_not_use_the_outer_terminator() {
    let mut child = vec![];
    for _ in 0..64 {
        named("Sequence", &mut child);
    }
    follow(&mut child, false, false, false);
    child.extend([0; 256]);
    assert!(form(&child, 0)
        .reference_contents(78)
        .unwrap_err()
        .to_string()
        .contains("nesting"));
}

fn observed_length(save_name: &str, raw: u32) -> usize {
    match raw {
        0x419DFD if save_name.starts_with("Clean Saves/") => 241,
        0x419DFD => 56,
        0x419DDD => 63,
        0x800E22 => 443,
        0x800956 => 465,
        0x000297 => 411,
        0x003D47 => 338,
        _ => panic!("unaccounted Sequence {raw:06x}"),
    }
}

fn occurrences() -> BTreeMap<String, Vec<(u32, usize)>> {
    let mut files = BTreeMap::<String, Vec<_>>::new();
    for row in LEDGER.lines().filter(|row| row.starts_with("save\t")) {
        files
            .entry(row.split('\t').nth(1).unwrap().into())
            .or_default();
    }
    for row in LEDGER
        .lines()
        .filter(|row| row.ends_with("PD18-SEQUENCE\tOPAQUE-CONTINUATION"))
    {
        let columns: Vec<_> = row.split('\t').collect();
        files.get_mut(columns[1]).unwrap().push((
            u32::from_str_radix(columns[2], 16).unwrap(),
            columns[5].parse().unwrap(),
        ));
    }
    files
}

#[test]
fn sav_12_every_ledger_payload_has_an_exact_independent_boundary() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        return;
    };
    let files = occurrences();
    assert_eq!(files.len(), 60);
    let mut lengths = BTreeMap::new();
    for (name, entries) in files {
        let save = Save::open(Path::new(&root).join(&name)).unwrap();
        for (raw, at) in entries {
            let change = save
                .change_forms
                .iter()
                .find(|change| change.form.raw() == raw)
                .unwrap();
            let payload = change.payload().unwrap();
            let length = observed_length(&name, raw);
            let bytes = &payload[at..at + length];
            assert_eq!(&bytes[..13], b"\x6c\x12\0\x08\0Sequence");
            let contents = change
                .reference_contents(save.form_version)
                .unwrap()
                .unwrap();
            assert_eq!(
                contents.data_bytes().unwrap(),
                payload[contents.start..contents.end]
            );
            assert_eq!(contents.remaining_bytes, payload.len() - contents.end);
            // CONTEXT: Replace all surrounding bytes so a suffix cannot supply framing.
            let mut isolated = form(
                &bytes[13..length - 4],
                u32::from_le_bytes(bytes[length - 4..].try_into().unwrap()),
            );
            check_boundary(&isolated);
            isolated.data.truncate(1 + length);
            isolated.data[0] = 4;
            isolated.change_flags = 1 << 11;
            let decoded = isolated.reference_contents(78).unwrap().unwrap();
            assert_eq!(decoded.end, 1 + length);
            assert_eq!(decoded.remaining_bytes, 0);
            assert_eq!(&decoded.data_bytes().unwrap()[1..], bytes);
            *lengths.entry(length).or_insert(0_usize) += 1;
        }
    }
    assert_eq!(
        lengths,
        BTreeMap::from([
            (56, 27),
            (63, 1),
            (241, 10),
            (338, 13),
            (411, 1),
            (443, 1),
            (465, 19)
        ])
    );
    eprintln!("SAV-12: all 72 payloads re-encode exactly with independent boundaries: {lengths:?}");
}

#[derive(Debug)]
struct Node {
    name: String,
    children: Vec<Node>,
}

fn tree(record: &Record) -> Node {
    let mut flat = vec![];
    let mut in_tree = false;
    for sub in &record.subrecords {
        if sub.tag == Tag(*b"XNAM") {
            in_tree = true;
        } else if in_tree && sub.tag == Tag(*b"UNAM") {
            break;
        } else if in_tree && sub.tag == Tag(*b"ANAM") {
            flat.push((
                String::from_utf8(sub.data.strip_suffix(&[0]).unwrap().to_vec()).unwrap(),
                0_usize,
            ));
        } else if in_tree && sub.tag == Tag(*b"PRCB") {
            flat.last_mut().unwrap().1 =
                u32::from_le_bytes(sub.data[..4].try_into().unwrap()) as usize;
        } else if in_tree && sub.tag == Tag(*b"PNAM") {
            flat.last_mut().unwrap().0 =
                String::from_utf8(sub.data.strip_suffix(&[0]).unwrap().to_vec()).unwrap();
        }
    }
    fn node(flat: &[(String, usize)], at: &mut usize) -> Node {
        let (name, count) = &flat[*at];
        *at += 1;
        Node {
            name: name.clone(),
            children: (0..*count).map(|_| node(flat, at)).collect(),
        }
    }
    let mut at = 0;
    let root = node(&flat, &mut at);
    assert_eq!(at, flat.len());
    root
}

fn group(field: &Field) -> &[Field] {
    let Field::Group(fields) = field else {
        panic!("expected group: {field:?}")
    };
    fields
}

fn match_child(fields: &[Field], authored: &Node) {
    let Field::String(name) = &fields[0] else {
        panic!("missing child name")
    };
    let expected = if authored.name == "Sit" {
        "SitSleep"
    } else {
        &authored.name
    };
    assert_eq!(name.to_text(), expected, "{authored:?}");
    if expected == "Simultaneous" {
        let Field::Array(array) = &group(&fields[1])[0] else {
            panic!("missing indexed children")
        };
        assert_eq!(array.entries.len(), authored.children.len());
        for entry in &array.entries {
            let Field::Word(index) = entry[0] else {
                panic!("missing child index")
            };
            match_child(group(&entry[1]), &authored.children[index as usize]);
        }
    }
}

#[test]
fn sav_12_vanilla_pack_trees_match_all_saved_sequence_child_indices() {
    let (Some(data), Some(root)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(Path::new(&data).join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut checked = 0;
    for (name, entries) in occurrences() {
        if entries.is_empty() {
            continue;
        }
        let save = Save::open(Path::new(&root).join(name)).unwrap();
        for (raw, _) in entries {
            let change = save
                .change_forms
                .iter()
                .find(|change| change.form.raw() == raw)
                .unwrap();
            let extras = change
                .reference_extra_data(save.form_version)
                .unwrap()
                .unwrap();
            let package = extras
                .entries
                .iter()
                .find(|extra| extra.kind == 25)
                .unwrap();
            let Field::Reference(package) = package.fields[0] else {
                panic!("missing PACK link")
            };
            let raw = save.resolve(package).unwrap();
            let plugin = save.plugins[(raw >> 24) as usize].to_text();
            let mut id = GlobalFormId {
                file: order.position_of(&plugin).unwrap() as u16,
                object: raw & 0xFFFFFF,
            };
            for _ in 0..8 {
                let record = order.winner(id).unwrap();
                assert_eq!(record.tag, Tag(*b"PACK"));
                let pkcu = record
                    .subrecords
                    .iter()
                    .find(|sub| sub.tag == Tag(*b"PKCU"))
                    .unwrap();
                let template = u32::from_le_bytes(pkcu.data[4..8].try_into().unwrap());
                if template == 0 {
                    break;
                }
                let owner = order.winner_ref(id).unwrap().file;
                id = order.resolve(owner, template);
            }
            let authored = tree(order.winner(id).unwrap());
            assert_eq!(authored.name, "Sequence");
            let sequence = extras
                .entries
                .iter()
                .find(|extra| extra.kind == 108)
                .unwrap();
            let state = group(&sequence.fields[3]);
            let Field::Word(index) = state[1] else {
                panic!("missing Sequence index")
            };
            match_child(group(&state[0]), &authored.children[index as usize]);
            checked += 1;
        }
    }
    assert_eq!(checked, 72);
}
