// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{RuntimeState, SettingValue, SitSleepState};
use soe_esm::LoadOrder;
use soe_game::world::World;

#[test]
fn shm_15_settings_and_sky_read_live_world_state() {
    let mut world = World::new(LoadOrder::new());
    world
        .settings
        .insert("ftestsetting".into(), soe_papyrus::Value::Float(2.75));
    world.clock.game_days = 0.75;
    assert_eq!(
        world.setting("fTestSetting"),
        Some(soe_commonlib_shim::world::Setting {
            name: "fTestSetting".into(),
            value: SettingValue::Float(2.75),
        })
    );
    assert_eq!(world.sky().hour, 18.0);
    assert_eq!(world.setting("missing"), None);
}

#[test]
fn shm_15_unmodeled_actor_state_and_binary_hooks_are_explicit() {
    let world = World::new(LoadOrder::new());
    let sleep = world.sit_sleep_state(FormId(0x14)).unwrap_err();
    assert_eq!(sleep.name, "SIT_SLEEP_STATE");
    assert!(sleep.reason.contains("not represented"));
    let hook = world.hook("write_vfunc");
    assert_eq!(hook.name, "binary hook");
    assert!(hook.reason.contains("no target"));
    assert_eq!(SitSleepState::Sleeping as u32, 7);
}
