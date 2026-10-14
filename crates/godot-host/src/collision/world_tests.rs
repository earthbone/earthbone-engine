// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed collision and exterior placement audits, using authored NIF and ACHR records.

use super::*;
use soe_physics::Vec3;
use std::collections::{BTreeMap, BTreeSet};

fn source() -> Option<DataSource> {
    std::env::var_os("SKYRIM_DATA")
        .map(|data| DataSource::open(std::path::Path::new(&data)).expect("installed data opens"))
}

#[test]
fn installed_world_collision_audit() {
    let Some(source) = source() else { return };
    let city = source
        .order()
        .winners_of(tag::WRLD)
        .find_map(|(id, record)| {
            (record.editor_id().as_deref() == Some("WhiterunWorld")).then_some(id)
        })
        .expect("world");
    let inn = source.resolve_cell("WhiterunBanneredMare").expect("inn");
    let mut ids = BTreeSet::new();
    for (cell, _) in source.cell_index().cells() {
        if cell == inn || source.exterior_world(cell) == Some(city) {
            ids.extend(source.cell_index().references(cell));
        }
    }
    let mut missing = BTreeMap::<String, usize>::new();
    let mut failures = Vec::new();
    let mut checked = 0;
    for id in ids {
        let owner = source.order().winner_ref(id).expect("reference");
        let record = source.order().record(owner);
        if record.tag != tag::REFR
            || !soe_game::World::authored_reference_enabled(source.order(), id)
        {
            continue;
        }
        let refr = Refr::from_record(record).expect("REFR");
        let Some(base) = source
            .order()
            .winner(source.order().resolve(owner.file, refr.base))
        else {
            continue;
        };
        let Some(model) = model_for(base) else {
            continue;
        };
        let file = crate::data_source::placed_model(
            &source
                .vfs()
                .read(&format!("meshes/{model}"))
                .expect("model"),
        )
        .expect("NIF");
        if !file.blocks.iter().any(|block| matches!(block, soe_nif::blocks::Block::Collision { type_name, .. } if type_name.ends_with("CollisionObject"))) { continue; }
        checked += 1;
        let types: Vec<_> = file
            .blocks
            .iter()
            .filter_map(|block| match block {
                soe_nif::blocks::Block::Collision { type_name, .. }
                    if type_name.ends_with("Shape") =>
                {
                    Some(type_name.clone())
                }
                _ => None,
            })
            .collect();
        match soe_physics::collect(&file) {
            Ok(collected) if !collected.colliders.is_empty() => {
                if !collected.colliders.iter().any(Collider::is_static) {
                    eprintln!("DYNAMIC {id} {model} {types:?}");
                }
                for collider in &collected.colliders {
                    let bounds = collider.aabb();
                    if !bounds.min.is_finite()
                        || !bounds.max.is_finite()
                        || (bounds.max - bounds.min).length() == 0.0
                    {
                        failures.push(format!("{id} {model}: invalid bounds {bounds:?}"));
                    }
                }
            }
            result => {
                *missing.entry(types.join("+")).or_default() += 1;
                failures.push(format!("{id} {model} {types:?}: {result:?}"));
            }
        }
        if model.to_ascii_lowercase().contains("well")
            || model.to_ascii_lowercase().contains("market")
        {
            eprintln!("LANDMARK {id} {model} {:?}", refr.transform);
        }
    }
    eprintln!(
        "COLLISION checked={checked} missing={} groups={missing:?}",
        failures.len()
    );
    for failure in &failures {
        eprintln!("MISSING {failure}");
    }
    assert!(checked > 1000, "world collision survey is nonempty");
    assert_eq!(
        missing.values().sum::<usize>(),
        3,
        "only the three named water phantoms lack rigid bodies"
    );
    assert!(
        failures
            .iter()
            .all(|failure| ["0:0587B0 ", "0:0587B1 ", "0:085FCB "]
                .iter()
                .any(|id| failure.starts_with(id))
                && failure.contains("Layer(11)")
                && failure.contains("skipped: []")),
        "unexplained missing or invalid collision: {failures:?}"
    );
}

