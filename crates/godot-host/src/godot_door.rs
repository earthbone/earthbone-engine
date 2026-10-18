// SPDX-License-Identifier: GPL-3.0-or-later

//! Resident door geometry uses server RIDs and one stable control node.
//! Nifly Animation.cpp and nif.xml supply named child-node sequences and
//! collision targets. OpenMW's rotateDoor fallback is only for sequence-less assets.

use godot::classes::{ArrayMesh, Node3D, RenderingServer};
use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_physics::Iso;

use crate::collision::PlacedCollider;
use crate::godot_layer::transform3d_from;
use crate::object_animation::{AnimatedSurface, DoorAnimation};
use crate::units::placement_to_godot;

pub(super) struct DoorSurface {
    pub rid: Rid,
    pub mesh: Gd<ArrayMesh>,
    pub original: Transform3D,
    pub model_transform: Transform3D,
    pub triangles: usize,
    pub submitted: Transform3D,
    pub animation: Option<AnimatedSurface>,
    pub pick_mesh: crate::godot_layer::picking::PickMesh,
}

pub(super) struct ResidentDoor {
    pub cell: GlobalFormId,
    pub node: Gd<Node3D>,
    pub surfaces: Vec<DoorSurface>,
    pose: Transform3D,
    pub visible: bool,
    opened: bool,
    fraction: f32,
    pub animation: Option<DoorAnimation>,
    pub colliders: Vec<PlacedCollider>,
    placement: Iso,
    revision: u64,
    load_opening: bool,
}

