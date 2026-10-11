// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_godot_host::mesh::{MaterialDesc, MaterialKey};

#[test]
fn rnd_31_tree_animation_flag_has_distinct_material_cache_identity() {
    let ordinary = MaterialDesc::default();
    let tree = MaterialDesc {
        shader_flags_2: soe_nif::blocks::flags2::TREE_ANIM,
        ..MaterialDesc::default()
    };

    assert_ne!(
        MaterialKey::from(&ordinary),
        MaterialKey::from(&tree),
        "the tree flag selects a different shader and must split cached materials"
    );
}

#[test]
fn rnd_31_installed_tundra_driftwood_animated_surfaces_and_textures() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed NIF texture survey");
        return;
    };

    let mut archives = std::fs::read_dir(&data)
        .expect("read Skyrim Data directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bsa"))
        })
        .collect::<Vec<_>>();
    archives.sort();
    let mut vfs = soe_bsa::Vfs::new();
    for archive in archives {
        vfs.add_archive_path(&archive)
            .unwrap_or_else(|error| panic!("open {}: {error}", archive.display()));
    }
    vfs.add_loose_root(&data).expect("index loose Data files");

    let path = "meshes/landscape/trees/tundradriftwood02.nif";
    let bytes = vfs
        .read(path)
        .unwrap_or_else(|error| panic!("read {path}: {error}"));
    let file = soe_nif::parse(&bytes).expect("parse installed Tundra Driftwood NIF");
    let meshes = file.meshes().expect("extract Tundra Driftwood meshes");
    let mut tree_surfaces = 0;
    let mut bark = false;
    let mut missing = Vec::new();
    let mut found = Vec::new();
    for mesh in meshes {
        for texture in mesh
            .material
            .textures
            .iter()
            .filter(|path| !path.is_empty())
        {
            found.push(texture.clone());
            if texture.ends_with("tundradriftwoodbark01.dds") {
                bark = true;
            }
            if !texture.starts_with("textures/") || !vfs.exists(texture) {
                missing.push(texture.clone());
            }
        }
        if mesh.material.flags2 & soe_nif::blocks::flags2::TREE_ANIM == 0 {
            continue;
        }
        tree_surfaces += 1;
        assert!(
            mesh.material.alpha_test.is_some(),
            "tree surface lacks alpha test"
        );
        assert!(mesh.material.double_sided, "tree surface is not two sided");
    }
    assert!(
        tree_surfaces > 0,
        "installed Tundra Driftwood has no animated tree surfaces"
    );
    assert!(
        bark,
        "installed Tundra Driftwood bark path was not normalized: {found:?}"
    );
    assert!(
        missing.is_empty(),
        "installed Tundra Driftwood textures did not resolve: {missing:?}"
    );
}

#[test]
fn rnd_31_installed_whiterun_tree_capture_shows_texture_detail_and_cutouts() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA").map(PathBuf::from),
        std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_GODOT_BIN unset, skipping RND-31 capture");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    assert!(
        extension.exists(),
        "build the release Godot extension before capture"
    );

    let source = soe_godot_host::data_source::DataSource::open(&data)
        .unwrap_or_else(|error| panic!("open installed DataSource: {error}"));
    let order = source.shared_order();
    let editor_cells = source.editor_cells();
    let mut target = None;
    for (origin, radius) in [("WhiterunExterior04", 8), ("WhiterunOrigin", 1)] {
        let block = source
            .resolve_exterior_block(origin, radius)
            .unwrap_or_else(|error| panic!("{origin} exterior block: {error}"));
        for cell in &block.cells {
            if cell.x.abs_diff(block.origin.0) > 2 || cell.y.abs_diff(block.origin.1) > 2 {
                continue;
            }
            for id in source.cell_index().references(cell.id) {
                let Some(reference) = order.winner_ref(*id) else {
                    continue;
                };
                let record = order.record(reference);
                if record.tag != soe_esm::tag::REFR {
                    continue;
                }
                let Ok(refr) = soe_esm::records::refr::Refr::from_record(record) else {
                    continue;
                };
                let base = order.resolve(reference.file, refr.base);
                let Some(base_record) = order.winner(base) else {
                    continue;
                };
                let model = if base_record.tag == soe_esm::tag::STAT {
                    let Ok(stat) = soe_esm::records::stat::Static::from_record(base_record) else {
                        continue;
                    };
                    stat.model
                } else if base_record.tag == soe_esm::Tag::new(b"TREE") {
                    base_record
                        .find(soe_esm::Tag::new(b"MODL"))
                        .map(soe_esm::cursor::zstring)
                } else {
                    None
                };
                let Some(model) = model else { continue };
                let model_path = model.replace('\\', "/").to_lowercase();
                if model_path.contains("/trees/") || model_path.starts_with("trees/") {
                    let position = refr.transform.unwrap_or_default().position;
                    let editor_cell = editor_cells
                        .iter()
                        .find(|editor_cell| editor_cell.id == cell.id.to_string());
                    let cell_name = match editor_cell {
                        Some(editor_cell) if editor_cell.name != editor_cell.id => {
                            editor_cell.name.clone()
                        }
                        _ => origin.to_string(),
                    };
                    target = Some((position, model, cell_name, 2));
                    break;
                }
            }
        }
        if target.is_some() {
            break;
        }
    }
    let (position, model, origin, capture_radius) =
        target.expect("landscape tree reference in Whiterun exterior blocks");
    let capture = workspace.join("target/rnd_31/whiterun-tree.png");
    std::fs::create_dir_all(capture.parent().unwrap()).expect("create capture folder");
    let output = std::process::Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args(["--resolution", "640x360", "--script"])
        .arg(project.join("rnd_31_tree_capture.gd"))
        .env("SKYRIM_DATA", &data)
        .env("SOE_RND31_CAPTURE_PATH", &capture)
        .env("SOE_RND31_ORIGIN", origin)
        .env("SOE_RND31_RADIUS", capture_radius.to_string())
        .env(
            "SOE_RND31_TARGET",
            format!("{},{},{}", position[0], position[1], position[2]),
        )
        .output()
        .expect("start windowed RND-31 tree capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RND-31 RESULT: PASS"),
        "RND-31 Godot capture failed for {model}\n{stdout}\n{stderr}"
    );
    assert!(
        capture.exists(),
        "tree capture was not written: {}",
        capture.display()
    );
}
