// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for `batch::instancing_candidates`, the grouping
//! step RND-04's benchmark measures against `batch_surfaces` but that
//! `crates/godot-host/src/batch.rs` itself carries no unit test for. Every
//! test here works only through the crate's public, Godot-free surface
//! (`batch`, `data_source`, `mesh`, `units`), the same seam
//! `tests/whiterun_bench.rs` already uses, so these run under plain
//! `cargo test` with no engine behind them.
//!
//! Run: `cargo test -p soe-godot-host --test instancing_grouping_adversarial`

use std::sync::Arc;
use std::time::{Duration, Instant};

use soe_esm::GlobalFormId;
use soe_godot_host::batch::{batch_surfaces, instancing_candidates, INSTANCE_MIN_PLACEMENTS};
use soe_godot_host::data_source::{LoadedSurface, SurfaceTextures};
use soe_godot_host::mesh::{build_surface, MaterialDesc, MeshData, MeshOptions, SurfaceArrays};
use soe_godot_host::units::{GodotPos, GodotTransform};

/// A small quad, standing in for one shape of a repeated exterior static.
/// `diffuse` lets a test build two placements that differ only in material.
fn quad_arrays(diffuse: &str) -> SurfaceArrays {
    let data = MeshData {
        positions: vec![
            [0.0, 0.0, 0.0],
            [70.0, 0.0, 0.0],
            [0.0, 70.0, 0.0],
            [70.0, 70.0, 0.0],
        ],
        indices: vec![0, 1, 2, 2, 1, 3],
        uvs: Some(vec![[0.0, 0.0]; 4]),
        material: MaterialDesc {
            diffuse: Some(diffuse.to_string()),
            ..MaterialDesc::default()
        },
        ..MeshData::default()
    };
    build_surface(&data, MeshOptions::default()).unwrap()
}

fn identity_transform(x: f32) -> GodotTransform {
    GodotTransform {
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        origin: GodotPos::new(x, 0.0, 0.0),
    }
}

/// One placement `data_source::stream_cell_id` would hand
/// `instancing_candidates`: local-space geometry, a shared mesh identity,
/// and the placement's own `instance_transform`.
fn instanced_surface(
    identity: &Arc<str>,
    arrays: SurfaceArrays,
    reference_object: u32,
    x: f32,
) -> LoadedSurface {
    LoadedSurface {
        reference: GlobalFormId {
            file: 1,
            object: reference_object,
        },
        arrays,
        textures: SurfaceTextures::default(),
        placement: soe_physics::Iso::IDENTITY,
        instance_transform: Some(identity_transform(x)),
        mesh_identity: Some(Arc::clone(identity)),
    }
}

// --- 1. Skinned models must never join a MultiMesh group ---
//
// `batch.rs`'s own module doc (the lines directly above `batch_surfaces`)
// states the reason the batching path keeps skinned surfaces alone: "their
// bone indices refer to their own skeleton", and the existing
// `skinned_surfaces_stay_alone` unit test in `batch.rs` pins that for
// batching. A `MultiMesh` instance carries only a rigid `Transform3D`, no
// skeleton pose, so a skinned shape shared across placements would render
// with a dead bind pose for the physics-cloth or animated statics that do
// carry skin data (banners, some flora). `instancing_candidates` groups
// purely by `mesh_identity` and `instance_transform` and has no analogous
// check, so this same invariant is missing on the instancing path.
#[test]
fn instancing_candidates_excludes_skinned_surfaces() {
    let identity: Arc<str> = Arc::from("meshes\\clutter\\banner01.nif#0");
    let surfaces: Vec<LoadedSurface> = (0..INSTANCE_MIN_PLACEMENTS as u32)
        .map(|i| {
            let mut arrays = quad_arrays("textures\\banner01.dds");
            // A skinned shape carries per-vertex bone indices and weights.
            // `SurfaceArrays::is_skinned` is exactly `!bones.is_empty()`.
            arrays.bones = vec![0; arrays.positions.len() * 4];
            arrays.weights = vec![1.0; arrays.positions.len() * 4];
            instanced_surface(&identity, arrays, i, i as f32 * 100.0)
        })
        .collect();

    let (groups, remainder) = instancing_candidates(&surfaces);

    assert!(
        groups.is_empty(),
        "a skinned shape must never become a MultiMesh group: its bone \
         indices refer to its own skeleton, and a MultiMesh has no per-\
         instance skeleton to bind them to, the same reason batch_surfaces \
         keeps skinned surfaces out of a merged ArrayMesh"
    );
    assert_eq!(
        remainder.len(),
        surfaces.len(),
        "every skinned placement must fall back to the remainder, which \
         batch_surfaces already knows to keep unmerged per surface"
    );
}

