// SPDX-License-Identifier: GPL-3.0-or-later

//! RND-11 against a written fixture: a row of exterior cells whose `LAND`
//! layers paint an `LTEX` that names one `GRAS`, and a one-quad grass NIF.
//! No installed data. `godot/rnd_11_grass_check.gd` checks the same
//! contract through the Godot node on the vanilla Whiterun block.

use super::fixture;

use std::path::Path;

use soe_esm::records::land::{Heights, ALPHA_GRID_SIDE, VERTS};
use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};
use soe_nif::blocks::flags1;
use soe_nif::vertex::attr;

use super::{DataSource, LoadedCell};
use crate::batch::{instancing_candidates, INSTANCE_MIN_PLACEMENTS};
use crate::streaming::{CellCoord, CellStreamer, StreamConfig};

const LTEX_ID: u32 = 0x800;
const GRAS_ID: u32 = 0x900;
const GRASS_MODEL: &str = "grass\\tuft.nif";

/// How a fixture cell's `LAND` is painted with the grass texture.
#[derive(Clone, Copy)]
enum Paint {
    /// Every quadrant, every alpha point, full opacity. Enough candidates
    /// to reach the per-cell cap.
    Everywhere,
    /// Quadrant 0, a 4 by 4 block of alpha points. The placement pass
    /// samples a quad at its south-west vertex, so this seeds 16 quads.
    Patch,
    /// Quadrant 0, one alpha point at half opacity: a single placement,
    /// below the instancing threshold.
    Single,
}

/// Fixture cells, west to east along row 0.
const CELLS: [(i32, Paint); 4] = [
    (-1, Paint::Patch),
    (0, Paint::Everywhere),
    (1, Paint::Patch),
    (2, Paint::Single),
];

fn tuft_nif() -> Vec<u8> {
    let mut writer = fixture::NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let textures = writer.texture_set(&["textures\\grass\\tuft.dds"]);
    let textures = writer.add("BSShaderTextureSet", textures);
    let shader = writer.lighting_shader(0, flags1::ZBUFFER_TEST, 0, textures, &[]);
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (mut vertices, triangles) = fixture::quad();
    for vertex in &mut vertices {
        for axis in &mut vertex.position {
            *axis *= 20.0;
        }
    }
    let shape = writer.bs_tri_shape(
        "Tuft",
        [0.0; 3],
        -1,
        shader,
        -1,
        attr::VERTEX | attr::UVS | attr::NORMALS,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    let bytes = writer.ni_node("Root", [0.0; 3], fixture::IDENTITY, 1.0, &[shape]);
    writer.fill(root, bytes);
    writer.roots = vec![root];
    writer.finish()
}

fn layer(quadrant: u8, points: &[(u16, f32)]) -> (Vec<u8>, Vec<u8>) {
    let mut atxt = LTEX_ID.to_le_bytes().to_vec();
    atxt.extend([quadrant, 0]);
    atxt.extend(0_u16.to_le_bytes());
    let mut vtxt = Vec::new();
    for &(position, opacity) in points {
        vtxt.extend(position.to_le_bytes());
        vtxt.extend([0, 0]);
        vtxt.extend(opacity.to_le_bytes());
    }
    (atxt, vtxt)
}

fn land(id: u32, paint: Paint) -> Record {
    let side = ALPHA_GRID_SIDE as u16;
    let mut record = Record::new(tag::LAND, id)
        .sub(b"DATA", 1_u32.to_le_bytes())
        .sub(b"VHGT", Heights::encode(&vec![0.0; VERTS]).to_bytes());
    let layers: Vec<(u8, Vec<(u16, f32)>)> = match paint {
        Paint::Everywhere => (0..4)
            .map(|q| (q, (0..side * side).map(|p| (p, 1.0)).collect()))
            .collect(),
        Paint::Patch => vec![(
            0,
            (0..4)
                .flat_map(|r| (0..4).map(move |c| (r * side + c, 1.0)))
                .collect(),
        )],
        Paint::Single => vec![(0, vec![(0, 0.5)])],
    };
    for (quadrant, points) in layers {
        let (atxt, vtxt) = layer(quadrant, &points);
        record = record.sub(b"ATXT", atxt).sub(b"VTXT", vtxt);
    }
    record
}

fn grass_data() -> Vec<u8> {
    let mut data = vec![50, 0, 90, 0];
    data.extend(0_u16.to_le_bytes());
    data.extend([0, 0]);
    data.extend(0_u32.to_le_bytes());
    for value in [1.0_f32, 0.5, 0.0, 0.0] {
        data.extend(value.to_le_bytes());
    }
    data.extend([0, 0, 0, 0]);
    data
}

/// Writes the fixture data folder: `Skyrim.esm` plus empty DLC masters and
/// `meshes/grass/tuft.nif`.
fn write_fixture(directory: &Path) {
    std::fs::create_dir_all(directory.join("meshes/grass")).unwrap();
    std::fs::write(directory.join("meshes/grass/tuft.nif"), tuft_nif()).unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut textures = Group::new(GroupKind::Top(tag::LTEX));
    textures.entries.push(Entry::Record(
        plugin.push_record(
            Record::new(tag::LTEX, LTEX_ID)
                .zsub(tag::EDID, "GrassTexture")
                .sub(b"GNAM", GRAS_ID.to_le_bytes()),
        ),
    ));
    let mut grasses = Group::new(GroupKind::Top(tag::GRAS));
    grasses.entries.push(Entry::Record(
        plugin.push_record(
            Record::new(tag::GRAS, GRAS_ID)
                .zsub(tag::EDID, "GrassTuft")
                .zsub(b"MODL", GRASS_MODEL)
                .sub(b"DATA", grass_data()),
        ),
    ));
    let world = plugin.push_record(Record::new(tag::WRLD, 0x100).zsub(tag::EDID, "GrassWorld"));
    let persistent = plugin.push_record(Record::new(tag::CELL, 0x200).sub(b"DATA", [0, 0]));
    let mut children = Group::new(GroupKind::WorldChildren(0x100));
    children.entries.push(Entry::Record(persistent));
    let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: 0 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 0, y: 0 });
    for (i, (x, paint)) in CELLS.into_iter().enumerate() {
        let id = 0x300 + i as u32;
        let mut grid = x.to_le_bytes().to_vec();
        grid.extend(0_i32.to_le_bytes());
        grid.extend(0_u32.to_le_bytes());
        let cell = plugin.push_record(
            Record::new(tag::CELL, id)
                .zsub(tag::EDID, &format!("Grass{x}"))
                .sub(b"DATA", [0, 0])
                .sub(b"XCLC", grid),
        );
        let mut temporary = Group::new(GroupKind::CellTemporary(id));
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(land(id + 0x100, paint))));
        let mut cell_children = Group::new(GroupKind::CellChildren(id));
        cell_children.entries.push(Entry::Group(temporary));
        sub.entries.push(Entry::Record(cell));
        sub.entries.push(Entry::Group(cell_children));
    }
    block.entries.push(Entry::Group(sub));
    children.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    top.entries.push(Entry::Record(world));
    top.entries.push(Entry::Group(children));
    plugin.groups.extend([textures, grasses, top]);
    std::fs::write(directory.join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            directory.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
}

