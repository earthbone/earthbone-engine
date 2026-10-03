// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn shm_11_hazard_view_rejects_non_hazard_base_and_missing_reference() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x100));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x200).sub(Tag(*b"NAME"), 0x100_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Hazard.esm", plugin).unwrap();
    let mut world = World::new(order);
    assert_eq!(world.commonlib_hazard(id(0x200)), None);
    assert_eq!(world.commonlib_hazard(id(0x999)), None);
}
