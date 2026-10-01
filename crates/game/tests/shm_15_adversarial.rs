// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{RuntimeState, SitSleepState};
use soe_esm::LoadOrder;
use soe_game::world::World;

#[test]
fn missing_setting_effect_and_actor_state_are_distinguished() {
    let world = World::new(LoadOrder::new());
    assert_eq!(world.setting("absent"), None);
    assert_eq!(world.reanimate_effect(u64::MAX), None);
    assert_eq!(world.sit_sleep_state(FormId(0)).unwrap_err().name, "SIT_SLEEP_STATE");
    assert_eq!(SitSleepState::Normal as u32, 0);
    assert_eq!(SitSleepState::WantToWake as u32, 8);
}
