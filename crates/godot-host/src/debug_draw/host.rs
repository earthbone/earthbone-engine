// SPDX-License-Identifier: GPL-3.0-or-later

//! One optional ImmediateMesh and one text label, following Godot 4.4's local
//! ImmediateMesh and VisualInstance3D contracts. No per-reference debug nodes.

use godot::classes::base_material_3d::{CullMode, Flags, ShadingMode, Transparency};
use godot::classes::{ImmediateMesh, Label3D, MeshInstance3D, Node3D, StandardMaterial3D};
use godot::prelude::*;
use soe_game::Game;

use super::{Frame, LoadedNavmesh, DEBUG_LAYER};

#[derive(Default)]
pub(crate) struct Draw {
    node: Option<Gd<MeshInstance3D>>,
    label: Option<Gd<Label3D>>,
    mesh: Option<Gd<ImmediateMesh>>,
    material: Option<Gd<StandardMaterial3D>>,
    navmeshes: Vec<LoadedNavmesh>,
    cells: Vec<soe_esm::GlobalFormId>,
    revision: u64,
    pub(crate) counts: VarDictionary,
}

impl Draw {
    pub(crate) fn clear(&mut self) {
        if let Some(mut node) = self.node.take() {
            node.queue_free();
        }
        if let Some(mut label) = self.label.take() {
            label.queue_free();
        }
        self.mesh = None;
        self.material = None;
        self.navmeshes.clear();
        self.cells.clear();
        self.counts = VarDictionary::new();
    }

    pub(crate) fn update(&mut self, parent: &mut Gd<Node3D>, game: &mut Game) {
        if !game.debug_views.drawing() {
            if self.node.is_some() {
                self.clear();
            }
            return;
        }
        let started = std::time::Instant::now();
        let player = game.player();
        let mut world = game.world_mut();
        let cells: Vec<_> = world
            .order()
            .winners_of(soe_esm::Tag(*b"CELL"))
            .filter_map(|(id, _)| world.cell_is_attached(id).then_some(id))
            .collect();
        let revision = world
            .ai
            .nav_graph()
            .map_or(0, |graph| graph.query_revision());
        if self.node.is_none() || cells != self.cells || revision != self.revision {
            self.navmeshes = super::loaded_navmeshes(&world);
            self.cells = cells;
            self.revision = revision;
        }
        let frame = super::frame(&mut world, &game.debug_views, &self.navmeshes, player);
        let position = world.ref_placement(player).map_or([0.0; 3], |p| p.position);
        drop(world);
        if self.node.is_none() {
            let mut material = StandardMaterial3D::new_gd();
            material.set_shading_mode(ShadingMode::UNSHADED);
            material.set_transparency(Transparency::ALPHA);
            material.set_cull_mode(CullMode::DISABLED);
            material.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
            material.set_flag(Flags::DISABLE_DEPTH_TEST, true);
            let mesh = ImmediateMesh::new_gd();
            let mut node = MeshInstance3D::new_alloc();
            node.set_name("SoeDebugGeometry");
            node.set_layer_mask(DEBUG_LAYER);
            node.set_mesh(&mesh);
            node.set_as_top_level(true);
            parent.add_child(&node);
            let mut label = Label3D::new_alloc();
            label.set_name("SoeDebugText");
            label.set_layer_mask(DEBUG_LAYER);
            label.set_font_size(24);
            label.set_billboard_mode(godot::classes::base_material_3d::BillboardMode::ENABLED);
            label.set_pixel_size(0.002);
            label.set_as_top_level(true);
            parent.add_child(&label);
            self.node = Some(node);
            self.label = Some(label);
            self.mesh = Some(mesh);
            self.material = Some(material);
        }
        if let (Some(mesh), Some(material)) = (&mut self.mesh, &self.material) {
            upload(mesh, material, &frame);
        }
        if let Some(label) = &mut self.label {
            label.set_text(&frame.text.join("\n"));
            let camera = parent
                .get_viewport()
                .and_then(|viewport| viewport.get_camera_3d());
            let anchor = camera.map_or_else(
                || point([position[0], position[1], position[2] + 160.0]),
                |camera| {
                    let transform = camera.get_global_transform();
                    transform.origin - transform.basis.col_c() * 3.0 + transform.basis.col_b() * 0.5
                },
            );
            label.set_global_position(anchor);
        }
        let world = game.world();
        let actor_path_points: usize = world
            .loaded_actors()
            .iter()
            .filter_map(|id| {
                let cell = world.reference_ref(*id)?.cell?;
                world
                    .cell_is_attached(cell)
                    .then(|| world.actor_ref(*id).map(|actor| actor.ai.path.len()))
                    .flatten()
            })
            .sum();
        self.counts = vdict! {
            "nodes" => 2, "render_layer" => i64::from(DEBUG_LAYER),
            "triangles" => frame.navmesh_triangles as i64, "highlighted" => frame.highlighted as i64,
            "path_segments" => frame.path_segments as i64, "waypoints" => frame.waypoints as i64,
            "edge_links" => frame.edge_links as i64, "door_links" => frame.door_links as i64,
            "teleport_links" => frame.teleport_links as i64, "cover_edges" => frame.cover_edges as i64,
            "triangle_vertices" => (frame.triangles.len() * 3) as i64,
            "line_vertices" => (frame.lines.len() * 2) as i64,
            "surface_count" => self.mesh.as_ref().map_or(0, |mesh| mesh.get_surface_count()),
            "update_us" => started.elapsed().as_secs_f64() * 1e6,
            "loaded_navmesh_triangles" => self.navmeshes.iter().map(|mesh| mesh.geometry.triangles.len() as i64).sum::<i64>(),
            "actor_path_points" => actor_path_points as i64,
        };
    }
}

fn point(position: [f32; 3]) -> Vector3 {
    let p = crate::units::SkyrimPos::new(position[0], position[1], position[2]).to_godot();
    Vector3::new(p.x, p.y, p.z)
}

fn upload(mesh: &mut Gd<ImmediateMesh>, material: &Gd<StandardMaterial3D>, frame: &Frame) {
    mesh.clear_surfaces();
    if !frame.triangles.is_empty() {
        mesh.surface_begin_ex(godot::classes::mesh::PrimitiveType::TRIANGLES)
            .material(material)
            .done();
        for primitive in &frame.triangles {
            mesh.surface_set_color(Color::from_rgba(
                primitive.color[0],
                primitive.color[1],
                primitive.color[2],
                primitive.color[3],
            ));
            for position in primitive.points {
                mesh.surface_add_vertex(point(position));
            }
        }
        mesh.surface_end();
    }
    if !frame.lines.is_empty() {
        mesh.surface_begin_ex(godot::classes::mesh::PrimitiveType::LINES)
            .material(material)
            .done();
        for primitive in &frame.lines {
            mesh.surface_set_color(Color::from_rgba(
                primitive.color[0],
                primitive.color[1],
                primitive.color[2],
                primitive.color[3],
            ));
            for position in primitive.points {
                mesh.surface_add_vertex(point(position));
            }
        }
        mesh.surface_end();
    }
}