#[test]
fn installed_whiterun_actor_contacts() {
    let Some(source) = source() else { return };
    let mut game = source.localized_game().expect("game");
    let city = game.form("WhiterunWorld").expect("world");
    let persistent = source
        .cell_index()
        .world_persistent_cell(city)
        .expect("persistent cell");
    let mut cells: Vec<_> = source
        .cell_index()
        .cells()
        .filter_map(|(cell, _)| {
            source
                .cell_index()
                .grid_of(cell)
                .filter(|grid| grid.world == city)
                .map(|_| cell)
        })
        .collect();
    cells.sort_unstable();
    game.change_cell(cells.first().copied());
    let mut entered = cells.clone();
    entered.remove(0);
    entered.push(persistent);
    game.attach_streaming_cells(&entered).expect("attach grids");
    let mut expected = BTreeSet::new();
    for cell in cells.iter().chain([persistent].iter()) {
        for &id in source.cell_index().references(*cell) {
            let record = source.order().winner(id).expect("reference");
            if record.tag == tag::ACHR
                && soe_game::World::authored_reference_enabled(source.order(), id)
            {
                let state = game.world_mut().reference(id).expect("actor").clone();
                eprintln!(
                    "ACTOR {id} {:?} {:?} enabled={}",
                    record.editor_id(),
                    state.position,
                    state.enabled
                );
                expected.insert(id);
            }
        }
    }
    game.world_mut().initialize_exterior_actor_placements(city);
    expected.clear();
    for (id, _) in source.order().winners_of(tag::ACHR) {
        let world = game.world();
        if let Some(reference) = world.reference_ref(id) {
            if reference
                .cell
                .is_some_and(|cell| world.cell_coordinate_space(cell) == city)
                && world.reference_enabled(id)
            {
                expected.insert(id);
                eprintln!("PLACED {id} {:?}", reference.position);
            }
        }
    }
    let player = game.player();
    let contacts: BTreeSet<_> = game
        .world_mut()
        .contact_actors()
        .iter()
        .filter_map(|actor| (actor.actor != player).then_some(actor.actor))
        .collect();
    eprintln!(
        "ACTORS expected={} contacts={} missing={:?}",
        expected.len(),
        contacts.len(),
        expected.difference(&contacts).collect::<Vec<_>>()
    );
    assert!(!expected.is_empty());
    assert_eq!(contacts, expected);
}

#[test]
fn installed_whiterun_market_records_have_activation_shapes() {
    let Some(source) = source() else { return };
    let city = source
        .order()
        .winners_of(tag::WRLD)
        .find_map(|(id, record)| {
            (record.editor_id().as_deref() == Some("WhiterunWorld")).then_some(id)
        })
        .expect("world");
    let mut checked = 0;
    let mut owned_items = 0;
    for (cell, _) in source.cell_index().cells() {
        if source.exterior_world(cell) != Some(city) {
            continue;
        }
        let bodies: BTreeSet<_> = source
            .placed_cell_colliders(cell)
            .expect("cell shapes")
            .0
            .iter()
            .map(|placed| placed.reference)
            .collect();
        for &id in source.cell_index().references(cell) {
            let owner = source.order().winner_ref(id).expect("reference");
            let record = source.order().record(owner);
            if record.tag != tag::REFR
                || !soe_game::World::authored_reference_enabled(source.order(), id)
            {
                continue;
            }
            let refr = Refr::from_record(record).expect("REFR");
            let base = source
                .order()
                .winner(source.order().resolve(owner.file, refr.base))
                .expect("base");
            let Some(transform) = refr.transform else {
                continue;
            };
            let [x, y, z] = transform.position;
            if !(23900.0..25600.0).contains(&x)
                || !(-8000.0..-6800.0).contains(&y)
                || !(-3390.0..-3250.0).contains(&z)
            {
                continue;
            }
            if !matches!(base.tag.as_bytes(), b"ALCH" | b"INGR" | b"MISC") {
                continue;
            }
            assert!(
                bodies.contains(&id),
                "enabled stall item {id} {:?} lacks an activation shape",
                base.editor_id()
            );
            checked += 1;
            owned_items += usize::from(refr.owner.is_some());
        }
    }
    eprintln!("STALL_SHAPES checked={checked} owned={owned_items}");
    assert!(
        checked >= 30 && owned_items >= 25,
        "stall inventory survey must include owned food and clutter"
    );
}

