// SPDX-License-Identifier: GPL-3.0-or-later

//! XPRM reference trigger volumes and their actor crossing events.
//!
//! XPRM field order comes from xEdit's `wbDefinitionsTES5.pas`. Event names,
//! arguments and occupant-count semantics come from the local Creation Kit
//! wiki pages for `OnTriggerEnter`, `OnTriggerLeave` and
//! `GetTriggerObjectCount`.

use super::{Alloc, Handle, Pending, World};
use soe_esm::records::refr::Refr;
use soe_esm::{tag, GlobalFormId, Tag};

#[derive(Clone, Copy)]
struct ActorCapsule {
    ends: [[f32; 3]; 2],
    radius: f32,
}

impl ActorCapsule {
    fn from_world(world: &mut World, actor: GlobalFormId) -> Option<Self> {
        let reference = world.reference(actor)?;
        let base = reference.base?;
        let placed = reference.scale;
        let placed = if placed.is_finite() && placed > 0.0 {
            placed
        } else {
            1.0
        };
        let scale = world.actor_controller_scale(base).unwrap_or(1.0) * placed;
        let soe_physics::Shape::Capsule { a, b, radius } =
            soe_physics::generated_controller_shape(scale)
        else {
            return None;
        };
        Some(Self {
            ends: [[a.x, a.y, a.z], [b.x, b.y, b.z]],
            radius,
        })
    }
}

#[derive(Clone, Copy)]
enum VolumeShape {
    Box([f32; 3]),
    Sphere(f32),
}

#[derive(Clone, Copy)]
struct TriggerVolume {
    cell: Option<GlobalFormId>,
    center: [f32; 3],
    rotation: [f32; 3],
    scale: f32,
    shape: VolumeShape,
}

impl TriggerVolume {
    fn matches_cell(self, cell: GlobalFormId, world: &World) -> bool {
        if self.cell == Some(cell) {
            return true;
        }
        let Some(index) = world.order.cell_index() else {
            return false;
        };
        let Some(grid) = index.grid_of(cell) else {
            return false;
        };
        if self.cell.and_then(|cell| index.persistent_cell_world(cell)) != Some(grid.world) {
            return false;
        }
        let extent = |axis| match self.shape {
            VolumeShape::Sphere(radius) => radius * self.scale,
            VolumeShape::Box(bounds) => {
                let unit = std::array::from_fn(|i| if i == axis { 1.0 } else { 0.0 });
                inverse_rotate(unit, self.rotation)
                    .into_iter()
                    .zip(bounds)
                    .map(|(axis, bound)| axis.abs() * bound * self.scale)
                    .sum()
            }
        };
        [grid.x, grid.y]
            .into_iter()
            .enumerate()
            .all(|(axis, coordinate)| {
                let low = f64::from(coordinate) * 4096.0;
                let high = low + 4096.0;
                let center = f64::from(self.center[axis]);
                let extent = f64::from(extent(axis));
                center + extent >= low && center - extent <= high
            })
    }

    fn from_refr(refr: &Refr, file: usize, world: &World) -> Option<Self> {
        let primitive = refr.primitive?;
        let bounds = primitive.bounds;
        let kind = primitive.kind;
        if !bounds.into_iter().all(f32::is_finite) || bounds.iter().any(|n| *n <= 0.0) {
            return None;
        }
        let transform = refr.transform?;
        if !transform.position.into_iter().all(f32::is_finite)
            || !transform.rotation.into_iter().all(f32::is_finite)
            || !refr.scale.is_finite()
            || refr.scale <= 0.0
            || refr.is_deleted()
        {
            return None;
        }
        let shape = match kind {
            1 => VolumeShape::Box(bounds),
            2 => VolumeShape::Sphere(bounds[0]),
            _ => return None,
        };
        let cell = refr.cell.map(|raw| world.order.resolve(file, raw));
        Some(Self {
            cell,
            center: transform.position,
            rotation: transform.rotation,
            scale: refr.scale,
            shape,
        })
    }

