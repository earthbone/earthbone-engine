// SPDX-License-Identifier: GPL-3.0-or-later

//! xEdit wbAlternateTexture and wbRecord(TXST) define MODS selectors and slots.
//! CommonLib's TESModelTextureSwap retains both the geometry name and index.
//! Use the existing ESM cursor and winning form table at the model boundary.

use super::{textures, DataSourceError, GlobalFormId, LoadOrder, NifMesh, Tag};

pub(super) fn apply(
    order: &LoadOrder,
    base: GlobalFormId,
    model: &str,
    meshes: &mut [NifMesh],
) -> Result<(), DataSourceError> {
    let Some(owner) = order.winner_ref(base) else {
        return Ok(());
    };
    let record = order.record(owner);
    // Destruction models and armor ground models have separate swap fields.
    if !record.find(Tag::new(b"MODL")).is_some_and(|path| {
        soe_bsa::normalize(&soe_esm::cursor::zstring(path)) == soe_bsa::normalize(model)
    }) {
        return Ok(());
    }
    let Some(data) = record.find(Tag::new(b"MODS")) else {
        return Ok(());
    };
    let mut cursor = soe_esm::cursor::Cursor::new(data, 0);
    let count = cursor.u32()?;
    let mut swaps = Vec::new();
    for _ in 0..count {
        let length = cursor.u32()? as usize;
        let name = soe_esm::cursor::zstring(cursor.take(length)?);
        let texture = order.resolve(owner.file, cursor.u32()?);
        let index = cursor.i32()?;
        swaps.push((name, texture, index));
    }
    if !cursor.is_empty() {
        return Err(soe_esm::Error::SubrecordSize {
            record: record.tag,
            tag: Tag::new(b"MODS"),
            len: data.len(),
            expected: "the declared alternate textures without trailing bytes".into(),
        }
        .into());
    }
    for (name, texture, index) in swaps {
        let Some(mesh) = usize::try_from(index).ok().and_then(|i| meshes.get_mut(i)) else {
            continue;
        };
        if mesh.name != name {
            continue;
        }
        let Some(record) = order
            .winner(texture)
            .filter(|record| record.tag == Tag::new(b"TXST") && !record.is_deleted())
        else {
            continue;
        };
        let mut textures = vec![String::new(); 8];
        for (tx, slot) in [0, 1, 5, 2, 3, 4, 6, 7].into_iter().enumerate() {
            let tag = Tag::new(&[b'T', b'X', b'0', b'0' + tx as u8]);
            if let Some(path) = record.find(tag) {
                let path = soe_esm::cursor::zstring(path);
                if !path.is_empty() {
                    let path = textures::texture_path(&path);
                    textures[slot] = if path.starts_with("textures/") {
                        path
                    } else {
                        format!("textures/{path}")
                    };
                }
            }
        }
        mesh.material.textures = textures;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_source::{fixture, material_desc_from, mesh_data_from, placed_model};
    use crate::mesh::{build_surface, MeshOptions};
    use soe_esm::{tag, FileHeader, Plugin, Record};
    use soe_nif::Transform as NifTransform;

    fn mods(name: &str, index: i32) -> Vec<u8> {
        let mut bytes = 1_u32.to_le_bytes().to_vec();
        bytes.extend((name.len() as u32).to_le_bytes());
        bytes.extend(name.as_bytes());
        bytes.extend(0x01000900_u32.to_le_bytes());
        bytes.extend(index.to_le_bytes());
        bytes
    }

    fn order(data: &[u8]) -> LoadOrder {
        let mut base = Plugin::new(FileHeader::default());
        base.push_top(Record::new(tag::STAT, 0x800).zsub(b"MODL", "model.nif"));
        let mut swap = Plugin::new(FileHeader {
            masters: vec!["Base.esm".into()],
            ..FileHeader::default()
        });
        swap.push_top(
            Record::new(tag::STAT, 0x800)
                .zsub(b"MODL", "model.nif")
                .sub(b"MODS", data),
        );
        swap.push_top(Record::new(Tag::new(b"TXST"), 0x01000900).zsub(b"TX00", "old.dds"));
        let mut winner = Plugin::new(FileHeader {
            masters: vec!["Swap.esm".into()],
            ..FileHeader::default()
        });
        let mut texture = Record::new(Tag::new(b"TXST"), 0x900);
        for index in 0..8_u8 {
            texture = texture.zsub(
                [b'T', b'X', b'0', b'0' + index],
                &format!("slot{index}.dds"),
            );
        }
        winner.push_top(texture);
        let mut order = LoadOrder::new();
        order.add("Base.esm", base).unwrap();
        order.add("Swap.esm", swap).unwrap();
        order.add("Winner.esp", winner).unwrap();
        order
    }

    fn meshes() -> Vec<NifMesh> {
        let mut writer = fixture::NifWriter::new(100);
        let root = writer.reserve("NiNode");
        let textures = writer.texture_set(&["textures/original.dds"]);
        let textures = writer.add("BSShaderTextureSet", textures);
        let shader = writer.lighting_shader(0, 0, 0, textures, &[]);
        let shader = writer.add("BSLightingShaderProperty", shader);
        let (vertices, triangles) = fixture::quad();
        let mut children = Vec::new();
        for _ in 0..2 {
            let shape = writer.bs_tri_shape(
                "Target",
                [0.0; 3],
                -1,
                shader,
                -1,
                soe_nif::vertex::attr::VERTEX,
                &vertices,
                &triangles,
                true,
            );
            children.push(writer.add("BSTriShape", shape));
        }
        let node = writer.ni_node("Root", [0.0; 3], fixture::IDENTITY, 1.0, &children);
        writer.fill(root, node);
        writer.roots = vec![root];
        placed_model(&writer.finish()).unwrap().meshes().unwrap()
    }

    const BASE: GlobalFormId = GlobalFormId {
        file: 0,
        object: 0x800,
    };

    #[test]
    fn winning_base_namespace_txst_override_slots_and_material_identity() {
        let mut meshes = meshes();
        let original = meshes.clone();
        apply(&order(&mods("Target", 1)), BASE, "MODEL.NIF", &mut meshes).unwrap();
        assert_eq!(meshes[0], original[0]);
        assert_eq!(
            meshes[1].material.textures,
            [0, 1, 3, 4, 5, 2, 6, 7].map(|i| format!("textures/slot{i}.dds"))
        );
        let before = material_desc_from(&original[1]);
        let after = material_desc_from(&meshes[1]);
        assert_ne!(
            crate::mesh::MaterialKey::from(&before),
            crate::mesh::MaterialKey::from(&after)
        );
        let a = build_surface(
            &mesh_data_from(&original[1], &NifTransform::IDENTITY),
            MeshOptions::default(),
        )
        .unwrap();
        let b = build_surface(
            &mesh_data_from(&meshes[1], &NifTransform::IDENTITY),
            MeshOptions::default(),
        )
        .unwrap();
        assert_ne!(
            crate::batch::BatchKey::of(&a),
            crate::batch::BatchKey::of(&b)
        );
    }

    #[test]
    fn mismatched_selectors_and_replacement_models_keep_their_own_textures() {
        for (name, index, model) in [
            ("Wrong", 1, "model.nif"),
            ("Target", -1, "model.nif"),
            ("Target", 2, "model.nif"),
            ("Target", 1, "broken.nif"),
        ] {
            let mut meshes = meshes();
            let original = meshes.clone();
            apply(&order(&mods(name, index)), BASE, model, &mut meshes).unwrap();
            assert_eq!(meshes, original);
        }
    }

    #[test]
    fn malformed_mods_does_not_partly_change_materials() {
        let valid = mods("Target", 1);
        for length in 0..valid.len() {
            let mut meshes = meshes();
            let original = meshes.clone();
            assert!(apply(&order(&valid[..length]), BASE, "model.nif", &mut meshes).is_err());
            assert_eq!(meshes, original);
        }
        let mut trailing = valid;
        trailing.push(0);
        assert!(apply(&order(&trailing), BASE, "model.nif", &mut meshes()).is_err());
    }
}
