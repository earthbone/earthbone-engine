// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-07 adversarial coverage for `InteriorPrefetch::request` and
//! `invalidate` against the installed game. Gated on `SKYRIM_DATA`, skips
//! cleanly without it.
//!
//! `gameplay.rs` calls `request` every `SELECTION_INTERVAL` (0.25 s) with
//! whatever door `nearby_interior_destination` currently returns, and calls
//! `invalidate` the moment that selection goes away or changes. A real
//! interior decode measured past 490 ms in `docs/godot-host.md`, so a
//! selection almost always changes while the previous job is still
//! running. Both tests below drive `request`/`invalidate` through exactly
//! that sequence and check that the player's current choice is the one
//! that ends up prefetched, per the done check: "Approaching a load door
//! prefetches its linked destination within a bounded budget."

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use soe_esm::GlobalFormId;
use soe_godot_host::data_source::DataSource;
use soe_godot_host::interior_prefetch::InteriorPrefetch;

fn open_source() -> Option<Arc<DataSource>> {
    let data = std::env::var_os("SKYRIM_DATA")?;
    let source = DataSource::open_with_threads(Path::new(&data), 1)
        .unwrap_or_else(|e| panic!("DataSource::open_with_threads: {e}"));
    Some(Arc::new(source))
}

/// Two interior cells light enough that admission never turns on their
/// payload size, chosen straight from the load order's own cell index so
/// the test needs no second named editor id.
fn two_light_interiors(source: &DataSource) -> (GlobalFormId, GlobalFormId) {
    let index = source.cell_index();
    let mut candidates: Vec<GlobalFormId> = index
        .cells()
        .filter(|&(id, refs)| refs > 5 && refs < 400 && index.is_interior(id))
        .map(|(id, _)| id)
        .collect();
    candidates.sort_unstable();
    assert!(
        candidates.len() >= 2,
        "installed data has fewer than two light interior cells to test with"
    );
    (candidates[0], candidates[1])
}

fn settle(prefetch: &mut InteriorPrefetch, budget: Duration) {
    let deadline = Instant::now() + budget;
    while prefetch.pending() {
        prefetch.poll();
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn re_approaching_the_same_door_before_the_first_job_finishes_still_gets_prefetched() {
    let Some(source) = open_source() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let destination = source
        .resolve_cell("WhiterunBanneredMare")
        .expect("the Bannered Mare is in Skyrim.esm");

    let mut prefetch = InteriorPrefetch::default();
    prefetch.request(Arc::clone(&source), destination);
    assert_eq!(prefetch.started, 1, "the first approach did not start a job");

    // The player stepped back out of the approach radius (gameplay.rs
    // invalidates every tick the selection is empty) and returned to the
    // same door well before the still-running decode could finish.
    prefetch.invalidate();
    prefetch.request(Arc::clone(&source), destination);

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if prefetch.take(destination).is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "re-approaching the same door never produced a prefetch: \
             started={}, rejected={}, pending={}",
            prefetch.started,
            prefetch.rejected,
            prefetch.pending()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn switching_to_a_different_destination_while_the_previous_job_is_still_running_is_not_dropped() {
    let Some(source) = open_source() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let (destination_a, destination_b) = two_light_interiors(&source);
    assert_ne!(destination_a, destination_b);

    let mut prefetch = InteriorPrefetch::default();
    prefetch.request(Arc::clone(&source), destination_a);
    assert_eq!(
        prefetch.started, 1,
        "approaching the first door did not start a job"
    );

    // The player left destination_a's door for destination_b's within the
    // same selection tick, an ordinary approach-then-approach-elsewhere
    // sequence, not a contrived race: nearby_interior_destination is
    // re-evaluated and fed to request every 0.25 s regardless of whether
    // the previous job has finished.
    prefetch.request(Arc::clone(&source), destination_b);

    settle(&mut prefetch, Duration::from_secs(30));
    assert!(
        !prefetch.pending(),
        "destination_a's cancelled job never finished within the 30 s budget"
    );
    assert_eq!(
        prefetch.started, 2,
        "the door the player is now standing at was never prefetched: \
         rejected={}, ready={:?}",
        prefetch.rejected,
        prefetch.ready_cell()
    );
}
