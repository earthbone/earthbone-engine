// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Fixed installed interior corpus for cold texture preparation.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use soe_godot_host::data_source::DataSource;
use soe_godot_host::door_fade::{DoorFade, COLD_TEXTURE_PREPARATION_BUDGET};

const FIXED_INTERIOR_CORPUS: [&str; 12] = [
    "RiverwoodSleepingGiantInn",
    "WinterholdTheFrozenHearth",
    "MorthalMoorsideInn",
    "DawnstarWindpeakInn",
    "WhiterunBanneredMare",
    "WhiterunDragonsreach",
    "WindhelmCandlehearthHall",
    "SolitudeWinkingSkeever",
    "RiftenBeeandBarb",
    "MarkarthSilverBloodInn",
    "FalkreathDeadMansDrink",
    "BleakFallsBarrow01",
];

#[derive(Debug)]
struct Sample {
    name: &'static str,
    preparation: Duration,
    wall: Duration,
    textures: usize,
    references: usize,
}

#[test]
fn fixed_interior_corpus_records_cold_texture_distribution_and_fade_decisions() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping cold-texture corpus");
        return;
    };
    let source = DataSource::open_with_threads(&data, 1).expect("open installed data");
    let mut samples = Vec::with_capacity(FIXED_INTERIOR_CORPUS.len());

    for name in FIXED_INTERIOR_CORPUS {
        source.clear_prepared_textures();
        let started = Instant::now();
        let loaded = source
            .stream_cell(name)
            .unwrap_or_else(|error| panic!("stream {name}: {error}"));
        let wall = started.elapsed();
        let preparation = loaded.stats.texture_decode_time;
        let expected_fade = preparation > COLD_TEXTURE_PREPARATION_BUDGET;
        let mut fade = DoorFade::default();
        assert_eq!(
            fade.record_preparation(preparation),
            expected_fade,
            "{name} fade decision did not match the declared budget"
        );
        assert!(
            loaded.stats.textures_decoded > 0,
            "{name} did not exercise cold texture preparation"
        );
        samples.push(Sample {
            name,
            preparation,
            wall,
            textures: loaded.stats.textures_decoded,
            references: loaded.stats.references,
        });
    }

    samples.sort_unstable_by_key(|sample| sample.preparation);
    println!(
        "cold texture budget: {:.3} ms, workers: {}",
        COLD_TEXTURE_PREPARATION_BUDGET.as_secs_f64() * 1000.0,
        source.threads()
    );
    for sample in &samples {
        println!(
            "{} references={} textures={} preparation_ms={:.3} wall_ms={:.3} fade={}",
            sample.name,
            sample.references,
            sample.textures,
            sample.preparation.as_secs_f64() * 1000.0,
            sample.wall.as_secs_f64() * 1000.0,
            sample.preparation > COLD_TEXTURE_PREPARATION_BUDGET
        );
    }
    let p50 = &samples[5];
    let p90 = &samples[10];
    let maximum = &samples[11];
    println!(
        "distribution_ms p50={:.3} p90={:.3} max={:.3}",
        p50.preparation.as_secs_f64() * 1000.0,
        p90.preparation.as_secs_f64() * 1000.0,
        maximum.preparation.as_secs_f64() * 1000.0
    );
}
