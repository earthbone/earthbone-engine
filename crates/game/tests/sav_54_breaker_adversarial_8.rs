// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::vanilla_save::{
    id,
    references::{fixture, fixture_order_with_flags, set_form_mask},
};
use soe_esm::plugin::flags::{INITIALLY_DISABLED, PERSISTENT};
use soe_game::World;

#[test]
fn sav_54_each_named_invert_bit_changes_its_live_property_and_survives_reload() {
    for (authored, mask, expected_enabled, expected_persistent) in [
        (0, INITIALLY_DISABLED, false, false),
        (INITIALLY_DISABLED, INITIALLY_DISABLED, true, false),
        (0, PERSISTENT, true, true),
        (PERSISTENT, PERSISTENT, true, false),
    ] {
        let mut save = fixture();
        set_form_mask(&mut save.change_forms[1], mask, 0, save.form_version);

        let mut world = World::new(fixture_order_with_flags(authored));
        world.restore_vanilla_reference_placements(&save).unwrap();
        let reference = world.reference_ref(id(0, 0xA00)).unwrap();
        assert_eq!(
            reference.enabled, expected_enabled,
            "authored={authored:#x}, mask={mask:#x}"
        );
        assert_eq!(
            reference.persistent, expected_persistent,
            "authored={authored:#x}, mask={mask:#x}"
        );

        let saved = world.save_state();
        let mut loaded = World::new(fixture_order_with_flags(authored));
        loaded.load_state(&saved).unwrap();
        let restored = loaded.reference_ref(id(0, 0xA00)).unwrap();
        assert_eq!(restored.enabled, expected_enabled);
        assert_eq!(restored.persistent, expected_persistent);
    }
}
