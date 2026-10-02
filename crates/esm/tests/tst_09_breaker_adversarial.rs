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
