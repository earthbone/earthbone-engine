// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Buf, NifWriter};
use soe_nif::blocks::Block;
use soe_nif::types::BlockRef;
use soe_nif::parse;
use std::path::PathBuf;

#[test]
fn fmt_11_regular_bone_count_retains_each_reference() {
    let mut writer = NifWriter::new(100);
    let mut data = Buf::default();
    data.i32(-1).i32(-1).i32(-1).u32(2).i32(7).i32(11);
    writer.add("NiSkinInstance", data.0);

    let file = parse(&writer.finish()).expect("two bone references parse");
    let Block::SkinInstance(instance) = &file.blocks[0] else { panic!("skin instance block") };
    assert_eq!(instance.bones, [BlockRef(7), BlockRef(11)]);
}

#[test]
fn fmt_11_non_sentinel_count_without_payload_is_rejected() {
    let mut writer = NifWriter::new(100);
    let mut data = Buf::default();
    data.i32(-1).i32(-1).i32(-1).u32(1);
    writer.add("NiSkinInstance", data.0);

    assert!(parse(&writer.finish()).is_err());
}

#[test]
fn fmt_11_all_installed_skeleton_meshes_parse_and_walk() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed skeleton sweep");
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
        "expected at least 52 skeleton meshes, found {skeletons}"
    );
}
