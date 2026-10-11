// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::vanilla_save::references::{fixture, fixture_order_with_flags, set_form_mask};
use soe_esm::GlobalFormId;
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn sav_54_preserves_both_serialized_mask_words_on_later_supported_versions() {
    // xEdit defines a u32 invert mask followed by a u16 word. FallrimTools
    // reads and writes both values verbatim.
    for (save_version, form_version) in [(12, 77), (13, 78), (14, 79)] {
        let mut save = fixture();
        save.header.version = save_version;
        save.form_version = form_version;
        let mask = u32::MAX;
        let unknown = 0xA55A;
        set_form_mask(&mut save.change_forms[1], mask, unknown, form_version);

        let mut world = World::new(fixture_order_with_flags(0));
        let report = world
            .restore_vanilla_reference_placements(&save)
            .expect("supported change-form versions restore");
        assert_eq!(
            report.remaining[&id(0xA00)].form_flags,
            Some((mask, unknown))
        );
    }
}

#[test]
fn sav_54_mask_cannot_clear_the_deleted_record_flag() {
    let deleted = soe_esm::plugin::flags::DELETED;
    let invert = soe_esm::plugin::flags::INITIALLY_DISABLED;
    let mut save = fixture();
    set_form_mask(&mut save.change_forms[1], invert, 0xFFFF, 78);

    let mut world = World::new(fixture_order_with_flags(deleted));
    world
        .restore_vanilla_reference_placements(&save)
        .expect("deleted reference still restores its saved identity");
    assert!(!world.reference_ref(id(0xA00)).unwrap().enabled);
}
