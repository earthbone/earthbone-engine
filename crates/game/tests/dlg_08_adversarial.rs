// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn dlg_08_unloaded_or_missing_merchant_cannot_expose_an_available_service() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"FACT"), 0x800)
            .sub(Tag(*b"DATA"), (1u32 << 14).to_le_bytes())
            .sub(Tag(*b"VEND"), 0x900u32.to_le_bytes())
            .sub(Tag(*b"VENC"), 0x501u32.to_le_bytes())
            .sub(Tag(*b"VENV"), [0, 0, 23, 0, 120, 0, 0, 0, 1, 0, 0, 0]),
    );
    plugin.push_top(Record::new(Tag(*b"FLST"), 0x900));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x501).sub(Tag(*b"NAME"), 0x400u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"CONT"), 0x400));
    let mut order = LoadOrder::new();
    order.add("Vendor.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let service = game
        .resolve_vendor_service(id(0x800), id(0x999))
        .unwrap()
        .unwrap();
    assert!(
        !service.available,
        "CK barter service is unavailable for a merchant that is not loaded"
    );
    assert!(
        service.merchandise.is_empty(),
        "an unloaded merchant exposes no merchandise"
    );
    assert_eq!(service.gold, 0, "an unloaded merchant exposes no gold");
}
