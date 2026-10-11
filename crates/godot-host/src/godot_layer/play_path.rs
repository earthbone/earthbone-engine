// SPDX-License-Identifier: GPL-3.0-or-later

//! TST-19 inspects resources submitted by the production streamer.
//! Godot RenderingServer and ArrayMesh APIs supply the observation boundary.

use super::{BatchTable, CellStreamerNode, InstanceMesh};
use godot::classes::ArrayMesh;
use godot::prelude::*;

fn surface_row(rid: Rid, mesh: &Gd<ArrayMesh>, bounds: Aabb, kind: &str) -> VarDictionary {
    let mut row = VarDictionary::new();
    row.set("rid", rid);
    row.set("mesh", mesh);
    row.set("bounds", bounds);
    row.set("kind", kind);
    row
}

#[godot_api(secondary)]
impl CellStreamerNode {
    /// Uploaded batches, doors and object LOD with bounds and reference IDs.
    /// Pixel controls can temporarily hide the returned RenderingServer RID.
    #[func]
    pub fn play_path_surfaces(&self) -> Array<VarDictionary> {
        let mut rows = Array::new();
        for instance in &self.instances {
            let mesh = match &instance.mesh {
                InstanceMesh::Array(mesh) => Some(mesh.clone()),
                InstanceMesh::Multi(multi) => multi
                    .get_mesh()
                    .and_then(|mesh| mesh.try_cast::<ArrayMesh>().ok()),
            };
            let Some(mesh) = mesh else { continue };
            let min = instance.bounds.min;
            let max = instance.bounds.max;
            let bounds = Aabb::new(
                Vector3::new(min.x, min.y, min.z),
                Vector3::new(max.x - min.x, max.y - min.y, max.z - min.z),
            );
            let kind = if instance
                .arrays
                .material
                .diffuse
                .as_deref()
                .is_some_and(|path| path.starts_with("terrain:"))
            {
                "terrain"
            } else {
                "object"
            };
            let mut row = surface_row(instance.rid, &mesh, bounds, kind);
            let references: PackedStringArray = match &instance.table {
                BatchTable::Triangles(members) => members
                    .iter()
                    .map(|member| GString::from(member.reference.to_string().as_str()))
                    .collect(),
                BatchTable::Instances(members) => members
                    .iter()
                    .map(|member| GString::from(member.to_string().as_str()))
                    .collect(),
            };
            row.set("references", &references);
            rows.push(&row);
        }
        if let Some(lod) = &self.object_lod {
            let mut blocks: Vec<_> = lod.blocks.iter().collect();
            blocks.sort_by_key(|(coord, _)| (coord.level.cells(), coord.x, coord.y));
            for (_, instances) in blocks {
                for instance in instances {
                    rows.push(&surface_row(
                        instance.rid,
                        &instance.mesh,
                        instance.mesh.get_aabb(),
                        "lod",
                    ));
                }
            }
        }
        let mut doors: Vec<_> = self.doors.iter().collect();
        doors.sort_by_key(|(id, _)| **id);
        for (id, door) in doors {
            for surface in &door.surfaces {
                let mut row =
                    surface_row(surface.rid, &surface.mesh, surface.mesh.get_aabb(), "door");
                row.set(
                    "references",
                    &PackedStringArray::from(&[GString::from(id.to_string().as_str())]),
                );
                rows.push(&row);
            }
        }
        rows
    }

    /// Highest of the four LAND samples around a Godot-space point.
    /// This conservative support height never raycasts the object being checked.
    #[func]
    pub fn play_path_terrain_height(&self, point: Vector3) -> Variant {
        if !point.is_finite() {
            return Variant::nil();
        }
        for instance in &self.instances {
            if !instance
                .arrays
                .material
                .diffuse
                .as_deref()
                .is_some_and(|path| path.starts_with("terrain:"))
            {
                continue;
            }
            let bounds = instance.bounds;
            if point.x < bounds.min.x
                || point.x > bounds.max.x
                || point.z < bounds.min.z
                || point.z > bounds.max.z
            {
                continue;
            }
            let spacing = crate::units::CELL_METRES / 32.0;
            let col = ((point.x - bounds.min.x) / spacing)
                .floor()
                .clamp(0.0, 32.0) as usize;
            let row = ((bounds.max.z - point.z) / spacing)
                .floor()
                .clamp(0.0, 32.0) as usize;
            let mut height: Option<f32> = None;
            for r in [row, (row + 1).min(32)] {
                for c in [col, (col + 1).min(32)] {
                    if let Some(vertex) = instance.arrays.positions.get(r * 33 + c) {
                        height = Some(height.map_or(vertex[1], |h| h.max(vertex[1])));
                    }
                }
            }
            return height.map_or_else(Variant::nil, |h| (h * 70.0).to_variant());
        }
        Variant::nil()
    }
}
