// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Every packfile inside `Skyrim - Animations.bsa`, when `SKYRIM_DATA`
//! points at a Skyrim SE `Data/` folder: parse, count the classes, and
//! decode every spline compressed animation found.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use soe_havok::animation::SplineCompressedAnimation;
use soe_havok::Packfile;

#[test]
fn every_shipped_packfile_parses_and_animations_decode() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let limit: usize = std::env::var("SOE_HKX_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let started = Instant::now();
    let archive =
        soe_bsa::Archive::open(&data.join("Skyrim - Animations.bsa")).expect("archive opens");
    let names: Vec<String> = archive
        .files()
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".hkx"))
        .cloned()
        .collect();
    let mut files = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut by_error: BTreeMap<String, usize> = BTreeMap::new();
    let mut roots: BTreeMap<String, usize> = BTreeMap::new();
    let mut animations = 0usize;
    let mut decoded = 0usize;
    let mut decode_failures: Vec<String> = Vec::new();
    let mut tracks = 0usize;
    for name in names {
        if limit != 0 && files >= limit {
            break;
        }
        let bytes = archive.read(&name).expect("file reads");
        files += 1;
        let pf = match Packfile::parse(&bytes) {
            Ok(pf) => pf,
            Err(e) => {
                let text = e.to_string();
                let key = text.split(" at ").next().unwrap_or(&text).to_string();
                *by_error.entry(key).or_default() += 1;
                failures.push(format!("{name}: {text}"));
                continue;
            }
        };
        let root = pf
            .root()
            .ok()
            .and_then(|r| r.class().map(str::to_string))
            .unwrap_or_else(|| "?".into());
        *roots.entry(root).or_default() += 1;
        for obj in pf.objects_of("hkaSplineCompressedAnimation") {
            animations += 1;
            match SplineCompressedAnimation::from_obj(obj).and_then(|a| a.decode().map(|b| (a, b)))
            {
                Ok((anim, blocks)) => {
                    decoded += 1;
                    tracks += anim.sample_pose(&blocks, 0.0).len();
                }
                Err(e) => decode_failures.push(format!("{name}: {e}")),
            }
        }
    }
    println!(
        "{files} packfiles in {:.1}s: {} parse failures, roots {roots:?}",
        started.elapsed().as_secs_f32(),
        failures.len()
    );
    println!(
        "{animations} spline animations, {decoded} decoded, {tracks} tracks sampled, {} decode failures",
        decode_failures.len()
    );
    for (err, n) in &by_error {
        println!("  {n} x {err}");
    }
    for f in failures.iter().chain(decode_failures.iter()).take(40) {
        println!("  {f}");
    }
    let rate = (failures.len() + decode_failures.len()) as f64 / files.max(1) as f64;
    assert!(
        rate < 0.01,
        "{} of {files} packfiles failed",
        failures.len() + decode_failures.len()
    );
}
