// SPDX-License-Identifier: GPL-3.0-or-later

//! Host constraints for linked placed references.

use std::collections::BTreeMap;

use godot::classes::PhysicsServer3D;
use godot::prelude::*;
use soe_esm::GlobalFormId;

/// Owns the server joint for each tethered cart and horse pair.
#[derive(Default)]
pub struct TetherConstraints {
    links: BTreeMap<(GlobalFormId, GlobalFormId), Rid>,
}

impl TetherConstraints {
    /// Connects two rigid bodies at their local pivots. Replacing a link for
    /// either endpoint frees the old joint first.
    pub fn tether(
        &mut self,
        cart: GlobalFormId,
        horse: GlobalFormId,
        cart_body: Rid,
        horse_body: Rid,
        cart_pivot: Vector3,
        horse_pivot: Vector3,
    ) -> bool {
        if cart == horse || !cart_body.is_valid() || !horse_body.is_valid() {
            return false;
        }
        self.release_reference(cart);
        self.release_reference(horse);

        let mut server = PhysicsServer3D::singleton();
        let joint = server.joint_create();
        server.joint_make_pin(joint, cart_body, cart_pivot, horse_body, horse_pivot);
        self.links.insert((cart, horse), joint);
        true
    }

    /// Frees every link touching a reference, including when it is the horse.
    pub fn release_reference(&mut self, reference: GlobalFormId) -> usize {
        let keys: Vec<_> = self
            .links
            .keys()
            .filter(|(cart, horse)| *cart == reference || *horse == reference)
            .copied()
            .collect();
        let mut server = PhysicsServer3D::singleton();
        for key in &keys {
            if let Some(joint) = self.links.remove(key) {
                server.free_rid(joint);
            }
        }
        keys.len()
    }

    /// Frees all remaining joints before their owning host node exits.
    pub fn free(&mut self) {
        let mut server = PhysicsServer3D::singleton();
        for joint in std::mem::take(&mut self.links).into_values() {
            server.free_rid(joint);
        }
    }

    #[must_use]
    pub fn link_count(&self) -> usize {
        self.links.len()
    }

    #[must_use]
    pub fn endpoints(&self) -> Vec<GlobalFormId> {
        let mut endpoints = Vec::with_capacity(self.links.len() * 2);
        for (cart, horse) in self.links.keys() {
            endpoints.push(*cart);
            endpoints.push(*horse);
        }
        endpoints.sort_unstable();
        endpoints.dedup();
        endpoints
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hst_15_rejects_invalid_body_handles_without_creating_a_link() {
        let mut tethers = TetherConstraints::default();
        let id = GlobalFormId { file: 1, object: 2 };
        assert!(!tethers.tether(
            id,
            GlobalFormId { file: 1, object: 3 },
            Rid::Invalid,
            Rid::Invalid,
            Vector3::ZERO,
            Vector3::ZERO,
        ));
        assert_eq!(tethers.link_count(), 0);
    }
}
