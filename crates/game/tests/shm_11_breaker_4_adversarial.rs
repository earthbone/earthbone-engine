// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"HAZD"), 0x200));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x201));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x202));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x300).sub(Tag(*b"NAME"), 0x202_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), 0x301)
            .sub(Tag(*b"NAME"), 0x201_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(Tag(*b"PHZD"), 0x302)
            .sub(Tag(*b"NAME"), 0x200_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 12]),
    );
    let mut order = LoadOrder::new();
    order.add("shm_11_adversarial.esp", plugin).unwrap();
    World::new(order)
}

#[test]
fn hazard_view_rejects_non_hazard_bases_and_truncated_phzd_data() {
    let mut world = world();
    assert!(world.commonlib_hazard(id(0x301)).is_none());
    assert!(world.commonlib_hazard(id(0x302)).is_none());
    assert!(world.commonlib_hazard(id(0x999)).is_none());
}

#[test]
fn hazard_view_reads_authored_phzd_transform_without_building_a_reference() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"HAZD"), 0x200));
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x100));
    let mut placed = Record::new(Tag(*b"PHZD"), 0x302)
        .sub(Tag(*b"NAME"), 0x200_u32.to_le_bytes())
        .sub(
            Tag(*b"DATA"),
            [11.0_f32, -12.0, 13.5, 0.0, 0.0, 0.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    placed.placement.cell = Some(0x100);
    plugin.push_top(placed);
    let mut order = LoadOrder::new();
    order.add("shm_11_phzd.esp", plugin).unwrap();
    let mut world = World::new(order);

    let hazard = world.commonlib_hazard(id(0x302)).unwrap();
    assert_eq!(hazard.reference.0, 0x302);
    assert_eq!(hazard.base.0, 0x200);
    assert_eq!(
        [hazard.position.x, hazard.position.y, hazard.position.z],
        [11.0, -12.0, 13.5]
    );
    assert!(hazard.enabled);
}

#[test]
fn missing_actor_or_item_has_no_worn_state() {
    let world = world();
    assert!(world.commonlib_extra_worn(id(0x300), id(0x999)).is_none());
    assert!(world.commonlib_extra_worn(id(0x999), id(0x201)).is_none());
}
