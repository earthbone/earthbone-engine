// SPDX-License-Identifier: GPL-3.0-or-later

//! Mountain coverage uses xEdit WRLD small-world flags and archived BTO detail.

use std::{error::Error, path::Path, process::Command};

use soe_godot_host::{
    data_source::DataSource,
    terrain_lod::{LodBlockCoord, LodLevel},
};

#[cfg(feature = "godot")]
#[path = "common/isolated_project.rs"]
mod isolated_project;

#[cfg(feature = "godot")]
#[path = "../../nif/tests/common/mod.rs"]
mod nif_fixture;

#[cfg(feature = "godot")]
#[test]
fn mountain_lod_remote_small_world_cell_keeps_parent_models() -> Result<(), Box<dyn Error>> {
    use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return Ok(());
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let library = std::env::var_os("SOE_TEST_EXTENSION")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/libsoe_godot_host.so"));
    let project = isolated_project::project(&["mountain_lod_marker_test.gd"], Some(&library));
    let data = tempfile::tempdir()?;
    let mut plugin = Plugin::new(FileHeader::default());
    let mut worlds = Group::new(GroupKind::Top(tag::WRLD));
    for (object, name) in [(0x100u32, "Parent"), (0x200, "Child")] {
        let mut world = Record::new(tag::WRLD, object).zsub(tag::EDID, name);
        if object == 0x200 {
            world = world
                .sub(b"WNAM", 0x100u32.to_le_bytes())
                .sub(b"PNAM", 2u16.to_le_bytes())
                .sub(b"DATA", [soe_esm::records::wrld::flags::SMALL_WORLD]);
        }
        worlds
            .entries
            .push(Entry::Record(plugin.push_record(world)));
    }
    let mut children = Group::new(GroupKind::WorldChildren(0x200));
    let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: -1 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 1, y: -2 });
    for (object, name, x) in [
        (0x201u32, "RemoteMarkerCell", 12i32),
        (0x202, "CityModelCell", 13),
    ] {
        sub.entries.push(Entry::Record(
            plugin.push_record(
                Record::new(tag::CELL, object)
                    .zsub(tag::EDID, name)
                    .sub(b"DATA", [0u8, 0])
                    .sub(
                        b"XCLC",
                        [x, -10, 0]
                            .into_iter()
                            .flat_map(i32::to_le_bytes)
                            .collect::<Vec<_>>(),
                    ),
            ),
        ));
    }
    let reference = plugin.push_record(
        Record::new(tag::REFR, 0x400)
            .sub(b"NAME", 0x300u32.to_le_bytes())
            .sub(
                b"DATA",
                [13.0f32 * 4096.0, -10.0 * 4096.0, 5000.0, 0.0, 0.0, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(0x202));
    temporary.entries.push(Entry::Record(reference));
    let mut cell_children = Group::new(GroupKind::CellChildren(0x202));
    cell_children.entries.push(Entry::Group(temporary));
    sub.entries.push(Entry::Group(cell_children));
    block.entries.push(Entry::Group(sub));
    children.entries.push(Entry::Group(block));
    worlds.entries.push(Entry::Group(children));
    plugin.groups.push(worlds);
    let mut statics = Group::new(GroupKind::Top(tag::STAT));
    statics.entries.push(Entry::Record(
        plugin.push_record(Record::new(tag::STAT, 0x300).zsub(b"MODL", "model.nif")),
    ));
    plugin.groups.push(statics);
    std::fs::write(data.path().join("Skyrim.esm"), plugin.to_bytes())?;
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            data.path().join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )?;
    }
    let mesh = soe_nif::Mesh {
        name: "mountain coverage".into(),
        block: 0,
        transform: soe_nif::Transform::IDENTITY,
        bound: soe_nif::Bound {
            center: soe_nif::Vec3::new(0.0, 0.0, 0.0),
            radius: 100.0,
        },
        positions: vec![
            [53248.0, -40960.0, 5000.0],
            [53348.0, -40960.0, 5000.0],
            [53248.0, -40860.0, 5100.0],
        ],
        normals: vec![],
        tangents: vec![],
        uv_sets: vec![],
        uvs: vec![],
        colors: vec![],
        indices: vec![0, 1, 2],
        bone_weights: vec![],
        bone_indices: vec![],
        skin: None,
        material: soe_nif::mesh::Material::default(),
    };
    let mut writer = nif_fixture::NifWriter::new(100);
    let mut positions = mesh.positions.clone();
    positions.extend(mesh.positions.iter().map(|p| [p[0] + 4096.0, p[1], p[2]]));
    let vertices: Vec<_> = positions
        .into_iter()
        .map(|p| nif_fixture::TestVertex::at(p, [0.0, 0.0]))
        .collect();
    let mut shape = nif_fixture::Buf(writer.bs_tri_shape(
        "mountain",
        [0.0; 3],
        -1,
        -1,
        -1,
        soe_nif::vertex::attr::VERTEX,
        &vertices,
        &[[0, 1, 2], [3, 4, 5]],
        true,
    ));
    shape.u32(11);
    for index in 0..11 {
        shape
            .u8(0)
            .u32(if index == 10 { 3 } else { 0 })
            .u32(u32::from(index == 6 || index == 10));
    }
    let shape = writer.add("BSSubIndexTriShape", shape.0);
    let node = writer.ni_node("Root", [0.0; 3], nif_fixture::IDENTITY, 1.0, &[shape]);
    writer.roots = vec![writer.add("NiNode", node)];
    let bytes = writer.finish();
    let objects = data.path().join("meshes/terrain/parent/objects");
    std::fs::create_dir_all(&objects)?;
    std::fs::write(objects.join("parent.4.12.-12.bto"), &bytes)?;
    let mut full = mesh;
    full.positions = vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 100.0]];
    std::fs::write(
        data.path().join("meshes/model.nif"),
        soe_nif::static_mesh::export_nif(&[full])?,
    )?;
    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "mountain_lod_marker_test.gd"])
        .env("SOE_LOD_FIXTURE", data.path())
        .env_remove("SKYRIM_DATA")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("MOUNTAIN_MARKER_RESULT PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
    eprintln!("{stdout}");
    Ok(())
}

