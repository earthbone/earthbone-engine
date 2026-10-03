// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::{DataHandler, Form, FormId, FormType};
use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;

#[test]
fn shm_16_populated_views_only_return_the_winning_override() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"MISC"), 0x800).zsub(Tag(*b"EDID"), "Old"));
    let mut patch = Plugin::new(FileHeader { masters: vec!["Base.esm".into()], ..Default::default() });
    patch.push_top(Record::new(Tag(*b"MISC"), 0x800).zsub(Tag(*b"EDID"), "New"));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let world = World::new(order);
    let data = world.commonlib_data_handler();
    assert_eq!(data.lookup_by_editor_id("New").unwrap().form_id(), FormId(0x800));
    assert!(data.lookup_by_editor_id("Old").is_none());
    assert_eq!(data.forms_of_type(FormType(*b"MISC")), vec![FormId(0x800)]);
}