type RenderFloorGrid = BTreeMap<(i32, i32), Vec<[Vec3; 3]>>;

fn rendered_floor_grid(
    source: &DataSource,
    cells: &[(GlobalFormId, crate::data_source::LoadedCell)],
) -> RenderFloorGrid {
    let mut grid = BTreeMap::<_, Vec<_>>::new();
    for (_, loaded) in cells {
        for surface in &loaded.surfaces {
            let Some(owner) = source.order().winner_ref(surface.reference) else {
                continue;
            };
            let record = source.order().record(owner);
            if record.tag == tag::LAND
                && !surface
                    .arrays
                    .material
                    .diffuse
                    .as_deref()
                    .is_some_and(|path| path.starts_with("terrain:"))
            {
                continue;
            }
            if record.tag != tag::LAND {
                if record.tag != tag::REFR {
                    continue;
                }
                let reference = Refr::from_record(record).expect("reference");
                let Some(model) = source
                    .order()
                    .winner(source.order().resolve(owner.file, reference.base))
                    .and_then(model_for)
                else {
                    continue;
                };
                let model = soe_bsa::normalize(&model);
                if !model.starts_with("architecture/") && !model.starts_with("landscape/rocks/") {
                    continue;
                }
            }
            let points: Vec<_> = surface
                .arrays
                .positions
                .iter()
                .map(|p| {
                    let p = surface.instance_transform.map_or(*p, |transform| {
                        let origin = [transform.origin.x, transform.origin.y, transform.origin.z];
                        std::array::from_fn(|r| {
                            origin[r] + (0..3).map(|c| transform.basis[r][c] * p[c]).sum::<f32>()
                        })
                    });
                    let p = crate::units::GodotPos::new(p[0], p[1], p[2]).to_skyrim();
                    Vec3::new(p.x, p.y, p.z)
                })
                .collect();
            for triangle in surface.arrays.indices.as_chunks::<3>().0 {
                let [a, b, c] = triangle.map(|index| points[index as usize]);
                let normal = (b - a).cross(c - a);
                if normal.length_squared() < 0.000001 || normal.z.abs() < 0.7 * normal.length() {
                    continue;
                }
                let min = a.min(b).min(c);
                let max = a.max(b).max(c);
                for x in (min.x / 256.0).floor() as i32..=(max.x / 256.0).floor() as i32 {
                    for y in (min.y / 256.0).floor() as i32..=(max.y / 256.0).floor() as i32 {
                        grid.entry((x, y)).or_default().push([a, b, c]);
                    }
                }
            }
        }
    }
    grid
}

fn rendered_floor_height(grid: &RenderFloorGrid, point: Vec3) -> Option<f32> {
    let triangles = grid.get(&(
        (point.x / 256.0).floor() as i32,
        (point.y / 256.0).floor() as i32,
    ))?;
    triangles
        .iter()
        .filter_map(|&[a, b, c]| {
            let cross =
                |u: Vec3, v: Vec3, w: Vec3| (v.x - u.x) * (w.y - u.y) - (v.y - u.y) * (w.x - u.x);
            let sides = [cross(a, b, point), cross(b, c, point), cross(c, a, point)];
            if !(sides.iter().all(|v| *v >= 0.0) || sides.iter().all(|v| *v <= 0.0)) {
                return None;
            }
            let normal = (b - a).cross(c - a);
            Some(a.z - (normal.x * (point.x - a.x) + normal.y * (point.y - a.y)) / normal.z)
        })
        .min_by(|a, b| (a - point.z).abs().total_cmp(&(b - point.z).abs()))
}

