// SPDX-License-Identifier: GPL-3.0-or-later

//! A synthetic Whiterun-sized streaming benchmark.
//!
//! No Skyrim data is on the development machine, so the load is generated:
//! Whiterun's exterior grid is about 9 by 9 cells, each cell here gets
//! forty statics of two thousand triangles plus a LAND height grid, which
//! is on the heavy side of what the real city carries per cell. The
//! benchmark walks the focus across the grid one cell per tick and times
//! the whole pipeline the host runs per cell: streaming decision, mesh
//! conversion for every static, terrain stitching for the cell's region.
//! It prints cells per second and triangles per second. Run with
//! `cargo test -p soe-godot-host --test whiterun_bench -- --nocapture`.
//! The numbers are a floor for the Rust side only. Godot's own upload and
//! draw cost sits on top and needs a Godot binary to measure.

use std::time::Instant;

use soe_godot_host::mesh::{build_surface, MeshData, MeshOptions};
use soe_godot_host::streaming::{CellCoord, CellStreamer, StreamConfig};
use soe_godot_host::terrain::{CellHeights, RegionBuilder, RegionSpec};
use soe_godot_host::units::{SkyrimPos, CELL_UNITS, LAND_SAMPLES};

const GRID: i32 = 9;
const STATICS_PER_CELL: usize = 40;
const TRIANGLES_PER_STATIC: usize = 2000;

/// A deterministic pseudo-random generator so runs are comparable.
struct Rng(u64);

impl Rng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// A closed triangle strip-like blob with `tris` triangles, roughly what a
/// rock or a wall segment decodes to.
fn synthetic_static(rng: &mut Rng, cell: CellCoord, tris: usize) -> MeshData {
    let verts = tris + 2;
    let ox = cell.x as f32 * CELL_UNITS + rng.next_f32() * CELL_UNITS;
    let oy = cell.y as f32 * CELL_UNITS + rng.next_f32() * CELL_UNITS;
    let mut positions = Vec::with_capacity(verts);
    let mut uvs = Vec::with_capacity(verts);
    for i in 0..verts {
        let t = i as f32 * 0.37;
        positions.push([
            ox + t.cos() * 200.0 + rng.next_f32() * 30.0,
            oy + t.sin() * 200.0 + rng.next_f32() * 30.0,
            (i as f32) * 2.0 + rng.next_f32() * 10.0,
        ]);
        uvs.push([rng.next_f32(), rng.next_f32()]);
    }
    let mut indices = Vec::with_capacity(tris * 3);
    for i in 0..tris as u32 {
        indices.extend_from_slice(&[i, i + 1, i + 2]);
    }
    MeshData {
        positions,
        normals: None,
        tangents: None,
        uvs: Some(uvs),
        indices,
        bone_indices: None,
        bone_weights: None,
        material: Default::default(),
    }
}

fn synthetic_land(rng: &mut Rng, cell: CellCoord) -> CellHeights {
    let mut units = Vec::with_capacity(LAND_SAMPLES * LAND_SAMPLES);
    let base = 2000.0 + rng.next_f32() * 500.0;
    for row in 0..LAND_SAMPLES {
        for col in 0..LAND_SAMPLES {
            let x = (cell.x * 32 + col as i32) as f32;
            let y = (cell.y * 32 + row as i32) as f32;
            units.push(base + (x * 0.05).sin() * 300.0 + (y * 0.07).cos() * 200.0);
        }
    }
    CellHeights::new(cell, units).unwrap()
}

#[test]
fn whiterun_sized_grid_streams_through_the_rust_pipeline() {
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    let mut streamer = CellStreamer::new(StreamConfig {
        load_radius: 2,
        unload_radius: 3,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
    })
    .unwrap();
    let mut regions = RegionBuilder::new(RegionSpec::default()).unwrap();

    let mut cells_loaded = 0usize;
    let mut triangles = 0usize;
    let mut regions_built = 0usize;
    let mut surfaces_kept = 0usize;

    let start = Instant::now();
    // Walk east along the middle row, then north up the last column, one
    // cell per tick, which is faster than a sprinting player.
    let mut path: Vec<SkyrimPos> = (0..GRID)
        .map(|x| SkyrimPos::new((x as f32 + 0.5) * CELL_UNITS, 4.5 * CELL_UNITS, 0.0))
        .collect();
    path.extend((0..GRID).map(|y| {
        SkyrimPos::new(
            (GRID as f32 - 0.5) * CELL_UNITS,
            (y as f32 + 0.5) * CELL_UNITS,
            0.0,
        )
    }));

    for focus in path {
        let plan = streamer.update(focus);
        for cell in plan.load {
            for _ in 0..STATICS_PER_CELL {
                let src = synthetic_static(&mut rng, cell, TRIANGLES_PER_STATIC);
                let surface = build_surface(&src, MeshOptions::default()).unwrap();
                triangles += surface.triangle_count();
                surfaces_kept += 1;
            }
            regions.insert(synthetic_land(&mut rng, cell));
            let region = regions.spec().region_of(cell);
            let built = regions.build(region);
            assert!(built.cells_present >= 1);
            regions_built += 1;
            cells_loaded += 1;
        }
        for cell in plan.unload {
            regions.remove(cell);
        }
    }
    let elapsed = start.elapsed();
    let secs = elapsed.as_secs_f64();

    eprintln!(
        "whiterun bench: {cells_loaded} cells, {surfaces_kept} surfaces, {triangles} triangles, {regions_built} region builds in {:.3} s",
        secs
    );
    eprintln!(
        "whiterun bench: {:.1} cells/s, {:.0} triangles/s",
        cells_loaded as f64 / secs,
        triangles as f64 / secs
    );

    assert!(
        cells_loaded >= 25,
        "the first tick alone loads a 5 by 5 square"
    );
    assert_eq!(
        triangles,
        cells_loaded * STATICS_PER_CELL * TRIANGLES_PER_STATIC
    );
    // A cell of forty statics at two thousand triangles converting in under
    // a second on any machine this runs on. If this trips, the pipeline
    // regressed, not the machine.
    assert!(
        cells_loaded as f64 / secs > 1.0,
        "slower than one cell per second: {secs} s for {cells_loaded} cells"
    );
}
