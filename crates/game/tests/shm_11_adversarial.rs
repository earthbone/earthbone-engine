// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::world_types::{SettingValue, unsupported_hook};
use soe_esm::{GlobalFormId, LoadOrder};
use soe_game::world::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId {
        file: soe_esm::CREATED_FILE,
        object,
    }
}

#[test]
fn setting_lookup_is_case_insensitive_and_preserves_value_kind() {
    let mut world = World::new(LoadOrder::new());
    world
        .settings
        .insert("bfixture".into(), soe_papyrus::Value::Bool(true));
    assert_eq!(
        world.commonlib_game_setting("BFixture"),
        Some(SettingValue::Bool(true))
    );
    assert_eq!(world.commonlib_game_setting("missing"), None);
}

#[test]
fn high_process_missing_actor_is_absent_and_unknown_hook_is_named() {
    let world = World::new(LoadOrder::new());
    assert_eq!(world.commonlib_high_process(id(1)), None);
    let hook = unsupported_hook("HighProcessData", "headtrack");
    assert_eq!(hook.type_name, "HighProcessData");
    assert_eq!(hook.hook_name, "headtrack");
    assert!(!hook.reason.is_empty());
}
