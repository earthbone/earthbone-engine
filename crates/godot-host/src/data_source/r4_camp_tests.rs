// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn r4_lighting_masks_keep_enabled_channels_and_distinct_materials() {
    use soe_nif::blocks::{flags1, flags2};
    let mut writer = fixture::NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let texture = writer.texture_set(&["textures/paint.dds"]);
    let texture = writer.add("BSShaderTextureSet", texture);
    let shader = writer.lighting_shader(0, 0, 0, texture, &[]);
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (vertices, triangles) = fixture::quad();
    let shape = writer.bs_tri_shape(
        "Paint",
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
    let node = writer.ni_node("Root", [0.0; 3], fixture::IDENTITY, 1.0, &[shape]);
    writer.fill(root, node);
    writer.roots = vec![root];
    let mut mesh = placed_model(&writer.finish())
        .unwrap()
        .meshes()
        .unwrap()
        .remove(0);
    mesh.colors = vec![[0.2, 0.4, 0.6, 0.125]; mesh.positions.len()];
    let plain = crate::mesh::MaterialKey::from(&material_desc_from(&mesh));
    for (rgb, alpha, expected) in [
        (true, false, [0.2, 0.4, 0.6, 1.0]),
        (false, true, [1.0, 1.0, 1.0, 0.125]),
        (true, true, [0.2, 0.4, 0.6, 0.125]),
    ] {
        mesh.material.vertex_colors = rgb;
        mesh.material.vertex_alpha = alpha;
        mesh.material.flags1 = if alpha { flags1::VERTEX_ALPHA } else { 0 };
        mesh.material.flags2 = if rgb { flags2::VERTEX_COLORS } else { 0 };
        let data = mesh_data_from(&mesh, &NifTransform::IDENTITY);
        let arrays = build_surface(&data, MeshOptions::default()).unwrap();
        assert_eq!(arrays.colors, vec![expected; mesh.positions.len()]);
        assert_ne!(crate::mesh::MaterialKey::from(&arrays.material), plain);
    }
    mesh.colors.clear();
    let data = mesh_data_from(&mesh, &NifTransform::IDENTITY);
    assert_eq!(data.colors.unwrap(), vec![[1.0; 4]; mesh.positions.len()]);
    mesh.material.flags2 |= flags2::TREE_ANIM;
    assert!(mesh_data_from(&mesh, &NifTransform::IDENTITY)
        .colors
        .is_none());
    assert!(!material_desc_from(&mesh).uses_vertex_colors());
    mesh.material.vertex_colors = false;
    mesh.material.vertex_alpha = false;
    mesh.material.flags1 = 0;
    mesh.material.flags2 = 0;
    assert!(mesh_data_from(&mesh, &NifTransform::IDENTITY)
        .colors
        .is_none());
}

#[test]
fn r4_installed_camp_obelisk_vertex_masks() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let reference = GlobalFormId {
        file: 0,
        object: 0x0C8E8B,
    };
    let model = "meshes/clutter/giant/giantobeliskblue02.nif";
    let file = placed_model(&source.vfs.read(model).unwrap()).unwrap();
    let meshes = file.meshes().unwrap();
    let loaded = source.stream_reference(reference).unwrap();
    assert_eq!(loaded.surfaces.len(), meshes.len());
    for (mesh, surface) in meshes.iter().zip(&loaded.surfaces) {
        assert_eq!(mesh.colors.len(), mesh.positions.len());
        let expected: Vec<_> = mesh
            .colors
            .iter()
            .map(|color| {
                [
                    if mesh.material.vertex_colors {
                        color[0]
                    } else {
                        1.0
                    },
                    if mesh.material.vertex_colors {
                        color[1]
                    } else {
                        1.0
                    },
                    if mesh.material.vertex_colors {
                        color[2]
                    } else {
                        1.0
                    },
                    if mesh.material.vertex_alpha {
                        color[3]
                    } else {
                        1.0
                    },
                ]
            })
            .collect();
        eprintln!(
            "R4 PAINT shape={} source={} uploaded={} alpha={:?}",
            mesh.name,
            mesh.colors.len(),
            surface.arrays.colors.len(),
            expected
                .iter()
                .map(|color| color[3])
                .fold((1.0_f32, 0.0_f32), |(low, high), alpha| (
                    low.min(alpha),
                    high.max(alpha)
                ))
        );
        assert_eq!(
            surface.arrays.colors, expected,
            "{} keeps authored masks",
            mesh.name
        );
    }
}

