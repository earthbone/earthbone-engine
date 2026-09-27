// SPDX-License-Identifier: GPL-3.0-or-later

//! Lockpick angle resolution and pick transactions. XLOC lock levels follow
//! xEdit's published record layout. The angle rule is engine policy because
//! the local CK references describe lock levels but not the menu's mechanics.

use soe_esm::GlobalFormId;

impl super::Game {
    /// Apply one lockpick turn. Success unlocks the reference. A failed turn
    /// removes one pick from the activator. Key-only locks cannot be picked.
    pub fn attempt_lockpick(
        &mut self,
        target: GlobalFormId,
        activator: GlobalFormId,
        pick: GlobalFormId,
        angle: f32,
    ) -> bool {
        let level = {
            let mut world = self.world.borrow_mut();
            let Some(level) = world
                .reference(target)
                .map(|reference| reference.lock_level)
            else {
                return false;
            };
            if !world
                .reference_ref(target)
                .is_some_and(|reference| reference.locked)
                || level == 255
            {
                return false;
            }
            if world.item_count(activator, pick) <= 0 {
                return false;
            }
            level
        };
        let skill = self
            .world
            .borrow_mut()
            .actor_value(activator, "lockpicking");
        if angle_succeeds(target, level, skill, angle) {
            let vm = &mut self.vm;
            self.world
                .borrow_mut()
                .set_locked(target, false, &mut |name, class| {
                    vm.create_object_of(name, class)
                });
            self.run_until_idle(64);
            true
        } else {
            let vm = &mut self.vm;
            self.world.borrow_mut().remove_item_notified(
                activator,
                pick,
                1,
                None,
                &mut |name, class| vm.create_object_of(name, class),
            );
            self.run_until_idle(64);
            false
        }
    }
}

/// Whether a turn aligns with the lock's deterministic sweet spot.
#[must_use]
pub fn angle_succeeds(target: GlobalFormId, level: i32, skill: f32, angle: f32) -> bool {
    if !angle.is_finite() || !(0.0..=90.0).contains(&angle) || !skill.is_finite() {
        return false;
    }
    let spot = ((u64::from(target.object) * 37 + u64::from(target.file) * 101) % 901) as f32 / 10.0;
    let difficulty = level.clamp(0, 254) / 20;
    let tolerance = (8.0 - difficulty as f32 + skill.clamp(0.0, 100.0) / 15.0).max(2.0);
    (angle - spot).abs() <= tolerance
}
