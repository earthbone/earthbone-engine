// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::{DataHandler, FormId};
use soe_esm::LoadOrder;
use soe_game::world::World;

#[test]
fn empty_load_order_has_no_file_or_form_and_out_of_range_lookup_is_safe() {
    let world = World::new(LoadOrder::new());
    let data = world.commonlib_data_handler();
    assert!(data.files().is_empty());
    assert_eq!(data.file(0), None);
    assert!(data.lookup(FormId(u32::MAX)).is_none());
    assert!(data.lookup_by_editor_id("missing").is_none());
}
