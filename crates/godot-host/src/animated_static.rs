// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain logic for driving a NIF authored transform controller against a
//! parsed file, with no Godot dependency. See `godot_animated_static.rs` for
//! the thin GDExtension wrapper that calls this every frame.
//!
//! Skyrim SE's animated statics carry an `NiTransformController` directly on
//! the animated `NiNode`'s `ObjectNet.controller` ref, over an
//! `NiTransformInterpolator` and an optional `NiTransformData` key track
//! (`crates/nif/src/controller.rs`). This module resolves that chain by node
//! name and evaluates it each frame through
//! `soe_nif::controller::evaluate_transform`, which already carries the
//! track math and cycle mode handling (RND-08's own check test,
//! `crates/nif/tests/rnd_08_transform_controller_check.rs`). What was
//! missing before this row is a host path that calls it at all.

use soe_nif::blocks::Block;
use soe_nif::controller::{self, TimeController, TransformData, TransformInterpolator};
use soe_nif::types::Transform as NifTransform;
use soe_nif::{BlockRef, NifFile};

/// Why a node's controller chain could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveError {
    /// No node with the requested name exists under the file's roots.
    NodeNotFound,
    /// The node exists but its `ObjectNet.controller` ref is empty or is not
    /// a `NiTransformController` / `NiKeyframeController`. A chain through
    /// an intermediate non-transform controller is not walked: only
    /// `TransformController` exposes a `next_controller` field in this
    /// codebase, and every animated static sampled so far carries exactly
    /// one controller directly on the node.
    NoTransformController,
    /// The controller's `interpolator` ref is empty or is not an
    /// `NiTransformInterpolator`.
    NoInterpolator,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            ResolveError::NodeNotFound => "node not found",
            ResolveError::NoTransformController => "node has no NiTransformController",
            ResolveError::NoInterpolator => "controller has no NiTransformInterpolator",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for ResolveError {}

/// A resolved, ready to evaluate transform controller: the time controller
/// (frequency, phase, cycle mode), the interpolator's default pose, and the
/// key track data when the interpolator has one.
pub struct AnimatedController {
    pub time: TimeController,
    pub interpolator: TransformInterpolator,
    pub data: Option<TransformData>,
}

