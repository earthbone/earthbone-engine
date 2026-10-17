// SPDX-License-Identifier: GPL-3.0-or-later

//! SSE source-cell segments follow nifly BSSubIndexTriShape::Sync.

mod common;

use common::{Buf, NifWriter, IDENTITY};
use soe_nif::blocks::{Block, GeometrySegment};

fn fixture(segments: &[(u32, u32)]) -> Vec<u8> {
    let mut writer = NifWriter::new(100);
    let (vertices, triangles) = common::quad();
    let mut shape = Buf(writer.bs_tri_shape(
        "LOD",
        [0.0; 3],
        -1,
        -1,
        -1,
        soe_nif::vertex::attr::VERTEX,
        &vertices,
        &triangles,
        true,
    ));
    shape.u32(segments.len() as u32);
    for &(first, count) in segments {
        shape.u8(0).u32(first).u32(count);
    }
    let shape = writer.add("BSSubIndexTriShape", shape.0);
    let node = writer.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    writer.roots = vec![writer.add("NiNode", node)];
    writer.finish()
}

#[test]
fn object_lod_segments_preserve_empty_cells_and_index_units(
) -> Result<(), Box<dyn std::error::Error>> {
    let file = soe_nif::parse(&fixture(&[(0, 1), (0, 0), (3, 1)]))?;
    let segments = file
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::BsTriShape(shape) => Some(&shape.segments),
            _ => None,
        })
        .ok_or("shape missing")?;
    assert_eq!(
        segments,
        &[
            GeometrySegment {
                flags: 0,
                first_index: 0,
                triangles: 1
            },
            GeometrySegment {
                flags: 0,
                first_index: 0,
                triangles: 0
            },
            GeometrySegment {
                flags: 0,
                first_index: 3,
                triangles: 1
            },
        ]
    );
    Ok(())
}

#[test]
fn object_lod_segments_reject_unaligned_and_out_of_buffer_ranges() {
    for segments in [
        &[(1, 1)][..],
        &[(3, u32::MAX)][..],
        &[(6, 1)][..],
        &[(0, 1), (0, 1)][..],
        &[(0, 1)][..],
    ] {
        assert!(soe_nif::parse(&fixture(segments)).is_err());
    }
}

#[test]
fn object_lod_segments_installed_archive_survey() -> Result<(), Box<dyn std::error::Error>> {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return Ok(());
    };
    let mut files = 0;
    let mut shapes = 0;
    let mut triangles = 0u64;
    for name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&std::path::Path::new(&data).join(name))?;
        let mut paths: Vec<_> = archive
            .files()
            .iter()
            .filter(|path| {
                std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("bto"))
            })
            .collect();
        paths.sort_unstable();
        for path in paths {
            let bytes = archive.read(path)?;
            let file = soe_nif::parse(&bytes).map_err(|error| format!("{path}: {error}"))?;
            files += 1;
            for block in &file.blocks {
                if let Block::BsTriShape(shape) = block {
                    if !shape.segments.is_empty() {
                        shapes += 1;
                        let count: u64 =
                            shape.segments.iter().map(|s| u64::from(s.triangles)).sum();
                        assert_eq!(count, u64::from(shape.num_triangles), "{path}");
                        triangles += count;
                    }
                }
            }
        }
    }
    assert!(files > 0 && shapes > 0);
    eprintln!("MOUNTAIN_SEGMENTS files={files} shapes={shapes} triangles={triangles}");
    Ok(())
}
