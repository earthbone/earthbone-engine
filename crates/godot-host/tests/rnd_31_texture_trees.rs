// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

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
                    let (cell_name, capture_radius) = match editor_cell {
                        Some(editor_cell) if editor_cell.name != editor_cell.id => {
                            (editor_cell.name.clone(), 0)
                        }
                        _ => (origin.to_string(), radius),
                    };
                    target = Some((position, model, cell_name, capture_radius));
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
    println!("RND-31 tree model: {model}\n{stdout}");
}

#[test]
fn rnd_31_vanilla_nif_texture_paths_resolve_from_archives() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping RND-31 vanilla NIF survey");
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
    for archive in &archives {
        vfs.add_archive_path(archive)
            .unwrap_or_else(|error| panic!("open {}: {error}", archive.display()));
    }
    vfs.add_loose_root(&data).expect("index loose Data files");

    let nifs = vfs
        .list("meshes")
        .into_iter()
        .filter(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("nif"))
        })
        .collect::<Vec<_>>();
    assert!(
        !nifs.is_empty(),
        "no vanilla NIFs found in installed archives"
    );
    let tree_path = "meshes/landscape/trees/tundradriftwood02.nif";
    assert!(vfs.exists(tree_path), "installed tree mesh is missing");
    let mut missing = Vec::new();
    let mut texture_references = 0usize;
    let mut tree_meshes = 0usize;
    let mut tree_leaf_alpha = false;
    let mut tree_two_sided = false;
    let mut tree_root_files = 0usize;
    let mut rooted_tree_meshes = 0usize;
    let mut rooted_tree_diffuse = 0usize;
    let mut rooted_tree_normal = 0usize;
    let mut target_diffuse = false;
    let mut target_normal = false;
    let mut tree_textures = Vec::new();

    for path in nifs {
        let bytes = vfs
            .read(path)
            .unwrap_or_else(|error| panic!("read {path}: {error}"));
        let Ok(file) = soe_nif::parse(&bytes) else {
            continue;
        };
        let is_target_tree = path == tree_path;
        let has_tree_root = file
            .header
            .block_type_index
            .iter()
            .enumerate()
            .any(|(index, _)| {
                matches!(
                    file.header.block_type(index),
                    "BSTreeNode" | "BSLeafAnimNode"
                )
            });
        if has_tree_root {
            tree_root_files += 1;
        }
        let meshes = file.meshes().unwrap_or_default();
        for mesh in meshes {
            let tree_animated = mesh.material.flags2 & soe_nif::blocks::flags2::TREE_ANIM != 0;
            if has_tree_root && tree_animated {
                rooted_tree_meshes += 1;
                tree_leaf_alpha |= mesh.material.alpha_test.is_some();
                tree_two_sided |= mesh.material.double_sided;
                rooted_tree_diffuse +=
                    usize::from(mesh.material.diffuse().is_some_and(|path| vfs.exists(path)));
                rooted_tree_normal +=
                    usize::from(mesh.material.normal().is_some_and(|path| vfs.exists(path)));
            }
            if is_target_tree && tree_animated {
                tree_meshes += 1;
                target_diffuse |= mesh.material.diffuse().is_some_and(|path| vfs.exists(path));
                target_normal |= mesh.material.normal().is_some_and(|path| vfs.exists(path));
            }
            for texture in mesh
                .material
                .textures
                .iter()
                .chain(mesh.material.effect.iter().flat_map(|effect| {
                    [
                        &effect.source_texture,
                        &effect.greyscale_texture,
                        &effect.env_map_texture,
                        &effect.normal_texture,
                        &effect.env_mask_texture,
                    ]
                }))
                .filter(|path| !path.is_empty())
            {
                texture_references += 1;
                if is_target_tree {
                    tree_textures.push(texture.clone());
                }
                if !texture.starts_with("textures/") || !vfs.exists(texture) {
                    missing.push(format!("{path} -> {texture}"));
                }
            }
        }
    }
    tree_textures.sort();
    tree_textures.dedup();
    assert!(
        texture_references > 0,
        "vanilla NIF survey found no texture paths"
    );
    assert!(
        tree_meshes > 0,
        "the target tree has no TREE_ANIM material surfaces"
    );
    assert!(
        tree_root_files > 0,
        "vanilla NIFs contain no named tree roots"
    );
    assert!(
        rooted_tree_meshes > 0,
        "no TREE_ANIM surfaces occur under a BSTreeNode or BSLeafAnimNode root"
    );
    assert!(
        rooted_tree_diffuse > 0,
        "tree diffuse textures did not resolve"
    );
    assert!(
        rooted_tree_normal > 0,
        "tree normal textures did not resolve"
    );
    assert!(
        tree_leaf_alpha,
        "target tree leaves lack NiAlphaProperty cutout"
    );
    assert!(
        tree_two_sided,
        "target tree leaves lack two-sided rendering"
    );
    assert!(
        tree_textures
            .iter()
            .any(|path| path.ends_with("tundradriftwoodbark01.dds")),
        "the target tree's authored bark texture was not read: {tree_textures:?}"
    );
    assert!(
        tree_textures.iter().all(|path| vfs.exists(path)),
        "the target tree has unresolved textures: {tree_textures:?}"
    );
    assert!(
        target_diffuse,
        "the target tree has no resolved diffuse texture"
    );
    assert!(
        target_normal,
        "the target tree has no resolved normal texture"
    );
    println!(
        "RND-31 vanilla NIF scan: archives={}, texture references={}, unavailable paths={}, tree roots={}, rooted tree meshes={}, target tree meshes={}, target tree textures={}",
        archives.len(), texture_references, missing.len(), tree_root_files, rooted_tree_meshes, tree_meshes, tree_textures.len()
    );
    for path in missing.iter().take(10) {
        println!("RND-31 unavailable vanilla texture: {path}");
    }
}
