// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::extra_data::{ReferenceExtra, ReferenceExtraData, ReferenceExtraKind};
use soe_commonlib_shim::refr::ObjectRefHandle;
use soe_esm::plugin::GroupKind;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn exterior_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"WRLD"), 1).zsub(Tag(*b"EDID"), "TestWorld"));
    let mut cell = Record::new(Tag(*b"CELL"), 2)
        .sub(Tag(*b"DATA"), [0_u8, 0])
        .sub(
            Tag(*b"XCLC"),
            [0_i32.to_le_bytes(), 0_i32.to_le_bytes()].concat(),
        );
    cell.placement.world = Some(1);
    cell.placement.group = Some(GroupKind::WorldChildren(1));
    plugin.push_top(cell);
    let mut order = LoadOrder::new();
    order
        .add("SHM10.esm", plugin)
        .expect("fixture plugin loads");
    World::new(order)
}

#[test]
fn shm_10_exterior_north_rotation_reads_owning_wrld_runtime_view() {
    let world = exterior_world();
    let result = world.extra(
        ObjectRefHandle(world.runtime_id(id(2))),
        ReferenceExtraKind::NorthRotation,
    );
    assert_eq!(result, Some(ReferenceExtra::NorthRotation(0.0)));
}

#[test]
fn shm_10_installed_exterior_cells_read_their_wrld_north_rotation() {
    let Some(order) = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA not set, skipping installed WRLD north-rotation survey");
        return;
    };
    let world = World::from_shared_order(order);
    let mut checked = 0;
    for (cell, record) in world.order().winners_of(Tag(*b"CELL")) {
        if record
            .find(Tag(*b"DATA"))
            .and_then(|flags| flags.get(..2))
            .is_none_or(|flags| u16::from_le_bytes([flags[0], flags[1]]) & 1 != 0)
        {
            continue;
        }
        let result = world.extra(
            ObjectRefHandle(world.runtime_id(cell)),
            ReferenceExtraKind::NorthRotation,
        );
        if let Some(ReferenceExtra::NorthRotation(angle)) = result {
            assert!(angle.is_finite(), "WRLD north rotation must be finite");
            assert_eq!(
                angle, 0.0,
                "plugin WRLD records do not serialize north rotation"
            );
            checked += 1;
            if checked == 16 {
                break;
            }
        }
    }
    assert!(
        checked > 0,
        "installed masters should contain exterior cells"
    );
}
