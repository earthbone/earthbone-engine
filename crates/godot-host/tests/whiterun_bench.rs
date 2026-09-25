// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! A synthetic Whiterun-sized streaming benchmark, and a repeated-rock
//! benchmark comparing batching against `MultiMesh` instancing.
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
//! draw cost sits on top and needs a Godot binary to measure, which is why
//! this file never calls into `godot_layer`: `array_mesh_from` and
//! `multimesh_from_group` build `Gd<T>` resources that need a running
//! engine behind them, not a `cargo test` process.

use std::mem::size_of;
use std::sync::Arc;
use std::time::{Duration, Instant};

use soe_esm::GlobalFormId;
use soe_godot_host::batch::{
    batch_surfaces, instancing_candidates, InstanceGroup, INSTANCE_MIN_PLACEMENTS,
};
use soe_godot_host::data_source::{LoadedSurface, SurfaceTextures};
use soe_godot_host::mesh::{build_surface, MaterialDesc, MeshData, MeshOptions, SurfaceArrays};
use soe_godot_host::streaming::{CellCoord, CellStreamer, StreamConfig};
use soe_godot_host::terrain::{CellHeights, RegionBuilder, RegionSpec};
use soe_godot_host::units::{GodotTransform, SkyrimPos, CELL_UNITS, LAND_SAMPLES};

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
        material: MaterialDesc::default(),
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
        "whiterun bench: {cells_loaded} cells, {surfaces_kept} surfaces, {triangles} triangles, {regions_built} region builds in {secs:.3} s"
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

// --- Repeated rock: batching against `MultiMesh` instancing (RND-04) ---

/// How many triangles one synthetic rock carries. A few hundred vertices at
/// full attribute width, the size `INSTANCE_MIN_PLACEMENTS`'s doc comment
/// reasons about.
const ROCK_TRIANGLES: usize = 300;

/// Placements of the one rock model in the scattered cell. Comfortably past
/// [`INSTANCE_MIN_PLACEMENTS`], the "generic rock placed dozens of times"
/// case `docs/godot-host.md`'s batching section named as the trigger for a
/// `MultiMesh` path.
const ROCK_REPEATS: usize = 40;

/// Calls per path timed for the percentiles below. `batch_surfaces` and
/// `instancing_candidates` are read-only over their input, so the same
/// built surfaces are reused every call instead of rebuilding them, which
/// would put allocation noise from the setup into the timing instead of
/// the grouping cost this benchmark measures.
const BENCH_ITERATIONS: usize = 300;