#[test]
fn r4_installed_camp_obelisk_alternate_texture() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let reference = GlobalFormId {
        file: 0,
        object: 0x0C8E8B,
    };
    let owner = source.order.winner_ref(reference).unwrap();
    let refr = Refr::from_record(source.order.record(owner)).unwrap();
    let base = source
        .order
        .winner_ref(source.order.resolve(owner.file, refr.base))
        .unwrap();
    let record = source.order.record(base);
    let model = model_for(record).unwrap();
    let file = placed_model(&source.vfs.read(&format!("meshes/{model}")).unwrap()).unwrap();
    let meshes = file.meshes().unwrap();
    eprintln!(
        "R4 OBELISK base={:?} model={model} mods={:?}",
        record.editor_id(),
        record.find(Tag::new(b"MODS"))
    );
    for (index, mesh) in meshes.iter().enumerate() {
        eprintln!(
            "R4 OBELISK index={index} block={} name={} material={:?}",
            mesh.block, mesh.name, mesh.material
        );
    }
    let txst = source
        .order
        .winner(GlobalFormId {
            file: 0,
            object: 0x0A9293,
        })
        .unwrap();
    eprintln!("R4 OBELISK txst={:?}", txst.subrecords);
    let expected = soe_esm::cursor::zstring(txst.find(Tag::new(b"TX00")).unwrap());
    let loaded = source.stream_reference(reference).unwrap();
    for (index, surface) in loaded.surfaces.iter().enumerate() {
        eprintln!(
            "R4 OBELISK uploaded={index} diffuse={:?} normal={:?}",
            surface.arrays.material.diffuse, surface.arrays.material.normal
        );
    }
    let index = meshes
        .iter()
        .position(|mesh| mesh.name == "GiantObeliskBlue02:2")
        .unwrap();
    let actual = loaded.surfaces[index]
        .arrays
        .material
        .diffuse
        .as_deref()
        .unwrap();
    assert_eq!(
        soe_bsa::normalize(actual),
        format!("textures/{}", soe_bsa::normalize(&expected))
    );
    let normal = soe_esm::cursor::zstring(txst.find(Tag::new(b"TX01")).unwrap());
    assert_eq!(
        loaded.surfaces[index].arrays.material.normal.as_deref(),
        Some(format!("textures/{}", soe_bsa::normalize(&normal)).as_str())
    );
    assert!(loaded.surfaces[index].textures.diffuse.is_some());
    assert!(loaded.surfaces[index].textures.normal.is_some());
    assert!(!Arc::ptr_eq(
        loaded.surfaces[index].textures.diffuse.as_ref().unwrap(),
        &checker_albedo()
    ));
    for untouched in [0, 1] {
        assert_eq!(
            loaded.surfaces[untouched]
                .arrays
                .material
                .diffuse
                .as_deref(),
            meshes[untouched].material.diffuse()
        );
    }
}

#[test]
fn installed_camp_obelisk_walkthrough() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("installed renderer requires Godot");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("target/r4-obelisk-regression");
    std::fs::create_dir_all(&directory).unwrap();
    let output = std::process::Command::new(binary)
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "r4_camp_obelisk_probe.gd",
        ])
        .env("DISPLAY", ":99")
        .env("SKYRIM_DATA", data)
        .env("SOE_GODOT_TIMEOUT", "900")
        .env("SOE_CHARACTER_OUTPUT", &directory)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(directory.join("test.log"), &log).unwrap();
    assert!(
        output.status.success()
            && log.contains("RESULT: PASS R4 camp obelisk")
            && !log.contains("SCRIPT ERROR"),
        "{log}"
    );
}
