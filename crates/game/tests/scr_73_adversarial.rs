// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_73_weapon_and_weather_inventory_is_explicit_about_unimplemented_weather_calls() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    let methods: Vec<_> = game
        .vm
        .natives
        .method_bindings()
        .into_iter()
        .filter(|(class, _)| class == "weapon" || class == "weather")
        .collect();
    let globals: Vec<_> = game
        .vm
        .natives
        .global_bindings()
        .into_iter()
        .filter(|(class, _)| class == "weapon" || class == "weather")
        .collect();
    assert_eq!(
        methods,
        [("weapon", "fire"), ("weather", "setactive")]
            .map(|(class, name)| (class.to_owned(), name.to_owned()))
    );
    assert!(
        globals.is_empty(),
        "unexpected Weapon or Weather globals: {globals:?}"
    );

    assert!(game.vm.natives.has_method("Weapon", "Fire"));
    assert!(game.vm.natives.has_method("Weather", "SetActive"));
    for (class, name, global) in [
        ("Weather", "ReleaseOverride", true),
        ("Weather", "ForceActive", false),
        ("Weather", "FindWeather", true),
        ("Weather", "GetCurrentWeather", true),
        ("Weather", "GetSkyMode", true),
    ] {
        let found = if global {
            game.vm.natives.has_global(class, name)
        } else {
            game.vm.natives.has_method(class, name)
        };
        assert!(!found, "{class}.{name} is assigned outside SCR-73");
    }
}