/// One rock's local-space geometry, in Skyrim units, centered near the
/// origin rather than a cell. This is what a `MultiMesh` group carries
/// once and repeats by transform: [`rock_local_geometry`] is built a
/// single time and either cloned once per placement unbaked (the
/// instancing side) or cloned and translated per placement (the batching
/// side), so both sides start from the identical shape.
fn rock_local_geometry(rng: &mut Rng, tris: usize) -> MeshData {
    let verts = tris + 2;
    let mut positions = Vec::with_capacity(verts);
    let mut uvs = Vec::with_capacity(verts);
    for i in 0..verts {
        let t = i as f32 * 0.41;
        positions.push([
            t.cos() * 50.0 + rng.next_f32() * 6.0,
            t.sin() * 50.0 + rng.next_f32() * 6.0,
            (i as f32) * 1.2 + rng.next_f32() * 3.0,
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
        material: MaterialDesc::default(),
    }
}

/// `count` scattered points inside `cell`, in Skyrim units, standing in for
/// where an exterior's rock references actually sit.
fn scattered_placements(rng: &mut Rng, cell: CellCoord, count: usize) -> Vec<(f32, f32)> {
    (0..count)
        .map(|_| {
            (
                cell.x as f32 * CELL_UNITS + rng.next_f32() * CELL_UNITS,
                cell.y as f32 * CELL_UNITS + rng.next_f32() * CELL_UNITS,
            )
        })
        .collect()
}

/// Bytes the arrays a `RenderingServer` upload actually carries would take:
/// three floats a position or normal, one float a tangent component (the
/// four are flattened), two a UV, one index, one bone index, one weight.
/// This is CPU-side buffer size, the same floor `docs/godot-host.md`'s
/// "Numbers" section measures RSS against for the batching change, not a
/// GPU residency or `RenderingServer` RID count, which only a Godot binary
/// can report.
fn buffer_bytes(arrays: &SurfaceArrays) -> usize {
    arrays.positions.len() * size_of::<[f32; 3]>()
        + arrays.normals.len() * size_of::<[f32; 3]>()
        + arrays.tangents.len() * size_of::<f32>()
        + arrays.uvs.len() * size_of::<[f32; 2]>()
        + arrays.indices.len() * size_of::<i32>()
        + arrays.bones.len() * size_of::<i32>()
        + arrays.weights.len() * size_of::<f32>()
}

/// Total CPU-side buffer bytes a `MultiMesh` group holds: its one shared
/// geometry buffer plus one `Transform3D` (12 floats, 48 bytes) a member.
fn instance_group_bytes(group: &InstanceGroup) -> usize {
    buffer_bytes(&group.arrays) + group.members.len() * size_of::<GodotTransform>()
}

/// Times `f` `BENCH_ITERATIONS` times and returns the elapsed durations,
/// sorted so a caller can read off a percentile.
fn timings<F: FnMut()>(mut f: F) -> Vec<Duration> {
    let mut out = Vec::with_capacity(BENCH_ITERATIONS);
    for _ in 0..BENCH_ITERATIONS {
        let start = Instant::now();
        f();
        out.push(start.elapsed());
    }
    out.sort_unstable();
    out
}

/// The `pct` percentile (`0.0` to `1.0`) of an already-sorted duration list.
fn percentile(sorted: &[Duration], pct: f64) -> Duration {
    let last = sorted.len() - 1;
    let idx = ((last as f64) * pct).round() as usize;
    sorted[idx.min(last)]
}

// Static check, not a runtime one: the scenario has to clear the threshold
// it is measuring, and both sides are known at compile time.
const _: () = assert!(ROCK_REPEATS >= INSTANCE_MIN_PLACEMENTS);

#[test]
fn repeated_rock_benchmark_measures_batching_vs_instancing() {
    let mut rng = Rng(0x5eed_1234_abcd_0002);
    let local = rock_local_geometry(&mut rng, ROCK_TRIANGLES);
    let local_arrays = build_surface(&local, MeshOptions::default()).unwrap();
    let cell = CellCoord { x: 12, y: -4 };
    let placements = scattered_placements(&mut rng, cell, ROCK_REPEATS);
    let identity: Arc<str> = Arc::from("meshes\\landscape\\rockcliff01.nif:0");
    let reference = |i: usize| GlobalFormId {
        file: 1,
        object: 0x0010_0000 + i as u32,
    };

    // The batching side: each placement's translation baked straight into
    // its own copy of the vertex data, the shape `data_source` hands
    // `batch_surfaces` for a model that does not clear the instancing
    // threshold. `to_godot` has no translation term of its own, so it
    // distributes over addition and offsetting the already-converted
    // positions by `to_godot(dx, dy, 0)` is the same shape `build_surface`
    // would produce from placement-baked Skyrim-unit input.
    let batched_surfaces: Vec<LoadedSurface> = placements
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let delta = SkyrimPos::new(x, y, 0.0).to_godot();
            let mut arrays = local_arrays.clone();
            for p in &mut arrays.positions {
                p[0] += delta.x;
                p[1] += delta.y;
                p[2] += delta.z;
            }
            arrays.bounds.min.x += delta.x;
            arrays.bounds.min.y += delta.y;
            arrays.bounds.min.z += delta.z;
            arrays.bounds.max.x += delta.x;
            arrays.bounds.max.y += delta.y;
            arrays.bounds.max.z += delta.z;
            LoadedSurface {
                reference: reference(i),
                arrays,
                textures: SurfaceTextures::default(),
                placement: soe_physics::Iso::translation(soe_physics::Vec3::new(x, y, 0.0)),
                instance_transform: None,
                mesh_identity: None,
            }
        })
        .collect();

    // The instancing side: the same local geometry shared unbaked, one
    // `GodotTransform` per placement, what `data_source` hands
    // `instancing_candidates` once a model clears the threshold.
    let instanced_surfaces: Vec<LoadedSurface> = placements
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let origin = SkyrimPos::new(x, y, 0.0).to_godot();
            LoadedSurface {
                reference: reference(i),
                arrays: local_arrays.clone(),
                textures: SurfaceTextures::default(),
                placement: soe_physics::Iso::translation(soe_physics::Vec3::new(x, y, 0.0)),
                instance_transform: Some(GodotTransform {
                    basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    origin,
                }),
                mesh_identity: Some(Arc::clone(&identity)),
            }
        })
        .collect();

    // Structural check before the timed runs: this scenario has to land on
    // one batch and one instance group, or the percentiles below would be
    // comparing something other than "one rock repeated forty times".
    let (batches, batch_stats) = batch_surfaces(&batched_surfaces);
    let (groups, remainder) = instancing_candidates(&instanced_surfaces);
    assert!(
        remainder.is_empty(),
        "every placement clears INSTANCE_MIN_PLACEMENTS, none should fall back to batching"
    );
    assert_eq!(
        groups.len(),
        1,
        "one mesh identity makes one MultiMesh group"
    );
    assert_eq!(groups[0].members.len(), ROCK_REPEATS);
    assert_eq!(
        batches.len(),
        1,
        "one material and shape merge to one batch"
    );
    assert_eq!(batch_stats.largest_batch_surfaces, ROCK_REPEATS);

    let batch_bytes: usize = batches.iter().map(|b| buffer_bytes(&b.arrays)).sum();
    let instance_bytes: usize = groups.iter().map(instance_group_bytes).sum();

    let batch_timings = timings(|| {
        std::hint::black_box(batch_surfaces(&batched_surfaces));
    });
    let instance_timings = timings(|| {
        std::hint::black_box(instancing_candidates(&instanced_surfaces));
    });
    let batch_p50 = percentile(&batch_timings, 0.50);
    let batch_p99 = percentile(&batch_timings, 0.99);
    let instance_p50 = percentile(&instance_timings, 0.50);
    let instance_p99 = percentile(&instance_timings, 0.99);

    eprintln!(
        "repeated rock bench: {ROCK_REPEATS} placements, {ROCK_TRIANGLES} triangles each, \
         {BENCH_ITERATIONS} calls a path"
    );
    eprintln!(
        "repeated rock bench: batched {batch_bytes} bytes ({:.1} KiB), instanced {instance_bytes} bytes ({:.1} KiB)",
        batch_bytes as f64 / 1024.0,
        instance_bytes as f64 / 1024.0
    );
    eprintln!(
        "repeated rock bench: batch_surfaces p50 {batch_p50:?}, p99 {batch_p99:?}; \
         instancing_candidates p50 {instance_p50:?}, p99 {instance_p99:?}"
    );

    // Memory: retain instancing. A shared geometry buffer plus forty 48
    // byte transforms has to cost less than forty baked copies of the same
    // buffer merged into one `ArrayMesh`, for any rock big enough to matter,
    // which this assertion checks on the actual buffer sizes rather than
    // the estimate `INSTANCE_MIN_PLACEMENTS`'s doc comment reasoned from.
    assert!(
        instance_bytes < batch_bytes,
        "a MultiMesh sharing one geometry buffer plus {ROCK_REPEATS} transforms \
         ({instance_bytes} bytes) should beat {ROCK_REPEATS} baked copies merged into one \
         ArrayMesh ({batch_bytes} bytes)"
    );
    // CPU time to build: `instancing_candidates` clones the shared geometry
    // once and pushes a small member per further placement, while
    // `batch_surfaces` copies the full vertex buffers into the merged
    // surface on every placement past the first, so instancing should not
    // cost more to build than batching for the same repeated rock.
    assert!(
        instance_p50 <= batch_p50,
        "instancing_candidates p50 {instance_p50:?} should not exceed batch_surfaces p50 \
         {batch_p50:?} for {ROCK_REPEATS} repeats of one rock"
    );

    // This is the Rust-side floor only, as in the Whiterun benchmark above.
    // The RenderingServer RID and draw call cost `INSTANCE_MIN_PLACEMENTS`'s
    // doc comment also reasons about needs a Godot binary
    // (`tools/performance.py` or `godot/bench.gd`) to measure, and is not
    // this test's job.
}
