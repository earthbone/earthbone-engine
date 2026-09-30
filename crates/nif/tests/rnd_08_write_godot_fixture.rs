// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Writes the on-disk `.nif` fixture the RND-08 Godot check
//! (`godot/rnd_08_test.gd`) loads through `AnimatedStaticNode::load_nif`.
//!
//! Run explicitly before the Godot check, this crate's test harness does not
//! order itself against another crate's tests:
//! `cargo test -p soe-nif --test rnd_08_write_godot_fixture`.
//!
//! The scene is a root over "Target", an `NiNode` carrying an
//! `NiTransformController` (Loop, `[0, 10]`) over an `NiTransformInterpolator`
//! and an `NiTransformData` moving translation from the origin to
//! `(100, 0, 0)` and rotation from identity to 90 degrees around Skyrim Z,
//! matching the track `crates/nif/tests/rnd_08_transform_controller_check.rs`
//! already proves the math against. "Target" has one `BSTriShape` child, a
//! unit quad offset to `(2, 0, 0)` in Target's own local space, so the
//! check script can tell the controller's live transform apart from the
//! mesh's own static placement.

mod common;

use common::{quad, NifWriter};
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/nif. The fixture is host-check scratch
    // output, not a workspace source file, so it goes under the worktree's
    // own target/ tree next to the disposable Godot project it feeds.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/godot-check/fixtures/rnd_08_animated_static.nif")
}

#[test]
fn rnd_08_write_godot_fixture() {
    let mut w = NifWriter::new(100);

    let (vertices, triangles) = quad();
    let shape = w.bs_tri_shape(
        "Shape",
        [2.0, 0.0, 0.0],
        -1,
        -1,
        -1,
        soe_nif::vertex::attr::VERTEX | soe_nif::vertex::attr::UVS | soe_nif::vertex::attr::NORMALS,
        &vertices,
        &triangles,
        true,
    );
    let shape = w.add("BSTriShape", shape);

    let half_angle = std::f32::consts::FRAC_PI_4;
    let data = w.transform_data_linear(
        &[
            (0.0, [0.0, 0.0, 0.0, 1.0]),
            (10.0, [0.0, 0.0, half_angle.sin(), half_angle.cos()]),
        ],
        &[(0.0, [0.0, 0.0, 0.0]), (10.0, [100.0, 0.0, 0.0])],
        &[],
    );
    let data = w.add("NiTransformData", data);

    let interp = w.transform_interpolator([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], 1.0, data);
    let interp = w.add("NiTransformInterpolator", interp);

    // Bit 3 (0x8) active, bits 1 to 2 clear select CycleType::Loop.
    const ACTIVE_LOOP: u16 = 0x8;
    let controller = w.transform_controller(-1, ACTIVE_LOOP, 1.0, 0.0, 0.0, 10.0, -1, interp);
    let controller = w.add("NiTransformController", controller);

    let target = w.ni_node_with_controller(
        "Target",
        [0.0; 3],
        common::IDENTITY,
        1.0,
        &[shape],
        controller,
    );
    let target = w.add("NiNode", target);

    let root = w.ni_node("Root", [0.0; 3], common::IDENTITY, 1.0, &[target]);
    let root = w.add("NiNode", root);
    w.roots = vec![root];

    let bytes = w.finish();

    // Sanity check the writer's own output through the reader before it
    // becomes an opaque fixture a GDScript can only fail against, not debug.
    let file = soe_nif::parse(&bytes).expect("the fixture parses");
    let node = soe_godot_host_fixture_check::find_target(&file);
    assert!(node.is_some(), "the fixture's own Target node must resolve");

    let path = fixture_path();
    std::fs::create_dir_all(path.parent().expect("fixture path has a parent"))
        .expect("create fixture dir");
    std::fs::write(&path, &bytes).expect("write the fixture");
    println!("wrote {} bytes to {}", bytes.len(), path.display());
}

/// A stand-in for `soe_godot_host::animated_static::find_node`, duplicated
/// rather than depended on: `nif` sits below `godot-host` in the crate
/// graph, and this is a two line depth first search over data this test
/// already built, not a re-test of the host's own resolver (covered in
/// `crates/godot-host/src/animated_static.rs`'s own tests instead).
mod soe_godot_host_fixture_check {
    use soe_nif::blocks::Block;
    use soe_nif::{BlockRef, NifFile};

    pub fn find_target(file: &NifFile) -> Option<BlockRef> {
        for &root in &file.roots {
            let idx = root.get()?;
            let Block::Node(n) = file.blocks.get(idx)? else {
                continue;
            };
            for &child in &n.children {
                let cidx = child.get()?;
                if let Some(Block::Node(cn)) = file.blocks.get(cidx) {
                    if cn.av.name() == "Target" {
                        return Some(child);
                    }
                }
            }
        }
        None
    }
}
