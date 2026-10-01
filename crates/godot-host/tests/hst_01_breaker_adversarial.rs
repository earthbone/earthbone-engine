// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial NIF controller selection cases for HST-01.

use soe_godot_host::animated_static::{self, ResolveError};
use soe_nif::blocks::{AvObject, Block, Node, ObjectNet};
use soe_nif::controller::{ControllerFlags, CycleType, TransformController, TransformInterpolator};
use soe_nif::types::{BlockRef, Mat33, Vec3};
use soe_nif::{Header, NifFile};

fn node(name: &str, controller: BlockRef, children: Vec<BlockRef>) -> Block {
    Block::Node(Node {
        av: AvObject {
            net: ObjectNet {
                name: name.to_owned(),
                extra_data: Vec::new(),
                controller,
            },
            flags: 0,
            transform: soe_nif::types::Transform {
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

fn controller(interpolator: BlockRef) -> Block {
    Block::TransformController(TransformController {
        time: soe_nif::controller::TimeController {
            next_controller: BlockRef::NONE,
            flags: ControllerFlags {
                cycle_type: CycleType::Clamp,
                active: true,
                ..Default::default()
            },
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 1.0,
            target: BlockRef::NONE,
        },
        interpolator,
    })
}

fn interpolator() -> Block {
    Block::TransformInterpolator(TransformInterpolator {
        default_value: soe_nif::controller::QuatTransform::default(),
        data: BlockRef::NONE,
    })
}

fn file(blocks: Vec<Block>, roots: Vec<BlockRef>) -> NifFile {
    NifFile {
        header: Header {
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
        roots,
    }
}

#[test]
fn hst_01_skips_bad_controller_chain_and_finds_later_supported_node() {
    let nif = file(
        vec![
            node("Root", BlockRef::NONE, vec![BlockRef(1)]),
            node("Bad", BlockRef(2), vec![BlockRef(3)]),
            // A controller with no interpolator is not a playable door swing.
            controller(BlockRef::NONE),
            node("Good", BlockRef(5), Vec::new()),
            interpolator(),
            controller(BlockRef(6)),
            interpolator(),
        ],
        vec![BlockRef(0)],
    );

    assert_eq!(animated_static::find_animated_node(&nif), Some(BlockRef(3)));
}

#[test]
fn hst_01_rejects_out_of_range_interpolator_without_panicking() {
    let nif = file(
        vec![
            node("Bad", BlockRef(1), Vec::new()),
            controller(BlockRef(99)),
        ],
        vec![BlockRef(0)],
    );

    assert_eq!(animated_static::find_animated_node(&nif), None);
    assert!(matches!(
        animated_static::resolve_controller(&nif, BlockRef(0)),
        Err(ResolveError::NoInterpolator)
    ));
}

#[test]
fn hst_01_selects_first_supported_root_in_file_order() {
    let nif = file(
        vec![
            node("First", BlockRef(2), Vec::new()),
            node("Second", BlockRef(4), Vec::new()),
            controller(BlockRef(3)),
            interpolator(),
            controller(BlockRef(5)),
            interpolator(),
        ],
        vec![BlockRef(0), BlockRef(1)],
    );

    assert_eq!(animated_static::find_animated_node(&nif), Some(BlockRef(0)));
}

#[test]
fn hst_01_cyclic_node_children_terminate_when_no_controller_is_playable() {
    let nif = file(
        vec![node("Cycle", BlockRef::NONE, vec![BlockRef(0)])],
        vec![BlockRef(0)],
    );

    assert_eq!(animated_static::find_animated_node(&nif), None);
}

#[test]
fn hst_01_skips_invalid_root_and_finds_later_supported_controller() {
    let nif = file(
        vec![
            node("Good", BlockRef(1), Vec::new()),
            controller(BlockRef(2)),
            interpolator(),
        ],
        vec![BlockRef(99), BlockRef(0)],
    );

    assert_eq!(animated_static::find_animated_node(&nif), Some(BlockRef(0)));
}
