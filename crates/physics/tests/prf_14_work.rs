// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-14 counts missed rays and headless capsule queries.

use soe_physics::{CharacterSettings, PhysicsWorld, SimpleWorld, Vec3};
use soe_trace::{Operation, WorkCounts, WorkFrame};

#[test]
fn prf_14_headless_rays_and_capsule_queries_count_even_without_contacts() {
    let mut world = SimpleWorld::new();
    world.create_character(CharacterSettings::default(), Vec3::ZERO);
    let frame = WorkFrame::begin();
    assert!(world.raycast(Vec3::ZERO, Vec3::Z).is_none());
    assert!(world.raycast(Vec3::ZERO, Vec3::ZERO).is_none());
    world.step(1.0 / 60.0);
    assert_eq!(frame.counts().get(Operation::Raycasts), 2);
    assert!(frame.counts().get(Operation::ShapeCasts) > 0);
    assert_eq!(
        frame
            .counts()
            .check(&WorkCounts([0, 0, 0, 0, 0, 0, 2, 64, 0, 0])),
        Ok(())
    );
}