// --- 2. Grouping by mesh identity alone silently drops material variation ---
//
// `target/task-pack/RND-04.md` (Key symbols, `BatchKey`) is explicit that
// the instancing key needs to be "mesh identity" grouping "by same NIF +
// material", by analogy with `BatchKey`, which folds every material field
// (diffuse, normal, alpha, and the rest) into the batching key so two
// surfaces with different textures never merge into one drawable. The
// `mesh_identity` field `data_source.rs` actually sets (its own doc comment,
// "The model path and shape index this surface came from") carries neither
// diffuse nor any other material field. A `MultiMesh` has exactly one mesh,
// so one material, for every instance it draws: two placements of the same
// NIF with different diffuse textures (a base and a recolored variant, both
// legal for the same STAT model through an alternate texture set) must not
// collapse into one group that silently renders every instance with the
// first-seen member's texture.
#[test]
fn instancing_candidates_separates_placements_with_different_materials() {
    let identity: Arc<str> = Arc::from("meshes\\landscape\\rockcliff01.nif#0");
    let mut surfaces = Vec::new();
    for i in 0..INSTANCE_MIN_PLACEMENTS as u32 {
        surfaces.push(instanced_surface(
            &identity,
            quad_arrays("textures\\landscape\\rockcliff01.dds"),
            i,
            i as f32 * 100.0,
        ));
    }
    for i in 0..INSTANCE_MIN_PLACEMENTS as u32 {
        surfaces.push(instanced_surface(
            &identity,
            quad_arrays("textures\\landscape\\rockcliff01_snow.dds"),
            100 + i,
            1000.0 + i as f32 * 100.0,
        ));
    }

    let (groups, remainder) = instancing_candidates(&surfaces);

    assert!(
        remainder.is_empty(),
        "both materials clear INSTANCE_MIN_PLACEMENTS on their own, neither \
         should fall back to batching: {remainder:?}"
    );
    assert_eq!(
        groups.len(),
        2,
        "two placements sharing a mesh identity but not a material must \
         form two MultiMesh groups (one shared texture per group), not one \
         group that applies the first member's diffuse texture to every \
         placement regardless of its own material"
    );
    for group in &groups {
        for member in &group.members {
            let source = &surfaces[member.surface].arrays.material;
            assert_eq!(
                source.diffuse, group.arrays.material.diffuse,
                "every member drawn by a MultiMesh group must share the \
                 group's own material, since Godot draws the whole group \
                 with one mesh and one material override"
            );
        }
    }
}

// --- 3 and 4. INSTANCE_MIN_PLACEMENTS is the actual break-even boundary ---
//
// Open question 1 in `target/task-pack/RND-04.md` names the threshold as
// tunable and untested: "Is 4 placements enough to break even on MultiMesh
// overhead?" `instancing_candidates`'s own doc says a candidate below the
// threshold "falls back to the remainder". Neither boundary of that
// `>= INSTANCE_MIN_PLACEMENTS` comparison had a test before this file.

