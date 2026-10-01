// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

//! PERK VMAD fixtures follow xEdit's `wbVMADFragmentedPERK` field order.

use soe_esm::Tag;
use soe_game::vmad::Vmad;

const PERK: Tag = Tag::new(b"PERK");

fn name(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn authored_perk(fragments: &[(u32, u8, &str, &str)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&5i16.to_le_bytes());
    bytes.extend_from_slice(&2i16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.push(2);
    name(&mut bytes, "PerkFragments");
    bytes.extend_from_slice(&(fragments.len() as u16).to_le_bytes());
    for (index, marker, script, function) in fragments {
        bytes.extend_from_slice(&index.to_le_bytes());
        bytes.push(*marker);
        name(&mut bytes, script);
        name(&mut bytes, function);
    }
    bytes
}

#[test]
fn fmt_17_perk_fragment_entries_retain_entry_point_identity() {
    let entries = [
        (0x0102_0304, 5, "PerkScript", "Fragment_0"),
        (0xA0B0_C0D0, 9, "OtherScript", "Fragment_1"),
    ];
    let parsed = Vmad::parse(&authored_perk(&entries), PERK).unwrap();
    let fragments = &parsed.perk_fragments.unwrap().fragments;

    assert_eq!(fragments.len(), 2);
    assert_eq!(fragments[0].index, entries[0].0);
    assert_eq!(fragments[1].index, entries[1].0);
    assert_eq!(fragments[0].marker, entries[0].1);
    assert_eq!(fragments[1].marker, entries[1].1);
    assert_eq!(fragments[0].script, entries[0].2);
    assert_eq!(fragments[0].function, entries[0].3);
    assert_eq!(fragments[1].script, entries[1].2);
    assert_eq!(fragments[1].function, entries[1].3);
}

#[test]
fn fmt_17_perk_fragments_reproduce_authored_bytes() {
    let mut bytes = authored_perk(&[
        (0x0102_0304, 5, "PerkScript", "Fragment_0"),
        (0xA0B0_C0D0, 9, "OtherScript", "Fragment_1"),
    ]);
    bytes[6] = 0xFF;
    let parsed = Vmad::parse(&bytes, PERK).unwrap();
    let fragments = parsed.perk_fragments.as_ref().unwrap();

    assert_eq!(fragments.version, -1);
    assert_eq!(fragments.file, "PerkFragments");
    assert_eq!(parsed.write(PERK), bytes);
}

#[test]
fn fmt_17_perk_fragment_tail_rejects_truncation_and_trailing_bytes() {
    let bytes = authored_perk(&[(3, 1, "PerkScript", "Fragment_3")]);
    for cut in 7..bytes.len() {
        assert!(Vmad::parse(&bytes[..cut], PERK).is_err(), "cut {cut}");
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(Vmad::parse(&trailing, PERK).is_err());
}
