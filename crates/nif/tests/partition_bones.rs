// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Does a skin partition's bone list actually reorder anything?
//!
//! These tables can reorder even though SSE's separate global vertex stream
//! already carries skin-instance indices. This survey counts table layouts,
//! it does not infer the vertex stream's index space from those counts.
//! Nifly's PrepareData and GetShapeBoneWeights establish that contract.
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
    let (mut global_streams, mut global_vertices) = (0, 0);
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
        if let Ok(meshes) = nif.meshes() {
            for mesh in meshes {
                if mesh.bone_indices.is_empty() {
                    continue;
                }
                let Some(Block::BsTriShape(shape)) = nif.blocks.get(mesh.block) else {
                    continue;
                };
                let vertices = if shape.vertices.is_empty() {
                    let Some(Block::SkinInstance(skin)) = nif.get(shape.skin) else {
                        continue;
                    };
                    let Some(Block::SkinPartition(partition)) = nif.get(skin.partition) else {
                        continue;
                    };
                    &partition.vertices
                } else {
                    &shape.vertices
                };
                let stored: Vec<_> = vertices.iter().map(|vertex| vertex.bone_indices).collect();
                assert_eq!(
                    mesh.bone_indices, stored,
                    "{name} keeps nifly's global SSE indices"
                );
                global_streams += 1;
                global_vertices += vertices.len();
            }
        }
    }

    eprintln!("partition bone lists over {} actor meshes:", names.len());
    eprintln!("  meshes with a skin partition  {skinned}");
    eprintln!("  partitions with a bone list   {parts}");
    eprintln!("  identity bone tables         {identity}");
    eprintln!("  reordered bone tables       {reordered}");
    eprintln!("  widest bone list              {widest}");
    eprintln!("  global streams matched         {global_streams}, vertices={global_vertices}");
    assert!(global_streams > 0);

    assert!(
        parts > 0,
        "no partition carried a bone list, so nothing was measured"
    );
}
