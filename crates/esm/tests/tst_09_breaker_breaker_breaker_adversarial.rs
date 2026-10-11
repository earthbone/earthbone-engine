// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[path = "tst_09_xedit_dump.rs"]
mod subject;

use soe_esm::{FileHeader, Group, GroupKind, Plugin, Record, Tag};

#[test]
fn manifest_counts_repeated_physical_records_with_the_same_form_id() {
    let manifest = concat!(
        "RECORD\tCELL\t00000001\n",
        "RECORD\tCELL\t00000001\n",
    );
    let expected = subject::parse_dump(manifest)
        .expect("xEdit visits each matching main record, including duplicate identities");

    let mut plugin = Plugin::new(FileHeader::default());
    let first = plugin.push_record(Record::new(Tag::new(b"CELL"), 1));
    let second = plugin.push_record(Record::new(Tag::new(b"CELL"), 1));
    let mut group = Group::new(GroupKind::Top(Tag::new(b"CELL")));
    group.entries.extend([
        soe_esm::Entry::Record(first),
        soe_esm::Entry::Record(second),
    ]);
    plugin.groups.push(group);

    assert!(subject::differences(&expected, &subject::from_plugin(&plugin)).is_empty());
}
