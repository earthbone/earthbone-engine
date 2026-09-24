// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-04 planned check: "Disabling or moving a reference updates rendering
//! and collision consistently at the next host tick."
//!
//! Both host consumers read the same value: `godot_layer.rs`'s
//! `CellStreamerNode::sync_reference_changes` and `godot_physics.rs`'s
//! `CellBodies::sync_placements` each call `World::ref_placement(reference)`
//! once a tick and apply exactly what it returns, one to a batch's baked
//! vertices, the other to a physics body's space and transform
//! (`docs/godot-host.md`, "Runtime enable and placement reflection, HST-04").
//! Neither consumer can be constructed here: both need a live
//! `RenderingServer` and `PhysicsServer3D`, which only exist inside a running
//! Godot process, so `sync_reference_changes` and `sync_placements` stay
//! feature-gated behind `godot` and are exercised by a headless Godot
//! acceptance script instead. What this test proves at its source is the
//! single value both of them read: that `World::ref_placement` reflects a
//! disable, a re-enable and a move consistently, so a host applying it
//! faithfully cannot help but keep rendering and collision in agreement,
//! since they are never told two different things.
//!
//! `Disable`/`Enable` field writes match `crates/game/src/natives.rs:456`
//! to `469`. `World::move_to` (`crates/game/src/world/refs.rs:374` to `384`)
//! is `MoveTo`'s own implementation, called directly rather than reproduced.

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{RefPlacement, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x210;
const REF: u32 = 0x320;
const ANCHOR: u32 = 0x330;
const BASE: u32 = 0x410;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed_reference(object: u32, position: [f32; 3], rotation: [f32; 3]) -> Record {
    let transform = Transform { position, rotation };
    let mut record = Record::new(T(b"REFR"), object)
        .sub(T(b"NAME"), BASE.to_le_bytes())
        .sub(T(b"DATA"), transform.to_bytes());
    record.placement.cell = Some(CELL);
    record
}

fn build_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"STAT"), BASE));
    plugin.push_top(Record::new(T(b"CELL"), CELL));
    plugin.push_top(placed_reference(REF, [1.0, 2.0, 3.0], [0.0, 0.0, 0.0]));
    plugin.push_top(placed_reference(ANCHOR, [50.0, 0.0, 0.0], [0.0, 0.0, 0.0]));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    World::new(order)
}

/// `Disable()`/`Enable()`'s own field write, `crates/game/src/natives.rs:456`
/// to `469`.
fn set_enabled(world: &mut World, reference: GlobalFormId, on: bool) {
    world.reference(reference).unwrap().enabled = on;
}

#[test]
fn disabling_a_reference_reports_a_hide_at_its_unchanged_placement() {
    let mut world = build_world();
    let reference = id(REF);

    // Touch once, the way a host does on first seeing the cell.
    world.reference(reference).unwrap();
    let baseline = world.ref_placement(reference).unwrap();
    assert!(
        baseline.visible,
        "a freshly loaded reference starts visible"
    );
    assert_eq!(baseline.position, [1.0, 2.0, 3.0]);

    set_enabled(&mut world, reference, false);
    let disabled = world.ref_placement(reference).unwrap();
    assert_eq!(
        disabled,
        RefPlacement {
            visible: false,
            ..baseline
        },
        "Disable() must hide the reference without moving it, so a host \
         collapses its mesh and pulls its body from the physics space at \
         its last known placement, not a stale or zeroed one"
    );

    set_enabled(&mut world, reference, true);
    let reenabled = world.ref_placement(reference).unwrap();
    assert_eq!(
        reenabled, baseline,
        "Enable() must restore exactly the pre-disable placement, so a host \
         puts the body back at the same transform it removed rather than a \
         drifted one"
    );
}

#[test]
fn moving_a_reference_reports_the_new_placement_while_staying_visible() {
    let mut world = build_world();
    let reference = id(REF);
    let anchor = id(ANCHOR);

    world.reference(reference).unwrap();
    let before = world.ref_placement(reference).unwrap();
    assert!(before.visible);

    let moved = world.move_to(reference, anchor, [4.0, 5.0, 6.0]);
    assert!(moved, "move_to must succeed against two loaded references");

    let after = world.ref_placement(reference).unwrap();
    assert_eq!(
        after,
        RefPlacement {
            position: [54.0, 5.0, 6.0],
            visible: true,
            ..before
        },
        "MoveTo must land at the anchor's position plus the offset and \
         leave the reference visible, so a host rebuilds its rendered \
         vertex range and its collision body at the new transform rather \
         than treating a plain move as a hide"
    );
}

#[test]
fn a_disable_then_move_then_enable_sequence_ends_at_the_moved_placement_visible() {
    // Exercises both clauses of the done check together and in the order a
    // script is free to issue them: nothing in `World::ref_placement` may
    // assume disable and move are mutually exclusive or ordered.
    let mut world = build_world();
    let reference = id(REF);
    let anchor = id(ANCHOR);
    world.reference(reference).unwrap();

    set_enabled(&mut world, reference, false);
    assert!(!world.ref_placement(reference).unwrap().visible);

    let moved = world.move_to(reference, anchor, [1.0, 0.0, 0.0]);
    assert!(moved);
    let while_disabled = world.ref_placement(reference).unwrap();
    assert_eq!(
        while_disabled.position,
        [51.0, 0.0, 0.0],
        "a move applied while disabled must still update the placement a \
         host will use once the reference is enabled again"
    );
    assert!(
        !while_disabled.visible,
        "a move alone must not implicitly re-enable a disabled reference"
    );

    set_enabled(&mut world, reference, true);
    let result = world.ref_placement(reference).unwrap();
    assert_eq!(
        result,
        RefPlacement {
            visible: true,
            ..while_disabled
        },
        "re-enabling must reveal the reference at the placement the move \
         left it at, not the placement it was disabled at"
    );
}
