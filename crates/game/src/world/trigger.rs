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
            let should_be_inside = same_cell && enabled && !dead && volume.contains(to);
            if was_inside != should_be_inside {
                transitions.push((id, should_be_inside));
            } else if same_cell
                && !was_inside
                && !should_be_inside
                && enabled
                && !dead
                && volume.intersects_segment(from, to)
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