#[test]
fn instancing_candidates_leaves_one_short_of_threshold_in_remainder() {
    let identity: Arc<str> = Arc::from("meshes\\landscape\\rockcliff02.nif#0");
    let count = INSTANCE_MIN_PLACEMENTS - 1;
    let surfaces: Vec<LoadedSurface> = (0..count as u32)
        .map(|i| {
            instanced_surface(
                &identity,
                quad_arrays("textures\\rock02.dds"),
                i,
                i as f32 * 100.0,
            )
        })
        .collect();

    let (groups, remainder) = instancing_candidates(&surfaces);

    assert!(
        groups.is_empty(),
        "{count} placements is one short of INSTANCE_MIN_PLACEMENTS \
         ({INSTANCE_MIN_PLACEMENTS}), a MultiMesh of that few instances has \
         no case for its own draw call per instancing_candidates's own doc \
         comment, so no group should form"
    );
    assert_eq!(
        remainder,
        (0..count).collect::<Vec<_>>(),
        "every under-threshold placement must fall back to the remainder, \
         in original surface order"
    );
}

#[test]
fn instancing_candidates_admits_exactly_the_threshold() {
    let identity: Arc<str> = Arc::from("meshes\\landscape\\rockcliff03.nif#0");
    let surfaces: Vec<LoadedSurface> = (0..INSTANCE_MIN_PLACEMENTS as u32)
        .map(|i| {
            instanced_surface(
                &identity,
                quad_arrays("textures\\rock03.dds"),
                i,
                i as f32 * 100.0,
            )
        })
        .collect();

    let (groups, remainder) = instancing_candidates(&surfaces);

    assert!(
        remainder.is_empty(),
        "exactly INSTANCE_MIN_PLACEMENTS placements must all join the group, \
         none held back: {remainder:?}"
    );
    assert_eq!(
        groups.len(),
        1,
        "one mesh identity makes one MultiMesh group"
    );
    assert_eq!(groups[0].members.len(), INSTANCE_MIN_PLACEMENTS);
}

// --- 5. Reference order must survive interleaving between identities ---
//
// `InstanceGroup`'s doc comment (`batch.rs`, directly above the struct) says
// `reference_of_instance` answers "in the order they were pushed, which is
// the order the Godot layer sets MultiMesh instance transforms in". A real
// cell interleaves references of different models as `stream_cell_id` walks
// its reference list, so two mesh identities' placements arrive interleaved
// in the surfaces slice, not clustered. Grouping must still keep each
// identity's own members in their own relative order.
#[test]
fn instancing_candidates_preserves_reference_order_when_identities_interleave() {
    let rock: Arc<str> = Arc::from("meshes\\landscape\\rockcliff01.nif#0");
    let tree: Arc<str> = Arc::from("meshes\\landscape\\trees\\pinetree01.nif#0");
    let mut surfaces = Vec::new();
    // Interleaved: rock, tree, rock, tree, ... four of each.
    for i in 0..INSTANCE_MIN_PLACEMENTS as u32 {
        surfaces.push(instanced_surface(
            &rock,
            quad_arrays("textures\\rock.dds"),
            i,
            i as f32 * 100.0,
        ));
        surfaces.push(instanced_surface(
            &tree,
            quad_arrays("textures\\tree.dds"),
            1000 + i,
            i as f32 * 100.0 + 5000.0,
        ));
    }

    let (groups, remainder) = instancing_candidates(&surfaces);
    assert!(remainder.is_empty(), "both identities clear the threshold");
    assert_eq!(groups.len(), 2);

    let rock_group = groups
        .iter()
        .find(|g| g.identity == rock)
        .expect("a rock group must exist");
    let tree_group = groups
        .iter()
        .find(|g| g.identity == tree)
        .expect("a tree group must exist");

    for (index, member) in rock_group.members.iter().enumerate() {
        assert_eq!(
            member.reference,
            GlobalFormId {
                file: 1,
                object: index as u32,
            },
            "rock_group's member {index} must map back to the rock \
             reference placed at that position, not a tree reference that \
             happened to interleave with it in the source slice"
        );
        assert_eq!(
            rock_group.reference_of_instance(index),
            Some(member.reference)
        );
    }
    for (index, member) in tree_group.members.iter().enumerate() {
        assert_eq!(
            member.reference,
            GlobalFormId {
                file: 1,
                object: 1000 + index as u32,
            },
            "tree_group's member {index} must map back to the tree \
             reference placed at that position"
        );
    }
}

