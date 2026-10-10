// SPDX-License-Identifier: GPL-3.0-or-later

//! Resident door geometry uses server RIDs and one stable control node.
//! OpenMW's worldimp.cpp rotateDoor supplies the reversible quarter-turn fallback.
//! Collision is disabled while open by the shared World query, rather than
//! keeping a second closed panel at the original placement.

use godot::classes::{ArrayMesh, Node3D, RenderingServer};
use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_physics::Iso;

use crate::godot_layer::transform3d_from;
use crate::units::placement_to_godot;

pub(super) struct DoorSurface {
    pub rid: Rid,
    pub mesh: Gd<ArrayMesh>,
    pub original: Transform3D,
    pub triangles: usize,
}

pub(super) struct ResidentDoor {
    pub cell: GlobalFormId,
    pub node: Gd<Node3D>,
    pub surfaces: Vec<DoorSurface>,
    pose: Transform3D,
    visible: bool,
    opened: bool,
    fraction: f32,
}

impl ResidentDoor {
    pub fn new(cell: GlobalFormId, node: Gd<Node3D>, iso: Iso, opened: bool) -> Self {
        Self {
            cell,
            node,
            surfaces: Vec::new(),
            pose: pose_transform(iso),
            visible: true,
            opened,
            fraction: if opened { 1.0 } else { 0.0 },
        }
    }

    pub fn sync(&mut self, iso: Iso, visible: bool, opened: bool) {
        let pose = pose_transform(iso);
        if self.pose == pose && self.visible == visible && self.opened == opened {
            return;
        }
        self.pose = pose;
        self.visible = visible;
        self.opened = opened;
        self.apply();
    }

    pub fn advance(&mut self, delta: f32) {
        let target = if self.opened { 1.0 } else { 0.0 };
        let next = advance_swing(self.fraction, target, delta);
        if self.fraction != next {
            self.fraction = next;
            self.apply();
        }
    }

    pub fn apply(&mut self) {
        let swing = Transform3D::new(
            Basis::from_axis_angle(Vector3::UP, self.fraction * std::f32::consts::FRAC_PI_2),
            Vector3::ZERO,
        );
        let transform = self.pose * swing;
        self.node.set_transform(transform);
        self.node.set_visible(self.visible);
        let mut server = RenderingServer::singleton();
        for surface in &self.surfaces {
            server
                .instance_set_transform(surface.rid, transform * surface.original.affine_inverse());
            server.instance_set_visible(surface.rid, self.visible);
        }
    }

    pub fn state(&self) -> VarDictionary {
        let mut state = VarDictionary::new();
        state.set("node", self.node.instance_id().to_i64());
        state.set("visible", self.visible);
        state.set("opened", self.opened);
        state.set("fraction", self.fraction);
        state.set("transform", self.node.get_transform());
        state.set("placement_transform", self.pose);
        let bounds = self
            .surfaces
            .iter()
            .map(|surface| surface.original.affine_inverse() * surface.mesh.get_aabb())
            .reduce(|a, b| a.merge(b));
        if let Some(bounds) = bounds {
            state.set("local_bounds", bounds);
        }
        state.set("surfaces", self.surfaces.len() as i64);
        let instances: Array<Rid> = self.surfaces.iter().map(|surface| surface.rid).collect();
        state.set("instances", &instances);
        state
    }

    pub fn free(self) {
        let mut server = RenderingServer::singleton();
        for surface in self.surfaces {
            server.free_rid(surface.rid);
            drop(surface.mesh);
        }
        self.node.free();
    }
}

pub(super) fn pose_transform(iso: Iso) -> Transform3D {
    transform3d_from(placement_to_godot(
        iso.rotation.rows,
        [iso.translation.x, iso.translation.y, iso.translation.z],
        iso.scale,
    ))
}

fn advance_swing(fraction: f32, target: f32, delta: f32) -> f32 {
    if !delta.is_finite() || delta <= 0.0 {
        return fraction;
    }
    if fraction < target {
        (fraction + delta).min(target)
    } else {
        (fraction - delta).max(target)
    }
}

#[cfg(test)]
mod tests {
    use super::advance_swing;

    #[test]
    fn hst_21_swing_reverses_without_restarting_and_holds_endpoints() {
        let midway = advance_swing(0.0, 1.0, 0.4);
        assert_eq!(midway, 0.4);
        assert_eq!(advance_swing(midway, 0.0, 0.2), 0.2);
        assert_eq!(advance_swing(midway, 1.0, 10.0), 1.0);
        assert_eq!(advance_swing(midway, 0.0, 10.0), 0.0);
        assert_eq!(advance_swing(1.0, 1.0, 10.0), 1.0);
    }

    #[test]
    fn hst_21_invalid_elapsed_time_preserves_the_door_pose() {
        for delta in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            assert_eq!(advance_swing(0.4, 1.0, delta), 0.4);
        }
    }
}
