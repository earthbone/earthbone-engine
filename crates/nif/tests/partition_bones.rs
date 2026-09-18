// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Does a skin partition's bone list actually reorder anything?
//!
//! A bone index stored in a partition addresses that partition's own short
//! bone list, and `Mesh::bone_indices` is documented as addressing the skin
//! instance's. The Special Edition path copied them raw until 2026-09-18,
//! which is a defect whether or not it changes a picture. This measures how
//! much it changed: where a partition's bone list is the identity the old
//! code happened to be right, and everywhere else it was wrong.
//!
//! Gated on `SKYRIM_DATA`, reads character meshes out of the mesh archive.

use soe_bsa::Archive;
use soe_nif::blocks::Block;
use std::path::PathBuf;

fn archive() -> Option<Archive> {
    let dir = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let path = dir.join("Skyrim - Meshes0.bsa");
    if !path.exists() {
        eprintln!("skipping: {} is not there", path.display());
        return None;
    }
    Some(Archive::open(&path).expect("the mesh archive opens"))
}

#[test]
fn partition_bone_lists_are_measured_not_assumed() {
    let Some(archive) = archive() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };

    let names: Vec<String> = archive
        .files()
        .iter()
        .filter(|n| {
            let lower = n.to_ascii_lowercase();
            std::path::Path::new(&lower)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("nif"))
                && lower.contains("actors")
                && !lower.contains("facegen")
        })
        .take(400)
        .cloned()
        .collect();
    if names.is_empty() {
        eprintln!("skipping: the archive lists no actor meshes");
        return;
    }

    let (mut skinned, mut parts, mut identity, mut reordered, mut widest) = (0, 0, 0, 0, 0usize);
    for name in &names {
        let Ok(bytes) = archive.read(name) else {
            continue;
        };
        let Ok(nif) = soe_nif::parse(&bytes) else {
            continue;
        };
        let mut has_partition = false;
        for block in &nif.blocks {
            let Block::SkinPartition(p) = block else {
                continue;
            };
            has_partition = true;
            for part in &p.partitions {
                if part.bones.is_empty() {
                    continue;
                }
                parts += 1;
                widest = widest.max(part.bones.len());
                if part
                    .bones
                    .iter()
                    .enumerate()
                    .all(|(i, &b)| usize::from(b) == i)
                {
                    identity += 1;
                } else {
                    reordered += 1;
                }
            }
        }
        if has_partition {
            skinned += 1;
        }
    }

    eprintln!("partition bone lists over {} actor meshes:", names.len());
    eprintln!("  meshes with a skin partition  {skinned}");
    eprintln!("  partitions with a bone list   {parts}");
    eprintln!("  identity, old code was right  {identity}");
    eprintln!("  reordered, old code was wrong {reordered}");
    eprintln!("  widest bone list              {widest}");

    assert!(
        parts > 0,
        "no partition carried a bone list, so nothing was measured"
    );
}
