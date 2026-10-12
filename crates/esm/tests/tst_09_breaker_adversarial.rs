#![allow(clippy::unwrap_used)]

#[path = "tst_09_xedit_dump.rs"]
mod subject;

use soe_esm::{FileHeader, Group, GroupKind, Plugin, Record, Tag};

#[test]
fn duplicate_engine_record_identity_still_changes_record_count() {
    let expected = subject::parse_dump("RECORD\tCELL\t00000001\n").unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let first = plugin.push_record(Record::new(Tag::new(b"CELL"), 1));
    let second = plugin.push_record(Record::new(Tag::new(b"CELL"), 1));
    let mut group = Group::new(GroupKind::Top(Tag::new(b"CELL")));
    group.entries.extend([
        soe_esm::Entry::Record(first),
        soe_esm::Entry::Record(second),
    ]);
    plugin.groups.push(group);

    assert!(
        !subject::differences(&expected, &subject::from_plugin(&plugin)).is_empty(),
        "two engine records must not compare equal to one xEdit record"
    );
}

#[test]
fn unexpected_manifest_signature_is_listed_as_a_difference() {
    let expected = subject::parse_dump("RECORD\tSTAT\t00000001\n").unwrap();
    let actual = subject::from_plugin(&Plugin::new(FileHeader::default()));

    assert!(subject::differences(&expected, &actual)
        .iter()
        .any(|line| line == "missing engine record STAT:00000001"));
}

#[test]
fn reports_physical_record_count_changes_per_form_id() {
    let expected = subject::parse_dump(concat!(
        "RECORD\tCELL\t00000001\n",
        "RECORD\tCELL\t00000001\n",
        "RECORD\tCELL\t00000002\n",
    ))
    .unwrap();

    let mut plugin = Plugin::new(FileHeader::default());
    let first = plugin.push_record(Record::new(Tag::new(b"CELL"), 1));
    let second = plugin.push_record(Record::new(Tag::new(b"CELL"), 2));
    let third = plugin.push_record(Record::new(Tag::new(b"CELL"), 2));
    let mut group = Group::new(GroupKind::Top(Tag::new(b"CELL")));
    group.entries.extend([
        soe_esm::Entry::Record(first),
        soe_esm::Entry::Record(second),
        soe_esm::Entry::Record(third),
    ]);
    plugin.groups.push(group);

    let found = subject::differences(&expected, &subject::from_plugin(&plugin));
    assert!(
        !found.is_empty(),
        "the xEdit and engine dumps have different physical counts for each FormID"
    );
}

#[test]
fn reports_per_form_id_multiplicity_even_when_signature_totals_match() {
    let expected = subject::parse_dump(concat!(
        "RECORD\tCELL\t00000001\n",
        "RECORD\tCELL\t00000001\n",
        "RECORD\tCELL\t00000002\n",
    ))
    .unwrap();

    let actual = subject::parse_dump(concat!(
        "RECORD\tCELL\t00000001\n",
        "RECORD\tCELL\t00000002\n",
        "RECORD\tCELL\t00000002\n",
    ))
    .unwrap();

    let found = subject::differences(&expected, &actual);
    assert!(
        !found.is_empty(),
        "per-FormID physical record count differences must be reported"
    );
}

#[test]
fn parses_fields_for_repeated_physical_records_with_the_same_form_id() {
    let manifest = concat!(
        "RECORD\tCELL\t00000001\n",
        "FIELD\tCELL\t00000001\tEDID\t0\t41\n",
        "RECORD\tCELL\t00000001\n",
        "FIELD\tCELL\t00000001\tEDID\t0\t42\n",
    );

    assert!(
        subject::parse_dump(manifest).is_ok(),
        "field ordinals restart for each physical record"
    );
}
