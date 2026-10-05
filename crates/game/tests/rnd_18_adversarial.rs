// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn weather(object: u32, data: bool) -> Record {
    let mut colors = Vec::new();
    for _ in 0..17 {
        for value in [20_u8, 40, 60, 10] {
            colors.extend_from_slice(&[value, value, value, 0]);
        }
    }
    let mut record = Record::new(Tag(*b"WTHR"), object).sub(b"NAM0", colors).sub(
        b"FNAM",
        floats(&[10.0, 1000.0, 20.0, 2000.0, 1.0, 1.0, 1.0, 1.0]),
    );
    if data {
        let mut body = vec![0; 19];
        body[3] = 255;
        record = record.sub(b"DATA", body);
    }
    record
}

fn fixture(bad_weather: bool) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(weather(10, !bad_weather));
    plugin.push_top(weather(11, true));
    plugin.push_top(
        Record::new(Tag(*b"CLMT"), 20)
            .sub(b"TNAM", [36, 48, 108, 120, 50, 0xc3])
            .sub(
                b"WLST",
                [10_u32, 50, 0, 11, 50, 0]
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"WRLD"), 22).sub(b"CNAM", 20_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REGN"), 21).sub(b"WNAM", 22_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 30)
            .sub(b"DATA", [0_u8])
            .sub(b"XCCM", 21_u32.to_le_bytes())
            .sub(b"XCLC", [0_u8; 8]),
    );
    let mut order = LoadOrder::new();
    order.add("Sky.esm", plugin).expect("fixture plugin");
    World::new(order)
}

#[test]
fn rnd_18_rejects_unresolved_and_wrong_type_weather_without_mutating_request() {
    let mut world = fixture(false);
    assert!(world.set_active_weather(id(99), true, false).is_err());
    assert!(world.weather_request().is_none());
    assert!(world.set_active_weather(id(20), true, false).is_err());
    assert!(world.weather_request().is_none());
}

#[test]
fn rnd_18_malformed_selected_weather_does_not_commit_a_partial_sky_transition() {
    let mut malformed = fixture(true);
    malformed
        .set_active_weather(id(10), true, false)
        .expect("selected form is a weather record");
    let before = malformed.save_state();
    assert!(malformed.sky_frame(id(30)).is_err());
    assert_eq!(malformed.save_state(), before);
}

#[test]
fn rnd_18_transition_resumes_from_saved_fraction_in_a_fresh_world() {
    let mut original = fixture(false);
    original.clock.game_days = 0.5;
    let current = original
        .sky_frame(id(30))
        .expect("initial frame")
        .unwrap()
        .weather;
    let destination = if current == id(10) { id(11) } else { id(10) };
    original
        .set_active_weather(destination, true, false)
        .expect("weather request");
    original.sky_frame(id(30)).expect("start transition");
    original.advance_clock(22.5);
    let expected = original.sky_frame(id(30)).expect("midpoint frame").unwrap();
    assert!((expected.transition - 0.5).abs() < 0.001);

    let saved = original.save_state();
    let mut restored = fixture(false);
    restored.load_state(&saved).expect("restore sky section");
    assert_eq!(
        restored.sky_frame(id(30)).expect("resumed frame"),
        Some(expected)
    );
}

#[test]
fn rnd_18_repeated_frame_reads_do_not_restart_an_unchanged_transition() {
    let mut world = fixture(false);
    world.clock.game_days = 0.5;
    let current = world
        .sky_frame(id(30))
        .expect("initial frame")
        .unwrap()
        .weather;
    let destination = if current == id(10) { id(11) } else { id(10) };
    world
        .set_active_weather(destination, true, false)
        .expect("weather request");
    world.sky_frame(id(30)).expect("start transition");
    world.advance_clock(22.5);
    let first = world.sky_frame(id(30)).expect("first midpoint").unwrap();
    for _ in 0..4 {
        assert_eq!(
            world.sky_frame(id(30)).expect("repeat frame"),
            Some(first.clone())
        );
    }
}
