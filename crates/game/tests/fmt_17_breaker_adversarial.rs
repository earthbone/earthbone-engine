// SPDX-License-Identifier: GPL-3.0-or-later
use soe_esm::Tag;
use soe_game::vmad::Vmad;

const PERK: Tag = Tag::new(b"PERK");

fn name(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn authored_perk() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&5i16.to_le_bytes());
    bytes.extend_from_slice(&2i16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.push(2);
    name(&mut bytes, "PerkFragments");
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn authored_perk_entry() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&5i16.to_le_bytes());
    bytes.extend_from_slice(&2i16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.push(2);
    name(&mut bytes, "PerkFragments");
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&0xA0B0_C0D0u32.to_le_bytes());
    bytes.push(0xFE);
    name(&mut bytes, "PerkScript");
    name(&mut bytes, "Fragment_42");
    bytes
}

#[test]
fn fmt_17_empty_perk_fragment_list_round_trips_signed_version() {
    let mut bytes = authored_perk();
    bytes[6] = 0x80;
    let parsed = Vmad::parse(&bytes, PERK).unwrap();
    let perk = parsed.perk_fragments.as_ref().unwrap();
    assert_eq!(perk.version, i8::MIN);
    assert!(perk.fragments.is_empty());
    assert_eq!(parsed.write(PERK), bytes);
}

#[test]
fn fmt_17_perk_count_cannot_hide_a_missing_entry() {
    let mut bytes = authored_perk();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    let result = Vmad::parse(&bytes, PERK);
    assert!(result.is_err(), "declared fragment without bytes must fail");
}

#[test]
fn fmt_17_perk_entry_identity_and_authored_bytes_survive() {
    let bytes = authored_perk_entry();
    let parsed = Vmad::parse(&bytes, PERK).unwrap();
    let fragment = &parsed.perk_fragments.as_ref().unwrap().fragments[0];
    assert_eq!(fragment.index, 0xA0B0_C0D0);
    assert_eq!(fragment.marker, 0xFE);
    assert_eq!(fragment.script, "PerkScript");
    assert_eq!(fragment.function, "Fragment_42");
    assert_eq!(parsed.write(PERK), bytes);
}