#[test]
fn mountain_lod_installed_walkthrough_bands() -> Result<(), Box<dyn Error>> {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return Ok(());
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").ok_or("SOE_GODOT_BIN required")?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("target/r4-lod");
    std::fs::create_dir_all(&directory)?;
    let source = DataSource::open_with_threads(Path::new(&data), 1)?;
    let (world, name) = source.resolve_world("Tamriel").ok_or("Tamriel missing")?;
    let anchor = source
        .lod_settings(&name)?
        .ok_or("Tamriel LOD settings missing")?
        .sw_cell;
    let mut bands = Vec::new();
    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        for point in [(12, -10), (12, -18)] {
            let coord = LodBlockCoord::containing_from(
                soe_godot_host::streaming::CellCoord::new(point.0, point.1),
                level,
                anchor,
            );
            for kind in ["terrain", "object"] {
                let loaded = if kind == "terrain" {
                    source.load_lod_block(world, &name, coord)?
                } else if level != LodLevel::L32 {
                    source.load_object_lod_block(world, &name, coord)?
                } else {
                    None
                };
                if let Some(loaded) = loaded {
                    for surface in loaded
                        .surfaces
                        .iter()
                        .filter(|s| s.arrays.material.water.is_none())
                    {
                        let size = |t: &Option<
                            std::sync::Arc<soe_godot_host::data_source::TextureData>,
                        >| {
                            t.as_ref().map(|t| [t.width(), t.height()])
                        };
                        bands.push(serde_json::json!({
                            "point": point, "kind": kind, "level": level.cells(), "x": coord.x, "y": coord.y,
                            "triangles": surface.arrays.triangle_count(),
                            "diffuse": surface.arrays.material.diffuse, "diffuse_size": size(&surface.textures.diffuse),
                            "normal": surface.arrays.material.normal, "normal_size": size(&surface.textures.normal),
                        }));
                    }
                }
            }
        }
    }
    std::fs::write(
        directory.join("source-bands.json"),
        serde_json::to_vec_pretty(&bands)?,
    )?;
    let loadout = soe_profile::LoadoutInput {
        game_dir: Path::new(&data)
            .parent()
            .ok_or("game directory missing")?
            .to_path_buf(),
        data_roots: vec![data.clone().into()],
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    }
    .resolve()?;
    let vfs = soe_godot_host::data_source::vfs_from_loadout(&loadout)?;
    let order = source.shared_order();
    let mut models = std::collections::BTreeMap::new();
    let mut statics = Vec::new();
    for (id, record) in order.winners_of(soe_esm::tag::STAT) {
        let view = soe_esm::records::stat::Static::from_record(record)?;
        let Some(full) = view.model.as_ref().filter(|path| {
            let path = path.to_ascii_lowercase();
            path.contains("mountain") || path.contains("cliff")
        }) else {
            continue;
        };
        let mut paths = vec![full.clone()];
        paths.extend(
            view.lod_models
                .iter()
                .flatten()
                .filter(|p| !p.is_empty())
                .cloned(),
        );
        for path in paths {
            if models.contains_key(&path) {
                continue;
            }
            let canonical = soe_bsa::normalize(&path);
            let asset = if canonical.starts_with("meshes/") {
                canonical
            } else {
                format!("meshes/{canonical}")
            };
            let mesh = match vfs.read(&asset) {
                Ok(bytes) => {
                    let file = soe_nif::parse(&bytes)?;
                    let meshes = file.meshes()?;
                    serde_json::json!({
                        "triangles": meshes.iter().map(soe_nif::Mesh::triangle_count).sum::<usize>(),
                        "materials": meshes.iter().map(|mesh| serde_json::json!({
                            "diffuse": mesh.material.diffuse(), "normal": mesh.material.normal(),
                        })).collect::<Vec<_>>(),
                    })
                }
                Err(soe_bsa::Error::NotFound { .. }) => serde_json::json!({"missing": true}),
                Err(error) => return Err(error.into()),
            };
            models.insert(path, mesh);
        }
        statics.push(
            serde_json::json!({"id": id.to_string(), "full": full, "lod_models": view.lod_models}),
        );
    }
    std::fs::write(
        directory.join("mountain-sources.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"statics": statics, "meshes": models}))?,
    )?;
    drop(order);
    drop(vfs);
    drop(source);
    let log = std::fs::File::create(directory.join("process.log"))?;
    let project = std::env::var_os("SOE_TEST_PROJECT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("godot"));
    let status = Command::new(godot)
        .current_dir(&root)
        .arg("--path")
        .arg(project)
        .args([
            "--resolution",
            "960x540",
            "--script",
            "mountain_lod_test.gd",
        ])
        .env("SOE_GODOT_TIMEOUT", "900")
        .env("SOE_LOG", "off")
        .env_remove("SOE_CAPTURE_PATH")
        .env_remove("SOE_AUD_11_TEST")
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()?;
    let output = std::fs::read_to_string(directory.join("process.log"))?;
    for line in output
        .lines()
        .filter(|line| line.starts_with("MOUNTAIN_") && !line.starts_with("MOUNTAIN_SURFACE"))
    {
        eprintln!("{line}");
    }
    assert!(
        status.success()
            && output.contains("MOUNTAIN_RESULT PASS")
            && !output.contains("SCRIPT ERROR"),
        "Godot {status}, see target/r4-lod/process.log"
    );
    Ok(())
}
