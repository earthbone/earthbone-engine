// SPDX-License-Identifier: GPL-3.0-or-later

//! Resource release uses Godot's RefCounted and ArrayMesh ownership contracts.

#[path = "../../nif/tests/common/mod.rs"]
mod fixture;

#[test]
fn integration_memory_clear_preserves_live_meshes_and_releases_unused_resources(
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return Ok(());
    };
    let directory = tempfile::tempdir()?;
    let data = directory.path().join("Data");
    std::fs::create_dir_all(data.join("meshes"))?;
    let master = soe_esm::Plugin::new(soe_esm::FileHeader::default()).to_bytes();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(data.join(name), &master)?;
    }
    std::fs::write(
        data.join("meshes/quad.nif"),
        mesh("textures/missing.dds", 0),
    )?;
    for (name, texture) in [
        ("first.nif", "textures/first.dds"),
        ("second.nif", "textures/second.dds"),
    ] {
        std::fs::write(
            data.join("meshes").join(name),
            mesh(texture, soe_nif::blocks::flags2::TREE_ANIM),
        )?;
    }
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project)
        .args(["--script", "integration_memory_resources_test.gd"])
        .env_remove("SKYRIM_DATA")
        .env("SOE_MEMORY_FIXTURE", &data)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("integration_memory_resources: PASS"),
        "{stdout}"
    );
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
    Ok(())
}

fn mesh(texture: &str, flags: u32) -> Vec<u8> {
    let mut writer = fixture::NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let textures = writer.texture_set(&[texture]);
    let textures = writer.add("BSShaderTextureSet", textures);
    let shader = writer.lighting_shader(0, 0, flags, textures, &[]);
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (vertices, triangles) = fixture::quad();
    let shape = writer.bs_tri_shape(
        "Quad",
        [0.0; 3],
        -1,
        shader,
        -1,
        soe_nif::vertex::attr::VERTEX,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    let mut node = fixture::Buf::default();
    writer.av_object(&mut node, "Root", [0.0; 3], fixture::IDENTITY, 1.0, -1, -1);
    node.refs(&[shape]).u32(0);
    writer.fill(root, node.0);
    writer.roots = vec![root];
    writer.finish()
}