pub(crate) struct DoorCollisionState {
    pub cell: GlobalFormId,
    pub reference: GlobalFormId,
    pub visible: bool,
    pub revision: u64,
    pub colliders: Vec<PlacedCollider>,
    pub deltas: Vec<(soe_nif::BlockRef, Iso)>,
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
            animation: None,
            colliders: Vec::new(),
            placement: iso,
            revision: 0,
            load_opening: false,
        }
    }

    pub fn sync(&mut self, iso: Iso, visible: bool, opened: bool) {
        let opened = opened || self.load_opening;
        let pose = pose_transform(iso);
        if self.pose == pose && self.visible == visible && self.opened == opened {
            return;
        }
        self.pose = pose;
        self.placement = iso;
        self.visible = visible;
        self.opened = opened;
        if let Some(animation) = &mut self.animation {
            animation.set_opened(opened);
        }
        self.apply();
    }

    pub fn begin_load_opening(&mut self) -> bool {
        let Some(animation) = &mut self.animation else {
            return false;
        };
        if animation.asset.sequence_duration("Open").is_none() {
            return false;
        }
        self.load_opening = true;
        self.opened = true;
        animation.set_opened(true);
        self.apply();
        true
    }

    pub fn opening_finished(&self) -> bool {
        self.fraction == 1.0
    }

    pub fn advance(&mut self, delta: f32) {
        if let Some(animation) = &mut self.animation {
            if animation.advance(delta) {
                self.fraction = animation.fraction();
                self.apply();
            }
            return;
        }
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
        let transform = if self.animation.is_some() {
            self.pose
        } else {
            self.pose * swing
        };
        self.node.set_transform(transform);
        self.node.set_visible(self.visible);
        let mut server = RenderingServer::singleton();
        let pose = self.animation.as_ref().map(DoorAnimation::pose);
        for surface in &mut self.surfaces {
            let model = match (&pose, &surface.animation) {
                (Some(pose), Some(animation)) => {
                    let shape = pose.transforms[animation.block];
                    pose_transform(Iso::from(shape))
                }
                _ => surface.model_transform,
            };
            surface.submitted = surface_transform(transform, surface.original) * model;
            server.instance_set_transform(surface.rid, surface.submitted);
            server.instance_set_visible(surface.rid, self.visible);
        }
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn collision_state(&self, reference: GlobalFormId) -> Option<DoorCollisionState> {
        let animation = self.animation.as_ref()?;
        let rest = animation.asset.rest_pose();
        let pose = animation.pose();
        let mut deltas = Vec::new();
        let colliders = self
            .colliders
            .iter()
            .map(|source| {
                let mut placed = source.clone();
                if let Some(node) = placed.collider.node.get() {
                    let old = source.original.then(&Iso::from(rest.transforms[node]));
                    let current = self.placement.then(&Iso::from(pose.transforms[node]));
                    let delta = current.then(&old.inverse());
                    placed.collider.transform = delta.then(&source.collider.transform);
                    placed.original = self.placement;
                    deltas.push((placed.collider.node, delta));
                }
                placed
            })
            .collect();
        Some(DoorCollisionState {
            cell: self.cell,
            reference,
            visible: self.visible,
            revision: self.revision,
            colliders,
            deltas,
        })
    }

    pub fn state(&self) -> VarDictionary {
        let mut state = VarDictionary::new();
        state.set("node", self.node.instance_id().to_i64());
        state.set("visible", self.visible);
        state.set("opened", self.opened);
        state.set("fraction", self.fraction);
        if let Some(animation) = &self.animation {
            let (sequence, seconds) = animation.playback();
            state.set("sequence", sequence);
            state.set("sequence_seconds", seconds);
            state.set(
                "open_duration",
                animation.asset.sequence_duration("Open").unwrap_or(0.0),
            );
            state.set(
                "close_duration",
                animation.asset.sequence_duration("Close").unwrap_or(0.0),
            );
            let transforms: Array<Transform3D> = animation
                .pose()
                .transforms
                .into_iter()
                .map(|transform| pose_transform(Iso::from(transform)))
                .collect();
            state.set("model_nodes", &transforms);
        }
        state.set("transform", self.node.get_transform());
        state.set("placement_transform", self.pose);
        let mut bounds: Option<Aabb> = None;
        let mut vertices = PackedVector3Array::new();
        for surface in &self.surfaces {
            let arrays = surface.mesh.surface_get_arrays(0);
            let positions: PackedVector3Array = arrays.at(0).to();
            let local = surface.original.affine_inverse() * surface.model_transform;
            for point in positions.as_slice() {
                let point_local = local * *point;
                bounds = Some(bounds.map_or_else(
                    || Aabb::new(point_local, Vector3::ZERO),
                    |bounds| bounds.expand(point_local),
                ));
                vertices.push(surface.submitted * *point);
            }
        }
        if let Some(bounds) = bounds {
            state.set("local_bounds", bounds);
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
        // Freeing a child sends the streamer a child-order notification that
        // godot-rust dispatches through a bind of the streamer. Every caller
        // already holds that bind, so free after the call returns.
        let mut node = self.node;
        node.queue_free();
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
    fn hst_27_animated_leaf_keeps_its_model_transform_inside_the_swing() {
        let placement = Transform3D::new(
            Basis::from_axis_angle(Vector3::UP, 1.2),
            Vector3::new(-1200.0, 50.0, 800.0),
        );
        let model = Transform3D::new(
            Basis::from_axis_angle(Vector3::UP, 0.3),
            Vector3::new(0.5, 0.2, 0.0),
        );
        let leaf = Vector3::new(1.0, 1.0, 0.0);
        let radius = (model * leaf).length();
        for fraction in [0.0, 0.5, 1.0] {
            let swing = Transform3D::new(
                Basis::from_axis_angle(Vector3::UP, fraction * std::f32::consts::FRAC_PI_2),
                Vector3::ZERO,
            );
            let submitted = surface_transform(placement * swing, Transform3D::IDENTITY) * model;
            let rendered = submitted * leaf;
            assert!(rendered.distance_to(placement.origin) <= radius + 0.001);
            assert!(rendered.distance_to(placement * (swing * (model * leaf))) < 0.001);
        }
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