/// A fresh fixture directory, or `SOE_GRASS_FIXTURE` when the Godot check
/// wants the same files kept on disk.
fn fixture_dir(tag: &str) -> (std::path::PathBuf, bool) {
    match std::env::var_os("SOE_GRASS_FIXTURE") {
        Some(dir) => (std::path::PathBuf::from(dir), true),
        None => (
            std::env::temp_dir().join(format!("soe-grass-{tag}-{}", std::process::id())),
            false,
        ),
    }
}

/// The grass surfaces of one streamed cell, the only ones whose reference
/// is the `GRAS` form.
fn grass_of(loaded: &LoadedCell) -> Vec<&super::LoadedSurface> {
    loaded
        .surfaces
        .iter()
        .filter(|s| s.reference.object == GRAS_ID)
        .collect()
}

/// Per cell: placement count, then every grass surface's mesh identity and
/// instance transform in order, then the instanced batches' member
/// transforms. Two equal signatures mean the same batches.
type Signature = Vec<(
    (i32, i32),
    usize,
    Vec<(Option<String>, Option<crate::units::GodotTransform>)>,
    Vec<Vec<crate::units::GodotTransform>>,
)>;

fn signature(source: &DataSource) -> Signature {
    let block = source.stream_exterior("Grass0", 2).unwrap();
    block
        .cells
        .iter()
        .map(|(cell, loaded)| {
            let grass = grass_of(loaded);
            let surfaces = grass
                .iter()
                .map(|s| {
                    (
                        s.mesh_identity.as_deref().map(str::to_owned),
                        s.instance_transform,
                    )
                })
                .collect();
            let (groups, _) = instancing_candidates(&loaded.surfaces);
            let batches = groups
                .iter()
                .map(|g| g.members.iter().map(|m| m.transform).collect())
                .collect();
            (
                (cell.x, cell.y),
                loaded.stats.grass_placements,
                surfaces,
                batches,
            )
        })
        .collect()
}

