// SPDX-License-Identifier: GPL-3.0-or-later

//! Real textures out of the vanilla archives, when `SKYRIM_DATA` points at a
//! Skyrim SE `Data/` folder. Every sampled texture is decoded here and by
//! image_dds (bcdec underneath) and the two must agree, which checks the BC7
//! tables and the BC1 arithmetic against an independent implementation on
//! thousands of real blocks. Skips cleanly when the variable is unset.

use std::collections::BTreeMap;
use std::path::PathBuf;

use soe_bsa::Archive;
use soe_dds::{Dds, Format};

fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    if dir.join("Skyrim.esm").is_file() {
        Some(dir)
    } else {
        eprintln!("SKYRIM_DATA={} has no Skyrim.esm, skipping", dir.display());
        None
    }
}

#[test]
fn vanilla_textures_decode_and_match_the_reference_decoder() {
    let Some(dir) = data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping the real-texture test");
        return;
    };
    let per_archive: usize = std::env::var("SOE_DDS_SAMPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(150);
    let mut formats: BTreeMap<String, usize> = BTreeMap::new();
    let mut compared = 0usize;
    let mut worst_by_format: BTreeMap<String, i32> = BTreeMap::new();
    let mut unsupported: BTreeMap<String, usize> = BTreeMap::new();
    let mut archives = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.starts_with("Skyrim - Textures") || !name.ends_with(".bsa") {
            continue;
        }
        archives += 1;
        let archive = Archive::open(&path).unwrap();
        let files: Vec<String> = archive
            .files()
            .iter()
            .filter(|f| f.ends_with(".dds"))
            .cloned()
            .collect();
        let step = (files.len() / per_archive).max(1);
        for file in files.iter().step_by(step).take(per_archive) {
            let bytes = archive.read(file).unwrap();
            let dds = match Dds::parse(&bytes) {
                Ok(d) => d,
                Err(soe_dds::Error::Unsupported(u)) => {
                    *unsupported.entry(u.to_string()).or_default() += 1;
                    continue;
                }
                Err(e) => panic!("{file}: {e}"),
            };
            *formats.entry(dds.format().name()).or_default() += 1;
            let ours = dds.decode(0, 0).unwrap_or_else(|e| panic!("{file}: {e}"));
            for mip in 1..dds.mip_count() {
                dds.decode(0, mip)
                    .unwrap_or_else(|e| panic!("{file} mip {mip}: {e}"));
            }
            let reference =
                image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(&bytes)).unwrap();
            let theirs = match image_dds::image_from_dds(&reference, 0) {
                Ok(i) => i,
                Err(_) => continue,
            };
            // image_dds stacks a cube map's faces vertically, so compare the
            // first face only.
            assert_eq!(theirs.width(), ours.width, "{file}");
            assert_eq!(theirs.height(), ours.height * dds.layer_count(), "{file}");
            let face_len = (ours.width * ours.height * 4) as usize;
            // image_dds decodes through bcndecode, whose BC1 to BC5 rounding
            // differs from the specification's by one step. BC7 interpolation
            // is exact integer arithmetic in every decoder, so it must match.
            let (channels, tolerance): (usize, i32) = match dds.format() {
                Format::Bc4 { .. } => (1, 1),
                Format::Bc5 { .. } => (2, 1),
                Format::Bc1 | Format::Bc2 | Format::Bc3 => (4, 1),
                Format::Bc7 | Format::Uncompressed(_) => (4, 0),
            };
            let mut worst = 0;
            for (a, b) in ours
                .rgba
                .chunks(4)
                .zip(theirs.as_raw()[..face_len].chunks(4))
            {
                for c in 0..channels {
                    worst = worst.max((a[c] as i32 - b[c] as i32).abs());
                }
            }
            assert!(
                worst <= tolerance,
                "{file} ({}): worst channel difference {worst}",
                dds.format().name()
            );
            *worst_by_format.entry(dds.format().name()).or_default() = worst_by_format
                .get(&dds.format().name())
                .copied()
                .unwrap_or(0)
                .max(worst);
            compared += 1;
        }
    }
    assert!(archives >= 1, "no texture archives under {}", dir.display());
    eprintln!("compared {compared} textures against image_dds, formats {formats:?}, worst difference per format {worst_by_format:?}, unsupported {unsupported:?}");
    assert!(compared > 100);
    if !formats.contains_key("BC7") {
        eprintln!("no BC7 textures in the sample, BC7 is covered by the encoder round trip in tests/bc7_roundtrip.rs");
    }
}
