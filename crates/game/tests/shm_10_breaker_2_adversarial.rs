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

fn world_with_cell_data(data: &[u8]) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"WRLD"), 1));
    let mut cell = Record::new(Tag(*b"CELL"), 2).sub(Tag(*b"DATA"), data).sub(
        Tag(*b"XCLC"),
        [0_i32.to_le_bytes(), 0_i32.to_le_bytes()].concat(),
    );
    cell.placement.world = Some(1);
    cell.placement.group = Some(GroupKind::WorldChildren(1));
    plugin.push_top(cell);
    let mut order = LoadOrder::new();
    order.add("SHM10.esm", plugin).expect("fixture loads");
    World::new(order)
}

fn north_rotation(world: &World) -> Option<ReferenceExtra> {
    world.extra(
        ObjectRefHandle(world.runtime_id(id(2))),
        ReferenceExtraKind::NorthRotation,
    )
}

#[test]
fn one_byte_exterior_cell_data_still_resolves_the_worldspace() {
    let exterior = world_with_cell_data(&[0]);
    assert_eq!(
        north_rotation(&exterior),
        Some(ReferenceExtra::NorthRotation(0.0)),
        "UESP permits one-byte CELL DATA, and exterior north rotation comes from WRLD"
    );
}

#[test]
fn one_byte_interior_cell_data_still_uses_the_zero_default() {
    let interior = world_with_cell_data(&[1]);
    assert_eq!(
        north_rotation(&interior),
        Some(ReferenceExtra::NorthRotation(0.0)),
        "an interior cell without ExtraNorthRotation defaults to zero"
    );
}

#[test]
fn cell_data_wider_than_its_u16_field_is_rejected() {
    let world = world_with_cell_data(&[0, 0, 0]);
    assert_eq!(
        north_rotation(&world),
        None,
        "CELL DATA is a uint16 field, so trailing bytes are malformed"
    );
}
