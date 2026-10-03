// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-14 counts retrievals once, including missing winners.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_trace::{Operation, WorkFrame};

#[test]
fn prf_14_record_lookup_counts_winners_missing_forms_and_direct_reads_once() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    let mut order = LoadOrder::new();
    order.add("Counts.esm", plugin).unwrap();
    let form = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    let reference = order.winner_ref(form).unwrap();
    let frame = WorkFrame::begin();
    assert!(order.winner(form).is_some());
    assert!(order
        .winner(GlobalFormId {
            object: 0x999,
            ..form
        })
        .is_none());
    assert_eq!(order.record(reference).tag, Tag(*b"STAT"));
    assert_eq!(frame.counts().get(Operation::RecordLookups), 3);
}
