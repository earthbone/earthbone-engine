// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Real Git metadata excludes cache output, unlike empty sandbox markers.

use soe_swf_convert::{build_cache, Decompiler};

#[test]
fn ui_20_skyui_icon_sprites_parse() {
    let Some(root) = std::env::var_os("SOE_TEST_SKYUI") else { return; };
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_archive(soe_bsa::Archive::open(&std::path::PathBuf::from(root).join("SkyUI_SE.bsa")).unwrap());
    let bytes = vfs.read("interface/skyui/icons_item_psychosteve.swf").unwrap();
    let buffer = swf::decompress_swf(&bytes[..]).unwrap();
    let mut reader = swf::read::Reader::new(&buffer.data, buffer.header.version());
    while !reader.get_ref().is_empty() {
        let start = reader.get_ref();
        let (code, length) = reader.read_tag_code_and_length().unwrap();
        let payload = &reader.get_ref()[..length];
        if code == 39 {
            let mut sprite = swf::read::Reader::new(start, buffer.header.version());
            if let Err(error) = sprite.read_tag() {
                eprintln!("sprite id={} length={length} tail={:02x?}: {error}", u16::from_le_bytes([payload[0],payload[1]]), &payload[length.saturating_sub(24)..]);
            }
        }
        *reader.get_mut() = &reader.get_ref()[length..];
        if code == 0 { break; }
    }
    soe_swf_convert::convert(&bytes).unwrap();
}

#[test]
fn ui_20_jpeg_empty_tables_and_leading_marker_pair_preserve_the_image() {
    let mut image = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut image)
        .encode(&[64, 128, 192], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    for data in [image.clone(), [&[0xff, 0xd9][..], &image].concat()] {
        let header = swf::Header::default_with_swf_version(9);
        let tags = [
            swf::Tag::JpegTables(&[]),
            swf::Tag::DefineBits {
                id: 1,
                jpeg_data: &data,
            },
            swf::Tag::End,
        ];
        let mut bytes = Vec::new();
        swf::write_swf(&header, &tags, &mut bytes).unwrap();
        let movie = soe_swf_convert::convert(&bytes).unwrap();
        assert_eq!((movie.bitmaps[&1].width, movie.bitmaps[&1].height), (1, 1));
    }
}

#[test]
fn ui_20_empty_depth_updates_do_not_reject_a_timeline() {
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 1;
    let tags = [
        0x83, 0x06, 0x01, 15, 0, 0x85, 0x06, 0x03, 16, 0, 1, 0, 0x40, 0, 0, 0,
    ];
    let mut bytes = Vec::new();
    swf::write::write_swf_raw_tags(&header, &tags, &mut bytes).unwrap();
    let movie = soe_swf_convert::convert(&bytes).unwrap();
    assert_eq!(movie.timelines[&0].frames[0].display_list.len(), 0);
}

#[test]
fn ui_20_cache_rejects_a_git_worktree_before_writing_art() {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::write(temporary.path().join(".git"), "gitdir: /external/git\n").unwrap();
    let error = build_cache(
        &soe_bsa::Vfs::new(),
        &[],
        &temporary.path().join("cache"),
        &Decompiler::Program("unused-ffdec".into()),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid interface art: interface caches must be outside git checkouts"
    );
}

#[test]
fn ui_20_cache_accepts_an_empty_marker_and_reuses_the_finalized_hash() {
    let temporary = tempfile::Builder::new()
        .prefix("soe-ui20-cache-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::create_dir(temporary.path().join(".git")).unwrap();
    let root = temporary.path().join("cache");
    let vfs = soe_bsa::Vfs::new();
    let decompiler = Decompiler::Program("unused-ffdec".into());
    let first = build_cache(&vfs, &[], &root, &decompiler).unwrap();
    let second = build_cache(&vfs, &[], &root, &decompiler).unwrap();
    assert!(!first.reused);
    assert!(second.reused);
    assert_eq!(first.directory, second.directory);
}
