// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for LVSP's source-defined record contract.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const LIST: u32 = 0x100;
const SPELL: u32 = 0x200;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn lvsp_form(form: u32, flags: u8, entries: &[(u16, u32, u16)]) -> Record {
    let mut record = Record::new(T(b"LVSP"), form)
        .sub(T(b"LVLD"), [0])
        .sub(T(b"LVLF"), [flags])
        .sub(T(b"LLCT"), [entries.len() as u8]);
    for &(level, form, count) in entries {
        let mut bytes = vec![0; 12];
        bytes[..2].copy_from_slice(&level.to_le_bytes());
        bytes[4..8].copy_from_slice(&form.to_le_bytes());
        bytes[8..10].copy_from_slice(&count.to_le_bytes());
        record = record.sub(T(b"LVLO"), bytes);
    }
    record
}

fn lvsp(flags: u8, entries: &[(u16, u32, u16)]) -> Record {
    lvsp_form(LIST, flags, entries)
}

fn world(records: Vec<Record>, extra: &[Record]) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"SPEL"), SPELL));
    for record in records.into_iter().chain(extra.iter().cloned()) {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Adversarial.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn defined_lvsp_flag_bits_are_accepted_and_reserved_bits_reject() {
    for flags in 0..=7 {
        let mut world = world(vec![lvsp(flags, &[(1, SPELL, 1)])], &[]);
        assert!(
            world.select_leveled_spells(id(LIST), 1).is_ok(),
            "flags {flags}"
        );
    }
    for flags in [8, 0x80, 0xff] {
        let mut world = world(vec![lvsp(flags, &[(1, SPELL, 1)])], &[]);
        assert!(
            world.select_leveled_spells(id(LIST), 1).is_err(),
            "flags {flags}"
        );
    }
}

#[test]
fn lvsp_entries_reject_forms_outside_spell_or_nested_lvsp_types() {
    for invalid_form in [0x300, 0x301] {
        let mut world = world(
            vec![lvsp(4, &[(1, invalid_form, 1)])],
            &[
                Record::new(T(b"ARMO"), 0x300),
                Record::new(T(b"SHOU"), 0x301),
            ],
        );
        assert!(world.select_leveled_spells(id(LIST), 1).is_err());
    }
}

#[test]
fn lvsp_nested_entries_resolve_to_concrete_spells_and_reject_a_cycle() {
    let child = 0x101;
    let mut nested = world(
        vec![
            lvsp(4, &[(1, child, 1)]),
            lvsp_form(child, 4, &[(1, SPELL, 1)]),
        ],
        &[],
    );
    assert_eq!(
        nested.select_leveled_spells(id(LIST), 1).unwrap(),
        [id(SPELL)].into()
    );

    let mut cyclic = world(
        vec![
            lvsp(0, &[(1, child, 1)]),
            lvsp_form(child, 0, &[(1, LIST, 1)]),
        ],
        &[],
    );
    assert!(cyclic.select_leveled_spells(id(LIST), 1).is_err());
}