#[test]
fn installed_walkable_floor_grid_has_collision() {
    let Some(source) = source() else { return };
    let inn = source.resolve_cell("WhiterunBanneredMare").expect("inn");
    let city = source
        .order()
        .winners_of(tag::WRLD)
        .find_map(|(id, record)| {
            (record.editor_id().as_deref() == Some("WhiterunWorld")).then_some(id)
        })
        .expect("city");
    let mut total_misses = 0;
    for (name, cell, bounds) in [
        ("inn", inn, [-1536.0, 256.0, -1152.0, 512.0]),
        (
            "square",
            source
                .cell_index()
                .exterior_cell(city, 6, -2)
                .expect("market grid"),
            [23552.0, 25984.0, -8192.0, -6144.0],
        ),
    ] {
        let location = source
            .load_location(
                cell,
                if cell == inn {
                    [0.0, 0.0, 64.0]
                } else {
                    [24675.0, -7391.0, -3392.0]
                },
                1,
            )
            .expect("location with collision and inherited terrain");
        let render = rendered_floor_grid(&source, &location.cells);
        let physics = location.collision.world;
        let mut checked = 0;
        let mut misses = Vec::new();
        for (id, record) in source.order().winners_of(tag::NAVM) {
            if record.is_deleted() {
                continue;
            }
            let owner = source.order().winner_ref(id).expect("navmesh");
            let Some(geometry) = soe_esm::records::navm::NavMesh::from_record(record)
                .expect("NAVM")
                .geometry
            else {
                continue;
            };
            let matches = match geometry.parent {
                soe_esm::records::navm::Parent::Cell(raw) => {
                    source.order().resolve(owner.file, raw) == cell
                }
                soe_esm::records::navm::Parent::Grid { world, .. } => {
                    cell != inn && source.order().resolve(owner.file, world) == city
                }
            };
            if !matches {
                continue;
            }
            for triangle in &geometry.triangles {
                if triangle.flags & 0x0008 != 0 {
                    continue;
                }
                let [a, b, c] = triangle.vertices.map(|v| {
                    let p = geometry.vertices[usize::from(v)];
                    Vec3::new(p[0], p[1], p[2])
                });
                let normal = (b - a).cross(c - a);
                if normal.z.abs() < 0.001 {
                    continue;
                }
                let min_x = a.x.min(b.x).min(c.x).max(bounds[0]);
                let max_x = a.x.max(b.x).max(c.x).min(bounds[1]);
                let min_y = a.y.min(b.y).min(c.y).max(bounds[2]);
                let max_y = a.y.max(b.y).max(c.y).min(bounds[3]);
                for x in (min_x / 32.0).ceil() as i32..=(max_x / 32.0).floor() as i32 {
                    for y in (min_y / 32.0).ceil() as i32..=(max_y / 32.0).floor() as i32 {
                        let mut p = Vec3::new(x as f32 * 32.0, y as f32 * 32.0, 0.0);
                        let cross = |u: Vec3, v: Vec3, w: Vec3| {
                            (v.x - u.x) * (w.y - u.y) - (v.y - u.y) * (w.x - u.x)
                        };
                        let edges = [cross(a, b, p), cross(b, c, p), cross(c, a, p)];
                        if !(edges.iter().all(|edge| *edge >= 0.0)
                            || edges.iter().all(|edge| *edge <= 0.0))
                        {
                            continue;
                        }
                        p.z = a.z - (normal.x * (p.x - a.x) + normal.y * (p.y - a.y)) / normal.z;
                        if cell == inn && p.z > 200.0 {
                            continue;
                        }
                        // CONTEXT: Board and paving seams can expose no render triangle
                        // at an exact lattice coordinate. Keep the NAVM height there,
                        // so a missing visual floor never skips the collision check.
                        p.z = rendered_floor_height(&render, p).unwrap_or(p.z);
                        checked += 1;
                        let hit = physics.raycast(p + Vec3::Z * 16.0, p - Vec3::Z * 32.0);
                        if hit.is_none() {
                            if misses.len() < 8 {
                                eprintln!(
                                    "FLOOR_DIAGNOSTIC {p:?} nav={id} flags={} triangle={} deep={:?}", record.flags, triangle.flags,
                                    physics.raycast(p + Vec3::Z * 16.0, p - Vec3::Z * 512.0)
                                );
                                if cell == inn {
                                    for placed in
                                        source.placed_cell_colliders(cell).expect("placed").0
                                    {
                                        let b = placed.collider.aabb();
                                        if p.x >= b.min.x
                                            && p.x <= b.max.x
                                            && p.y >= b.min.y
                                            && p.y <= b.max.y
                                        {
                                            eprintln!(
                                                "FLOOR_MODEL {} {} bounds={b:?} shape={:?}",
                                                placed.reference,
                                                placed.collider.node_name,
                                                placed.collider.shape.triangle_count()
                                            );
                                        }
                                    }
                                }
                            }
                            misses.push(p);
                        }
                    }
                }
            }
        }
        eprintln!(
            "FLOOR {name} checked={checked} misses={} {:?}",
            misses.len(),
            &misses[..misses.len().min(8)]
        );
        assert!(checked > 100, "{name} floor survey must be nonempty");
        std::fs::write(
            format!("/tmp/lead-world-{name}-misses.json"),
            serde_json::to_vec(&misses.iter().map(|p| [p.x, p.y, p.z]).collect::<Vec<_>>())
                .expect("coordinates"),
        )
        .expect("miss report");
        total_misses += misses.len();
    }
    assert_eq!(
        total_misses, 0,
        "walkable floor grid must have collision under every sample"
    );
}

