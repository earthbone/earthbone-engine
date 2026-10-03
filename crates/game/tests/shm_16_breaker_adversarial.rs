// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial SHM-16 checks grounded in docs/shim.md, the shim traits, and
//! CommonLibSSE-NG's TESDataHandler, TESFile, TESObjectREFR, and ActorState.

use soe_commonlib_shim::form::{DataHandler, Form, FormId, FormType};
use soe_commonlib_shim::refr::{ObjectRef, ObjectRefHandle, ObjectRefPtr};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;

fn misc(object: u32, editor_id: &str) -> Record {
    Record::new(Tag(*b"MISC"), object).zsub(Tag(*b"EDID"), editor_id)
}

fn reference(object: u32, base: u32) -> Record {
    Record::new(Tag(*b"REFR"), object)
        .sub(Tag(*b"NAME"), base.to_le_bytes())
        .sub(
            Tag(*b"DATA"),
            [1.0_f32, 2.0, 3.0, 0.25, 0.5, 0.75]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<u8>>(),
        )
}

fn light_plugin() -> Plugin {
    let mut header = FileHeader::default();
    header.flags |= soe_esm::plugin::flags::LIGHT_MASTER;
    Plugin::new(header)
}

#[test]
fn data_handler_orders_regular_slots_before_light_slots() {
    let mut regular0 = Plugin::new(FileHeader::default());
    regular0.push_top(misc(0x800, "RegularZero"));
    let mut light0 = light_plugin();
    light0.push_top(misc(0x10, "LightZero"));
    let mut regular1 = Plugin::new(FileHeader {
        masters: vec!["Regular0.esm".into()],
        ..FileHeader::default()
    });
    regular1.push_top(misc(0x801, "RegularOne"));
    let mut light1 = light_plugin();
    light1.push_top(misc(0x11, "LightOne"));
    let mut order = LoadOrder::new();
    order.add("Regular0.esm", regular0).unwrap();
    order.add("Light0.esl", light0).unwrap();
    order.add("Regular1.esp", regular1).unwrap();
    order.add("Light1.esl", light1).unwrap();

    let data = World::new(order).commonlib_data_handler();
    assert_eq!(
        data.files(),
        ["Regular0.esm", "Regular1.esp", "Light0.esl", "Light1.esl"]
    );
    assert_eq!(data.file(0).unwrap().regular_index, Some(0));
    assert_eq!(data.file(1).unwrap().regular_index, Some(1));
    assert_eq!(data.file(2).unwrap().light_index, Some(0));
    assert_eq!(data.file(3).unwrap().light_index, Some(1));
}

#[test]
fn data_handler_exposes_only_the_winning_form_for_an_overridden_identity() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(misc(0x800, "LosingName"));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(misc(0x800, "WinningName"));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();

    let data = World::new(order).commonlib_data_handler();
    let winner = data.lookup_by_editor_id("winningname").unwrap();
    assert_eq!(winner.form_id(), FormId(0x0000_0800));
    assert_eq!(winner.form_type(), FormType(*b"MISC"));
    assert_eq!(winner.editor_id(), Some("WinningName"));
    assert!(data.lookup_by_editor_id("LosingName").is_none());
    assert_eq!(data.forms_of_type(FormType(*b"MISC")), [FormId(0x800)]);
}

#[test]
fn reference_pointer_lookup_rejects_null_and_unknown_runtime_handles() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(misc(0x800, "Base"));
    plugin.push_top(reference(0x801, 0x800));
    let mut order = LoadOrder::new();
    order.add("Refs.esm", plugin).unwrap();
    let mut world = World::new(order);
    let id = GlobalFormId {
        file: 0,
        object: 0x801,
    };

    assert!(world
        .commonlib_reference_ptr(ObjectRefPtr::default())
        .is_none());
    assert!(world
        .commonlib_reference_ptr(ObjectRefPtr(Some(ObjectRefHandle(0x00FF_FFFF))))
        .is_none());
    let pointer = world.commonlib_reference(id).unwrap().pointer();
    assert_eq!(
        world.commonlib_reference_ptr(pointer).unwrap().form_id(),
        FormId(0x801)
    );
}

#[test]
fn rejected_set_base_keeps_the_live_reference_base_after_repeated_calls() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(misc(0x800, "Base"));
    plugin.push_top(misc(0x900, "OtherBase"));
    plugin.push_top(reference(0x801, 0x800));
    let mut order = LoadOrder::new();
    order.add("Refs.esm", plugin).unwrap();
    let mut world = World::new(order);
    let id = GlobalFormId {
        file: 0,
        object: 0x801,
    };

    for _ in 0..2 {
        let mut view = world.commonlib_reference(id).unwrap();
        assert_eq!(
            view.set_base(FormId(0x900)).unwrap_err().operation,
            "TESObjectREFR::SetBaseObject"
        );
        assert_eq!(view.base(), FormId(0x800));
    }
}
