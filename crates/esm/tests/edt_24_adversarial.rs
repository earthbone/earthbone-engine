// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{Error, GlobalFormId, LoadOrder, PatcherSession, Plugin, Tag};

#[test]
fn edt_24_callback_cannot_change_the_overridden_record_signature() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"FULL"), "Marker"));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    let mut session = PatcherSession::new(
        &order,
        Plugin::new(FileHeader {
            masters: vec!["Base.esm".into()],
            ..Default::default()
        }),
    )
    .unwrap();

    let result = session.patch_winning_stat_records(|_, record| {
        record.tag = Tag(*b"ARMO");
        record.subrecords[0].data = b"Changed\0".to_vec();
    });

    assert!(matches!(result, Err(Error::Invalid(_))));
    assert!(session.output().records.is_empty());
    assert_eq!(
        order
            .winner(GlobalFormId {
                file: 0,
                object: 0x800
            })
            .unwrap()
            .tag,
        Tag(*b"STAT")
    );
}