    fn contains(self, point: [f32; 3]) -> bool {
        let offset = [
            point[0] - self.center[0],
            point[1] - self.center[1],
            point[2] - self.center[2],
        ];
        let local = inverse_rotate(offset, self.rotation);
        match self.shape {
            VolumeShape::Box(half) => {
                (0..3).all(|axis| local[axis].abs() <= half[axis] * self.scale)
            }
            VolumeShape::Sphere(radius) => {
                let radius = radius * self.scale;
                local.iter().map(|n| n * n).sum::<f32>() <= radius * radius
            }
        }
    }

    fn intersects_segment(self, from: [f32; 3], to: [f32; 3]) -> bool {
        let local = |point: [f32; 3]| {
            inverse_rotate(
                [
                    point[0] - self.center[0],
                    point[1] - self.center[1],
                    point[2] - self.center[2],
                ],
                self.rotation,
            )
        };
        let start = local(from);
        let end = local(to);
        match self.shape {
            VolumeShape::Box(half) => {
                let mut enter: f32 = 0.0;
                let mut leave: f32 = 1.0;
                for axis in 0..3 {
                    let delta = end[axis] - start[axis];
                    if delta.abs() <= f32::EPSILON {
                        if start[axis].abs() > half[axis] * self.scale {
                            return false;
                        }
                        continue;
                    }
                    let extent = half[axis] * self.scale;
                    let a = (-extent - start[axis]) / delta;
                    let b = (extent - start[axis]) / delta;
                    enter = enter.max(a.min(b));
                    leave = leave.min(a.max(b));
                    if enter > leave {
                        return false;
                    }
                }
                true
            }
            VolumeShape::Sphere(radius) => {
                let direction = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
                let length_squared = direction.iter().map(|n| n * n).sum::<f32>();
                let t = if length_squared > 0.0 {
                    (-(start[0] * direction[0] + start[1] * direction[1] + start[2] * direction[2])
                        / length_squared)
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let closest = [
                    start[0] + t * direction[0],
                    start[1] + t * direction[1],
                    start[2] + t * direction[2],
                ];
                let radius = radius * self.scale;
                closest.iter().map(|n| n * n).sum::<f32>() <= radius * radius
            }
        }
    }

    fn capsule_axis(self, feet: [f32; 3], capsule: ActorCapsule) -> [[f64; 3]; 2] {
        capsule.ends.map(|end| {
            inverse_rotate(
                std::array::from_fn(|i| feet[i] - self.center[i] + end[i]),
                self.rotation,
            )
            .map(f64::from)
        })
    }

    fn capsule_axis_distance_squared(self, axis: [[f64; 3]; 2]) -> f64 {
        match self.shape {
            VolumeShape::Box(half) => segment_box_distance_squared(
                axis,
                half.map(|extent| f64::from(extent) * f64::from(self.scale)),
            ),
            VolumeShape::Sphere(_) => {
                let [start, end] = axis;
                let delta: [f64; 3] = std::array::from_fn(|i| end[i] - start[i]);
                let length = delta.iter().map(|value| value * value).sum::<f64>();
                let t = if length > 0.0 {
                    (-start.iter().zip(delta).map(|(a, b)| a * b).sum::<f64>() / length)
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (0..3).map(|i| (start[i] + t * delta[i]).powi(2)).sum()
            }
        }
    }

    fn capsule_contact_radius(self, capsule: ActorCapsule) -> f64 {
        f64::from(capsule.radius)
            + match self.shape {
                VolumeShape::Sphere(radius) => f64::from(radius) * f64::from(self.scale),
                VolumeShape::Box(_) => 0.0,
            }
    }

    fn overlaps_capsule(self, feet: [f32; 3], capsule: ActorCapsule) -> bool {
        self.capsule_axis_distance_squared(self.capsule_axis(feet, capsule))
            <= self.capsule_contact_radius(capsule).powi(2)
    }

    fn intersects_swept_capsule(self, from: [f32; 3], to: [f32; 3], capsule: ActorCapsule) -> bool {
        if from == to {
            return self.overlaps_capsule(from, capsule);
        }
        let start = self.capsule_axis(from, capsule);
        let end = self.capsule_axis(to, capsule);
        let radius = self.capsule_contact_radius(capsule);
        let half = match self.shape {
            VolumeShape::Box(half) => half.map(|extent| f64::from(extent) * f64::from(self.scale)),
            VolumeShape::Sphere(_) => [0.0; 3],
        };
        if (0..3).any(|i| {
            let low = start[0][i].min(start[1][i]).min(end[0][i]).min(end[1][i]);
            let high = start[0][i].max(start[1][i]).max(end[0][i]).max(end[1][i]);
            low > half[i] + radius || high < -half[i] - radius
        }) {
            return false;
        }
        let distance = |t: f64| {
            let axis = std::array::from_fn(|cap| {
                std::array::from_fn(|i| start[cap][i] + t * (end[cap][i] - start[cap][i]))
            });
            self.capsule_axis_distance_squared(axis)
        };
        // CONTEXT: Distance between two convex sets is convex under translation.
        // This bounded minimum retains rounded corners during fast crossings.
        let mut low = 0.0;
        let mut high = 1.0;
        for _ in 0..48 {
            let a = low + (high - low) / 3.0;
            let b = high - (high - low) / 3.0;
            if distance(a) < distance(b) {
                high = b;
            } else {
                low = a;
            }
        }
        distance(f64::midpoint(low, high))
            .min(distance(0.0))
            .min(distance(1.0))
            <= radius * radius
    }
}

fn segment_box_distance_squared([start, end]: [[f64; 3]; 2], half: [f64; 3]) -> f64 {
    let delta: [f64; 3] = std::array::from_fn(|i| end[i] - start[i]);
    let mut breaks = [0.0; 8];
    breaks[1] = 1.0;
    let mut count = 2;
    for axis in 0..3 {
        if delta[axis] == 0.0 {
            continue;
        }
        for side in [-half[axis], half[axis]] {
            let t = (side - start[axis]) / delta[axis];
            if t > 0.0 && t < 1.0 {
                breaks[count] = t;
                count += 1;
            }
        }
    }
    breaks[..count].sort_by(f64::total_cmp);
    // CONTEXT: Each interval keeps the same outside faces, so its squared
    // distance is quadratic and has an explicit minimum.
    let distance = |t: f64| {
        (0..3)
            .map(|i| ((start[i] + t * delta[i]).abs() - half[i]).max(0.0).powi(2))
            .sum::<f64>()
    };
    let mut best = distance(0.0).min(distance(1.0));
    for interval in breaks[..count].windows(2) {
        let midpoint = f64::midpoint(interval[0], interval[1]);
        let mut quadratic = 0.0;
        let mut linear = 0.0;
        for axis in 0..3 {
            let point = start[axis] + midpoint * delta[axis];
            if point.abs() <= half[axis] {
                continue;
            }
            let offset = start[axis] - half[axis].copysign(point);
            quadratic += delta[axis] * delta[axis];
            linear += delta[axis] * offset;
        }
        let t = if quadratic > 0.0 {
            (-linear / quadratic).clamp(interval[0], interval[1])
        } else {
            midpoint
        };
        best = best.min(distance(t));
    }
    best
}

impl World {
    pub(super) fn load_persistent_trigger_volumes(&mut self, cell: GlobalFormId) {
        self.ensure_trigger_index(Some(cell));
        let ids = self
            .trigger_index
            .as_ref()
            .and_then(|index| index.get(&cell))
            .cloned()
            .unwrap_or_default();
        for id in ids {
            let persistent = self
                .order
                .winner(id)
                .and_then(|record| record.placement.cell)
                .and_then(|raw| self.resolve_from(id, raw))
                != Some(cell);
            if !persistent {
                continue;
            }
            let enabled = self.reference_enabled(id);
            if enabled && self.loaded_3d_references.insert(id) {
                self.reference(id);
                self.commonlib_object_loaded(id, true);
                self.pending.push(Pending::ReferenceEvent {
                    reference: id,
                    name: "OnLoad".into(),
                });
            } else if !enabled && self.loaded_3d_references.remove(&id) {
                self.commonlib_object_loaded(id, false);
                self.pending.push(Pending::ReferenceEvent {
                    reference: id,
                    name: "OnUnload".into(),
                });
            }
        }
    }

    pub(super) fn unload_persistent_trigger_volumes(&mut self, cell: GlobalFormId) {
        let ids = self
            .trigger_index
            .as_ref()
            .and_then(|index| index.get(&cell))
            .cloned()
            .unwrap_or_default();
        for id in ids {
            let persistent = self
                .order
                .winner(id)
                .and_then(|record| record.placement.cell)
                .and_then(|raw| self.resolve_from(id, raw))
                != Some(cell);
            let retained = self.attached_cells.iter().any(|cell| {
                self.trigger_index
                    .as_ref()
                    .and_then(|index| index.get(cell))
                    .is_some_and(|ids| ids.contains(&id))
            });
            if persistent && !retained && self.loaded_3d_references.remove(&id) {
                self.commonlib_object_loaded(id, false);
                self.pending.push(Pending::ReferenceEvent {
                    reference: id,
                    name: "OnUnload".into(),
                });
            }
        }
    }

    /// Returns the current actor count for an XPRM box or sphere reference.
    ///
    /// `None` means the reference has no supported trigger primitive.
    pub fn trigger_object_count(&self, trigger: GlobalFormId) -> Option<usize> {
        let (_, refr) = self.trigger(trigger)?;
        TriggerVolume::from_refr(&refr, self.order.winner_ref(trigger)?.file, self)?;
        Some(
            self.trigger_occupants
                .get(&trigger)
                .map_or(0, std::collections::BTreeSet::len),
        )
    }

    /// Updates one actor's trigger membership after a movement step.
    ///
    /// Enter and leave events are queued once when membership changes. A
    /// move to another cell removes the actor from triggers in its old cell.
    pub fn update_trigger_actor_position(
        &mut self,
        actor: GlobalFormId,
        cell: Option<GlobalFormId>,
        from: [f32; 3],
        to: [f32; 3],
        alloc: Alloc<'_>,
    ) {
        self.update_trigger_membership(actor, cell, from, to, None, alloc);
    }

    /// Update trigger membership using the actor contact capsule and its sweep.
    /// Dimensions follow the shared generated controller and current record scale.
    pub fn update_trigger_actor_body(
        &mut self,
        actor: GlobalFormId,
        cell: Option<GlobalFormId>,
        from: [f32; 3],
        to: [f32; 3],
        alloc: Alloc<'_>,
    ) {
        let capsule = ActorCapsule::from_world(self, actor);
        self.update_trigger_membership(actor, cell, from, to, capsule, alloc);
    }

    fn update_trigger_membership(
        &mut self,
        actor: GlobalFormId,
        cell: Option<GlobalFormId>,
        from: [f32; 3],
        to: [f32; 3],
        capsule: Option<ActorCapsule>,
        alloc: Alloc<'_>,
    ) {
        let cell = cell.map(|cell| {
            let index = self.order.build_cell_index();
            index
                .persistent_cell_world(cell)
                .and_then(|world| {
                    index.exterior_cell(
                        world,
                        (to[0] / 4096.0).floor() as i32,
                        (to[1] / 4096.0).floor() as i32,
                    )
                })
                .unwrap_or(cell)
        });
        self.ensure_trigger_index(cell);
        if let Some(cell) = cell.filter(|cell| self.attached_cells.contains(cell)) {
            self.load_persistent_trigger_volumes(cell);
        }
        let mut candidates = cell
            .and_then(|cell| self.trigger_index.as_ref()?.get(&cell).cloned())
            .unwrap_or_default();
        candidates.extend(
            self.trigger_occupants
                .iter()
                .filter_map(|(id, occupants)| occupants.contains(&actor).then_some(*id)),
        );
        candidates.sort_unstable();
        candidates.dedup();

        let mut transitions = Vec::new();
        for id in candidates {
            let Some(record) = self.order.winner(id) else {
                continue;
            };
            if record.tag != tag::REFR {
                continue;
            }
            let Some(rref) = self.order.winner_ref(id) else {
                continue;
            };
            let Ok(refr) = Refr::from_record(record) else {
                continue;
            };
            let Some(volume) = TriggerVolume::from_refr(&refr, rref.file, self) else {
                continue;
            };
            let was_inside = self
                .trigger_occupants
                .get(&id)
                .is_some_and(|occupants| occupants.contains(&actor));
            let same_cell = cell.is_some_and(|cell| volume.matches_cell(cell, self));
            let enabled = self.reference_enabled(id);
            let dead = self.actors.get(&actor).is_some_and(|state| state.dead);
            let should_be_inside = same_cell
                && enabled
                && !dead
                && capsule.map_or_else(
                    || volume.contains(to),
                    |capsule| volume.overlaps_capsule(to, capsule),
                );
            if was_inside != should_be_inside {
                transitions.push((id, should_be_inside));
            } else if same_cell
                && !was_inside
                && !should_be_inside
                && enabled
                && !dead
                && capsule.map_or_else(
                    || volume.intersects_segment(from, to),
                    |capsule| volume.intersects_swept_capsule(from, to, capsule),
                )
            {
                transitions.push((id, true));
                transitions.push((id, false));
            }
        }

        for (trigger, entering) in transitions {
            let (changed, empty) = {
                let occupants = self.trigger_occupants.entry(trigger).or_default();
                let changed = if entering {
                    occupants.insert(actor)
                } else {
                    occupants.remove(&actor)
                };
                (changed, occupants.is_empty())
            };
            if !changed {
                continue;
            }
            let action = self.form_value(Some(actor), alloc);
            let object = self.object_of(Handle::Form(trigger), alloc);
            self.pending.push(Pending::Event {
                object,
                name: if entering {
                    "OnTriggerEnter".into()
                } else {
                    "OnTriggerLeave".into()
                },
                args: vec![action],
            });
            if !entering && empty {
                self.trigger_occupants.remove(&trigger);
            }
        }
    }

    fn trigger(&self, id: GlobalFormId) -> Option<(usize, Refr)> {
        let rref = self.order.winner_ref(id)?;
        let record = self.order.record(rref);
        (record.tag == Tag::new(b"REFR"))
            .then(|| Refr::from_record(record).ok())
            .flatten()
            .map(|refr| (rref.file, refr))
    }

    fn ensure_trigger_index(&mut self, cell: Option<GlobalFormId>) {
        let Some(cell) = cell else {
            return;
        };
        if self
            .trigger_index
            .as_ref()
            .is_some_and(|index| index.contains_key(&cell))
        {
            return;
        }
        let mut triggers = Vec::new();
        let mut references = self.cell_reference_ids(cell);
        if let Some(persistent) = self.order.cell_index().and_then(|index| {
            index
                .grid_of(cell)
                .and_then(|grid| index.world_persistent_cell(grid.world))
        }) {
            references.extend(self.cell_reference_ids(persistent));
        }
        for id in references {
            let Some(record) = self.order.winner(id).filter(|record| {
                record.tag == tag::REFR && record.find(Tag::new(b"XPRM")).is_some()
            }) else {
                continue;
            };
            let Some(rref) = self.order.winner_ref(id) else {
                continue;
            };
            let Ok(refr) = Refr::from_record(record) else {
                continue;
            };
            let Some(volume) = TriggerVolume::from_refr(&refr, rref.file, self) else {
                continue;
            };
            if volume.matches_cell(cell, self) {
                triggers.push(id);
            }
        }
        triggers.sort_unstable();
        self.trigger_index
            .get_or_insert_with(std::collections::BTreeMap::new)
            .insert(cell, triggers);
    }
}

fn inverse_rotate(point: [f32; 3], rotation: [f32; 3]) -> [f32; 3] {
    let [sx, sy, sz] = rotation.map(|angle| (-angle).sin());
    let [cx, cy, cz] = rotation.map(|angle| (-angle).cos());
    let rz = [
        cz * point[0] - sz * point[1],
        sz * point[0] + cz * point[1],
        point[2],
    ];
    let ry = [cy * rz[0] + sy * rz[2], rz[1], -sy * rz[0] + cy * rz[2]];
    [ry[0], cx * ry[1] - sx * ry[2], sx * ry[1] + cx * ry[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capsule() -> ActorCapsule {
        ActorCapsule {
            ends: [[0.0, 0.0, 2.0], [0.0, 0.0, 8.0]],
            radius: 2.0,
        }
    }

    fn box_volume() -> TriggerVolume {
        TriggerVolume {
            cell: None,
            center: [0.0; 3],
            rotation: [0.0; 3],
            scale: 1.0,
            shape: VolumeShape::Box([10.0; 3]),
        }
    }

    #[test]
    fn ply_03_trigger_detects_capsule_side_and_top_without_footpoint_entry() {
        let volume = box_volume();
        assert!(volume.overlaps_capsule([11.9, 0.0, 0.0], capsule()));
        assert!(volume.overlaps_capsule([0.0, 0.0, -19.9], capsule()));
        assert!(!volume.overlaps_capsule([12.1, 0.0, 0.0], capsule()));
        assert!(!volume.overlaps_capsule([0.0, 0.0, -20.1], capsule()));
        assert!(!volume.overlaps_capsule([11.5, 11.5, 0.0], capsule()));
    }

    #[test]
    fn ply_03_trigger_detects_an_axis_crossing_with_both_cap_centers_outside() {
        let volume = TriggerVolume {
            shape: VolumeShape::Box([1.0; 3]),
            ..box_volume()
        };
        let capsule = ActorCapsule {
            ends: [[0.0, 0.0, 0.25], [0.0, 0.0, 9.75]],
            radius: 0.25,
        };
        assert!(volume.overlaps_capsule([0.0, 0.0, -5.0], capsule));
        assert!(!volume.overlaps_capsule([1.3, 0.0, -5.0], capsule));
    }

    #[test]
    fn ply_03_trigger_capsule_respects_rotated_and_scaled_box() {
        let volume = TriggerVolume {
            rotation: [0.0, 0.0, std::f32::consts::FRAC_PI_2],
            scale: 2.0,
            shape: VolumeShape::Box([10.0, 1.0, 3.0]),
            ..box_volume()
        };
        assert!(volume.overlaps_capsule([3.9, 19.0, 0.0], capsule()));
        assert!(!volume.overlaps_capsule([4.1, 19.0, 0.0], capsule()));
        let tilted = TriggerVolume {
            rotation: [std::f32::consts::FRAC_PI_2, 0.0, 0.0],
            shape: VolumeShape::Box([10.0, 1.0, 3.0]),
            ..box_volume()
        };
        assert!(tilted.overlaps_capsule([0.0, 4.0, -3.0], capsule()));
        assert!(!tilted.overlaps_capsule([0.0, 5.1, -3.0], capsule()));
    }

    #[test]
    fn ply_03_trigger_sweep_detects_body_only_crossing_and_rejects_corners() {
        let volume = box_volume();
        assert!(volume.intersects_swept_capsule(
            [-100.0, 11.9, 0.0],
            [100.0, 11.9, 0.0],
            capsule(),
        ));
        assert!(!volume.intersects_swept_capsule(
            [-100.0, 12.1, 0.0],
            [100.0, 12.1, 0.0],
            capsule(),
        ));
        assert!(!volume.intersects_swept_capsule(
            [11.5, 11.5, -100.0],
            [11.5, 11.5, 100.0],
            capsule(),
        ));
    }

    #[test]
    fn ply_03_sphere_trigger_uses_capsule_axis_and_caps() {
        let volume = TriggerVolume {
            shape: VolumeShape::Sphere(10.0),
            ..box_volume()
        };
        assert!(volume.overlaps_capsule([11.9, 0.0, -5.0], capsule()));
        assert!(volume.overlaps_capsule([0.0, 0.0, -19.9], capsule()));
        assert!(!volume.overlaps_capsule([11.9, 0.0, 0.0], capsule()));
        assert!(!volume.overlaps_capsule([0.0, 0.0, -20.1], capsule()));
        assert!(volume.intersects_swept_capsule(
            [-100.0, 11.9, -5.0],
            [100.0, 11.9, -5.0],
            capsule(),
        ));
    }

    #[test]
    fn ply_03_installed_route_overlaps_the_second_stage_120_box_with_its_capsule() {
        let (Some(data), Some(saves)) = (
            std::env::var_os("SKYRIM_DATA"),
            std::env::var_os("SOE_TEST_SAVES"),
        ) else {
            eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping capsule route survey");
            return;
        };
        let source = std::path::PathBuf::from(saves)
            .join("Main Quest + Dawnguard(Vampire) + Dragonborn")
            .join("01 Right Off The Block.ess");
        let mut game =
            crate::play::load_world_checkpoint(std::path::Path::new(&data), &source).unwrap();
        let route = crate::play::Route::parse(include_str!("../../tests/fixtures/ply_03_01.route"))
            .unwrap();
        let report = crate::play::run(&mut game, &route);
        assert!(report.passed(), "{}", report.format());
        let samples: Vec<[f32; 3]> = report
            .trace
            .iter()
            .filter_map(|line| line.split_once(": navigate cell "))
            .filter_map(|(_, line)| line.split_once(" position "))
            .map(|(_, point)| {
                point
                    .trim_matches(['[', ']'])
                    .split(", ")
                    .map(|value| value.parse::<f32>().unwrap())
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap()
            })
            .collect();
        assert!(!samples.is_empty());
        let player = game.player();
        let mut world = game.world_mut();
        let capsule = ActorCapsule::from_world(&mut world, player).unwrap();
        let mut point_counts = [0; 2];
        let mut capsule_counts = [0; 2];
        for (index, object) in [0xcd6ac, 0xed5fc].into_iter().enumerate() {
            let id = GlobalFormId { file: 0, object };
            let (file, refr) = world.trigger(id).unwrap();
            let volume = TriggerVolume::from_refr(&refr, file, &world).unwrap();
            point_counts[index] = samples
                .iter()
                .filter(|point| volume.contains(**point))
                .count();
            capsule_counts[index] = samples
                .iter()
                .filter(|point| volume.overlaps_capsule(**point, capsule))
                .count();
            eprintln!(
                "stage 120 trigger {id}: {} points, {} capsule contacts, enabled {}",
                point_counts[index],
                capsule_counts[index],
                world.reference_enabled(id),
            );
        }
        assert_eq!(point_counts, [0, 0]);
        assert_eq!(capsule_counts[0], 0);
        assert!(capsule_counts[1] > 0);
    }

    #[test]
    fn hst_14_rotated_box_containment_uses_reference_transform() {
        let volume = TriggerVolume {
            cell: None,
            center: [0.0; 3],
            rotation: [0.0, 0.0, std::f32::consts::FRAC_PI_2],
            scale: 1.0,
            shape: VolumeShape::Box([10.0, 2.0, 3.0]),
        };
        assert!(volume.contains([0.0, 8.0, 0.0]));
        assert!(!volume.contains([8.0, 0.0, 0.0]));
    }
}