#[cfg(feature = "godot")]
#[test]
fn installed_exterior_scene_restores_npcs_and_stall_activation() {
    let (Some(data), Some(binary)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = tempfile::tempdir().expect("scene directory");
    let script = directory.path().join("world_scene.gd");
    std::fs::write(&script, include_str!("world_scene.gd")).expect("scene script");
    let output = std::process::Command::new(binary)
        .env("SKYRIM_DATA", data)
        .env("SOE_PLAY_PATH", "gate")
        .env("SOE_THREADED_SIMULATION", "0")
        .env_remove("SOE_AUD_11_TEST")
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .output()
        .expect("Godot starts");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(root.join("target/world-scene.log"), &log).expect("scene log");
    eprintln!("{log}");
    assert!(
        output.status.success()
            && log.contains("WORLD_SCENE PASS")
            && !log.contains("SCRIPT ERROR"),
        "host exit {:?}",
        output.status.code()
    );
}

#[test]
fn installed_paving_and_floor_diagnostics() {
    let Some(source) = source() else { return };
    let block = source
        .resolve_exterior_block("WhiterunOrigin", 1)
        .expect("block");
    eprintln!("BLOCK origin={:?} cells={:?}", block.origin, block.cells);
    for object in [0x01A460, 0x05B2E4, 0x05D67F] {
        let id = GlobalFormId { file: 0, object };
        let owner = source.order().winner_ref(id).expect("reference");
        let record = source.order().record(owner);
        let refr = Refr::from_record(record).expect("REFR");
        let model = model_for(
            source
                .order()
                .winner(source.order().resolve(owner.file, refr.base))
                .expect("base"),
        )
        .expect("model");
        let file = crate::data_source::placed_model(
            &source.vfs().read(&format!("meshes/{model}")).expect("NIF"),
        )
        .expect("placed NIF");
        eprintln!("MODEL {id} {model} placement={:?}", placement_iso(&refr));
        for mesh in file.meshes().expect("geometry") {
            let bounds = soe_physics::Aabb::of_points(mesh.positions.iter().map(|p| {
                let v = mesh.transform.apply(soe_nif::Vec3::new(p[0], p[1], p[2]));
                placement_iso(&refr).apply(Vec3::new(v.x, v.y, v.z))
            }));
            eprintln!("RENDER {} {bounds:?}", mesh.name);
        }
        for (i, block) in file.blocks.iter().enumerate() {
            if let soe_nif::blocks::Block::Collision { type_name, data } = block {
                let decoded = soe_physics::bhk::decode(type_name, data, &file.header).expect("bhk");
                match decoded {
                    soe_physics::bhk::Bhk::CompressedMeshData(data) => {
                        eprintln!(
                            "CHUNKS {i} error={} bounds={:?} {:?} chunks={} big_triangles={}",
                            data.error,
                            data.aabb_min,
                            data.aabb_max,
                            data.chunks.len(),
                            data.big_triangles.len()
                        );
                        for chunk in &data.chunks {
                            eprintln!("CHUNK translation={:?} transform={} reference={} vertices={} tris={}", chunk.translation,chunk.transform_index,chunk.reference,chunk.vertices.len(),chunk.triangles().len());
                        }
                    }
                    _ => eprintln!("BHK {i} {decoded:?}"),
                }
            }
        }
        let colliders = source.colliders_of(&model, &refr).expect("colliders");
        for collider in &colliders.colliders {
            eprintln!(
                "COLLIDER static={} bounds={:?} layer={:?} transform={:?}",
                collider.is_static(),
                collider.aabb(),
                collider.layer,
                collider.transform
            );
            let mut world = SimpleWorld::new();
            world.add_static(collider);
            for [x, y] in [[24500.0, -7500.0], [24675.0, -7391.0], [25244.0, -7425.0]] {
                let p = Vec3::new(x, y, -3300.0);
                eprintln!(
                    "PAVING_RAY {p:?} {:?}",
                    world.raycast(p, p - Vec3::Z * 400.0)
                );
            }
        }
    }
}

#[test]
fn installed_square_render_and_collision_diagnostics() {
    let Some(source) = source() else { return };
    let city = source
        .order()
        .winners_of(tag::WRLD)
        .find_map(|(id, record)| {
            (record.editor_id().as_deref() == Some("WhiterunWorld")).then_some(id)
        })
        .expect("world");
    let cell = source
        .cell_index()
        .exterior_cell(city, 6, -2)
        .expect("market cell");
    let location = source
        .load_location(cell, [24675.0, -7391.0, -3392.0], 1)
        .expect("market");
    let points = [
        [23584.0, -8192.0, -3407.1265],
        [24832.0, -6208.0, -3202.3074],
        [24896.0, -6464.0, -3334.208],
        [25600.0, -6496.0, -3228.8188],
        [25408.0, -7840.0, -3403.2144],
        [24000.0, -7936.0, -3403.0613],
        [24224.0, -6752.0, -3225.3125],
    ];
    for [x, y, z] in points {
        let mut hits = Vec::new();
        for (_, loaded) in &location.cells {
            for surface in &loaded.surfaces {
                let vertices: Vec<_> = surface
                    .arrays
                    .positions
                    .iter()
                    .map(|p| {
                        let p = surface.instance_transform.map_or(*p, |transform| {
                            let origin =
                                [transform.origin.x, transform.origin.y, transform.origin.z];
                            std::array::from_fn(|r| {
                                origin[r]
                                    + (0..3).map(|c| transform.basis[r][c] * p[c]).sum::<f32>()
                            })
                        });
                        let p = crate::units::GodotPos::new(p[0], p[1], p[2]).to_skyrim();
                        Vec3::new(p.x, p.y, p.z)
                    })
                    .collect();
                for triangle in surface.arrays.indices.as_chunks::<3>().0 {
                    let [a, b, c] = triangle.map(|index| vertices[index as usize]);
                    let normal = (b - a).cross(c - a);
                    if normal.z.abs() < 0.7 * normal.length() {
                        continue;
                    }
                    let p = Vec3::new(x, y, 0.0);
                    let cross = |u: Vec3, v: Vec3, w: Vec3| {
                        (v.x - u.x) * (w.y - u.y) - (v.y - u.y) * (w.x - u.x)
                    };
                    let sides = [cross(a, b, p), cross(b, c, p), cross(c, a, p)];
                    if !(sides.iter().all(|v| *v >= 0.0) || sides.iter().all(|v| *v <= 0.0)) {
                        continue;
                    }
                    let height = a.z - (normal.x * (x - a.x) + normal.y * (y - a.y)) / normal.z;
                    if (height - z).abs() < 256.0 {
                        let model = source
                            .order()
                            .winner_ref(surface.reference)
                            .and_then(|owner| {
                                let record = source.order().record(owner);
                                if record.tag != tag::REFR {
                                    return None;
                                }
                                let reference = Refr::from_record(record).ok()?;
                                source
                                    .order()
                                    .winner(source.order().resolve(owner.file, reference.base))
                                    .and_then(model_for)
                            })
                            .unwrap_or_else(|| "LAND".into());
                        hits.push((height, surface.reference, model));
                    }
                }
            }
        }
        hits.sort_by(|a, b| (a.0 - z).abs().total_cmp(&(b.0 - z).abs()));
        hits.dedup_by(|a, b| a.1 == b.1 && (a.0 - b.0).abs() < 0.1);
        let p = Vec3::new(x, y, z);
        eprintln!(
            "SURFACE_SAMPLE {p:?} render={:?} physics={:?}",
            &hits[..hits.len().min(6)],
            location
                .collision
                .world
                .raycast(p + Vec3::Z * 256.0, p - Vec3::Z * 256.0)
        );
    }
}
