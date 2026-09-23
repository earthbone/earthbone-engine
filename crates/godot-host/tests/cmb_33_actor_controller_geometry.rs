// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-33 end to end: `DataSource::actor_controller_shape` against installed
//! skeletons, fed a scale computed from real RACE/NPC_ records through
//! `World::actor_controller_scale`.
//!
//! `crates/physics/tests/cmb_33_generated_controller.rs` proves the capsule
//! generator in isolation and `crates/game/tests/cmb_33_actor_controller_scale.rs`
//! proves the scale formula in isolation. This ties both to a real `Data`
//! folder: a skeleton without an authored `CHAR_CONTROLLER` phantom must
//! resolve to a generated capsule sized by a real actor's scale, and a
//! skeleton with one (giant, confirmed by CMB-29) must resolve to that
//! phantom instead, unaffected by scale.
//!
//! Runs only when `SKYRIM_DATA` points at the game's `Data` folder:
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test cmb_33_actor_controller_geometry -- --nocapture
//! ```

use std::path::PathBuf;

use soe_esm::{GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::World;
use soe_godot_host::data_source::DataSource;
use soe_physics::{generated_controller_shape, Iso};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

/// The same five-master load order `actor_templates.rs`'s installed survey
/// uses, built independently from `DataSource::open`'s own `LoadOrder` so
/// this test proves the scale side end to end rather than reaching into the
/// source's private state. Returns the world plus every non-deleted `NPC_`
/// base, collected before the order moves into `World::new`.
fn installed_world(data: &std::path::Path) -> (World, Vec<GlobalFormId>) {
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let bases: Vec<_> = order
        .winners_of(T(b"NPC_"))
        .filter(|(_, record)| !record.is_deleted())
        .map(|(id, _)| id)
        .collect();
    assert!(!bases.is_empty(), "expected installed NPC_ records");
    (World::new(order), bases)
}

/// A finite, positive `actor_controller_scale` from a real NPC_, so the
/// generated-capsule assertions below exercise a genuine scale rather than
/// the 1.0 fallback every failure would also produce.
fn a_real_scale(world: &World, bases: &[GlobalFormId]) -> (GlobalFormId, f32) {
    for &base in bases {
        if let Ok(scale) = world.actor_controller_scale(base) {
            return (base, scale);
        }
    }
    panic!("no installed NPC_ resolved a valid actor controller scale");
}

#[test]
fn cmb_33_a_skeleton_without_an_authored_controller_gets_a_generated_capsule_at_a_real_scale() {
    let Some(data) = data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let (world, bases) = installed_world(&data);
    let (base, scale) = a_real_scale(&world, &bases);
    eprintln!("scale {scale} from NPC_ {base:?}");
    assert!(scale.is_finite() && scale > 0.0);

    let source = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}"));
    let mut checked_without = 0;
    let mut checked_with = 0;
    for name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&data.join(name)).unwrap();
        for path in archive.files().iter().filter(|path| {
            path.contains("actors/")
                && path.contains("skeleton")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("nif"))
        }) {
            let file = soe_nif::parse(&archive.read(path).unwrap()).unwrap();
            let collected = soe_physics::collect(&file).unwrap_or_else(|e| panic!("{path}: {e}"));
            let (iso, shape) = source
                .actor_controller_shape(path, scale)
                .unwrap_or_else(|e| panic!("actor_controller_shape({path}): {e}"));
            if collected.phantoms.is_empty() {
                // No authored controller: the resolver must fall back to the
                // generated capsule, at this scale exactly, identity placed.
                assert_eq!(
                    iso,
                    Iso::IDENTITY,
                    "{path}: generated shape must be identity placed"
                );
                assert_eq!(
                    shape,
                    generated_controller_shape(scale),
                    "{path}: generated shape must match the scale it was given"
                );
                checked_without += 1;
            } else {
                // Authored: must return the largest phantom's own transform
                // and shape, unaffected by the scale argument.
                let extent = |s: &soe_physics::Shape| {
                    let a = s.aabb(&Iso::IDENTITY);
                    (a.max - a.min).length()
                };
                let largest = collected
                    .phantoms
                    .iter()
                    .max_by(|a, b| extent(&a.shape).total_cmp(&extent(&b.shape)))
                    .unwrap();
                assert_eq!(
                    iso, largest.transform,
                    "{path}: authored transform mismatch"
                );
                assert_eq!(shape, largest.shape, "{path}: authored shape mismatch");
                assert_ne!(
                    shape,
                    generated_controller_shape(scale),
                    "{path}: an authored phantom must not coincide with the generated fallback"
                );
                checked_with += 1;
            }
        }
    }
    eprintln!("CMB33: {checked_without} generated, {checked_with} authored");
    assert!(
        checked_without > 0,
        "expected at least one installed skeleton with no authored controller"
    );
    assert!(
        checked_with > 0,
        "expected at least one installed skeleton with an authored controller (giant, dragon)"
    );
}
