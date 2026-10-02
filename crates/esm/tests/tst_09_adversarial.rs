// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

// Load the live differential target so these assertions exercise its parser.
#[path = "tst_09_xedit_dump.rs"]
mod subject;

#[test]
fn non_ascii_field_hex_returns_a_parse_error_instead_of_panicking() {
    let input = "RECORD\tCELL\t00000001\nFIELD\tCELL\t00000001\tDATA\t0\tAéB\n";
    let outcome = std::panic::catch_unwind(|| subject::parse_dump(input));
    assert!(matches!(outcome, Ok(Err(_))));
}

#[test]
fn repeated_subrecords_keep_distinct_ordinals() {
    let input = concat!(
        "RECORD\tCELL\t00000001\n",
        "FIELD\tCELL\t00000001\tDATA\t0\t0100\n",
        "FIELD\tCELL\t00000001\tDATA\t1\t0200\n",
    );
    let expected = subject::parse_dump(input).unwrap();
    let plugin = soe_esm::Plugin::parse(&two_data_fields()).unwrap();
    assert!(subject::differences(&expected, &subject::from_plugin(&plugin)).is_empty());
}

fn two_data_fields() -> Vec<u8> {
    let record = soe_esm::Record::new(soe_esm::Tag::new(b"CELL"), 1)
        .sub(b"DATA", [1u8, 0])
        .sub(b"DATA", [2u8, 0]);
    let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
    let index = plugin.push_record(record);
    let mut group = soe_esm::Group::new(soe_esm::GroupKind::Top(soe_esm::Tag::new(b"CELL")));
    group.entries.push(soe_esm::Entry::Record(index));
    plugin.groups.push(group);
    plugin.to_bytes()
}
