// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Real Git metadata excludes cache output, unlike empty sandbox markers.

use soe_swf_convert::{build_cache, Decompiler};

#[test]
fn ui_20_converted_font_style_flags_survive_godot_loading() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return Ok(());
    };
    let inputs = tempfile::tempdir_in("/tmp")?;
    let output = tempfile::tempdir_in("/tmp")?;
    std::fs::create_dir(inputs.path().join("interface"))?;
    let mut tags = Vec::new();
    for style in 0..4_u16 {
        let mut flags = swf::FontFlag::HAS_WIDE_CODES;
        if style & 1 != 0 {
            flags |= swf::FontFlag::IS_BOLD;
        }
        if style & 2 != 0 {
            flags |= swf::FontFlag::IS_ITALIC;
        }
        tags.push(swf::Tag::DefineFont2(Box::new(swf::Font {
            version: 3,
            id: style + 1,
            name: swf::SwfStr::from_utf8_str("Style fixture"),
            language: swf::Language::Unknown,
            layout: None,
            glyphs: vec![],
            flags,
        })));
    }
    tags.extend([swf::Tag::ShowFrame, swf::Tag::End]);
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 1;
    let mut bytes = Vec::new();
    swf::write_swf(&header, &tags, &mut bytes)?;
    std::fs::write(inputs.path().join("interface/fonts.swf"), bytes)?;
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_loose_root(inputs.path())?;
    let cache = build_cache(
        &vfs,
        &[],
        output.path(),
        &Decompiler::Program("unused-ffdec".into()),
    )?;
    let resources = cache.movies["interface/fonts.swf"]
        .parent()
        .ok_or("movie directory")?;
    let project = tempfile::tempdir_in("/tmp")?;
    std::fs::write(project.path().join("project.godot"), "config_version=5\n")?;
    std::fs::write(
        project.path().join("font_test.gd"),
        r#"extends SceneTree
func _initialize():
    var directory = OS.get_environment("SOE_UI_20_FONT_RESOURCE_DIR")
    for style in range(4):
        var font = load(directory.path_join("font-%d.tres" % (style + 1)))
        if not font is FontFile or font.get_font_style() != style:
            printerr("font style failed: ", style)
            quit(1)
            return
    print("UI-20 FONT STYLE PASS")
    quit(0)
"#,
    )?;
    let result = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "font_test.gd"])
        .env("SOE_UI_20_FONT_RESOURCE_DIR", resources)
        .output()?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success()
            && stdout.contains("UI-20 FONT STYLE PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
    Ok(())
}

#[test]
fn ui_20_font_config_changes_rebuild_the_external_cache() -> Result<(), Box<dyn std::error::Error>>
{
    let inputs = tempfile::tempdir_in("/tmp")?;
    let output = tempfile::tempdir_in("/tmp")?;
    std::fs::create_dir(inputs.path().join("interface"))?;
    let path = inputs.path().join("interface/fontconfig.txt");
    std::fs::write(&path, b"fontlib \"first.swf\"\n")?;
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_loose_root(inputs.path())?;
    let tool = Decompiler::Program("unused-ffdec".into());
    let first = build_cache(&vfs, &[], output.path(), &tool)?;
    assert_eq!(
        std::fs::read(first.directory.join("fontconfig.txt"))?,
        b"fontlib \"first.swf\"\n"
    );
    assert!(build_cache(&vfs, &[], output.path(), &tool)?.reused);
    std::fs::write(&path, b"fontlib \"second.swf\"\n")?;
    let second = build_cache(&vfs, &[], output.path(), &tool)?;
    assert_ne!(first.directory, second.directory);
    assert!(!second.reused);
    assert_eq!(
        std::fs::read(second.directory.join("fontconfig.txt"))?,
        b"fontlib \"second.swf\"\n"
    );
    Ok(())
}

#[test]
fn ui_20_skyui_icon_sprites_parse() {
    let Some(root) = std::env::var_os("SOE_TEST_SKYUI") else {
        return;
    };
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_archive(
        soe_bsa::Archive::open(&std::path::PathBuf::from(root).join("SkyUI_SE.bsa")).unwrap(),
    );
    let bytes = vfs
        .read("interface/skyui/icons_item_psychosteve.swf")
        .unwrap();
    let buffer = swf::decompress_swf(&bytes[..]).unwrap();
    let mut reader = swf::read::Reader::new(&buffer.data, buffer.header.version());
    while !reader.get_ref().is_empty() {
        let start = reader.get_ref();
        let (code, length) = reader.read_tag_code_and_length().unwrap();
        let payload = &reader.get_ref()[..length];
        if code == 39 {
            let mut sprite = swf::read::Reader::new(start, buffer.header.version());
            if let Err(error) = sprite.read_tag() {
                eprintln!(
                    "sprite id={} length={length} tail={:02x?}: {error}",
                    u16::from_le_bytes([payload[0], payload[1]]),
                    &payload[length.saturating_sub(24)..]
                );
            }
        }
        *reader.get_mut() = &reader.get_ref()[length..];
        if code == 0 {
            break;
        }
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
