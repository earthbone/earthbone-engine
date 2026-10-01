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

    let expected_globals = [
        ("utility", "captureframerate"),
        ("utility", "endframeratecapture"),
        ("utility", "gametimetostring"),
        ("utility", "getaverageframerate"),
        ("utility", "getbudgetcount"),
        ("utility", "getbudgetname"),
        ("utility", "getcurrentbudget"),
        ("utility", "getcurrentgametime"),
        ("utility", "getcurrentmemory"),
        ("utility", "getcurrentrealtime"),
        ("utility", "getinibool"),
        ("utility", "getinifloat"),
        ("utility", "getiniint"),
        ("utility", "getinistring"),
        ("utility", "getmaxframerate"),
        ("utility", "getminframerate"),
        ("utility", "isinmenumode"),
        ("utility", "overbudget"),
        ("utility", "randomfloat"),
        ("utility", "randomint"),
        ("utility", "setinibool"),
        ("utility", "setinifloat"),
        ("utility", "setiniint"),
        ("utility", "setinistring"),
        ("utility", "startframeratecapture"),
    ];
    let expected_methods = [("visualeffect", "play"), ("visualeffect", "stop")];
    let globals: Vec<_> = game
        .vm
        .natives
        .global_bindings()
        .into_iter()
        .filter(|(class, _)| class == "utility" || class == "visualeffect")
        .collect();
    let methods: Vec<_> = game
        .vm
        .natives
        .method_bindings()
        .into_iter()
        .filter(|(class, _)| class == "utility" || class == "visualeffect")
        .collect();
    assert_eq!(
        globals,
        expected_globals.map(|(class, name)| (class.to_owned(), name.to_owned()))
    );
    assert_eq!(
        methods,
        expected_methods.map(|(class, name)| (class.to_owned(), name.to_owned()))
    );
}
