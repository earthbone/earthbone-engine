// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::{MissingLinkedRefId, ReferenceExtraData};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world_with(reference: Record) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MISC"), 0x800));
    plugin.push_top(Record::new(T(b"LVLI"), 0x801));
    plugin.push_top(Record::new(T(b"KYWD"), 0x802));
    plugin.push_top(Record::new(T(b"REFR"), 0x901).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("Shim.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.reference(id(0x900)).unwrap();
    world
}

fn reference() -> Record {
    Record::new(T(b"REFR"), 0x900)
        .sub(T(b"NAME"), 0x800_u32.to_le_bytes())
}

#[test]
fn shm_09_leveled_item_base_rejects_a_resolvable_non_lvli_form() {
    let mut world = world_with(reference().sub(T(b"XLIB"), 0x800_u32.to_le_bytes()));
    let view = world.shim_reference_extras(id(0x900)).unwrap();

    assert_eq!(
        view.leveled_item_base(),
        None,
        "xEdit constrains REFR XLIB to LVLI, so a MISC form is not a leveled-item base"
    );
}

#[test]
fn shm_09_light_data_accepts_the_optional_four_byte_tail_only() {
    let short = [1.0_f32, 2.0, 3.0, 4.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut short_world = world_with(reference().sub(T(b"XLIG"), short));
    let short_view = short_world.shim_reference_extras(id(0x900)).unwrap();
    assert_eq!(short_view.light_data().unwrap().unknown, 0);

    let full = [1.0_f32, 2.0, 3.0, 4.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .chain([0xA5, 0, 0, 0])
        .collect::<Vec<_>>();
    let mut full_world = world_with(reference().sub(T(b"XLIG"), full));
    let full_view = full_world.shim_reference_extras(id(0x900)).unwrap();
    assert_eq!(full_view.light_data().unwrap().unknown, 0xA5);

    let mut malformed_world = world_with(reference().sub(T(b"XLIG"), [0; 17]));
    let malformed_view = malformed_world.shim_reference_extras(id(0x900)).unwrap();
    assert_eq!(malformed_view.light_data(), None);
}

#[test]
fn shm_09_missing_links_exclude_loaded_forms_even_when_the_reference_is_unloaded() {
    let mut world = world_with(reference());
    world
        .reference(id(0x900))
        .unwrap()
        .linked
        .extend([(Some(id(0x802)), id(0x901)), (Some(id(0x802)), id(0x999))]);
    let view = world.shim_reference_extras(id(0x900)).unwrap();

    assert_eq!(
        view.missing_linked_ref_ids(),
        &[MissingLinkedRefId {
            keyword: FormId(world.runtime_id(id(0x802))),
            linked_reference: FormId(world.runtime_id(id(0x999))),
        }],
        "a loaded form table entry is not missing merely because no RefState is loaded"
    );
}
