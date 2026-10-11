// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::{DataHandler, Form, FormId, FormType};
use soe_commonlib_shim::refr::{ObjectRef, ObjectRefPtr};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;

fn fixture() -> (World, GlobalFormId) {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"MISC"), 0x800).zsub(Tag(*b"EDID"), "OriginalName"));
    base.push_top(Record::new(Tag(*b"ACHR"), 0x801).sub(Tag(*b"NAME"), 0x802_u32.to_le_bytes()));
    base.push_top(Record::new(Tag(*b"NPC_"), 0x802));
    base.push_top(Record::new(Tag(*b"CELL"), 0x803));
    let mut update_header = FileHeader::default();
    update_header.masters.push("Base.esm".to_owned());
    let mut update = Plugin::new(update_header);
    update.push_top(Record::new(Tag(*b"MISC"), 0x800).zsub(Tag(*b"EDID"), "WinningName"));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Update.esm", update).unwrap();
    let actor = GlobalFormId { file: 0, object: 0x801 };
    (World::new(order), actor)
}

#[test]
fn data_handler_editor_lookup_and_type_list_use_only_the_winning_override() {
    let (world, _) = fixture();
    let data = world.commonlib_data_handler();
    let winner = data.lookup_by_editor_id("winningname").unwrap();

    assert_eq!(winner.form_id(), FormId(0x0000_0800));
    assert_eq!(winner.editor_id(), Some("WinningName"));
    assert!(data.lookup_by_editor_id("OriginalName").is_none());
    assert_eq!(data.forms_of_type(FormType(*b"MISC")), [FormId(0x800)]);
}

#[test]
fn null_and_stale_reference_pointers_fail_without_creating_a_reference() {
    let (mut world, actor) = fixture();
    assert!(world.commonlib_reference_ptr(ObjectRefPtr(None)).is_none());
    assert!(world
        .commonlib_reference_ptr(ObjectRefPtr(Some(soe_commonlib_shim::refr::ObjectRefHandle(0xDEAD_BEEF))))
        .is_none());
    assert!(world.commonlib_reference(actor).is_some());
}

#[test]
fn set_base_rejection_leaves_winning_reference_base_unchanged() {
    let (mut world, actor) = fixture();
    let mut reference = world.commonlib_reference(actor).unwrap();
    let before = reference.base();
    assert!(reference.set_base(FormId(0x800)).is_err());
    assert_eq!(reference.base(), before);
}
