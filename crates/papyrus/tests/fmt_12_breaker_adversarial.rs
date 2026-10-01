// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::pex::{self, Operand};

fn pex_39_with_const_structs_and_groups() -> Vec<u8> {
    const STRINGS: &[&str] = &[
        "Sample",
        "Parent",
        "",
        "",
        "",
        "Example",
        "count",
        "Int",
        "member",
        "Speed",
        "Movement",
        "Movement settings",
        "Speed",
        "Example",
        "Example",
        "member",
    ];
    fn string_index(out: &mut Vec<u8>, value: &str) {
        let index = STRINGS.iter().position(|s| *s == value).unwrap() as u16;
        out.extend_from_slice(&index.to_le_bytes());
    }
    fn u16le(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_le_bytes());
    }
    fn u32le(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }
    fn raw_string(out: &mut Vec<u8>, value: &str) {
        u16le(out, value.len() as u16);
        out.extend_from_slice(value.as_bytes());
    }

    let mut out = Vec::new();
    u32le(&mut out, pex::MAGIC);
    out.extend_from_slice(&[3, 9]);
    u16le(&mut out, 2);
    out.extend_from_slice(&0u64.to_le_bytes());
    raw_string(&mut out, "Sample.psc");
    raw_string(&mut out, "test");
    raw_string(&mut out, "machine");
    u16le(&mut out, STRINGS.len() as u16);
    for value in STRINGS {
        raw_string(&mut out, value);
    }

    // Debug functions, property groups, then struct member order.
    out.push(1);
    out.extend_from_slice(&0u64.to_le_bytes());
    u16le(&mut out, 0);
    u16le(&mut out, 1);
    string_index(&mut out, "Sample");
    string_index(&mut out, "Movement");
    string_index(&mut out, "Movement settings");
    u32le(&mut out, 0);
    u16le(&mut out, 1);
    string_index(&mut out, "Speed");
    u16le(&mut out, 1);
    string_index(&mut out, "Sample");
    string_index(&mut out, "Example");
    u16le(&mut out, 1);
    string_index(&mut out, "member");

    u16le(&mut out, 0); // user flags
    u16le(&mut out, 1); // objects
    string_index(&mut out, "Sample");
    u32le(&mut out, 0); // object size
    string_index(&mut out, "Parent");
    string_index(&mut out, "");
    out.push(1); // object const
    u32le(&mut out, 0);
    string_index(&mut out, ""); // auto state

    u16le(&mut out, 1); // structs
    string_index(&mut out, "Example");
    u16le(&mut out, 1); // struct members
    string_index(&mut out, "member");
    string_index(&mut out, "Int");
    u32le(&mut out, 0);
    out.push(3); // integer default operand
    u32le(&mut out, 7);
    out.push(1); // member const
    string_index(&mut out, "");

    u16le(&mut out, 1); // variables
    string_index(&mut out, "count");
    string_index(&mut out, "Int");
    u32le(&mut out, 0);
    out.push(3); // integer default operand
    u32le(&mut out, 4);
    out.push(1); // variable const
    u16le(&mut out, 0); // properties
    u16le(&mut out, 0); // states
    out
}

#[test]
fn fmt_12_pex_39_const_structs_and_property_groups_load() {
    let script = pex::parse(&pex_39_with_const_structs_and_groups())
        .expect("3.9 PEX with extended object layout parses");

    assert_eq!(script.header.major, 3);
    assert_eq!(script.header.minor, 9);
    assert_eq!(script.header.game_id, 2);

    let object = script.object();
    assert!(object.is_const);
    assert!(object.variables[0].is_const);
    assert_eq!(object.variables[0].name, "count");
    let structure = &object.structs[0];
    assert_eq!(structure.name, "Example");
    assert!(structure.members[0].is_const);
    assert_eq!(structure.members[0].name, "member");
    assert_eq!(structure.members[0].default, Operand::Int(7));

    let group = &script.property_groups[0];
    assert_eq!(group.object, "Sample");
    assert_eq!(group.name, "Movement");
    assert_eq!(group.docstring, "Movement settings");
    assert_eq!(group.properties, ["Speed"]);
    assert_eq!(script.struct_orders[0].members, ["member"]);
}

#[test]
fn fmt_12_pex_39_truncated_layout_is_rejected() {
    let bytes = pex_39_with_const_structs_and_groups();
    for end in 0..bytes.len() {
        assert!(pex::parse(&bytes[..end]).is_err(), "accepted prefix {end}");
    }
}

#[test]
fn fmt_12_game_id_one_does_not_use_the_39_layout() {
    let mut bytes = pex_39_with_const_structs_and_groups();
    bytes[6..8].copy_from_slice(&1u16.to_le_bytes());
    assert!(matches!(
        pex::parse(&bytes),
        Err(pex::PexError::UnsupportedVersion { major: 3, minor: 9 })
    ));
}
