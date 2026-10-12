// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only TST-19 observations of the shared World and capsule collision.
//! The play route uses the existing gameplay operations and Godot input API.

use godot::prelude::*;
use soe_esm::{GlobalFormId, Tag};
use soe_physics::{PhysicsWorld, Vec3};

use super::PlayerNode;

#[godot_api(secondary)]
impl PlayerNode {
    /// Existing NAVM path used to steer scripted key input, without moving World.
    #[func]
    pub fn play_path_navigation(&self, destination: Vector3) -> PackedVector3Array {
        let Some(game) = &self.game else {
            return PackedVector3Array::new();
        };
        let world = game.world();
        let Some(graph) = world.ai.nav_graph() else {
            return PackedVector3Array::new();
        };
        let feet = self.feet();
        graph
            .find_path_between(
                [feet.x, feet.y, feet.z],
                [destination.x, destination.y, destination.z],
            )
            .map_or_else(PackedVector3Array::new, |path| {
                path.points
                    .iter()
                    .map(|point| Vector3::new(point[0], point[1], point[2]))
                    .collect()
            })
    }

    /// Enabled references in the player's resident cells, sorted by FormID.
    /// Positions come from World, rather than a separate test world.
    #[func]
    pub fn play_path_references(&self) -> Array<VarDictionary> {
        let (Some(game), Some(source), Some(cell)) =
            (&self.game, &self.data_source, self.loaded_cell)
        else {
            return Array::new();
        };
        let mut cells = vec![cell];
        if let Some(exterior) = &self.exterior {
            cells.extend(exterior.cells.keys().copied());
        }
        cells.sort_unstable();
        cells.dedup();
        let mut world = game.world_mut();
        let mut references: Vec<_> = cells
            .into_iter()
            .flat_map(|cell| world.cell_references(cell))
            .collect();
        references.sort_unstable();
        references.dedup();
        let mut rows = Array::new();
        for id in references {
            if id == world.player() || !world.reference_enabled(id) {
                continue;
            }
            let Some(base) = world.base_of(id) else {
                continue;
            };
            let Some(record) = world.record(base) else {
                continue;
            };
            let kind = record.tag.to_string();
            let editor_id = record.editor_id().unwrap_or_default();
            let model = record
                .find(Tag(*b"MODL"))
                .map(|bytes| {
                    String::from_utf8_lossy(bytes)
                        .trim_end_matches('\0')
                        .to_owned()
                })
                .unwrap_or_default();
            let name = world.name_of(id).unwrap_or_default();
            let Some(pose) = world.ref_placement(id) else {
                continue;
            };
            let mut row = VarDictionary::new();
            row.set("id", id.to_string().as_str());
            row.set("file", i64::from(id.file));
            row.set("object", i64::from(id.object));
            row.set("base_file", i64::from(base.file));
            row.set("base_object", i64::from(base.object));
            row.set("kind", kind.as_str());
            row.set("editor_id", editor_id.as_str());
            row.set("model", model.as_str());
            row.set("name", name.as_str());
            row.set(
                "position",
                Vector3::new(pose.position[0], pose.position[1], pose.position[2]),
            );
            row.set("visible", pose.visible);
            row.set("exterior", source.exterior_world(cell).is_some());
            rows.push(&row);
        }
        rows
    }

    /// Current inventory quantity, using the same item initialization as menus.
    #[func]
    pub fn play_path_item_count(&self, file: i32, object: i64) -> i32 {
        let (Some(game), Ok(file), Ok(object)) =
            (&self.game, u16::try_from(file), u32::try_from(object))
        else {
            return -1;
        };
        let mut world = game.world_mut();
        let player = world.player();
        world.item_count(player, GlobalFormId { file, object })
    }

    /// Ground under a Skyrim-space point, including its record-selected material.
    #[func]
    pub fn play_path_ground(&self, position: Vector3) -> VarDictionary {
        let mut row = VarDictionary::new();
        if !position.is_finite() {
            return row;
        }
        let Some(physics) = &self.world else {
            return row;
        };
        let point = Vec3::new(position.x, position.y, position.z);
        let Some(hit) = physics.raycast(point + Vec3::Z * 8.0, point - Vec3::Z * 8192.0) else {
            return row;
        };
        row.set("height", hit.point.z);
        row.set("normal_z", hit.normal.z);
        row.set("material_hash", i64::from(hit.material.unwrap_or_default()));
        let material = self.data_source.as_ref().and_then(|source| {
            let footsteps = soe_game::footsteps::Footsteps::new(source.order());
            hit.material
                .and_then(|hash| footsteps.material(hash).ok().flatten())
                .and_then(|id| source.order().winner(id)?.editor_id())
        });
        row.set("material", material.as_deref().unwrap_or("unresolved"));
        row
    }

    /// Whether static collision leaves the camera-to-target segment unobstructed.
    #[func]
    pub fn play_path_line_of_sight(&self, target: Vector3) -> bool {
        if !target.is_finite() {
            return false;
        }
        let Some(physics) = &self.world else {
            return false;
        };
        let eye = self.feet() + Vec3::Z * 115.0;
        let target = Vec3::new(target.x, target.y, target.z);
        physics
            .raycast(eye, target)
            .is_none_or(|hit| (hit.point - target).length() < 24.0)
    }
}