#[test]
fn rnd_11_land_grass_fixture_places_deterministic_bounded_batches() {
    let (directory, kept) = fixture_dir("batches");
    write_fixture(&directory);
    let first = signature(&DataSource::open(&directory).unwrap());
    let second = signature(&DataSource::open(&directory).unwrap());
    assert_eq!(first.len(), CELLS.len(), "every fixture cell streams");
    assert_eq!(
        first, second,
        "two independent opens must place the same grass batches"
    );

    let by_x = |x: i32| first.iter().find(|entry| entry.0 == (x, 0)).unwrap();
    // Everywhere: 4 quadrants of 16 by 16 quads at 2 instances each would be
    // 2048 placements, the per-cell cap holds it at 512.
    let dense = by_x(0);
    assert_eq!(dense.1, super::grass::MAX_GRASS_PLACEMENTS_PER_CELL);
    assert_eq!(dense.3.len(), 1, "one model, one material, one MultiMesh");
    assert_eq!(dense.3[0].len(), dense.1);
    // Patch: sixteen painted points, each the south-west corner of one quad,
    // at density 50 and full opacity.
    for x in [-1, 1] {
        let patch = by_x(x);
        assert_eq!(patch.1, 32);
        assert_eq!(patch.3.len(), 1);
        assert_eq!(patch.3[0].len(), 32);
    }
    // Neighbouring cells with the same paint must not share placements.
    assert_ne!(by_x(-1).3, by_x(1).3);
    // Single: one placement stays below `INSTANCE_MIN_PLACEMENTS`, so it is
    // not a MultiMesh candidate and its placement is baked into vertices.
    let single = by_x(2);
    const { assert!(1 < INSTANCE_MIN_PLACEMENTS) };
    assert_eq!(single.1, 1);
    assert!(single.3.is_empty());
    assert_eq!(single.2, vec![(None, None)]);
    if !kept {
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn rnd_11_single_grass_placement_is_baked_into_its_cell() {
    let (directory, kept) = fixture_dir("baked");
    write_fixture(&directory);
    let source = DataSource::open(&directory).unwrap();
    let block = source.stream_exterior("Grass2", 0).unwrap();
    let (cell, loaded) = &block.cells[0];
    assert_eq!((cell.x, cell.y), (2, 0));
    let grass = grass_of(loaded);
    assert_eq!(grass.len(), 1);
    // The tuft is 40 units across in local space. Baked at cell (2, 0) its
    // vertices sit two cells east of the origin, well past 100 metres.
    let reach = grass[0]
        .arrays
        .positions
        .iter()
        .map(|p| p[0].abs().max(p[2].abs()))
        .fold(0.0_f32, f32::max);
    assert!(reach > 100.0, "grass drew at {reach} m, not in its cell");
    if !kept {
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn rnd_11_grass_batches_are_released_as_cells_unload() {
    let (directory, kept) = fixture_dir("release");
    write_fixture(&directory);
    let source = DataSource::open(&directory).unwrap();
    let mut streamer = CellStreamer::new(StreamConfig {
        load_radius: 0,
        unload_radius: 0,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
    })
    .unwrap();
    // Resident batches by cell, filled from `plan.load` and emptied by
    // `plan.unload`, the way the node answers its two signals.
    let mut resident: std::collections::HashMap<CellCoord, usize> =
        std::collections::HashMap::new();
    let cell_centre = |x: i32| {
        crate::units::SkyrimPos::new(
            (x as f32 + 0.5) * crate::units::CELL_UNITS,
            0.5 * crate::units::CELL_UNITS,
            0.0,
        )
    };
    let mut released = Vec::new();
    for x in [-1, 0, 1] {
        let plan = streamer.settle(cell_centre(x));
        for coord in plan.unload {
            let batches = resident.remove(&coord).unwrap();
            released.push((coord.x, batches));
        }
        for coord in plan.load {
            let block = source
                .stream_exterior(&format!("Grass{}", coord.x), 0)
                .unwrap();
            let (groups, _) = instancing_candidates(&block.cells[0].1.surfaces);
            resident.insert(coord, groups.len());
        }
        assert_eq!(resident.len(), 1, "only the focus cell stays resident");
        assert_eq!(resident.get(&CellCoord::new(x, 0)), Some(&1));
    }
    assert_eq!(released, vec![(-1, 1), (0, 1)]);
    if !kept {
        std::fs::remove_dir_all(directory).unwrap();
    }
}
