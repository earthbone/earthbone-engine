// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::world_types::WornState;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::{Extra, Stack};
use soe_game::world::World;

fn loaded_id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn placed_hazard_reads_the_phzd_record_form() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"HAZD"), 0x200));
    plugin.push_top(
        Record::new(Tag(*b"PHZD"), 0x300)
            .sub(Tag(*b"NAME"), 0x200_u32.to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                [4.0_f32, 5.0, 6.0, 0.0, 0.0, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Hazard.esm", plugin).unwrap();
    let mut world = World::new(order);

    let hazard = world.commonlib_hazard(loaded_id(0x300));
    assert!(hazard.is_some(), "PHZD is the source-defined placed hazard form");
    let hazard = hazard.unwrap();
    assert_eq!(
        hazard.reference,
        soe_commonlib_shim::form::FormId(world.order().runtime_id(loaded_id(0x300)))
    );
    assert_eq!(
        hazard.base,
        soe_commonlib_shim::form::FormId(world.order().runtime_id(loaded_id(0x200)))
    );
    assert_eq!(
        [hazard.position.x, hazard.position.y, hazard.position.z],
        [4.0, 5.0, 6.0]
    );
}

#[test]
fn extra_worn_reports_both_markers_across_distinct_stacks() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x100));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 0x101).sub(Tag(*b"NAME"), 0x100_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Inventory.esm", plugin).unwrap();
    let mut world = World::new(order);
    let actor = loaded_id(0x101);
    let item = loaded_id(2);
    world
        .reference(actor)
        .unwrap()
        .inventory
        .restore(
            item,
            2,
            vec![
                Stack::new(
                    1,
                    vec![Extra {
                        kind: 22,
                        fields: Vec::new(),
                    }],
                )
                .unwrap(),
                Stack::new(
                    1,
                    vec![Extra {
                        kind: 23,
                        fields: Vec::new(),
                    }],
                )
                .unwrap(),
            ],
        )
        .unwrap();

    assert_eq!(
        world.commonlib_extra_worn(actor, item),
        Some(WornState {
            worn: true,
            worn_left: true,
        })
    );
    assert_eq!(world.commonlib_extra_worn(actor, loaded_id(3)), None);
}
