// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-04 adversarial tests: a deleted reference must still reach the host's
//! per tick sync as an explicit hide, never as silence.
//!
//! `docs/godot-host.md`, "Runtime enable and placement reflection, HST-04"
//! (around lines 572 to 645), states the contract both host consumers rely
//! on: "the host reads `World::ref_placement` every tick" (docs/godot-host.md:582)
//! for "a reference `World` already knows about, moved or hidden by a
//! script, activation or the editor" (docs/godot-host.md:641). `Delete()` is
//! exactly such a script driven hide: `crates/game/src/natives.rs:475` to
//! `479` sets both `deleted` and `enabled` on the target `ObjectReference`,
//! and vanilla deletion means "not present in the simulation," the same
//! contract a disable already carries (`crates/godot-host/src/godot_physics.rs:100`
//! to `102`).
//!
//! `World::ref_placement` (`crates/game/src/world/refs.rs:217` to `228`)
//! returns `None` for a deleted reference instead of
//! `Some(RefPlacement { visible: false, .. })`. Both host consumers treat a
//! `None` result as "nothing to sync" and skip the member outright:
//! `crates/godot-host/src/godot_layer.rs:351`,
//! `let Some(placement) = world.ref_placement(member.reference) else { continue; };`,
//! and `crates/godot-host/src/godot_physics.rs:116` to `118`, the identical
//! pattern gating `body_set_space(entry.body, Rid::Invalid)`. A reference
//! the host already drew or collided, then deleted by a script, keeps its
//! last mesh visible and its last collision body solid, because the one
//! signal that would tell the host to hide it is swallowed at the source
//! instead of forwarded as a hide.
//!
//! These tests build a `World` directly and drive `RefState` the same way
//! the `Enable`, `Disable` and `Delete` natives do (`crates/game/src/natives.rs:452`
//! to `486`), so they exercise `World`'s own public contract rather than the
//! Papyrus call plumbing. `godot-host`'s two sync methods cannot be
//! constructed in this environment without a live `RenderingServer` and
//! `PhysicsServer3D`, so the defect is proven here at its source: the value
//! `ref_placement` hands both of them.

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{RefPlacement, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x200;
const REF: u32 = 0x300;
const ANCHOR: u32 = 0x310;
const BASE: u32 = 0x400;

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
    plugin.push_top(placed_reference(REF, [10.0, 20.0, 30.0], [0.0, 0.0, 0.0]));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    World::new(order)
}

/// `Disable()`'s own field write (`crates/game/src/natives.rs:456` to `465`).
/// Applied directly to `RefState`, matching what the native does to the
/// same struct, without going through the Papyrus VM.
fn disable(world: &mut World, reference: GlobalFormId) {
    world.reference(reference).unwrap().enabled = false;
}

/// `Delete()`'s own field write (`crates/game/src/natives.rs:475` to `479`).
fn delete(world: &mut World, reference: GlobalFormId) {
    let r = world.reference(reference).unwrap();
    r.deleted = true;
    r.enabled = false;
}

#[test]
fn delete_on_a_previously_synced_record_backed_reference_must_still_yield_a_hide_signal() {
    let mut world = build_world();
    let reference = id(REF);

    // Touch the reference once, the way the host does on first sight of the
    // cell. This is what makes it appear in `World`'s `refs` map, the same
    // step that would have populated `ref_cache` (rendering) and
    // `StaticEntry::synced` (collision) with its first placement.
    world.reference(reference).unwrap();
    let before = world.ref_placement(reference).unwrap();
    assert!(
        before.visible,
        "a freshly loaded, non disabled reference must start visible"
    );
    assert_eq!(before.position, [10.0, 20.0, 30.0]);

    // Control: `Disable()` alone must report a hide at the same placement.
    // This passes today and shows what `Delete()` should also produce.
    disable(&mut world, reference);
    assert_eq!(
        world.ref_placement(reference),
        Some(RefPlacement {
            position: [10.0, 20.0, 30.0],
            rotation: [0.0, 0.0, 0.0],
            scale: 1.0,
            visible: false,
        }),
        "Disable() must report a hide at the reference's placement"
    );
    world.reference(reference).unwrap().enabled = true;
    assert!(world.ref_placement(reference).unwrap().visible);

    delete(&mut world, reference);
    assert_eq!(
        world.ref_placement(reference),
        Some(RefPlacement {
            position: [10.0, 20.0, 30.0],
            rotation: [0.0, 0.0, 0.0],
            scale: 1.0,
            visible: false,
        }),
        "Delete() must reach the host the same way Disable() does: a hide \
         signal at the reference's last known placement, not None. Returning \
         None lets godot_layer.rs:351 and godot_physics.rs:116 to 118 skip \
         the member outright, so a deleted reference's mesh stays visible \
         and its collision body stays solid after the tick that deleted it."
    );
}

#[test]
fn delete_on_a_dynamically_placed_reference_must_still_yield_a_hide_signal() {
    let mut world = build_world();
    let anchor = id(REF);
    world.load_cell(id(CELL));

    // `PlaceAtMe`, crates/game/src/world/refs.rs:240 to 281. A dynamic
    // reference never has a record, so `deleted` is the only route by which
    // a script can ever ask the host to remove it again.
    let dynamic = world.place_at(id(BASE), anchor, false);
    let before = world.ref_placement(dynamic).unwrap();
    assert!(
        before.visible,
        "PlaceAtMe with disabled = false must start visible"
    );

    delete(&mut world, dynamic);
    let after = world.ref_placement(dynamic);
    assert!(
        after.is_some_and(|p| !p.visible),
        "a deleted dynamic reference must still report a hide (got {after:?}), \
         since place_at's spawned id is only ever reachable through World, \
         the host has no other way to learn it must stop drawing and \
         colliding with it"
    );
}

#[test]
fn delete_after_a_runtime_move_reports_the_moved_placement_as_hidden_not_absent() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"STAT"), BASE));
    plugin.push_top(Record::new(T(b"CELL"), CELL));
    plugin.push_top(placed_reference(REF, [10.0, 20.0, 30.0], [0.0, 0.0, 0.0]));
    plugin.push_top(placed_reference(ANCHOR, [100.0, 0.0, 0.0], [0.0, 0.0, 0.0]));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut world = World::new(order);
    let reference = id(REF);

    // Move the reference away from its record authored placement first, so
    // a fix that reverts to the record's `position` on delete rather than
    // preserving the live one would also be caught here.
    let moved = world.move_to(reference, id(ANCHOR), [5.0, 0.0, 0.0]);
    assert!(moved, "move_to must succeed against a loaded reference");
    let moved_placement = world.ref_placement(reference).unwrap();
    assert!(moved_placement.visible);
    assert_eq!(
        moved_placement.position,
        [105.0, 0.0, 0.0],
        "move_to must land at the anchor plus the offset before delete happens"
    );

    delete(&mut world, reference);
    assert_eq!(
        world.ref_placement(reference),
        Some(RefPlacement {
            visible: false,
            ..moved_placement
        }),
        "a reference deleted after a runtime move must still report the \
         moved placement, only hidden, so the host retires the mesh and \
         body it last built for the moved position rather than silently \
         keeping them where they were left"
    );
}
