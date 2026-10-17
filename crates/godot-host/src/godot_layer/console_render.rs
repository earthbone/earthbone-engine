// SPDX-License-Identifier: GPL-3.0-or-later

//! UESP TG, TT and TWS over existing server instances. Record tags classify batches.

use super::{BatchTable, CellStreamerNode, LodStream, MaterialDesc, RenderingServer};
use godot::prelude::*;

impl CellStreamerNode {
    pub(super) fn console_render_kind(&self, table: &BatchTable, material: &MaterialDesc) -> usize {
        if material.water.is_some() {
            return 3;
        }
        let reference = match table {
            BatchTable::Triangles(members) => members.first().map(|m| m.reference),
            BatchTable::Instances(members) => members.first().copied(),
        };
        let Some((source, reference)) = self.data_source.as_ref().zip(reference) else {
            return 0;
        };
        let order = source.order();
        let Some(record) = order.winner(reference) else {
            return 0;
        };
        let tag = if record.tag == soe_esm::tag::REFR {
            order.winner_ref(reference).and_then(|owner| {
                soe_esm::records::refr::Refr::from_record(record)
                    .ok()
                    .and_then(|refr| order.winner(order.resolve(owner.file, refr.base)))
                    .map(|base| base.tag)
            })
        } else {
            Some(record.tag)
        };
        match tag.map(|tag| *tag.as_bytes()) {
            Some([b'G', b'R', b'A', b'S']) => 1,
            Some([b'T', b'R', b'E', b'E']) => 2,
            _ => 0,
        }
    }

    pub(crate) fn toggle_console_geometry(&mut self, name: &str) -> bool {
        let kind = match name {
            "tg" => 1,
            "tt" => 2,
            _ => 3,
        };
        self.console_visibility[kind] = !self.console_visibility[kind];
        let visible = self.console_visibility[kind];
        let mut server = RenderingServer::singleton();
        for instance in &self.instances {
            if instance.render_kind == kind {
                server.instance_set_visible(
                    instance.rid,
                    visible && !instance.arrays.indices.is_empty(),
                );
            }
        }
        if kind == 2 {
            self.set_tree_lod_visible(self.lod_visibility[2]);
        }
        visible
    }

    pub(super) fn console_lod_visible(&self, kind: LodStream) -> bool {
        match kind {
            LodStream::Terrain => self.lod_visibility[0],
            LodStream::Object => self.lod_visibility[1],
            LodStream::Tree => self.lod_visibility[2] && self.console_visibility[2],
        }
    }

    pub(crate) fn toggle_console_borders(&mut self) -> bool {
        self.console_borders_visible = !self.console_borders_visible;
        self.refresh_console_borders();
        self.console_borders_visible
    }

    pub(super) fn free_console_borders(&mut self) {
        if let Some((rid, _mesh)) = self.console_borders.take() {
            RenderingServer::singleton().free_rid(rid);
        }
    }

    pub(super) fn refresh_console_borders(&mut self) {
        self.free_console_borders();
        if !self.console_borders_visible {
            return;
        }
        let Some(scenario) = self.scenario() else {
            return;
        };
        let mut cells: Vec<_> = self.loaded_terrain_heights().into_iter().collect();
        cells.sort_by_key(|(coord, _)| (coord.x, coord.y));
        let points: PackedVector3Array = cells
            .iter()
            .flat_map(|(coord, heights)| border_points(*coord, heights))
            .map(|p| Vector3::new(p[0], p[1], p[2]))
            .collect();
        if points.is_empty() {
            return;
        }
        let mut arrays = VarArray::new();
        for _ in 0..super::ArrayType::MAX.ord() {
            arrays.push(&Variant::nil());
        }
        arrays.set(
            super::ArrayType::VERTEX.ord() as usize,
            &points.to_variant(),
        );
        let mut mesh = super::ArrayMesh::new_gd();
        mesh.add_surface_from_arrays(super::PrimitiveType::LINES, &arrays);
        let mut material = super::StandardMaterial3D::new_gd();
        material.set_shading_mode(godot::classes::base_material_3d::ShadingMode::UNSHADED);
        material.set_albedo(Color::WHITE);
        mesh.surface_set_material(0, &material);
        let rid = RenderingServer::singleton().instance_create2(mesh.get_rid(), scenario);
        self.console_borders = Some((rid, mesh));
    }
}

fn border_points(coord: crate::streaming::CellCoord, heights: &[f32]) -> Vec<[f32; 3]> {
    let size = crate::units::LAND_SAMPLES;
    if heights.len() != size * size {
        return Vec::new();
    }
    let point = |x: usize, y: usize| {
        [
            (coord.x as f32 + x as f32 / (size - 1) as f32) * crate::units::CELL_METRES,
            heights[y * size + x] + 0.03,
            -(coord.y as f32 + y as f32 / (size - 1) as f32) * crate::units::CELL_METRES,
        ]
    };
    let mut points = Vec::with_capacity((size - 1) * 8);
    for i in 0..size - 1 {
        for ((x, y), (next_x, next_y)) in [
            ((i, 0), (i + 1, 0)),
            ((i, size - 1), (i + 1, size - 1)),
            ((0, i), (0, i + 1)),
            ((size - 1, i), (size - 1, i + 1)),
        ] {
            points.extend([point(x, y), point(next_x, next_y)]);
        }
    }
    points
}

#[cfg(test)]
mod tests {
    #[test]
    fn borders_follow_land_edges_and_negative_cell_coordinates() {
        let size = crate::units::LAND_SAMPLES;
        let heights: Vec<_> = (0..size * size).map(|i| i as f32).collect();
        let points = super::border_points(crate::streaming::CellCoord::new(-2, 3), &heights);
        assert_eq!(points.len(), (size - 1) * 8);
        assert_eq!(
            points[0],
            [
                -2.0 * crate::units::CELL_METRES,
                0.03,
                -3.0 * crate::units::CELL_METRES
            ]
        );
        assert!((points[1][1] - 1.03).abs() < 0.0001);
        assert!(super::border_points(crate::streaming::CellCoord::new(0, 0), &[]).is_empty());
    }
}
