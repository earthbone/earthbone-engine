#![allow(clippy::unwrap_used)]

#[path = "tst_09_xedit_dump.rs"]
mod subject;

use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin, Record, Tag};

#[test]
fn duplicate_engine_records_are_not_silently_collapsed_by_identity() {
    let expected = subject::parse_dump("RECORD\tCELL\t00000001\n").unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let first = plugin.push_record(Record::new(Tag::new(b"CELL"), 1));
    let second = plugin.push_record(Record::new(Tag::new(b"CELL"), 1));
    let mut group = Group::new(GroupKind::Top(Tag::new(b"CELL")));
    group
        .entries
        .extend([Entry::Record(first), Entry::Record(second)]);
    plugin.groups.push(group);

    assert!(
        subject::differences(&expected, &subject::from_plugin(&plugin))
            .iter()
            .any(|line| line.contains("count CELL:")),
        "duplicate source records must affect the count comparison"
    );
}
