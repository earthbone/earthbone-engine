use std::path::Path;
use std::time::{Duration, Instant};

use soe_godot_host::data_source::DataSource;

#[test]
fn prf_05_persistent_workers_keep_bannered_mare_stream_time_measurable(
) -> Result<(), Box<dyn std::error::Error>> {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA is unset, skipping the real-data stream measurement");
        return Ok(());
    };
    let mut source = DataSource::open_with_threads(Path::new(&data), 24)?;
    let cell = source
        .resolve_cell("WhiterunBanneredMare")
        .ok_or_else(|| std::io::Error::other("Bannered Mare cell is absent"))?;
    let _ = source.stream_cell_id(cell)?;

    let mut samples = Vec::with_capacity(5);
    let mut references = 0;
    for _ in 0..5 {
        let start = Instant::now();
        let loaded = source.stream_cell_id(cell)?;
        samples.push(start.elapsed());
        references = loaded.stats.references;
    }
    samples.sort_unstable();
    let median: Duration = samples[samples.len() / 2];
    eprintln!(
        "prf_05 Bannered Mare warm stream: {median:?}, references: {references}, workers: {}",
        source.threads()
    );
    assert!(references > 0, "the real cell has references to stream");
    source.set_threads(1);
    let sequential = source.stream_cell_id(cell)?;
    assert_eq!(sequential.stats.references, references);
    Ok(())
}
