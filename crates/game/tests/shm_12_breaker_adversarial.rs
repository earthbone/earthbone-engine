// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_game::{commonlib, Game};
use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

#[test]
fn shm_12_reference_handle_lookup_rejects_non_reference_and_unknown_ids() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x100));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x200).sub(Tag(*b"NAME"), 0x100_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Inventory.esm", plugin).unwrap();
    let game = Game::new(order);
    assert!(commonlib::lookup_reference_by_handle(&mut game.world_mut(), FormId(0x100)).is_err());
    assert!(commonlib::lookup_reference_by_handle(&mut game.world_mut(), FormId(0x999)).is_err());
    assert_eq!(commonlib::lookup_reference_by_handle(&mut game.world_mut(), FormId(0x200)).unwrap().form_id, FormId(0x200));
}
