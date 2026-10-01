// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::WeatherRequest, Game};

#[test]
fn repeated_weather_requests_retain_only_the_latest_valid_choice() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"WTHR"), 1));
    plugin.push_top(Record::new(Tag::new(b"WTHR"), 2));
    let mut order = LoadOrder::new();
    order
        .add("scr54-breaker.esp", plugin)
        .expect("plugin loads");
    let game = Game::new(order);
    game.world_mut()
        .set_active_weather(GlobalFormId { file: 0, object: 1 }, true, false)
        .expect("weather A valid");
    game.world_mut()
        .set_active_weather(GlobalFormId { file: 0, object: 2 }, false, true)
        .expect("weather B valid");
    assert_eq!(
        game.world().weather_request(),
        Some(WeatherRequest {
            weather: GlobalFormId { file: 0, object: 2 },
            override_weather: false,
            accelerate: true,
        })
    );
}
