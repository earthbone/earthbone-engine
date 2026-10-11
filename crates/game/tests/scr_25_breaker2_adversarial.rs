// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_25_assault_alarm_is_registered_on_the_actor_receiver() {
    let game = Game::new(LoadOrder::new());

    assert!(game.vm.natives.has_method("Actor", "SendAssaultAlarm"));
}
