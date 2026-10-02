// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial checks for the live CommonLib extra-data adapter.
//!
//! XAPR is a many-allowed eight-byte reference and delay structure in the
//! local xEdit and UESP layouts. CommonLib's ExtraTeleport exposes its
//! DoorTeleportData flags, while xEdit defines XTEL's final word as No Alarm.

use soe_commonlib_shim::extra_data::{
    reject_hook, ReferenceExtra, ReferenceExtraData, ReferenceExtraKind,
};
use soe_commonlib_shim::refr::ObjectRefHandle;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world_with(records: impl IntoIterator<Item = Record>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("SHM10.esm", plugin).expect("valid fixture plugin");
    World::new(order)
}

fn refr(object: u32, base: u32) -> Record {
    Record::new(Tag(*b"REFR"), object).sub(Tag(*b"NAME"), base.to_le_bytes())
}

#[test]
fn teleport_preserves_xtel_no_alarm_flag() {
    let mut xtel = 3_u32.to_le_bytes().to_vec();
    for value in [1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
        xtel.extend_from_slice(&value.to_le_bytes());
    }
    xtel.extend_from_slice(&1_u32.to_le_bytes());
    let mut world = world_with([
        Record::new(Tag(*b"DOOR"), 2),
        refr(1, 2).sub(Tag(*b"XTEL"), xtel),
        refr(3, 2),
    ]);
    world.reference(id(1)).expect("placed door should load");
    let extra = world.extra(
        ObjectRefHandle(world.runtime_id(id(1))),
        ReferenceExtraKind::Teleport,
    );
    assert!(matches!(extra, Some(ReferenceExtra::Teleport(data)) if data.flags == 1));
}

#[test]
fn missing_ref_ids_keeps_all_xapr_entries_in_record_order() {
    let mut first = 2_u32.to_le_bytes().to_vec();
    first.extend_from_slice(&0.25_f32.to_le_bytes());
    let mut second = 3_u32.to_le_bytes().to_vec();
    second.extend_from_slice(&1.5_f32.to_le_bytes());
    let world = world_with([
        refr(1, 9).sub(Tag(*b"XAPR"), first).sub(Tag(*b"XAPR"), second),
        refr(2, 9),
        refr(3, 9),
    ]);
    let result = world.extra(
        ObjectRefHandle(world.runtime_id(id(1))),
        ReferenceExtraKind::MissingRefIds,
    );
    let Some(ReferenceExtra::MissingRefIds(parents)) = result else {
        panic!("both authored activate parents should be returned");
    };
    assert_eq!(parents.len(), 2);
    assert_eq!(parents[0].reference.0, world.runtime_id(id(2)));
    assert_eq!(parents[0].activate_delay, 0.25);
    assert_eq!(parents[1].reference.0, world.runtime_id(id(3)));
    assert_eq!(parents[1].activate_delay, 1.5);
}

#[test]
fn supported_kinds_return_none_when_a_live_reference_has_no_extra() {
    let world = world_with([refr(1, 9)]);
    let handle = ObjectRefHandle(world.runtime_id(id(1)));
    for kind in [
        ReferenceExtraKind::MissingRefIds,
        ReferenceExtraKind::NorthRotation,
        ReferenceExtraKind::OutfitItem,
        ReferenceExtraKind::Poison,
        ReferenceExtraKind::Teleport,
    ] {
        assert!(world.supports_extra(kind));
        assert_eq!(world.extra(handle, kind), None, "{kind:?}");
    }
}

#[test]
fn all_binary_hook_kinds_are_rejected_repeatedly() {
    for name in ["write_call", "write_branch", "write_vfunc"] {
        assert_eq!(reject_hook(name), Err(soe_commonlib_shim::extra_data::UnsupportedHook { name }));
        assert_eq!(reject_hook(name), Err(soe_commonlib_shim::extra_data::UnsupportedHook { name }));
    }
}