// --- 6. Short groups fall back to the remainder, in full and in order ---
//
// `instancing_candidates`'s doc comment describes this explicitly: "A
// candidate whose identity turns out to have fewer than
// INSTANCE_MIN_PLACEMENTS members once grouped ... falls back to the
// remainder". A cell realistically mixes three kinds of surface: ones with
// no mesh identity at all (never marked as instancing candidates), an
// under-threshold identity, and an over-threshold one. The remainder must
// carry the first two kinds completely, with none of the over-threshold
// group's members leaking in and none of the short group's members lost.
#[test]
fn instancing_candidates_returns_short_groups_and_plain_surfaces_together() {
    let short: Arc<str> = Arc::from("meshes\\clutter\\urn01.nif#0");
    let full: Arc<str> = Arc::from("meshes\\landscape\\rockcliff01.nif#0");
    let mut surfaces = Vec::new();

    // index 0, 1: plain surfaces, never marked for instancing at all.
    for i in 0..2u32 {
        surfaces.push(LoadedSurface {
            reference: GlobalFormId { file: 1, object: i },
            arrays: quad_arrays("textures\\plain.dds"),
            textures: SurfaceTextures::default(),
            placement: soe_physics::Iso::IDENTITY,
            instance_transform: None,
            mesh_identity: None,
        });
    }
    // index 2, 3: an identity with only two placements, short of the
    // threshold (INSTANCE_MIN_PLACEMENTS is 4 in this codebase).
    for i in 0..2u32 {
        surfaces.push(instanced_surface(
            &short,
            quad_arrays("textures\\urn.dds"),
            10 + i,
            i as f32 * 50.0,
        ));
    }
    // index 4..8: an identity with enough placements to instance.
    for i in 0..INSTANCE_MIN_PLACEMENTS as u32 {
        surfaces.push(instanced_surface(
            &full,
            quad_arrays("textures\\rock.dds"),
            20 + i,
            i as f32 * 100.0,
        ));
    }

    let (groups, remainder) = instancing_candidates(&surfaces);

    assert_eq!(groups.len(), 1, "only the over-threshold identity groups");
    assert_eq!(groups[0].identity, full);
    assert_eq!(groups[0].members.len(), INSTANCE_MIN_PLACEMENTS);

    assert_eq!(
        remainder,
        vec![0usize, 1, 2, 3],
        "the remainder must hold the two plain surfaces and the two \
         short-group members, all in ascending original-surface order, \
         with none of the four full-group members among them"
    );
}

