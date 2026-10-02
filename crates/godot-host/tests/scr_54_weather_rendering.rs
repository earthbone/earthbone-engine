// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::image_space::{ImageSpaceStack, RenderedBase};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let floats = |values: &[f32]| {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>()
    };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"IMGS"), 10).sub(Tag::new(b"CNAM"), floats(&[1.0, 1.5, 2.0])),
    );
    plugin.push_top(
        Record::new(Tag::new(b"IMGS"), 11).sub(Tag::new(b"CNAM"), floats(&[0.4, 0.7, 0.9])),
    );
    plugin.push_top(
        Record::new(Tag::new(b"WTHR"), 20).sub(
            Tag::new(b"IMSP"),
            [10_u32, 11, 10, 11]
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
    );
    let mut order = LoadOrder::new();
    order.add("Weather.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn scr_54_weather_request_selects_authored_values_for_host_rendering() {
    let game = game();
    game.world_mut()
        .set_active_weather(id(20), false, true)
        .unwrap();

    let mut host = ImageSpaceStack::default();
    host.sync(&game.world()).unwrap();
    let rendered = RenderedBase::from(host.base().unwrap());
    assert_eq!(
        (rendered.saturation, rendered.brightness, rendered.contrast),
        (0.4, 0.7, 0.9)
    );
}

#[test]
fn scr_54_explicit_image_space_selection_overrides_native_weather_request() {
    let game = game();
    game.world_mut()
        .set_active_weather(id(20), false, false)
        .unwrap();
    game.world_mut()
        .select_weather_image_space(Some(soe_game::world::image_space::WeatherSelection {
            weather: id(20),
            from: 0,
            to: 0,
            blend: 0.0,
        }))
        .unwrap();

    let mut host = ImageSpaceStack::default();
    host.sync(&game.world()).unwrap();
    let rendered = RenderedBase::from(host.base().unwrap());
    assert_eq!(
        (rendered.saturation, rendered.brightness, rendered.contrast),
        (1.0, 1.5, 2.0)
    );
}
