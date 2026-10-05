// SPDX-License-Identifier: GPL-3.0-or-later

//! RND-17 authored cell selection uses UESP REFR XESP and Initially Disabled.

use super::fixture::{quad, NifWriter, IDENTITY};
use super::*;
use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, Plugin, Record};

#[test]
fn rnd_17_streaming_omits_initially_disabled_and_parent_disabled_models() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("meshes")).unwrap();
    let mut writer = NifWriter::new(100);
    let textures = writer.texture_set(&[]);
    let textures = writer.add("BSShaderTextureSet", textures);
    let shader = writer.lighting_shader(0, 0, 0, textures, &[]);
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (vertices, triangles) = quad();
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
    let root = writer.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    let root = writer.add("NiNode", root);
    writer.roots = vec![root];
    std::fs::write(directory.path().join("meshes/quad.nif"), writer.finish()).unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag::STAT, 0x800).zsub(b"MODL", "quad.nif"));
    plugin.push_top(Record::new(Tag::new(b"ACTI"), 0x801).zsub(b"MODL", "quad.nif"));
    let cell = plugin.push_record(
        Record::new(tag::CELL, 0x810)
            .zsub(b"EDID", "Rnd17Cell")
            .sub(b"DATA", [1, 0]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(0x810));
    for (id, disabled, parent) in [
        (0x900, true, None),
        (0x901, false, Some((0x900_u32, 0_u32))),
        (0x902, true, Some((0x901, 1))),
        (0x903, false, Some((0x902, 1))),
        (0x904, false, None),
        (0x905, false, Some((0x900, 0xFFFF_FFFE))),
    ] {
        let mut reference = Record::new(tag::REFR, id)
            .sub(
                b"NAME",
                if id == 0x904 { 0x801_u32 } else { 0x800_u32 }.to_le_bytes(),
            )
            .sub(b"DATA", [0; 24]);
        if disabled {
            reference.flags = flags::INITIALLY_DISABLED;
        }
        if let Some((parent, flag)) = parent {
            reference = reference.sub(b"XESP", [parent.to_le_bytes(), flag.to_le_bytes()].concat());
        }
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(reference)));
    }
    let mut children = Group::new(GroupKind::CellChildren(0x810));
    children.entries.push(Entry::Group(temporary));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries
        .extend([Entry::Record(cell), Entry::Group(children)]);
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(tag::CELL));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);
    std::fs::write(directory.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in &BASE_MASTERS[1..] {
        std::fs::write(
            directory.path().join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let source = DataSource::open(directory.path()).unwrap();
    let loaded = source.stream_cell("Rnd17Cell").unwrap();
    let references: std::collections::BTreeSet<_> = loaded
        .surfaces
        .iter()
        .map(|surface| surface.reference.object)
        .collect();
    assert_eq!(references, [0x902, 0x904].into_iter().collect());
    assert_eq!(loaded.stats.meshes_parsed, 2);
    assert_eq!(loaded.stats.activators_with_model, 1);
    assert_eq!(loaded.stats.containers_with_model, 0);
    assert_eq!(loaded.stats.references, 6);
    let collision = source
        .collide_cell_id(source.resolve_cell("Rnd17Cell").unwrap())
        .unwrap();
    assert_eq!(collision.stats.references, 2);
    assert!(source
        .stream_reference(GlobalFormId {
            file: 0,
            object: 0x999
        })
        .is_err());
    assert!(source
        .placed_reference_colliders(GlobalFormId {
            file: 0,
            object: 0x999
        })
        .is_err());
}
