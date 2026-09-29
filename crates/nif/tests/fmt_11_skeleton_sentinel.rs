// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Buf, NifWriter};
use soe_nif::blocks::{Block, SkinInstance};
use soe_nif::parse;
use std::path::PathBuf;

#[test]
fn fmt_11_skin_instances_treat_u32_max_as_empty_bones() {
    for (type_name, dismember) in [("NiSkinInstance", false), ("BSDismemberSkinInstance", true)] {
        let mut w = NifWriter::new(100);
        let mut bytes = Buf::default();
        bytes.i32(-1).i32(-1).i32(-1).u32(u32::MAX);
        if dismember {
            bytes.u32(1).u16(7).u16(9);
        }
        w.add(type_name, bytes.0);

        let file = parse(&w.finish()).expect("sentinel bone count parses");
        let Block::SkinInstance(SkinInstance {
            bones, body_parts, ..
        }) = &file.blocks[0]
        else {
            panic!("{type_name} is parsed as a skin instance");
        };
        assert!(bones.is_empty(), "{type_name} sentinel means no bones");
        assert_eq!(body_parts.len(), usize::from(dismember));
        if dismember {
            assert_eq!(body_parts[0].flags, 7);
            assert_eq!(body_parts[0].part, 9);
        }
    }
}

#[test]
fn fmt_11_all_vanilla_skeleton_meshes_parse() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping vanilla skeleton sweep");
        return;
    };

    let mut skeletons = 0usize;
    for archive_name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&data.join(archive_name)).expect("archive opens");
        let names: Vec<String> = archive
            .files()
            .iter()
            .filter(|name| {
                let lower = name.to_ascii_lowercase();
                lower.contains("skeleton")
                    && std::path::Path::new(name.as_str())
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("nif"))
            })
            .cloned()
            .collect();
        for name in names {
            let bytes = archive.read(&name).expect("skeleton mesh reads");
            let file = parse(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
            file.meshes()
                .unwrap_or_else(|error| panic!("{name}: mesh walk: {error}"));
            skeletons += 1;
        }
    }

    assert!(
        skeletons >= 52,
        "expected to cover the 52 previously refused skeleton meshes, found {skeletons}"
    );
}
