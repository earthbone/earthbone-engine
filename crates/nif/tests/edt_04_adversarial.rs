// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use soe_nif::static_mesh::{export_nif, import_gltf};

fn fixture(document: &str) -> (PathBuf, PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock follows the epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("soe-edt-04-adversarial-{nonce}"));
    fs::create_dir_all(&dir).expect("create fixture directory");
    let mut buffer = Vec::new();
    for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        buffer.extend(value.to_le_bytes());
    }
    for value in [1.0_f32, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0] {
        buffer.extend(value.to_le_bytes());
    }
    for value in [0.0_f32, 0.0, 1.0, 0.0, 0.0, 1.0] {
        buffer.extend(value.to_le_bytes());
    }
    for value in [0_u16, 1, 2] {
        buffer.extend(value.to_le_bytes());
    }
    fs::write(dir.join("mesh.bin"), buffer).expect("write glTF buffer");
    let path = dir.join("mesh.gltf");
    fs::write(&path, document).expect("write glTF document");
    (dir, path)
}

fn document(node: &str, material: &str) -> String {
    format!(
        r#"{{
          "asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
          "nodes":[{node}],
          "meshes":[{{"name":"Panel","primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3,"material":0}}]}}],
          "materials":[{material}],"textures":[{{"source":0}},{{"source":1}},{{"source":2}},{{"source":3}}],
          "images":[{{"uri":"textures/diffuse.dds"}},{{"uri":"textures/normal.dds"}},{{"uri":"textures/glow.dds"}},{{"uri":"textures/specular.dds"}}],
          "buffers":[{{"uri":"mesh.bin","byteLength":102}}],
          "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":24}},{{"buffer":0,"byteOffset":96,"byteLength":6}}],
          "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}},{{"bufferView":3,"componentType":5123,"count":3,"type":"SCALAR"}}]
        }}"#
    )
}

#[test]
fn edt_04_all_supported_material_texture_links_survive_export() {
    let material = r#"{"pbrMetallicRoughness":{"baseColorTexture":{"index":0},"metallicRoughnessTexture":{"index":3}},"normalTexture":{"index":1},"emissiveTexture":{"index":2}}"#;
    let (dir, path) = fixture(&document(r#"{"name":"PanelNode","mesh":0}"#, material));
    let imported = import_gltf(&path).expect("import static material");
    let bytes = export_nif(&imported).expect("export NIF");
    let file = soe_nif::parse(&bytes).expect("parse exported NIF");
    let mesh = file.meshes().expect("read exported shapes").remove(0);
    assert_eq!(mesh.material.textures[0], "textures/diffuse.dds");
    assert_eq!(mesh.material.textures[1], "textures/normal.dds");
    assert_eq!(mesh.material.textures[2], "textures/glow.dds");
    assert_eq!(mesh.material.textures[7], "textures/specular.dds");
    fs::remove_dir_all(dir).expect("remove fixture directory");
}

#[test]
fn edt_04_nested_nonuniform_transform_preserves_geometry_and_normals() {
    let nested = r#"{"name":"Parent","translation":[10,20,30],"children":[1]},{"name":"PanelNode","mesh":0,"scale":[2,1,0.5]}"#;
    let document = document(
        r#"{"name":"Parent","translation":[10,20,30],"children":[1]}"#,
        "{}",
    )
    .replace(
        r#""nodes":[{"name":"Parent","translation":[10,20,30],"children":[1]}]"#,
        &format!(r#""nodes":[{nested}]"#),
    );
    let (dir, path) = fixture(&document);
    let imported = import_gltf(&path).expect("import nested scene");
    assert_eq!(imported[0].positions[1], [12.0, 20.0, 30.0]);
    let normal = imported[0].normals[0];
    assert!((normal[0] - 0.4472).abs() < 0.002, "{normal:?}");
    assert!((normal[1] - 0.8944).abs() < 0.002, "{normal:?}");
    let exported = export_nif(&imported).expect("export transformed shape");
    let mesh = soe_nif::parse(&exported)
        .expect("parse transformed NIF")
        .meshes()
        .expect("read transformed shape")
        .remove(0);
    assert_eq!(mesh.positions, imported[0].positions);
    assert_eq!(mesh.indices, imported[0].indices);
    fs::remove_dir_all(dir).expect("remove fixture directory");
}

#[test]
fn edt_04_missing_scene_mesh_is_rejected() {
    let (dir, path) = fixture(r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[]}],"scene":0}"#);
    assert!(import_gltf(&path).is_err());
    fs::remove_dir_all(dir).expect("remove fixture directory");
}
