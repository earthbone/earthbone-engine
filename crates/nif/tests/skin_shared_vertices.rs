// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Does a skin vertex belong to more than one partition, and does the
//! partition bone remap then run twice over the same vertex?
//! Gated on `SKYRIM_DATA`.

use soe_nif::blocks::Block;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn shared_partition_vertices_are_counted_for_large_creatures() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let archive = soe_bsa::Archive::open(&data.join("Skyrim - Meshes0.bsa")).unwrap();
    let names: Vec<String> = archive
        .files()
        .iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            std::path::Path::new(&l)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("nif"))
                && l.contains("actors")
                && !l.contains("facegen")
        })
        .cloned()
        .collect();
    let (mut shapes, mut shared_shapes, mut shared_verts) = (0, 0, 0usize);
    let mut worst: Vec<(usize, String)> = Vec::new();
    for name in &names {
        let Ok(bytes) = archive.read(name) else {
            continue;
        };
        let Ok(nif) = soe_nif::parse(&bytes) else {
            continue;
        };
        for block in &nif.blocks {
            let Block::SkinPartition(p) = block else {
                continue;
            };
            shapes += 1;
            let mut seen: HashMap<u16, usize> = HashMap::new();
            for part in &p.partitions {
                for &v in &part.vertex_map {
                    *seen.entry(v).or_default() += 1;
                }
            }
            let n = seen.values().filter(|&&c| c > 1).count();
            if n > 0 {
                shared_shapes += 1;
                shared_verts += n;
                worst.push((n, name.clone()));
            }
        }
    }
    worst.sort();
    eprintln!("partitions blocks {shapes}, with shared vertices {shared_shapes}, shared vertices {shared_verts}");
    for (n, name) in worst.iter().rev().take(8) {
        eprintln!("  {n} {name}");
    }
}
