// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-33 adversarial: `DataSource::actor_controller_shape` against the
//! failure modes its own doc comment claims to handle ("Propagates a
//! missing or unparsable skeleton mesh", `crates/godot-host/src/collision.rs`)
//! and against repeated open and close cycles, per the breaker's own aim
//! list. `crates/godot-host/tests/cmb_33_actor_controller_geometry.rs`
//! already proves the happy path against every installed skeleton, so this
//! stays off that path: a resource that does not exist, a resource that
//! exists but is not a skeleton, and the same resource read across several
//! independently opened `DataSource`s.
//!
//! Runs only when `SKYRIM_DATA` points at the game's `Data` folder.

use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

/// A real skeleton path known to lack an authored `CHAR_CONTROLLER`, used by
/// the repeated open/close check below. Sourced the same way
/// `cmb_33_actor_controller_geometry.rs` finds one: any `skeleton*.nif`
/// under `actors/` in the meshes archives whose collected phantoms are
/// empty. Falls back to `None` (test skips its own body) rather than
/// hard-coding a path that could move between game versions.
fn a_skeleton_without_authored_controller(data: &std::path::Path) -> Option<String> {
    for name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&data.join(name)).ok()?;
        for path in archive.files().iter().filter(|path| {
            path.contains("actors/")
                && path.contains("skeleton")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("nif"))
        }) {
            let Ok(bytes) = archive.read(path) else {
                continue;
            };
            let Ok(file) = soe_nif::parse(&bytes) else {
                continue;
            };
            let Ok(collected) = soe_physics::collect(&file) else {
                continue;
            };
            if collected.phantoms.is_empty() {
                return Some(path.clone());
            }
        }
    }
    None
}

/// A real archived file that is not a NIF at all, so `soe_nif::parse` must
/// reject it rather than the resolver misreading it as an empty skeleton.
fn a_non_nif_resource(data: &std::path::Path) -> Option<String> {
    let archive = soe_bsa::Archive::open(&data.join("Skyrim - Textures0.bsa")).ok()?;
    archive
        .files()
        .iter()
        .find(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dds"))
        })
        .cloned()
}

#[test]
fn cmb_33_actor_controller_shape_rejects_a_missing_resource_adversarial() {
    let Some(data) = data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let source = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}"));
    let result = source.actor_controller_shape("meshes/actors/does/not/exist.nif", 1.0);
    assert!(
        result.is_err(),
        "a skeleton path absent from every archive and loose override must return Err, got {result:?}"
    );
}

#[test]
fn cmb_33_actor_controller_shape_rejects_a_non_skeleton_resource_adversarial() {
    let Some(data) = data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let Some(texture) = a_non_nif_resource(&data) else {
        eprintln!("no .dds found in Skyrim - Textures0.bsa, skipping");
        return;
    };
    let source = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}"));
    let result = source.actor_controller_shape(&texture, 1.0);
    assert!(
        result.is_err(),
        "{texture}: a real but non-NIF resource must fail to parse, not be treated as an emptied skeleton, got {result:?}"
    );
}

#[test]
fn cmb_33_actor_controller_shape_is_stable_across_repeated_open_close_cycles_adversarial() {
    let Some(data) = data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let Some(skeleton) = a_skeleton_without_authored_controller(&data) else {
        eprintln!("no unauthored skeleton found, skipping");
        return;
    };
    let mut results = Vec::new();
    for _ in 0..8 {
        let source = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}"));
        let (iso, shape) = source
            .actor_controller_shape(&skeleton, 1.35)
            .unwrap_or_else(|e| panic!("actor_controller_shape({skeleton}): {e}"));
        results.push((iso, shape));
    }
    let (first_iso, first_shape) = &results[0];
    for (i, (iso, shape)) in results.iter().enumerate().skip(1) {
        assert_eq!(
            iso, first_iso,
            "{skeleton}: open/close cycle {i} placed the capsule differently than cycle 0"
        );
        assert_eq!(
            shape, first_shape,
            "{skeleton}: open/close cycle {i} sized the capsule differently than cycle 0"
        );
    }
}
