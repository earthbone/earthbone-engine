// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::refr::ReferenceExtraData;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn linked_children_include_authored_links_before_the_child_is_materialized() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 10));
    plugin.push_top(Record::new(Tag(*b"REFR"), 1).sub(Tag(*b"NAME"), 10_u32.to_le_bytes()));
    let mut link = 0_u32.to_le_bytes().to_vec();
    link.extend_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"REFR"), 2)
            .sub(Tag(*b"NAME"), 10_u32.to_le_bytes())
            .sub(Tag(*b"XLKR"), link),
    );
    let mut order = LoadOrder::new();
    order.add("SHM09.esm", plugin).expect("fixture plugin is valid");
    let mut world = World::new(order);
    world.reference(id(1)).expect("parent reference loads");

    let view = world.shim_reference_extras(id(1)).expect("worlds expose a live reference view");
    assert_eq!(
        view.linked_ref_children().len(),
        1,
        "XLKR on the child record links it to this parent even before its RefState is built"
    );
    assert_eq!(view.linked_ref_children()[0].reference.0, world.runtime_id(id(2)));
}
