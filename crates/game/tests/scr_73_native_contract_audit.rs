// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Registry audit for the Weapon and Weather Papyrus classes. The local
//! Creation Kit corpus has no class pages for them, so signatures are compared
//! with the vanilla PSC declarations and the absence of CK pages is recorded
//! in `docs/reference-audit.md`.

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_73_registered_weapon_weather_methods_match_vanilla_contract_inventory() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();

    let bound = [("Weapon", "Fire"), ("Weather", "SetActive")];
    for (class, method) in bound {
        assert!(
            game.vm.natives.has_method(class, method),
            "{class}.{method} must be present in the native registry"
        );
    }
    assert!(!game.vm.natives.has_global("Weapon", "Fire"));
    assert!(!game.vm.natives.has_global("Weather", "SetActive"));
}

#[test]
fn scr_73_weather_vanilla_natives_missing_from_registry_are_assigned() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();

    let missing = [
        ("Weather", "ForceActive", false),
        ("Weather", "FindWeather", true),
        ("Weather", "GetClassification", false),
        ("Weather", "GetCurrentWeather", true),
        ("Weather", "GetOutgoingWeather", true),
        ("Weather", "GetCurrentWeatherTransition", true),
        ("Weather", "GetSkyMode", true),
    ];
    for (class, name, global) in missing {
        let registered = if global {
            game.vm.natives.has_global(class, name)
        } else {
            game.vm.natives.has_method(class, name)
        };
        assert!(!registered, "{class}.{name} is assigned to SCR-54");
    }
}
