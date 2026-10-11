// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::WeatherRequest, Game};

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, tint, record_flags) in [(1, 40_u8, 0), (2, 220_u8, 0), (3, 100_u8, flags::DELETED)]
    {
        let colors = (0..17)
            .flat_map(|_| {
                [tint / 2, tint, tint / 3, tint / 10]
                    .map(|v| [v, v, v, 0])
                    .concat()
            })
            .collect::<Vec<_>>();
        let mut data = vec![0; 19];
        data[3] = 255;
        plugin.push_top(
            Record::new(Tag::new(b"WTHR"), object)
                .with_flags(record_flags)
                .sub(b"NAM0", colors)
                .sub(b"DATA", data)
                .sub(
                    b"FNAM",
                    [0.0_f32, 1.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0]
                        .iter()
                        .flat_map(|value| value.to_le_bytes())
                        .collect::<Vec<_>>(),
                ),
        );
    }
    plugin.push_top(
        Record::new(Tag::new(b"CLMT"), 0x20)
            .sub(b"TNAM", [36, 48, 108, 120, 50, 0xc3])
            .sub(
                b"WLST",
                [1_u32, 100, 0]
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(Record::new(Tag::new(b"WRLD"), 0x21).sub(b"CNAM", 0x20_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag::new(b"REGN"), 0x23).sub(b"WNAM", 0x21_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag::new(b"CELL"), 0x22)
            .sub(b"EDID", b"Exterior\0".to_vec())
            .sub(b"DATA", [0_u8])
            .sub(b"XCCM", 0x23_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Scr54Breaker4.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn scr_54_deleted_weather_is_rejected_without_replacing_the_active_request() {
    let game = game();
    game.world_mut()
        .set_active_weather(id(1), true, false)
        .unwrap();
    let before = game.world().weather_request();
    assert!(game
        .world_mut()
        .set_active_weather(id(3), false, true)
        .is_err());
    assert_eq!(game.world().weather_request(), before);
}

#[test]
fn scr_54_repeated_weather_calls_update_the_sky_target_and_flags() {
    let game = game();
    game.world_mut().sky_frame(id(0x22)).unwrap();
    game.world_mut()
        .set_active_weather(id(2), true, false)
        .unwrap();
    let first = game.world_mut().sky_frame(id(0x22)).unwrap().unwrap();
    assert_eq!(first.weather, id(2));
    assert_eq!(first.transition, 0.0);

    game.world_mut()
        .set_active_weather(id(1), true, true)
        .unwrap();
    let second = game.world_mut().sky_frame(id(0x22)).unwrap().unwrap();
    assert_eq!(second.weather, id(1));
    assert_eq!(second.transition, 0.0);
    game.world_mut().advance_clock(0.01);
    let accelerated = game.world_mut().sky_frame(id(0x22)).unwrap().unwrap();
    assert_eq!(accelerated.transition, 1.0);
    assert!(accelerated.colors[0][0] < first.colors[0][0]);
}

#[test]
fn scr_54_release_override_keeps_non_override_selection_but_clears_override() {
    let game = game();
    game.world_mut()
        .set_active_weather(id(1), false, false)
        .unwrap();
    game.world_mut().release_weather_override();
    assert_eq!(
        game.world().weather_request(),
        Some(WeatherRequest {
            weather: id(1),
            override_weather: false,
            accelerate: false,
        })
    );
    game.world_mut()
        .set_active_weather(id(2), true, false)
        .unwrap();
    game.world_mut().release_weather_override();
    assert_eq!(game.world().weather_request(), None);
}
