// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use soe_nif::blocks::{Block, ShaderType};
use soe_nif::parse;
use soe_nif::static_mesh::{export_nif, import_gltf};

fn scratch_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock follows the epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("soe-edt-04-{nonce}"))
}

#[test]
fn edt_04_static_gltf_geometry_and_material_links_survive_nif_export() {
    let dir = scratch_dir();
    fs::create_dir_all(dir.join("textures")).expect("create fixture directories");
    let mut buffer = Vec::new();
    for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        buffer.extend(value.to_le_bytes());
    }
    for value in [0.0_f32, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0] {
        buffer.extend(value.to_le_bytes());
    }
    for value in [0.0_f32, 0.0, 1.0, 0.0, 0.0, 1.0] {
        buffer.extend(value.to_le_bytes());
    }
    for value in [0_u16, 1, 2] {
        buffer.extend(value.to_le_bytes());
    }
    fs::write(dir.join("mesh.bin"), buffer).expect("write glTF buffer");
    fs::write(
        dir.join("textures/diffuse.png"),
        [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207,
            192, 240, 31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
            96, 130,
        ],
    )
    .expect("write linked image");
    let document = r#"{
      "asset":{"version":"2.0"},
      "scene":0,
      "scenes":[{"nodes":[0]}],
      "nodes":[{"name":"StaticPanel","mesh":0,"translation":[2,3,4]}],
      "meshes":[{"name":"Panel","primitives":[{"attributes":{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2},"indices":3,"material":0}]}],
      "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],
      "textures":[{"source":0}],
      "images":[{"uri":"textures/diffuse.png"}],
      "buffers":[{"uri":"mesh.bin","byteLength":102}],
      "bufferViews":[
        {"buffer":0,"byteOffset":0,"byteLength":36},
        {"buffer":0,"byteOffset":36,"byteLength":36},
        {"buffer":0,"byteOffset":72,"byteLength":24},
        {"buffer":0,"byteOffset":96,"byteLength":6}
      ],
      "accessors":[
        {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
        {"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"},
        {"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"},
        {"bufferView":3,"componentType":5123,"count":3,"type":"SCALAR"}
      ]
    }"#;
    let path = dir.join("static.gltf");
    fs::write(&path, document).expect("write glTF document");

    let imported = import_gltf(&path).expect("import declared static glTF subset");
    assert_eq!(imported.len(), 1);
    let mesh = &imported[0];
    assert_eq!(mesh.name, "StaticPanel");
    assert_eq!(
        mesh.positions,
        [[2.0, 3.0, 4.0], [3.0, 3.0, 4.0], [2.0, 4.0, 4.0]]
    );
    assert_eq!(mesh.indices, [0, 1, 2]);
    assert_eq!(mesh.uvs, [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    assert_eq!(mesh.material.diffuse(), Some("textures\\diffuse.png"));

    let bytes = export_nif(&imported).expect("export static NIF");
    let file = parse(&bytes).expect("exported NIF parses");
    let roundtrip = file.meshes().expect("exported NIF exposes meshes");
    assert_eq!(roundtrip.len(), 1);
    assert_eq!(roundtrip[0].positions, mesh.positions);
    assert_eq!(roundtrip[0].indices, mesh.indices);
    assert_eq!(roundtrip[0].uvs, mesh.uvs);
    assert_eq!(roundtrip[0].material.diffuse(), mesh.material.diffuse());
    let Block::LightingShader(shader) = &file.blocks[1] else {
        panic!("expected lighting shader block")
    };
    assert_eq!(shader.shader_type, ShaderType::Default);
    assert_eq!(shader.texture_set.get(), Some(0));

    fs::remove_dir_all(dir).expect("remove fixture directory");
}
