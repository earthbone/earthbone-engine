// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{clock::GameClock, Game};

#[test]
fn scr_72_utility_calendar_and_registry_contracts_cover_boundaries() {
    assert_eq!(GameClock::format_game_time(0.0), "08/17/201 00:00");
    assert_eq!(GameClock::format_game_time(0.5), "08/17/201 12:00");
    assert_eq!(GameClock::format_game_time(1.0), "08/18/201 00:00");
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    assert!(game.vm.natives.has_global("Utility", "RandomFloat"));
    assert!(game.vm.natives.has_global("Utility", "GameTimeToString"));
    assert!(game.vm.natives.has_method("VisualEffect", "Play"));
}
