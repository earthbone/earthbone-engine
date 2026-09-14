// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Parses every loose `.nif` under `$SKYRIM_DATA/meshes` and every file
//! under `$SOE_NIF_SAMPLES`, and reports what loaded. Skips when neither
//! variable is set. Skyrim SE ships almost all meshes inside BSA archives,
//! so `SOE_NIF_SAMPLES` pointed at an extracted folder is the useful one.
//!
//! ```text
//! SOE_NIF_SAMPLES=/tmp/meshes cargo test -p soe-nif --test real_files -- --nocapture
//! ```

use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("nif"))
        {
            out.push(path);
        }
    }
}

#[test]
fn every_real_file_parses_and_yields_meshes() {
    let mut roots = Vec::new();
    if let Some(data) = std::env::var_os("SKYRIM_DATA") {
        roots.push(PathBuf::from(data).join("meshes"));
    }
    if let Some(samples) = std::env::var_os("SOE_NIF_SAMPLES") {
        roots.push(PathBuf::from(samples));
    }
    if roots.is_empty() {
        eprintln!("SKYRIM_DATA and SOE_NIF_SAMPLES unset, skipping the real-file test");
        return;
    }
    let mut files = Vec::new();
    for root in &roots {
        collect(root, &mut files);
    }
    if files.is_empty() {
        eprintln!("no .nif files under {roots:?}, skipping");
        return;
    }
    let mut parsed = 0usize;
    let mut with_meshes = 0usize;
    let mut triangles = 0usize;
    let mut failures = Vec::new();
    let mut unsupported = 0usize;
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        match soe_nif::parse(&bytes) {
            Ok(file) => {
                parsed += 1;
                match file.meshes() {
                    Ok(meshes) => {
                        if !meshes.is_empty() {
                            with_meshes += 1;
                        }
                        triangles += meshes.iter().map(|m| m.triangle_count()).sum::<usize>();
                    }
                    Err(e) => failures.push(format!("{}: mesh walk: {e}", path.display())),
                }
            }
            Err(soe_nif::Error::UnsupportedVersion { .. }) => unsupported += 1,
            Err(e) => failures.push(format!("{}: {e}", path.display())),
        }
    }
    eprintln!(
        "{} files: {parsed} parsed, {with_meshes} with meshes, {triangles} triangles, {unsupported} unsupported streams, {} failures",
        files.len(),
        failures.len()
    );
    for f in failures.iter().take(50) {
        eprintln!("  {f}");
    }
    assert!(
        failures.is_empty(),
        "{} files failed to load",
        failures.len()
    );
}
