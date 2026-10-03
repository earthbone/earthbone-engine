// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::extra_data::{ReferenceExtraData, ReferenceExtraKind};
use soe_commonlib_shim::refr::ObjectRefHandle;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn activate_parent_rejects_a_resolvable_non_reference_form() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"REFR"), 1).sub(Tag(*b"XAPR"), {
        let mut bytes = 2_u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&0.25_f32.to_le_bytes());
        bytes
    }));
    plugin.push_top(Record::new(Tag(*b"MISC"), 2));
    let mut order = LoadOrder::new();
    order.add("SHM10.esm", plugin).expect("fixture plugin is valid");
    let world = World::new(order);

    assert_eq!(
        world.extra(
            ObjectRefHandle(world.runtime_id(id(1))),
            ReferenceExtraKind::MissingRefIds,
        ),
        None,
        "XAPR's source layout only permits reference forms as activate parents"
    );
}
