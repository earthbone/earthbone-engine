// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::time::{Duration, Instant};

use soe_godot_host::data_source::DataSource;

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn process_thread_count() -> std::io::Result<usize> {
    Ok(std::fs::read_dir("/proc/self/task")?.filter_map(Result::ok).count())
}

#[test]
fn prf_05_single_and_persistent_workers_stream_same_cell_and_report_timings(
) -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("SKYRIM_DATA").unwrap_or_else(|_| {
        "/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data"
            .to_string()
    });
    if !Path::new(&data).is_dir() {
        eprintln!("installed Skyrim Data is absent, skipping worker comparison");
        return Ok(());
    }

    let open_start = Instant::now();
    let mut source = DataSource::open_with_threads(Path::new(&data), 24)?;
    let pooled_open = open_start.elapsed();
    assert!(matches!(
        source.stream_cell("PRF05MissingCell"),
        Err(soe_godot_host::data_source::DataSourceError::CellNotFound(_))
    ));
    let cell = source
        .resolve_cell("WhiterunBanneredMare")
        .ok_or_else(|| std::io::Error::other("Bannered Mare cell is absent"))?;

    source.set_threads(0);
    assert_eq!(source.threads(), 1, "zero workers selects the sequential path");
    let sequential = source.stream_cell_id(cell)?;
    let signature = (
        sequential.stats.references,
        sequential.stats.statics_with_model,
        sequential.stats.meshes_parsed,
        sequential.stats.meshes_failed,
    );
    assert!(signature.0 > 0, "the cell must contain references");
    assert_eq!(signature.3, 0, "the surveyed cell must parse without failures");

    let mut sequential_samples = Vec::with_capacity(5);
    for _ in 0..5 {
        let start = Instant::now();
        let loaded = source.stream_cell_id(cell)?;
        sequential_samples.push(start.elapsed());
        assert_eq!(loaded.stats.references, signature.0);
        assert_eq!(loaded.stats.meshes_parsed, signature.2);
        assert_eq!(loaded.stats.meshes_failed, 0);
    }

    source.set_threads(24);
    let pool_start = Instant::now();
    let pooled_first = source.stream_cell_id(cell)?;
    let pool_startup_and_first_stream = pool_start.elapsed();
    let threads_with_pool = process_thread_count().ok();
    assert_eq!(
        (
            pooled_first.stats.references,
            pooled_first.stats.statics_with_model,
            pooled_first.stats.meshes_parsed,
            pooled_first.stats.meshes_failed,
        ),
        signature,
        "changing worker count must preserve the stream result"
    );

    let mut pooled = Vec::with_capacity(5);
    for _ in 0..5 {
        let start = Instant::now();
        let loaded = source.stream_cell_id(cell)?;
        pooled.push(start.elapsed());
        assert_eq!(loaded.stats.references, signature.0);
        assert_eq!(loaded.stats.meshes_parsed, signature.2);
        assert_eq!(loaded.stats.meshes_failed, 0);
    }

    source.set_threads(1);
    let sequential_again = source.stream_cell_id(cell)?;
    assert_eq!(sequential_again.stats.references, signature.0);
    assert_eq!(sequential_again.stats.meshes_parsed, signature.2);
    assert_eq!(sequential_again.stats.meshes_failed, 0);
    if let Some(active_threads) = threads_with_pool.filter(|count| *count >= 12) {
        assert!(
            process_thread_count()? + 12 <= active_threads,
            "switching back to one worker should release most of the cached pool threads"
        );
    }

    eprintln!(
        "prf_05 open with 24 workers {pooled_open:?}; 24-worker pool first stream {pool_startup_and_first_stream:?}; warm 1-worker stream median {:?}; warm 24-worker stream median {:?}; references {}, modeled statics {}, meshes {}, failures {}",
        median(sequential_samples), median(pooled), signature.0, signature.1, signature.2, signature.3
    );
    Ok(())
}