/// Depth first search under `file`'s roots for a node named exactly `name`.
/// Mirrors `mesh::Walker::walk`'s own cycle guard: a NIF's child refs form a
/// tree in every sample seen, but nothing in the format forbids a cycle, so
/// the search bails out past a depth of 256 rather than looping forever.
pub fn find_node(file: &NifFile, name: &str) -> Option<BlockRef> {
    fn visit(file: &NifFile, r: BlockRef, name: &str, depth: u32) -> Option<BlockRef> {
        if depth > 256 {
            return None;
        }
        let idx = r.get()?;
        match file.blocks.get(idx)? {
            Block::Node(n) => {
                if n.av.name() == name {
                    return Some(r);
                }
                for &child in &n.children {
                    if let Some(found) = visit(file, child, name, depth + 1) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }
    for &root in &file.roots {
        if let Some(found) = visit(file, root, name, 0) {
            return Some(found);
        }
    }
    None
}

/// Resolve `node`'s controller chain: `ObjectNet.controller` must directly be
/// a `NiTransformController` (or `NiKeyframeController`) whose interpolator
/// is an `NiTransformInterpolator`. The interpolator's `data` ref is
/// optional, an animated static with no data block just holds the
/// interpolator's default pose forever (`controller::evaluate_transform`
/// falls back to it per channel).
pub fn resolve_controller(
    file: &NifFile,
    node: BlockRef,
) -> Result<AnimatedController, ResolveError> {
    let idx = node.get().ok_or(ResolveError::NodeNotFound)?;
    let Some(Block::Node(n)) = file.blocks.get(idx) else {
        return Err(ResolveError::NodeNotFound);
    };
    let ctrl_idx =
        n.av.net
            .controller
            .get()
            .ok_or(ResolveError::NoTransformController)?;
    let Some(Block::TransformController(tc)) = file.blocks.get(ctrl_idx) else {
        return Err(ResolveError::NoTransformController);
    };
    let interp_idx = tc.interpolator.get().ok_or(ResolveError::NoInterpolator)?;
    let Some(Block::TransformInterpolator(interp)) = file.blocks.get(interp_idx) else {
        return Err(ResolveError::NoInterpolator);
    };
    let data = interp
        .data
        .get()
        .and_then(|i| file.blocks.get(i))
        .and_then(|b| match b {
            Block::TransformData(d) => Some(d.clone()),
            _ => None,
        });
    Ok(AnimatedController {
        time: tc.time,
        interpolator: *interp,
        data,
    })
}

/// Whether `controller` should be evaluated this frame, per its own `active`
/// flag.
pub fn is_active(controller: &AnimatedController) -> bool {
    controller::is_active(&controller.time)
}

/// The node's local transform at `elapsed` seconds of game time.
pub fn evaluate(controller: &AnimatedController, elapsed: f32) -> NifTransform {
    controller::evaluate_transform(
        &controller.time,
        &controller.interpolator,
        controller.data.as_ref(),
        elapsed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_nif::blocks::{AvObject, Node, ObjectNet};
    use soe_nif::controller::{ControllerFlags, CycleType};
    use soe_nif::types::{Mat33, Vec3};

    fn plain_node(name: &str, controller: BlockRef, children: Vec<BlockRef>) -> Block {
        Block::Node(Node {
            av: AvObject {
                net: ObjectNet {
                    name: name.to_string(),
                    extra_data: Vec::new(),
                    controller,
                },
                flags: 0,
                transform: NifTransform {
                    rotation: Mat33::IDENTITY,
                    translation: Vec3::ZERO,
                    scale: 1.0,
                },
                collision: BlockRef::NONE,
            },
            children,
            effects: Vec::new(),
        })
    }

    /// A minimal file: root at 0 has child 1, child 1 ("Target") carries the
    /// controller at block 2, over the interpolator at block 3.
    fn file_with_controller(target_data: BlockRef) -> NifFile {
        file_with_controller_cycle(target_data, CycleType::Loop)
    }

    fn file_with_controller_cycle(target_data: BlockRef, cycle_type: CycleType) -> NifFile {
        let time = TimeController {
            next_controller: BlockRef::NONE,
            flags: ControllerFlags {
                cycle_type,
                active: true,
                ..Default::default()
            },
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 10.0,
            target: BlockRef::NONE,
        };
        let blocks = vec![
            plain_node("Root", BlockRef::NONE, vec![BlockRef(1)]),
            plain_node("Target", BlockRef(2), Vec::new()),
            Block::TransformController(controller::TransformController {
                time,
                interpolator: BlockRef(3),
            }),
            Block::TransformInterpolator(TransformInterpolator {
                default_value: controller::QuatTransform::default(),
                data: target_data,
            }),
        ];
        NifFile {
            header: soe_nif::Header {
                version: soe_nif::header::VERSION_20_2_0_7,
                user_version: 12,
                bs_version: soe_nif::header::BS_SKYRIM_SE,
                author: String::new(),
                process_script: String::new(),
                export_script: String::new(),
                max_filepath: String::new(),
                block_types: Vec::new(),
                block_type_index: Vec::new(),
                block_sizes: Vec::new(),
                strings: Vec::new(),
                groups: Vec::new(),
            },
            blocks,
            roots: vec![BlockRef(0)],
        }
    }

    #[test]
    fn rnd_08_find_node_locates_a_nested_animated_node() {
        let file = file_with_controller(BlockRef::NONE);
        assert_eq!(find_node(&file, "Target"), Some(BlockRef(1)));
        assert_eq!(find_node(&file, "Nope"), None);
    }

    #[test]
    fn rnd_08_resolve_controller_reads_the_chain_from_the_node() {
        let file = file_with_controller(BlockRef::NONE);
        let node = find_node(&file, "Target").expect("node present");
        let resolved = resolve_controller(&file, node).expect("controller resolves");
        assert!(is_active(&resolved));
        assert_eq!(resolved.time.stop_time, 10.0);
    }

    #[test]
    fn rnd_08_resolve_controller_reports_a_missing_controller() {
        let file = file_with_controller(BlockRef::NONE);
        // "Root" carries no controller ref at all.
        let node = find_node(&file, "Root").expect("node present");
        assert!(matches!(
            resolve_controller(&file, node),
            Err(ResolveError::NoTransformController)
        ));
    }

    /// `file_with_controller_cycle` plus a real `NiTransformData` block (at
    /// index 4) moving translation from the origin to `(100, 0, 0)` over
    /// `[0, 10]`, the same track shape `crates/nif/tests/rnd_08_transform_controller_check.rs`
    /// proves the underlying math against. This checks the host's own
    /// `evaluate` reaches that math through `resolve_controller`, not the
    /// nif crate's `evaluate_transform` directly.
    fn file_with_moving_controller(cycle_type: CycleType) -> NifFile {
        let mut file = file_with_controller_cycle(BlockRef(4), cycle_type);
        let data = TransformData {
            rotations: soe_nif::controller::RotationKeys::None,
            translations: soe_nif::controller::KeyGroup {
                interpolation: soe_nif::controller::Interpolation::Linear,
                keys: vec![
                    soe_nif::controller::Key {
                        time: 0.0,
                        value: Vec3::ZERO,
                        in_tan: Vec3::ZERO,
                        out_tan: Vec3::ZERO,
                        tbc: soe_nif::controller::Tbc::default(),
                    },
                    soe_nif::controller::Key {
                        time: 10.0,
                        value: Vec3::new(100.0, 0.0, 0.0),
                        in_tan: Vec3::ZERO,
                        out_tan: Vec3::ZERO,
                        tbc: soe_nif::controller::Tbc::default(),
                    },
                ],
            },
            scales: soe_nif::controller::KeyGroup::default(),
        };
        file.blocks.push(Block::TransformData(data));
        file
    }

    /// Host layer clause 1: `evaluate` moves the target along its track as
    /// elapsed game time advances, resolved through `resolve_controller`
    /// from the node, not by calling `controller::evaluate_transform`
    /// directly.
    #[test]
    fn rnd_08_evaluate_moves_the_target_along_its_track() {
        let file = file_with_moving_controller(CycleType::Clamp);
        let node = find_node(&file, "Target").expect("node present");
        let resolved = resolve_controller(&file, node).expect("controller resolves");

        let start = evaluate(&resolved, 0.0);
        let mid = evaluate(&resolved, 5.0);
        let end = evaluate(&resolved, 10.0);
        assert_eq!(start.translation, Vec3::ZERO);
        assert_eq!(mid.translation, Vec3::new(50.0, 0.0, 0.0));
        assert_eq!(end.translation, Vec3::new(100.0, 0.0, 0.0));
        assert!(start.translation.x < mid.translation.x);
        assert!(mid.translation.x < end.translation.x);
    }

    /// Host layer clause 2: a `Loop` cycle wraps `evaluate` back into
    /// `[start_time, stop_time]` past `stop_time`, resolved the same way.
    #[test]
    fn rnd_08_evaluate_respects_loop_cycle_wrap() {
        let file = file_with_moving_controller(CycleType::Loop);
        let node = find_node(&file, "Target").expect("node present");
        let resolved = resolve_controller(&file, node).expect("controller resolves");

        // game_time 13 is one full 10 second cycle plus 3, so it must land
        // on the same pose as game_time 3, and differ from the held end
        // pose a Clamp controller would show instead.
        let wrapped = evaluate(&resolved, 13.0);
        let direct = evaluate(&resolved, 3.0);
        let at_end = evaluate(&resolved, 10.0);
        assert!((wrapped.translation.x - direct.translation.x).abs() < 1e-3);
        assert_ne!(wrapped.translation, at_end.translation);
    }
}
