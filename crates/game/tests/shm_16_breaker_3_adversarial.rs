// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{request_hook, HookError, HookKind};
use soe_commonlib_shim::form::{DataHandler, Form, FormId, FormType};
use soe_commonlib_shim::refr::{Actor, ObjectRef};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;

fn fixture_order() -> LoadOrder {
    let mut regular = Plugin::new(FileHeader::default());
    regular.push_top(Record::new(Tag(*b"MISC"), 0x800).zsub(Tag(*b"EDID"), "Regular"));
    regular.push_top(Record::new(Tag(*b"NPC_"), 0x803).zsub(Tag(*b"EDID"), "ActorBase"));
    regular.push_top(
        Record::new(Tag(*b"ACHR"), 0x801)
            .sub(Tag(*b"NAME"), 0x803u32.to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                [1.0_f32, 2.0, 3.0, 0.25, 0.5, 0.75]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    regular.push_top(Record::new(Tag(*b"CELL"), 0x802));
    let mut light_header = FileHeader::default();
    light_header.flags |= soe_esm::plugin::flags::LIGHT_MASTER;
    let mut light = Plugin::new(light_header);
    light.push_top(Record::new(Tag(*b"MISC"), 0x10).zsub(Tag(*b"EDID"), "Light"));
    let mut order = LoadOrder::new();
    order.add("Base.esm", regular).unwrap();
    order.add("Light.esl", light).unwrap();
    order
}

#[test]
fn light_forms_and_tesfile_indices_round_trip_through_live_data_handler() {
    let world = World::new(fixture_order());
    let data = world.commonlib_data_handler();

    let light = data.lookup(FormId(0xFE00_0010)).unwrap();
    assert_eq!(light.form_type(), FormType(*b"MISC"));
    assert_eq!(light.editor_id(), Some("Light"));
    assert_eq!(data.forms_of_type(FormType(*b"MISC")), [FormId(0x800), FormId(0xFE00_0010)]);
    assert_eq!(data.file(0).unwrap().regular_index, Some(0));
    assert_eq!(data.file(1).unwrap().light_index, Some(0));
    assert_eq!(data.file(2), None);
}

#[test]
fn reference_pointer_reads_current_transform_flags_and_cell() {
    let mut world = World::new(fixture_order());
    let reference_id = GlobalFormId { file: 0, object: 0x801 };
    let cell_id = GlobalFormId { file: 0, object: 0x802 };
    let state = world.reference(reference_id).unwrap();
    state.position = [21.0, -3.0, 7.5];
    state.rotation = [0.25, 0.5, 0.75];
    state.enabled = false;
    state.deleted = true;
    state.cell = Some(cell_id);

    let reference = world.commonlib_reference(reference_id).unwrap();
    assert_eq!(reference.position().x, 21.0);
    assert_eq!(reference.position().y, -3.0);
    assert_eq!(reference.position().z, 7.5);
    assert_eq!(reference.rotation().z, 0.75);
    assert_eq!(reference.parent_cell(), Some(FormId(0x802)));
    assert!(reference.is_disabled());
    assert!(reference.is_deleted());
    let pointer = reference.pointer();
    drop(reference);
    assert_eq!(world.commonlib_reference_ptr(pointer).unwrap().form_id(), FormId(0x801));
}

#[test]
fn weapon_state_and_actor_snapshot_follow_repeated_live_changes() {
    let mut world = World::new(fixture_order());
    let actor_id = GlobalFormId { file: 0, object: 0x801 };
    for (drawn, expected) in [(false, 0), (true, 3), (false, 0), (true, 3)] {
        world.actor(actor_id).unwrap().weapon_drawn = drawn;
        let actor = world.commonlib_actor(actor_id).unwrap();
        assert_eq!(actor.weapon_state() as u32, expected);
        assert_eq!(actor.state().weapon_drawn, drawn);
    }
}

#[test]
fn every_binary_hook_kind_is_rejected_with_its_typed_reason() {
    for kind in [HookKind::CallSite, HookKind::Branch, HookKind::Vtable] {
        assert_eq!(request_hook(kind), Err(HookError::Unsupported(kind)));
    }
}
