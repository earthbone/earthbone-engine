// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Scan installed NIF texture references against the load-order view, as RND-31 requires.

use std::path::PathBuf;

#[test]
fn rnd_31_every_installed_nif_texture_reference_resolves_after_normalization() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping installed NIF texture survey");
        return;
    };
    let mut archives = std::fs::read_dir(&data)
        .expect("read installed Data directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| soe_bsa::is_bsa_or_ba2(path))
        .collect::<Vec<_>>();
    archives.sort();
    let mut vfs = soe_bsa::Vfs::new();
    for archive in archives {
        vfs.add_archive_path(&archive)
            .unwrap_or_else(|error| panic!("open {}: {error}", archive.display()));
    }
    vfs.add_loose_root(&data).expect("index loose Data files");

    let mut scanned = 0usize;
    let mut malformed = Vec::new();
    let mut unavailable = Vec::new();
    for path in vfs.paths().filter(|path| path.starts_with("meshes/") && path.ends_with(".nif")) {
        let bytes = vfs.read(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
        let file = soe_nif::parse(&bytes).unwrap_or_else(|error| panic!("parse {path}: {error}"));
        for mesh in file.meshes().unwrap_or_else(|error| panic!("meshes {path}: {error}")) {
            for texture in mesh.material.textures.iter().filter(|texture| !texture.is_empty()) {
                scanned += 1;
                if !texture.starts_with("textures/") {
                    malformed.push(format!("{path} -> {texture}"));
                } else if !vfs.exists(texture) {
                    unavailable.push(format!("{path} -> {texture}"));
                }
            }
        }
    }
    assert!(scanned > 0, "installed archives exposed no NIF texture references");
    assert!(malformed.is_empty(), "un-normalized installed NIF textures: {malformed:?}");

    let path = "meshes/landscape/trees/tundradriftwood02.nif";
    let bytes = vfs.read(path).expect("installed Tundra Driftwood NIF resolves");
    let file = soe_nif::parse(&bytes).expect("parse installed Tundra Driftwood NIF");
    let meshes = file.meshes().expect("extract Tundra Driftwood surfaces");
    let mut tree_surfaces = 0;
    let mut bark_resolved = false;
    for mesh in meshes {
        for texture in mesh.material.textures.iter().filter(|texture| !texture.is_empty()) {
            assert!(texture.starts_with("textures/"), "tree path not normalized: {texture}");
            assert!(vfs.exists(texture), "tree texture unavailable: {texture}");
            bark_resolved |= texture.ends_with("tundradriftwoodbark01.dds");
        }
        if mesh.material.flags2 & soe_nif::blocks::flags2::TREE_ANIM != 0 {
            tree_surfaces += 1;
            assert!(mesh.material.alpha_test.is_some(), "tree surface lacks alpha cutout");
            assert!(mesh.material.double_sided, "tree surface is not two sided");
        }
    }
    assert!(tree_surfaces > 0, "installed NIF has no flagged tree surfaces");
    assert!(bark_resolved, "build-prefixed driftwood bark path did not resolve");
    eprintln!("scanned {scanned} references, {} unavailable in this installed view", unavailable.len());
}

#[test]
fn rnd_31_whiterun_tree_capture_keeps_leaf_textures_and_cutouts() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA").map(PathBuf::from),
        std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from),
    ) else {
        panic!("installed data and SOE_GODOT_BIN are required for the tree capture");
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = soe_godot_host::data_source::DataSource::open(&data).unwrap();
    let order = source.shared_order();
    let editor_cells = source.editor_cells();
    let mut target = None;
    for (origin, radius) in [("WhiterunExterior04", 8), ("WhiterunOrigin", 1)] {
        let block = source.resolve_exterior_block(origin, radius).unwrap();
        for cell in &block.cells {
            for id in source.cell_index().references(cell.id) {
                let Some(reference) = order.winner_ref(*id) else { continue };
                let record = order.record(reference);
                if record.tag != soe_esm::tag::REFR { continue; }
                let Ok(refr) = soe_esm::records::refr::Refr::from_record(record) else { continue };
                let base = order.resolve(reference.file, refr.base);
                let Some(base_record) = order.winner(base) else { continue };
                let model = if base_record.tag == soe_esm::tag::STAT {
                    soe_esm::records::stat::Static::from_record(base_record).ok().and_then(|s| s.model)
                } else if base_record.tag == soe_esm::Tag::new(b"TREE") {
                    base_record.find(soe_esm::Tag::new(b"MODL")).map(soe_esm::cursor::zstring)
                } else {
                    None
                };
                let Some(model) = model else { continue };
                let normalized = model.replace('\\', "/").to_lowercase();
                if normalized.contains("/trees/") || normalized.starts_with("trees/") {
                    let position = refr.transform.unwrap_or_default().position;
                    let editor = editor_cells.iter().find(|entry| entry.id == cell.id.to_string());
                    let (cell_name, capture_radius) = match editor {
                        Some(entry) if entry.name != entry.id => (entry.name.clone(), 0),
                        _ => (origin.to_owned(), radius),
                    };
                    target = Some((position, cell_name, capture_radius));
                    break;
                }
            }
            if target.is_some() { break; }
        }
        if target.is_some() { break; }
    }
    let (position, origin, radius) = target.expect("Whiterun exterior contains a tree reference");
    let capture = workspace.join("target/rnd_31/whiterun-tree-adversarial.png");
    std::fs::create_dir_all(capture.parent().unwrap()).unwrap();
    let output = std::process::Command::new(godot)
        .arg("--path")
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script", "rnd_31_tree_capture.gd"])
        .env("SKYRIM_DATA", &data)
        .env("SOE_RND31_CAPTURE_PATH", &capture)
        .env("SOE_RND31_ORIGIN", origin)
        .env("SOE_RND31_RADIUS", radius.to_string())
        .env("SOE_RND31_TARGET", format!("{},{},{}", position[0], position[1], position[2]))
        .output()
        .expect("start Whiterun tree renderer");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stdout.contains("RND-31 RESULT: PASS"), "{stdout}\n{stderr}");
    assert!(capture.is_file(), "tree capture image was not written");
}
