// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, LoadOrder, PatcherSession, Plugin, Tag};

const NAME: Tag = Tag(*b"FULL");
const MARKER: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const SECOND: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x801,
};

fn source_order() -> LoadOrder {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(NAME, "Base marker"));
    base.push_top(Record::new(Tag(*b"STAT"), 0x801).zsub(NAME, "Unchanged marker"));

    let mut source = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_string()],
        ..Default::default()
    });
    source.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(NAME, "Source marker"));

    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Source.esp", source).unwrap();
    order
}

fn output() -> Plugin {
    Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_string(), "Source.esp".to_string()],
        ..Default::default()
    })
}

fn patch_bytes(order: &LoadOrder) -> Vec<u8> {
    let mut session = PatcherSession::new(order, output()).unwrap();
    let count = session
        .patch_winning_stat_records(|form, record| {
            if form == MARKER {
                let name = record
                    .subrecords
                    .iter_mut()
                    .find(|subrecord| subrecord.tag == NAME)
                    .unwrap();
                name.data = b"Patched marker\0".to_vec();
            }
        })
        .unwrap();
    assert_eq!(count, 1);
    session.into_output().to_bytes()
}

#[test]
fn edt_24_patcher_writes_deterministic_override_with_expected_winner() {
    let order = source_order();
    let first_bytes = patch_bytes(&order);
    let second_bytes = patch_bytes(&order);
    assert_eq!(first_bytes, second_bytes);

    let patch = Plugin::parse(&first_bytes).unwrap();
    assert_eq!(patch.records.len(), 1);
    assert_eq!(patch.header.record_count, 1);
    assert_eq!(patch.records[0].form_id, 0x0000_0800);
    assert_eq!(patch.records[0].find(NAME), Some(&b"Patched marker\0"[..]));

    let mut reloaded_order = source_order();
    reloaded_order.add("Patch.esp", patch).unwrap();
    assert_eq!(
        reloaded_order.winner(MARKER).unwrap().find(NAME),
        Some(&b"Patched marker\0"[..])
    );
    assert_eq!(
        reloaded_order.winner(SECOND).unwrap().find(NAME),
        Some(&b"Unchanged marker\0"[..])
    );
}

#[test]
fn edt_24_patcher_omits_unchanged_winners() {
    let order = source_order();
    let mut session = PatcherSession::new(&order, output()).unwrap();
    let count = session.patch_winning_stat_records(|_, _| {}).unwrap();
    assert_eq!(count, 0);
    assert!(session.output().records.is_empty());
}

#[test]
fn edt_24_patcher_rejects_incompatible_master_order() {
    let order = source_order();
    let incompatible = Plugin::new(FileHeader {
        masters: vec!["Source.esp".to_string(), "Base.esm".to_string()],
        ..Default::default()
    });
    let mut session = PatcherSession::new(&order, incompatible).unwrap();
    let error = session.patch_winning_stat_records(|form, record| {
        if form == MARKER {
            record.subrecords[0].data = b"Changed\0".to_vec();
        }
    });
    assert!(matches!(error, Err(soe_esm::Error::Invalid(_))));
}

#[test]
fn edt_24_patcher_rejects_unrelocated_form_links() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(
        Record::new(Tag(*b"STAT"), 0x800)
            .zsub(NAME, "Linked marker")
            .sub(Tag(*b"DNAM"), 0x0000_0801_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    let mut session = PatcherSession::new(
        &order,
        Plugin::new(FileHeader {
            masters: vec!["Base.esm".to_string()],
            ..Default::default()
        }),
    )
    .unwrap();
    let error = session.patch_winning_stat_records(|_, record| {
        record.subrecords[0].data = b"Changed\0".to_vec();
    });
    assert!(matches!(error, Err(soe_esm::Error::Invalid(_))));
}
