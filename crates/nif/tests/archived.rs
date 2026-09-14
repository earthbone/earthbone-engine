// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Every mesh inside the vanilla mesh archives, when `SKYRIM_DATA` points at
//! a Skyrim SE `Data/` folder. The loose-file test in `real_files.rs` finds
//! nothing on a plain install, the meshes ship packed.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

#[test]
fn every_archived_mesh_parses() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let started = Instant::now();
    let mut files = 0usize;
    let mut with_meshes = 0usize;
    let mut triangles = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut by_error: BTreeMap<String, usize> = BTreeMap::new();
    for archive_name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&data.join(archive_name)).expect("archive opens");
        let names: Vec<String> = archive
            .files()
            .iter()
            .filter(|n| n.to_lowercase().ends_with(".nif"))
            .cloned()
            .collect();
        for name in names {
            let bytes = archive.read(&name).expect("file reads");
            files += 1;
            match soe_nif::parse(&bytes) {
                Ok(file) => match file.meshes() {
                    Ok(meshes) => {
                        if !meshes.is_empty() {
                            with_meshes += 1;
                        }
                        triangles += meshes.iter().map(|m| m.triangle_count()).sum::<usize>();
                    }
                    Err(e) => failures.push(format!("{name}: mesh walk: {e}")),
                },
                Err(e) => {
                    let text = e.to_string();
                    let key = text.split(" at byte").next().unwrap_or(&text).to_string();
                    *by_error.entry(key).or_default() += 1;
                    failures.push(format!("{name}: {text}"));
                }
            }
        }
    }
    println!(
        "{files} archived meshes in {:.1}s: {with_meshes} with geometry, {triangles} triangles, {} failures",
        started.elapsed().as_secs_f32(),
        failures.len()
    );
    for (err, n) in &by_error {
        println!("  {n} x {err}");
    }
    for f in failures.iter().take(40) {
        println!("  {f}");
    }
    let rate = failures.len() as f64 / files.max(1) as f64;
    assert!(rate < 0.01, "{} of {files} meshes failed", failures.len());
}
