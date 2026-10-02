// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-11 registry coverage for documented state queries and setters.

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_11_state_and_vehicle_bindings_are_present_as_actor_methods() {
    let game = Game::new(LoadOrder::new());
    for name in [
        "GetSitState",
        "GetFlyingState",
        "SetEssential",
        "SetVehicle",
        "SetLookAt",
    ] {
        assert!(game.vm.natives.has_method("Actor", name), "Actor.{name}");
    }
}
