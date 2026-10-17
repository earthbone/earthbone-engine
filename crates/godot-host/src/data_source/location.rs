// SPDX-License-Identifier: GPL-3.0-or-later

//! Host resources for a logical player destination. Exterior persistent cells
//! identify a worldspace, while the landing position chooses its grid cells.
//! CellIndex and the existing parent LAND path supply the same record semantics
//! used by the exterior renderer. Logical reference ownership is left intact.

use std::collections::{BTreeMap, HashMap, HashSet};

use soe_esm::{tag, GlobalFormId};
use soe_physics::{BodyId, Collider, Iso, PhysicsWorld, Shape, SimpleWorld, Vec3};

use super::{DataSource, DataSourceError, ExteriorBlock, ExteriorCell, LoadedCell};
use crate::collision::{CellCollision, CollisionStats, PlacedCollider};
#[cfg(feature = "godot")]
use crate::streaming::CellCoord;
use crate::units::{GodotPos, CELL_UNITS};

pub struct LoadedLocation {
    pub cells: Vec<(GlobalFormId, LoadedCell)>,
    pub collision: CellCollision,
    /// Host body ownership, including parent-world terrain on the local cell.
    pub collision_cells: BTreeMap<GlobalFormId, Vec<BodyId>>,
}

impl LoadedLocation {
    /// Prepared mesh and distinct image payloads, shared across all selected cells.
    pub(crate) fn payload_bytes(&self) -> usize {
        let mut payload = LocationPayload::default();
        for (_, cell) in &self.cells {
            payload.add_cell(cell);
        }
        payload.bytes
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LocationLimits {
    pub payload_bytes: usize,
    pub primitives: usize,
}

#[derive(Default)]
struct LocationPayload {
    bytes: usize,
    images: HashSet<usize>,
}

impl LocationPayload {
    fn add_cell(&mut self, cell: &LoadedCell) {
        self.bytes = cell.surfaces.iter().fold(self.bytes, |bytes, s| {
            bytes.saturating_add(s.arrays.mesh_byte_count())
        });
        for textures in cell
            .surfaces
            .iter()
            .map(|s| &s.textures)
            .chain(cell.particles.iter().map(|p| &p.textures))
        {
            for image in [
                &textures.diffuse,
                &textures.normal,
                &textures.environment,
                &textures.environment_mask,
                &textures.glow,
                &textures.greyscale,
                &textures.terrain_noise,
            ]
            .into_iter()
            .chain(textures.water_noise.iter())
            .chain([&textures.water_flow])
            .flatten()
            .chain(textures.terrain_images())
            {
                if self.images.insert(std::sync::Arc::as_ptr(image) as usize) {
                    self.bytes = self.bytes.saturating_add(image.payload_bytes());
                }
            }
        }
    }
}

#[derive(Default)]
struct LocationBudget {
    limits: Option<LocationLimits>,
    payload: LocationPayload,
}

impl LocationBudget {
    fn check_cell(&mut self, cell: &LoadedCell) -> Result<(), DataSourceError> {
        if self.limits.is_some() {
            self.payload.add_cell(cell);
            self.check_primitives(0)?;
        }
        Ok(())
    }

    fn check_primitives(&self, primitives: usize) -> Result<(), DataSourceError> {
        if self.limits.is_some_and(|limits| {
            self.payload.bytes > limits.payload_bytes || primitives > limits.primitives
        }) {
            return Err(DataSourceError::LocationBudget {
                payload_bytes: self.payload.bytes,
                primitives,
            });
        }
        Ok(())
    }
}

#[cfg(feature = "godot")]
pub(crate) struct PreparedCell {
    pub id: GlobalFormId,
    pub loaded: LoadedCell,
    pub colliders: Vec<PlacedCollider>,
}

#[cfg(feature = "godot")]
impl PreparedCell {
    pub(crate) fn stream_cost(&self) -> crate::streaming::StreamCost {
        let mut uploaded = std::collections::HashSet::new();
        let mut texture_bytes = 0usize;
        for surface in &self.loaded.surfaces {
            let material = &surface.arrays.material;
            let textures = &surface.textures;
            for (kind, path, texture) in [
                (
                    0u8,
                    material.diffuse.as_deref(),
                    textures.diffuse.as_deref(),
                ),
                (0, material.normal.as_deref(), textures.normal.as_deref()),
                (0, material.glow_texture(), textures.glow.as_deref()),
                (
                    0,
                    Some("textures/terrain/noise.dds"),
                    textures.terrain_noise.as_deref(),
                ),
                (
                    0,
                    material
                        .effect
                        .as_ref()
                        .map(|e| e.params.greyscale_texture.as_str()),
                    textures.greyscale.as_deref(),
                ),
                (
                    1,
                    material.environment_texture(),
                    textures.environment.as_deref(),
                ),
                (
                    0,
                    material.environment_mask.as_deref(),
                    textures.environment_mask.as_deref(),
                ),
            ]
            .into_iter()
            .chain(material.water.iter().flat_map(|water| {
                (0..3)
                    .map(|i| (0, water.noise_layer(i), textures.water_noise[i].as_deref()))
                    .chain([(
                        0,
                        water.flow_texture.as_deref(),
                        textures.water_flow.as_deref(),
                    )])
            })) {
                let Some(texture) = texture else {
                    continue;
                };
                let key = path.map(|path| (kind, soe_bsa::normalize(path)));
                if key.as_ref().is_none_or(|key| uploaded.insert(key.clone())) {
                    texture_bytes = texture_bytes.saturating_add(texture_payload_bytes(texture));
                }
            }
            if let Some(terrain) = &textures.terrain {
                for (q, quadrant) in terrain.quadrants.iter().enumerate() {
                    for (path, image) in &quadrant.layers {
                        if uploaded.insert((0, soe_bsa::normalize(path))) {
                            texture_bytes =
                                texture_bytes.saturating_add(texture_payload_bytes(image));
                        }
                    }
                    for (i, image) in quadrant.weights.iter().enumerate() {
                        let path = format!(
                            "{}:weights:{q}:{i}",
                            material.diffuse.as_deref().unwrap_or("terrain")
                        );
                        if uploaded.insert((0, path)) {
                            texture_bytes =
                                texture_bytes.saturating_add(texture_payload_bytes(image));
                        }
                    }
                }
            }
        }
        crate::streaming::StreamCost {
            triangles: self.loaded.stats.triangles,
            bytes: self
                .loaded
                .surfaces
                .iter()
                .fold(texture_bytes, |bytes, surface| {
                    bytes.saturating_add(surface.arrays.mesh_byte_count())
                }),
        }
    }
}

#[cfg(feature = "godot")]
fn texture_payload_bytes(texture: &super::TextureData) -> usize {
    match texture {
        super::TextureData::Cubemap(faces) => faces
            .iter()
            .map(texture_payload_bytes)
            .fold(0usize, usize::saturating_add),
        super::TextureData::Compressed { bytes, .. } => bytes.len(),
        super::TextureData::Rgba8(image) => image.rgba.len(),
    }
}

impl DataSource {
    /// Prepare visible geometry and matching collision for a logical destination.
    /// The exterior radius is bounded to prevent an accidental unbounded load.
    /// A small world selects all its grid cells instead of a radius square.
    pub fn load_location(
        &self,
        cell: GlobalFormId,
        position: [f32; 3],
        radius: i32,
    ) -> Result<LoadedLocation, DataSourceError> {
        self.load_location_with_limits(cell, position, radius, None)
    }

    pub(crate) fn load_location_with_limits(
        &self,
        cell: GlobalFormId,
        position: [f32; 3],
        radius: i32,
        limits: Option<LocationLimits>,
    ) -> Result<LoadedLocation, DataSourceError> {
        let _owner = crate::allocations::OwnerScope::new(crate::allocations::Owner::Cell);
        let mut budget = LocationBudget {
            limits,
            ..LocationBudget::default()
        };
        if !(0..=4).contains(&radius) || position.iter().any(|v| !v.is_finite()) {
            return Err(DataSourceError::CellNotFound(
                "invalid destination coordinates or radius".into(),
            ));
        }
        let index = self.cell_index();
        let cells = if index.is_interior(cell) {
            let loaded = self.stream_cell_id(cell)?;
            budget.check_cell(&loaded)?;
            vec![(cell, loaded)]
        } else {
            let owner = self
                .order
                .winner_ref(cell)
                .ok_or_else(|| DataSourceError::CellNotFound(cell.to_string()))?;
            let record = self.order.record(owner);
            if record.tag != tag::CELL {
                return Err(DataSourceError::CellNotFound(cell.to_string()));
            }
            let world = record
                .placement
                .world
                .map(|raw| self.order.resolve(owner.file, raw))
                .ok_or_else(|| {
                    DataSourceError::CellNotFound(format!("{cell} has no worldspace"))
                })?;
            let origin = (
                (position[0] / CELL_UNITS).floor() as i32,
                (position[1] / CELL_UNITS).floor() as i32,
            );
            let cells = self.exterior_cells_at(world, origin, radius);
            let land_parent = self.land_parent_of(world);
            let parent_cells = land_parent
                .map(|parent| {
                    cells
                        .iter()
                        .filter_map(|cell| {
                            index
                                .exterior_cell(parent, cell.x, cell.y)
                                .map(|id| ExteriorCell { id, ..*cell })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let block = ExteriorBlock {
                world,
                world_editor_id: self
                    .order
                    .winner(world)
                    .and_then(soe_esm::Record::editor_id),
                origin,
                radius,
                cells,
                land_parent,
                parent_cells,
            };
            if !block.cells.iter().any(|c| (c.x, c.y) == origin) {
                return Err(DataSourceError::CellNotFound(format!(
                    "{world} at {origin:?}"
                )));
            }
            let mut cells = Vec::new();
            self.try_stream_exterior_each(&block, |cell, loaded, _| {
                budget.check_cell(&loaded)?;
                cells.push((cell.id, loaded));
                Ok(())
            })?;
            if let Some(persistent) = index.world_persistent_cell(world) {
                if !cells.iter().any(|(id, _)| *id == persistent) {
                    let loaded = self.stream_cell_id(persistent)?;
                    budget.check_cell(&loaded)?;
                    cells.push((persistent, loaded));
                }
            }
            cells
        };
        let _owner = crate::allocations::OwnerScope::new(crate::allocations::Owner::Movement);
        let mut colliders = Vec::new();
        let mut world = SimpleWorld::new();
        let mut references = HashMap::new();
        let mut placed_colliders = Vec::new();
        let mut collision_cells = BTreeMap::new();
        let mut stats = CollisionStats::default();
        for (id, loaded) in &cells {
            let _capture = loaded.stats.profile.enter();
            let (placed, found) = self.location_colliders(*id, loaded)?;
            stats.references += found.references;
            stats.with_collision += found.with_collision;
            stats.colliders += found.colliders;
            stats.failed += found.failed;
            stats.skipped_blocks += found.skipped_blocks;
            let mut bodies = Vec::new();
            for placed in placed {
                if !placed.collider.is_static() {
                    placed_colliders.push(placed);
                    continue;
                }
                let _build = soe_trace::LoadMeasure::new(soe_trace::LoadStage::CollisionBuild, "");
                let before = world.primitive_count();
                let body = world.add_static(&placed.collider);
                budget.check_primitives(world.primitive_count())?;
                references.insert(body, placed.reference);
                bodies.push(body);
                colliders.push(placed.collider.clone());
                placed_colliders.push(placed);
                stats.bodies += 1;
                stats.primitives += world.primitive_count() - before;
            }
            collision_cells.insert(*id, bodies);
        }
        Ok(LoadedLocation {
            cells,
            collision_cells,
            collision: CellCollision {
                colliders,
                world,
                stats,
                references,
                placed: placed_colliders,
            },
        })
    }

    /// Worldspace ownership comes from the winning CELL's own file namespace.
    pub(crate) fn exterior_world(&self, cell: GlobalFormId) -> Option<GlobalFormId> {
        if self.cell_index().is_interior(cell) {
            return None;
        }
        let owner = self.order.winner_ref(cell)?;
        let record = self.order.record(owner);
        if record.tag != tag::CELL || record.is_deleted() {
            return None;
        }
        record
            .placement
            .world
            .map(|raw| self.order.resolve(owner.file, raw))
    }

    /// Admission uses the ordinary grid and small-world location selection.
    pub(crate) fn prefetch_cells(
        &self,
        cell: GlobalFormId,
        position: [f32; 3],
        radius: i32,
    ) -> Option<Vec<GlobalFormId>> {
        if !(0..=4).contains(&radius) || position.iter().any(|v| !v.is_finite()) {
            return None;
        }
        if self.cell_index().is_interior(cell) {
            return Some(vec![cell]);
        }
        let world = self.exterior_world(cell)?;
        let origin = (
            (position[0] / CELL_UNITS).floor() as i32,
            (position[1] / CELL_UNITS).floor() as i32,
        );
        let mut cells: Vec<_> = self
            .exterior_cells_at(world, origin, radius)
            .into_iter()
            .map(|cell| cell.id)
            .collect();
        cells.extend(self.cell_index().world_persistent_cell(world));
        cells.sort_unstable();
        cells.dedup();
        Some(cells)
    }

    /// Prepare only the requested grid cells. Missing border cells have no resources.
    #[cfg(feature = "godot")]
    pub(crate) fn prepare_grid_cells(
        &self,
        world: GlobalFormId,
        coordinates: &[CellCoord],
    ) -> Result<Vec<PreparedCell>, DataSourceError> {
        if coordinates.is_empty() {
            return Ok(Vec::new());
        }
        let index = self.cell_index();
        let select = |world| {
            coordinates
                .iter()
                .filter_map(|c| {
                    index.exterior_cell(world, c.x, c.y).map(|id| ExteriorCell {
                        x: c.x,
                        y: c.y,
                        id,
                    })
                })
                .collect()
        };
        let land_parent = self.land_parent_of(world);
        let block = ExteriorBlock {
            world,
            world_editor_id: None,
            origin: coordinates.first().map_or((0, 0), |c| (c.x, c.y)),
            radius: 0,
            cells: select(world),
            land_parent,
            parent_cells: land_parent.map(select).unwrap_or_default(),
        };
        self.stream_exterior_block(block)?
            .cells
            .into_iter()
            .map(|(cell, loaded)| {
                let _capture = loaded.stats.profile.enter();
                let (colliders, _) = self.location_colliders(cell.id, &loaded)?;
                Ok(PreparedCell {
                    id: cell.id,
                    loaded,
                    colliders,
                })
            })
            .collect()
    }

    pub(crate) fn location_colliders(
        &self,
        cell: GlobalFormId,
        loaded: &LoadedCell,
    ) -> Result<(Vec<PlacedCollider>, CollisionStats), DataSourceError> {
        let _owner = crate::allocations::OwnerScope::new(crate::allocations::Owner::Movement);
        let (mut placed, stats) = self.placed_cell_colliders(cell)?;
        let _build = soe_trace::LoadMeasure::new(soe_trace::LoadStage::CollisionBuild, "");
        for surface in &loaded.surfaces {
            if !surface
                .arrays
                .material
                .diffuse
                .as_deref()
                .is_some_and(|path| path.starts_with("terrain:"))
            {
                continue;
            }
            if self
                .order
                .winner(surface.reference)
                .is_none_or(|r| r.tag != tag::LAND)
            {
                continue;
            }
            let arrays = &surface.arrays;
            if arrays.indices.is_empty() {
                continue;
            }
            placed.push(PlacedCollider {
                reference: surface.reference,
                // LAND has no separate reference placement, matching the
                // `Iso::IDENTITY` `LoadedSurface::placement` its own draw
                // path carries.
                original: Iso::IDENTITY,
                collider: Collider {
                    node: soe_nif::BlockRef(-1),
                    node_name: String::new(),
                    layer: soe_physics::bhk::Layer::STATIC,
                    motion: soe_physics::bhk::MotionType::Fixed,
                    mass: 0.0,
                    transform: Iso::IDENTITY,
                    shape: Shape::TriangleMesh {
                        radius: 0.0,
                        materials: self
                            .land_triangle_materials(surface.reference, &arrays.indices)?,
                        vertices: arrays
                            .positions
                            .iter()
                            .map(|p| {
                                let p = GodotPos::new(p[0], p[1], p[2]).to_skyrim();
                                // CONTEXT: LAND vertices lie on the native 128-unit
                                // grid. Undo coordinate conversion roundoff at cell seams.
                                let step = CELL_UNITS
                                    / (soe_esm::records::land::VERTS_PER_SIDE - 1) as f32;
                                Vec3::new(
                                    (p.x / step).round() * step,
                                    (p.y / step).round() * step,
                                    p.z,
                                )
                            })
                            .collect(),
                        // SurfaceArrays uses Godot's clockwise front faces.
                        triangles: arrays
                            .indices
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .map(|t| [t[0] as u32, t[2] as u32, t[1] as u32])
                            .collect(),
                    },
                },
            });
        }
        Ok((placed, stats))
    }
}

#[cfg(all(test, feature = "godot"))]
mod hst_08_tests {
    use super::*;
    use crate::data_source::{CellStats, LoadedSurface, SurfaceTextures, TextureData};
    use crate::mesh::{Aabb, MaterialDesc, SurfaceArrays};
    use crate::streaming::{CellStreamer, StreamBudget, StreamConfig};
    use crate::units::{GodotPos, SkyrimPos};
    use std::sync::Arc;

    #[test]
    fn speculative_location_stops_after_first_over_budget_cell() {
        use super::super::fixture;
        use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin, Record};

        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("meshes")).unwrap();
        std::fs::create_dir_all(directory.path().join("textures")).unwrap();
        let mut plugin = Plugin::new(FileHeader::default());
        let mut top = Group::new(GroupKind::Top(tag::WRLD));
        top.entries
            .push(Entry::Record(plugin.push_record(Record::new(tag::WRLD, 1))));
        let mut children = Group::new(GroupKind::WorldChildren(1));
        let mut block = Group::new(GroupKind::ExteriorBlock { y: 0, x: 0 });
        let mut sub = Group::new(GroupKind::ExteriorSubBlock { y: 0, x: 0 });
        for x in 0_i32..3 {
            let cell = 10 + x as u32;
            let base = 100 + x as u32;
            let mut grid = x.to_le_bytes().to_vec();
            grid.extend_from_slice(&[0; 8]);
            sub.entries.push(Entry::Record(
                plugin.push_record(
                    Record::new(tag::CELL, cell)
                        .sub(b"DATA", [0, 0])
                        .sub(b"XCLC", grid),
                ),
            ));
            let mut temporary = Group::new(GroupKind::CellTemporary(cell));
            temporary.entries.push(Entry::Record(
                plugin.push_record(
                    Record::new(tag::REFR, 200 + x as u32)
                        .sub(b"NAME", base.to_le_bytes())
                        .sub(b"DATA", [0; 24]),
                ),
            ));
            let mut cell_children = Group::new(GroupKind::CellChildren(cell));
            cell_children.entries.push(Entry::Group(temporary));
            sub.entries.push(Entry::Group(cell_children));
            plugin.push_top(Record::new(tag::STAT, base).zsub(b"MODL", &format!("{x}.nif")));

            let mut writer = fixture::NifWriter::new(100);
            let root = writer.reserve("NiNode");
            let texture = writer.texture_set(&[&format!("textures/{x}.dds")]);
            let texture = writer.add("BSShaderTextureSet", texture);
            let shader = writer.lighting_shader(0, 0, 0, texture, &[]);
            let shader = writer.add("BSLightingShaderProperty", shader);
            let (vertices, triangles) = fixture::quad();
            let shape = writer.bs_tri_shape(
                "Fixture",
                [0.0; 3],
                -1,
                shader,
                -1,
                soe_nif::vertex::attr::VERTEX | soe_nif::vertex::attr::UVS,
                &vertices,
                &triangles,
                true,
            );
            let shape = writer.add("BSTriShape", shape);
            let node = writer.ni_node("Root", [0.0; 3], fixture::IDENTITY, 1.0, &[shape]);
            writer.fill(root, node);
            writer.roots = vec![root];
            std::fs::write(
                directory.path().join(format!("meshes/{x}.nif")),
                writer.finish(),
            )
            .unwrap();
            let header = soe_dds::header::Header {
                flags: soe_dds::header::flags::CAPS
                    | soe_dds::header::flags::HEIGHT
                    | soe_dds::header::flags::WIDTH
                    | soe_dds::header::flags::PIXELFORMAT,
                height: 4,
                width: 4,
                pitch_or_linear_size: 8,
                depth: 0,
                mip_map_count: 1,
                pixel_format: soe_dds::header::PixelFormat {
                    flags: soe_dds::header::pf::FOURCC,
                    four_cc: *b"DXT1",
                    rgb_bit_count: 0,
                    r_mask: 0,
                    g_mask: 0,
                    b_mask: 0,
                    a_mask: 0,
                },
                caps: soe_dds::header::caps::TEXTURE,
                caps2: 0,
                dx10: None,
            };
            let mut dds = header.to_bytes();
            dds.extend_from_slice(&[0; 8]);
            std::fs::write(directory.path().join(format!("textures/{x}.dds")), dds).unwrap();
        }
        block.entries.push(Entry::Group(sub));
        children.entries.push(Entry::Group(block));
        top.entries.push(Entry::Group(children));
        plugin.groups.push(top);
        std::fs::write(directory.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
        for master in &super::super::BASE_MASTERS[1..] {
            std::fs::write(
                directory.path().join(master),
                Plugin::new(FileHeader::default()).to_bytes(),
            )
            .unwrap();
        }
        let source = DataSource::open_with_threads(directory.path(), 1).unwrap();
        let destination = GlobalFormId {
            file: 0,
            object: 10,
        };
        let error = source
            .load_location_with_limits(
                destination,
                [0.0; 3],
                2,
                Some(LocationLimits {
                    payload_bytes: 0,
                    primitives: usize::MAX,
                }),
            )
            .err()
            .unwrap();
        assert!(
            matches!(error, DataSourceError::LocationBudget { payload_bytes, primitives: 0 } if payload_bytes > 0)
        );
        assert_eq!(
            source.textures.counters().decoded,
            1,
            "later cell textures were prepared after rejection"
        );
        let required = source.load_location(destination, [0.0; 3], 2).unwrap();
        assert_eq!(required.cells.len(), 3);
        assert_eq!(source.textures.counters().decoded, 3);
    }

    fn surface(texture: Arc<TextureData>) -> LoadedSurface {
        LoadedSurface {
            animation: None,
            reference: GlobalFormId { file: 0, object: 1 },
            arrays: SurfaceArrays {
                positions: vec![[0.0; 3]; 3],
                normals: Vec::new(),
                tangents: Vec::new(),
                uvs: Vec::new(),
                colors: Vec::new(),
                indices: vec![0, 1, 2],
                bones: Vec::new(),
                weights: Vec::new(),
                material: MaterialDesc {
                    diffuse: Some("textures/shared.dds".into()),
                    ..MaterialDesc::default()
                },
                bounds: Aabb {
                    min: GodotPos::default(),
                    max: GodotPos::default(),
                },
            },
            textures: SurfaceTextures {
                diffuse: Some(texture),
                ..SurfaceTextures::default()
            },
            placement: Iso::IDENTITY,
            instance_transform: None,
            mesh_identity: None,
        }
    }

    #[test]
    fn water_shader_location_budget_counts_all_water_images_once() {
        let image = |bytes| {
            Arc::new(TextureData::Rgba8(soe_dds::Image {
                width: 1,
                height: 1,
                depth: 1,
                rgba: vec![0; bytes],
            }))
        };
        let mut first = surface(image(4));
        first.textures.water_noise = [Some(image(8)), Some(image(12)), Some(image(16))];
        first.textures.water_flow = Some(image(20));
        let mut second = surface(Arc::clone(first.textures.diffuse.as_ref().unwrap()));
        second.textures = first.textures.clone();
        let mesh_bytes = first.arrays.mesh_byte_count() + second.arrays.mesh_byte_count();
        let cell = LoadedCell {
            particles: Vec::new(),
            stats: CellStats::default(),
            surfaces: vec![first, second],
        };
        let expected = mesh_bytes + 4 + 8 + 12 + 16 + 20;
        let mut payload = LocationPayload::default();
        payload.add_cell(&cell);
        assert_eq!(payload.bytes, expected);
        let mut budget = LocationBudget {
            limits: Some(LocationLimits {
                payload_bytes: expected - 1,
                primitives: usize::MAX,
            }),
            ..LocationBudget::default()
        };
        assert!(matches!(
            budget.check_cell(&cell),
            Err(DataSourceError::LocationBudget { payload_bytes, .. }) if payload_bytes == expected
        ));
    }

    #[test]
    fn rnd_26_stream_budget_counts_shared_terrain_noise_once() {
        let texture = Arc::new(TextureData::Rgba8(soe_dds::Image {
            width: 1,
            height: 1,
            depth: 1,
            rgba: vec![128; 4],
        }));
        let mut surfaces = vec![surface(Arc::clone(&texture)), surface(texture)];
        for surface in &mut surfaces {
            surface.textures.terrain_noise = surface.textures.diffuse.take();
            surface.arrays.material.diffuse = None;
        }
        let mut cell = PreparedCell {
            id: GlobalFormId { file: 0, object: 2 },
            loaded: LoadedCell {
                particles: Vec::new(),
                stats: CellStats::default(),
                surfaces,
            },
            colliders: Vec::new(),
        };
        let with_noise = cell.stream_cost().bytes;
        for surface in &mut cell.loaded.surfaces {
            surface.textures.terrain_noise = None;
        }
        assert_eq!(with_noise - cell.stream_cost().bytes, 4);
    }

    #[test]
    fn hst_08_prepared_cell_budget_includes_deduplicated_texture_upload_bytes() {
        let texture = Arc::new(TextureData::Compressed {
            width: 4,
            height: 4,
            format: crate::data_source::BlockFormat::Bc1,
            mips: 1,
            mip0_len: 8,
            bytes: vec![0; 7],
        });
        let cell = PreparedCell {
            id: GlobalFormId { file: 0, object: 2 },
            loaded: LoadedCell {
                particles: Vec::new(),
                stats: CellStats {
                    triangles: 2,
                    ..CellStats::default()
                },
                surfaces: vec![surface(Arc::clone(&texture)), surface(texture)],
            },
            colliders: Vec::new(),
        };
        let cost = cell.stream_cost();
        assert_eq!(cost.triangles, 2);
        assert_eq!(cost.bytes, 103);

        let mut streamer = CellStreamer::new(StreamConfig {
            load_radius: 0,
            unload_radius: 0,
            max_loads_per_tick: 0,
            max_unloads_per_tick: 0,
        })
        .unwrap();
        let focus = SkyrimPos::new(0.0, 0.0, 0.0);
        let deferred = streamer.update_with_budget(
            focus,
            StreamBudget {
                triangles: 2,
                bytes: 102,
            },
            |_| cost,
        );
        assert_eq!(deferred.load.len(), 0);
        let selected = streamer.update_with_budget(
            focus,
            StreamBudget {
                triangles: 2,
                bytes: 103,
            },
            |_| cost,
        );
        assert_eq!(selected.load.len(), 1);
    }
}
