// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_73_weapon_and_weather_inventory_is_explicit_about_unimplemented_weather_calls() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    assert!(game.vm.natives.has_method("Weapon", "Fire"));
    assert!(game.vm.natives.has_method("Weather", "SetActive"));
    for (class, name, global) in [
        ("Weather", "ReleaseOverride", true),
        ("Weather", "ForceActive", false),
        ("Weather", "FindWeather", true),
        ("Weather", "GetCurrentWeather", true),
        ("Weather", "GetSkyMode", true),
    ] {
        let found = if global { game.vm.natives.has_global(class, name) } else { game.vm.natives.has_method(class, name) };
        assert!(!found, "{class}.{name} is assigned outside SCR-73");
    }
}