// --- 7. The measured win at 40 repeats must not be the only case that holds ---
//
// The RND-04 done check is "retains the faster supported path", and
// `INSTANCE_MIN_PLACEMENTS`'s own doc comment names the breaker's benchmark
// as "what tunes this against a measured p99 and memory trace". The
// existing `repeated_rock_benchmark_measures_batching_vs_instancing` only
// measures at ROCK_REPEATS = 40, comfortably above the threshold. The
// threshold itself, where the "faster path" claim actually has to hold for
// INSTANCE_MIN_PLACEMENTS to be a correct cutoff, is never measured. This
// repeats that same benchmark at exactly INSTANCE_MIN_PLACEMENTS repeats.
#[test]
fn repeated_rock_benchmark_at_threshold_still_favors_instancing() {
    const ROCK_TRIANGLES: usize = 300;
    const ITERATIONS: usize = 300;
    let repeats = INSTANCE_MIN_PLACEMENTS;

    let local = rock_geometry(ROCK_TRIANGLES);

    let batched_surfaces: Vec<LoadedSurface> = (0..repeats)
        .map(|i| {
            let mut arrays = local.clone();
            let dx = i as f32 * 300.0;
            for p in &mut arrays.positions {
                p[0] += dx;
            }
            arrays.bounds.min.x += dx;
            arrays.bounds.max.x += dx;
            LoadedSurface {
                reference: GlobalFormId {
                    file: 1,
                    object: i as u32,
                },
                arrays,
                textures: SurfaceTextures::default(),
                placement: soe_physics::Iso::IDENTITY,
                instance_transform: None,
                mesh_identity: None,
            }
        })
        .collect();

    let identity: Arc<str> = Arc::from("meshes\\landscape\\rockcliff01.nif#0");
    let instanced_surfaces: Vec<LoadedSurface> = (0..repeats)
        .map(|i| instanced_surface(&identity, local.clone(), i as u32, i as f32 * 300.0))
        .collect();

    let (batches, _) = batch_surfaces(&batched_surfaces);
    let (groups, remainder) = instancing_candidates(&instanced_surfaces);
    assert!(remainder.is_empty());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members.len(), repeats);
    assert_eq!(batches.len(), 1);

    let batch_bytes: usize = batches.iter().map(|b| buffer_bytes(&b.arrays)).sum();
    let instance_bytes: usize = buffer_bytes(&groups[0].arrays)
        + groups[0].members.len() * std::mem::size_of::<GodotTransform>();

    let batch_times = timings(ITERATIONS, || {
        std::hint::black_box(batch_surfaces(&batched_surfaces));
    });
    let instance_times = timings(ITERATIONS, || {
        std::hint::black_box(instancing_candidates(&instanced_surfaces));
    });
    let batch_p50 = percentile(&batch_times, 0.50);
    let instance_p50 = percentile(&instance_times, 0.50);

    eprintln!(
        "threshold bench ({repeats} repeats): batched {batch_bytes} bytes, \
         instanced {instance_bytes} bytes; batch p50 {batch_p50:?}, \
         instance p50 {instance_p50:?}"
    );

    assert!(
        instance_bytes < batch_bytes,
        "at exactly INSTANCE_MIN_PLACEMENTS ({repeats}) repeats, instancing \
         must still cost less memory than batching, the same claim the \
         done check makes at 40 repeats: instanced {instance_bytes} bytes, \
         batched {batch_bytes} bytes"
    );
    assert!(
        instance_p50 <= batch_p50,
        "at exactly INSTANCE_MIN_PLACEMENTS ({repeats}) repeats, \
         instancing_candidates must not cost more CPU time than \
         batch_surfaces, or the constant is set past the point where the \
         benchmark's own conclusion holds: instance p50 {instance_p50:?}, \
         batch p50 {batch_p50:?}"
    );
}

fn rock_geometry(tris: usize) -> SurfaceArrays {
    let verts = tris + 2;
    let mut positions = Vec::with_capacity(verts);
    let mut uvs = Vec::with_capacity(verts);
    for i in 0..verts {
        let t = i as f32 * 0.41;
        positions.push([t.cos() * 50.0, t.sin() * 50.0, (i as f32) * 1.2]);
        uvs.push([0.0, 0.0]);
    }
    let mut indices = Vec::with_capacity(tris * 3);
    for i in 0..tris as u32 {
        indices.extend_from_slice(&[i, i + 1, i + 2]);
    }
    let data = MeshData {
        positions,
        normals: None,
        tangents: None,
        uvs: Some(uvs),
        indices,
        colors: None,
        bone_indices: None,
        bone_weights: None,
        material: MaterialDesc {
            diffuse: Some("textures\\landscape\\rockcliff01.dds".to_string()),
            ..MaterialDesc::default()
        },
    };
    build_surface(&data, MeshOptions::default()).unwrap()
}

fn buffer_bytes(arrays: &SurfaceArrays) -> usize {
    arrays.positions.len() * std::mem::size_of::<[f32; 3]>()
        + arrays.normals.len() * std::mem::size_of::<[f32; 3]>()
        + arrays.tangents.len() * std::mem::size_of::<f32>()
        + arrays.uvs.len() * std::mem::size_of::<[f32; 2]>()
        + arrays.indices.len() * std::mem::size_of::<i32>()
        + arrays.bones.len() * std::mem::size_of::<i32>()
        + arrays.weights.len() * std::mem::size_of::<f32>()
}

fn timings<F: FnMut()>(iterations: usize, mut f: F) -> Vec<Duration> {
    let mut out = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        f();
        out.push(start.elapsed());
    }
    out.sort_unstable();
    out
}

fn percentile(sorted: &[Duration], pct: f64) -> Duration {
    let last = sorted.len() - 1;
    let idx = ((last as f64) * pct).round() as usize;
    sorted[idx.min(last)]
}
