// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
