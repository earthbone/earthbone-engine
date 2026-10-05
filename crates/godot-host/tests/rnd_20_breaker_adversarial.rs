// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../nif/tests/common/mod.rs"]
mod nif_fixture;

use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};
use soe_godot_host::data_source::DataSource;
use std::path::Path;

fn visible_nif() -> Vec<u8> {
    use nif_fixture::NifWriter;
    use soe_nif::blocks::{flags1, flags2};
    use soe_nif::vertex::attr;

    let mut writer = NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let texture_set = writer.texture_set(&["textures/editor-marker.dds"]);
    let texture_set = writer.add("BSShaderTextureSet", texture_set);
    let shader = writer.lighting_shader(
        0,
        flags1::ZBUFFER_TEST,
        flags2::DOUBLE_SIDED,
        texture_set,
        &[],
    );
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (mut vertices, triangles) = nif_fixture::quad();
    for vertex in &mut vertices {
        for axis in &mut vertex.position {
            *axis *= 70.0;
        }
    }
    let visible = writer.bs_tri_shape(
        "Visible",
        [0.0; 3],
        -1,
        shader,
        -1,
        attr::VERTEX | attr::UVS | attr::NORMALS,
        &vertices,
        &triangles,
        true,
    );
    let visible = writer.add("BSTriShape", visible);
    let hidden = writer.bs_tri_shape(
        "HiddenEmitterHelper",
        [0.0; 3],
        -1,
        -1,
        -1,
        attr::VERTEX | attr::UVS | attr::NORMALS,
        &vertices,
        &triangles,
        true,
    );
    let hidden = writer.add("BSTriShape", hidden);
    let node = writer.ni_node(
        "Root",
        [0.0; 3],
        nif_fixture::IDENTITY,
        1.0,
        &[visible, hidden],
    );
    writer.fill(root, node);
    writer.roots = vec![root];
    writer.finish()
}

#[test]
fn rnd_20_runtime_stream_does_not_draw_a_record_editor_marker() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path();
    std::fs::create_dir_all(directory.join("meshes")).unwrap();
    std::fs::write(directory.join("meshes/marker.nif"), visible_nif()).unwrap();

    let mut plugin = Plugin::new(FileHeader::default());
    let mut statics = Group::new(GroupKind::Top(tag::STAT));
    let visible = plugin.push_record(Record::new(tag::STAT, 0x801).zsub(b"MODL", "marker.nif"));
    statics.entries.push(Entry::Record(visible));
    let marker = plugin.push_record(
        Record::new(tag::STAT, 0x800)
            .with_flags(1 << 23)
            .zsub(b"MODL", "marker.nif"),
    );
    statics.entries.push(Entry::Record(marker));
    let mut cells = Group::new(GroupKind::Top(tag::CELL));
    let cell = plugin.push_record(
        Record::new(tag::CELL, 0x810)
            .zsub(b"EDID", "Rnd20MarkerCell")
            .sub(b"DATA", [1, 0]),
    );
    cells.entries.push(Entry::Record(cell));
    let mut children = Group::new(GroupKind::CellChildren(0x810));
    let mut references = Group::new(GroupKind::CellTemporary(0x810));
    references.entries.push(Entry::Record(
        plugin.push_record(
            Record::new(tag::REFR, 0x900)
                .sub(b"NAME", 0x800_u32.to_le_bytes())
                .sub(b"DATA", [0; 24]),
        ),
    ));
    references.entries.push(Entry::Record(
        plugin.push_record(
            Record::new(tag::REFR, 0x901)
                .sub(b"NAME", 0x801_u32.to_le_bytes())
                .sub(b"DATA", [0; 24]),
        ),
    ));
    children.entries.push(Entry::Group(references));
    cells.entries.push(Entry::Group(children));
    plugin.groups.extend([statics, cells]);
    std::fs::write(directory.join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            directory.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }

    let source = DataSource::open(Path::new(directory)).unwrap();
    let cell = source.stream_cell("Rnd20MarkerCell").unwrap();
    assert_eq!(
        cell.surfaces.len(),
        1,
        "only ordinary geometry should render on cell load"
    );
    let enabled = source
        .stream_reference(soe_esm::GlobalFormId {
            file: 0,
            object: 0x900,
        })
        .unwrap();
    assert!(
        enabled.surfaces.is_empty(),
        "runtime stream must keep editor marker hidden"
    );
}

#[test]
fn rnd_20_installed_gate_surfaces_have_albedo_or_documented_glow_exception() {
    let data = std::env::var("SKYRIM_DATA").expect("SKYRIM_DATA must point at installed game data");
    let source = DataSource::open_with_threads(Path::new(&data), 2).unwrap();
    let order = source.shared_order();
    let mut checked = 0;
    let mut intended = 0;
    for origin in ["WhiterunOrigin", "WhiterunExterior04"] {
        let exterior = source.stream_exterior(origin, 1).unwrap();
        assert_eq!(exterior.cells.len(), 9);
        for (_, cell) in exterior.cells {
            assert_eq!(cell.stats.meshes_failed, 0);
            for surface in cell.surfaces {
                checked += 1;
                if surface.arrays.material.water.is_some() || surface.textures.diffuse.is_some() {
                    continue;
                }
                let owner = order.winner_ref(surface.reference).unwrap();
                let refr = soe_esm::records::refr::Refr::from_record(order.record(owner)).unwrap();
                let base = order.resolve(owner.file, refr.base);
                assert_eq!(
                    base,
                    soe_esm::GlobalFormId {
                        file: 0,
                        object: 0x03DE33
                    }
                );
                assert_eq!(
                    surface.reference,
                    soe_esm::GlobalFormId {
                        file: 0,
                        object: 0x03DE34
                    }
                );
                let record = order.winner(base).unwrap();
                assert_eq!(record.tag, tag::STAT);
                assert_eq!(record.editor_id().as_deref(), Some("WRHallOftheDead01"));
                intended += 1;
            }
        }
    }
    assert!(checked > 1500, "installed gate survey was too small");
    assert_eq!(intended, 1, "only the documented glow may lack albedo");
}
