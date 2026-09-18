// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Builds a real cell's collision from the installed game and walks a capsule
//! through it. Runs only when `SKYRIM_DATA` points at the game's `Data`
//! folder:
//!
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test real_collision -- --nocapture
//! ```
//!
//! `crates/physics/tests/character.rs` proves the controller against geometry
//! whose angles are exact. This proves the other half: that the collision the
//! artist attached to a shipped mesh, placed by the same reference transform
//! the renderer uses, is a surface a character can stand on.

use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;
use soe_physics::{CharacterSettings, PhysicsWorld, Vec3};

const DT: f32 = 1.0 / 60.0;

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

#[test]
fn a_capsule_stands_on_a_real_cell_floor() {
    let Some(data) = data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let source = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}"));
    let wanted = "WhiterunBanneredMare";
    let cell_id = match source.resolve_cell(wanted) {
        Some(id) => id,
        None => {
            eprintln!("{wanted} not found, falling back to a populated interior cell");
            source
                .first_populated_interior_cell(50)
                .expect("no populated interior cell was found")
        }
    };

    let started = std::time::Instant::now();
    let built = source
        .collide_cell_id(cell_id)
        .unwrap_or_else(|e| panic!("collide_cell_id: {e}"));
    let elapsed = started.elapsed();
    let s = &built.stats;
    eprintln!(
        "collision for {wanted}: {} references, {} with collision, {} colliders, \
         {} bodies, {} primitives, {} failed, {} blocks skipped, in {:.3} s",
        s.references,
        s.with_collision,
        s.colliders,
        s.bodies,
        s.primitives,
        s.failed,
        s.skipped_blocks,
        elapsed.as_secs_f64()
    );

    assert!(s.references > 0, "the cell placed no statics with a model");
    assert!(
        s.with_collision > 0,
        "not one shipped mesh in this cell carried collision, which cannot be right"
    );
    assert!(s.bodies > 0, "no static bodies reached the world");
    assert!(
        s.primitives > s.bodies,
        "every body flattened to one primitive or fewer, {} from {}",
        s.primitives,
        s.bodies
    );

    // Drop the capsule down the middle of the cell's own bounds, from above
    // everything, and let it settle.
    let mut world = built.world;
    let mut ceiling = f32::NEG_INFINITY;
    let mut probe = Vec3::ZERO;
    if let Some(hit) = world.raycast(
        Vec3::new(0.0, 0.0, 100_000.0),
        Vec3::new(0.0, 0.0, -100_000.0),
    ) {
        probe = hit.point;
        ceiling = hit.point.z;
    }
    assert!(
        ceiling.is_finite(),
        "a ray down the cell's origin hit nothing, so there is no floor under it"
    );
    eprintln!("topmost surface over the origin at z {ceiling:.1}, point {probe:?}");

    let start = Vec3::new(0.0, 0.0, ceiling + 300.0);
    let id = world.create_character(CharacterSettings::default(), start);
    let mut lowest = f32::INFINITY;
    for i in 0..600 {
        world.step(DT);
        let ch = world.character(id).unwrap();
        lowest = lowest.min(ch.position.z);
        if i % 120 == 0 || i == 599 {
            eprintln!(
                "t={:.2}s pos=({:.1}, {:.1}, {:.1}) ground={}",
                (i + 1) as f32 * DT,
                ch.position.x,
                ch.position.y,
                ch.position.z,
                ch.on_ground
            );
        }
    }

    let end = world.character(id).unwrap();
    assert!(
        end.on_ground,
        "the capsule never found ground in a real cell: {end:?}"
    );
    assert!(
        end.position.z < start.z,
        "it did not fall at all, from {} to {}",
        start.z,
        end.position.z
    );
    assert!(
        end.position.z > ceiling - 2000.0,
        "it fell far past the cell's geometry to {}, so nothing caught it",
        end.position.z
    );

    // It may rest above the surface a ray down the exact centre finds, and in
    // the Bannered Mare it does: the hearth leaves an open shaft at the
    // origin, so the ray drops to the ground floor while a capsule 32 units
    // wide catches the upper floor around it. What must not happen is resting
    // on nothing, wedged inside geometry by the depenetration loop. So look
    // under the feet where the capsule actually stands.
    let under = world
        .raycast(end.position + Vec3::Z * 8.0, end.position - Vec3::Z * 200.0)
        .expect("nothing under the feet, so it is wedged rather than standing");
    let drop = end.position.z - under.point.z;
    eprintln!(
        "floor under the feet at z {:.1}, {:.1} below them, normal {:?}",
        under.point.z, drop, under.normal
    );
    assert!(
        drop < 40.0,
        "the surface under the feet is {drop:.1} units down, so it is not what it is standing on"
    );
    assert!(
        under.normal.z > 0.5,
        "standing on a face that points sideways, normal {:?}",
        under.normal
    );
}

#[test]
fn walking_into_a_real_cell_is_blocked_somewhere() {
    let Some(data) = data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let source = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}"));
    let Some(cell_id) = source.resolve_cell("WhiterunBanneredMare") else {
        eprintln!("WhiterunBanneredMare not found, skipping the wall test");
        return;
    };
    let built = source
        .collide_cell_id(cell_id)
        .unwrap_or_else(|e| panic!("collide_cell_id: {e}"));
    let mut world = built.world;

    let Some(ground) = world.raycast(
        Vec3::new(0.0, 0.0, 100_000.0),
        Vec3::new(0.0, 0.0, -100_000.0),
    ) else {
        eprintln!("no floor over the origin, skipping");
        return;
    };
    let id = world.create_character(CharacterSettings::default(), ground.point + Vec3::Z * 40.0);
    for _ in 0..180 {
        world.step(DT);
    }
    let settled = world.character(id).unwrap().position;
    eprintln!("settled at {settled:?}");

    // An interior is a closed box, so walking far enough in any direction has
    // to stop against something. Ten seconds at a jog covers 1,800 units,
    // which is wider than the Bannered Mare.
    let mut blocked = Vec::new();
    for (dx, dy, name) in [
        (1.0, 0.0, "+x"),
        (-1.0, 0.0, "-x"),
        (0.0, 1.0, "+y"),
        (0.0, -1.0, "-y"),
    ] {
        let mut probe = SimpleRestart::new(&mut world, id, settled);
        probe.push(Vec3::new(dx * 180.0, dy * 180.0, 0.0), 600);
        let end = probe.position();
        let travelled = ((end.x - settled.x).powi(2) + (end.y - settled.y).powi(2)).sqrt();
        eprintln!("{name}: travelled {travelled:.1} units before stopping at {end:?}");
        blocked.push(travelled < 1_800.0);
    }
    assert!(
        blocked.iter().any(|b| *b),
        "the capsule walked 1,800 units in every direction, so nothing in this interior blocks it"
    );
}

/// Puts the character back at a known spot, then pushes it one way.
struct SimpleRestart<'a> {
    world: &'a mut soe_physics::SimpleWorld,
    id: soe_physics::BodyId,
}

impl<'a> SimpleRestart<'a> {
    fn new(
        world: &'a mut soe_physics::SimpleWorld,
        id: soe_physics::BodyId,
        at: Vec3,
    ) -> SimpleRestart<'a> {
        world.remove(id);
        let id = world.create_character(CharacterSettings::default(), at);
        SimpleRestart { world, id }
    }

    fn push(&mut self, desired: Vec3, steps: usize) {
        self.world.move_character(self.id, desired);
        for _ in 0..steps {
            self.world.step(DT);
        }
    }

    fn position(&self) -> Vec3 {
        self.world.character(self.id).unwrap().position
    }
}
