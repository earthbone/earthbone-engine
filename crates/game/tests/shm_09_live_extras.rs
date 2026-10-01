// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::{
    LinkedRefChild, MissingLinkedRefId, ObjectRefHandle, ReferenceExtraData,
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MISC"), 0x800));
    plugin.push_top(Record::new(T(b"LVLI"), 0x801));
    plugin.push_top(Record::new(T(b"KYWD"), 0x802));
    let light = [10.0_f32, 2.0, 300.0, 0.25]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .chain([7, 0, 0, 0])
        .collect::<Vec<_>>();
    plugin.push_top(
        Record::new(T(b"REFR"), 0x900)
            .sub(T(b"NAME"), 0x800_u32.to_le_bytes())
            .sub(T(b"XLIB"), 0x801_u32.to_le_bytes())
            .sub(T(b"XLIG"), light)
            .sub(T(b"XMRK"), [])
            .sub(T(b"FNAM"), [4])
            .sub(T(b"TNAM"), [39, 0])
            .zsub(T(b"FULL"), "Marker"),
    );
    plugin.push_top(Record::new(T(b"REFR"), 0x901).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Shim.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.reference(id(0x900)).unwrap();
    world.reference(id(0x901)).unwrap();
    world
}

#[test]
fn shm_09_reference_values_read_records_and_current_world_state() {
    let mut world = world();
    world.reference(id(0x901)).unwrap().linked = vec![(Some(id(0x802)), id(0x900))];
    let state = world.reference(id(0x900)).unwrap();
    state.linked = vec![(None, id(0x999)), (None, id(0x901))];
    state.on_map = true;
    state.fast_travel = true;
    state.display_name = Some("Renamed".into());
    let view = world.shim_reference_extras(id(0x900)).unwrap();
    assert_eq!(
        view.leveled_item_base().unwrap().leveled_item,
        FormId(0x801)
    );
    let light = view.light_data().unwrap();
    assert_eq!(
        (
            light.fov,
            light.fade,
            light.end_distance_cap,
            light.shadow_depth_bias,
            light.unknown
        ),
        (10.0, 2.0, 300.0, 0.25, 7)
    );
    let marker = view.map_marker().unwrap();
    assert_eq!(
        (marker.name.as_str(), marker.flags, marker.marker_type),
        ("Renamed", 7, 39)
    );
    assert_eq!(
        view.linked_ref_children(),
        &[LinkedRefChild {
            keyword: FormId(0x802),
            reference: ObjectRefHandle(0x901),
        }]
    );
    assert_eq!(
        view.missing_linked_ref_ids(),
        &[MissingLinkedRefId {
            keyword: FormId(0),
            linked_reference: FormId(0x999),
        }]
    );

    world.reference(id(0x901)).unwrap().linked.clear();
    let state = world.reference(id(0x900)).unwrap();
    state.linked.clear();
    state.display_name = None;
    let view = world.shim_reference_extras(id(0x900)).unwrap();
    assert!(view.linked_ref_children().is_empty());
    assert!(view.missing_linked_ref_ids().is_empty());
    assert_eq!(view.map_marker().unwrap().name, "Marker");
}

#[test]
fn shm_09_missing_reference_and_absent_extras_have_no_values() {
    let world = world();
    assert!(world.shim_reference_extras(id(0x999)).is_none());
    let view = world.shim_reference_extras(id(0x901)).unwrap();
    assert_eq!(view.leveled_item_base(), None);
    assert_eq!(view.light_data(), None);
    assert_eq!(view.map_marker(), None);
    assert!(view.linked_ref_children().is_empty());
    assert!(view.missing_linked_ref_ids().is_empty());
}
