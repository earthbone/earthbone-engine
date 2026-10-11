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
    pub submitted: Transform3D,
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
        for surface in &mut self.surfaces {
            surface.submitted = surface_transform(transform, surface.original);
            server.instance_set_transform(surface.rid, surface.submitted);
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
        let mut vertices = PackedVector3Array::new();
        for surface in &self.surfaces {
            let arrays = surface.mesh.surface_get_arrays(0);
            let positions: PackedVector3Array = arrays.at(0).to();
            for point in positions.as_slice() {
                vertices.push(surface.submitted * *point);
            }
        }
        state.set("world_vertices", &vertices);
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

fn surface_transform(hinge_pose: Transform3D, baked_placement: Transform3D) -> Transform3D {
    hinge_pose * baked_placement.affine_inverse()
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
    use super::{advance_swing, surface_transform};
    use godot::prelude::*;

    #[test]
    fn hst_27_baked_leaf_stays_at_the_hinge_through_all_swing_states() {
        let placement = Transform3D::new(
            Basis::from_axis_angle(Vector3::UP, 0.7).scaled(Vector3::splat(0.75)),
            Vector3::new(2500.0, 27.0, -900.0),
        );
        let leaf = Vector3::new(1.0, 1.5, 0.1);
        let radius = (placement.basis * leaf).length();
        for fraction in [0.0, 0.25, 0.5, 1.0, 0.5, 0.0] {
            let swing = Transform3D::new(
                Basis::from_axis_angle(Vector3::UP, fraction * std::f32::consts::FRAC_PI_2),
                Vector3::ZERO,
            );
            let rendered = surface_transform(placement * swing, placement) * (placement * leaf);
            assert!(rendered.distance_to(placement.origin) <= radius + 0.001);
            assert!(rendered.distance_to(placement * (swing * leaf)) < 0.001);
        }
    }

    #[test]
    fn hst_27_local_leaf_applies_the_reference_placement_once() {
        let placement = Transform3D::new(
            Basis::from_axis_angle(Vector3::UP, 1.2),
            Vector3::new(-1200.0, 50.0, 800.0),
        );
        let leaf = Vector3::new(1.0, 1.0, 0.0);
        let rendered = surface_transform(placement, Transform3D::IDENTITY) * leaf;
        assert!(rendered.distance_to(placement * leaf) < 0.001);
    }

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
