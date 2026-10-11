// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! UI-19 checks paged fonts, bounded SkyUI sprites and actual FFDec exports.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use soe_bsa::{Archive, Vfs};
use soe_swf_convert::{build_cache, convert, Decompiler};
use swf::extensions::ReadSwfExt;
use swf::Tag;

const FFDEC: &str = "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/SkyUI-Community/tools/FFDec/ffdec-cli.jar";

fn font_movie(count: u16, edge: i32) -> Vec<u8> {
    let mut records = vec![swf::ShapeRecord::StyleChange(Box::new(
        swf::StyleChangeData {
            move_to: Some(swf::Point::new(swf::Twips::ZERO, swf::Twips::ZERO)),
            fill_style_0: None,
            fill_style_1: Some(1),
            line_style: None,
            new_styles: None,
        },
    ))];
    for (dx, dy) in [(edge, 0), (0, edge), (-edge, 0), (0, -edge)] {
        records.push(swf::ShapeRecord::StraightEdge {
            delta: swf::PointDelta {
                dx: swf::Twips::new(dx),
                dy: swf::Twips::new(dy),
            },
        });
    }
    let font = swf::Font {
        version: 3,
        id: 1,
        name: swf::SwfStr::from_utf8_str("UI-19 fixture"),
        language: swf::Language::Unknown,
        layout: Some(swf::FontLayout {
            ascent: 16384,
            descent: 4096,
            leading: 0,
            kerning: vec![],
        }),
        glyphs: (0..count)
            .map(|code| swf::Glyph {
                shape_records: records.clone(),
                code: code + 32,
                advance: 20480,
                bounds: Some(swf::Rectangle {
                    x_min: swf::Twips::ZERO,
                    y_min: swf::Twips::ZERO,
                    x_max: swf::Twips::new(edge),
                    y_max: swf::Twips::new(edge),
                }),
            })
            .collect(),
        // CONTEXT: Ruffle's writer chooses offset widths but retains the caller's flags.
        flags: swf::FontFlag::HAS_WIDE_CODES
            | swf::FontFlag::HAS_LAYOUT
            | if count > 4096 {
                swf::FontFlag::HAS_WIDE_OFFSETS
            } else {
                swf::FontFlag::empty()
            },
    };
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 1;
    let mut bytes = Vec::new();
    swf::write_swf(
        &header,
        &[Tag::DefineFont2(Box::new(font)), Tag::ShowFrame, Tag::End],
        &mut bytes,
    )
    .unwrap();
    bytes
}

fn fixture_vfs(directory: &Path, bytes: &[u8]) -> Vfs {
    std::fs::create_dir(directory.join("interface")).unwrap();
    std::fs::write(directory.join("interface/fixture.swf"), bytes).unwrap();
    let mut vfs = Vfs::new();
    vfs.add_loose_root(directory).unwrap();
    vfs
}

#[test]
fn ui_19_font_pages_preserve_glyphs_pixels_and_resource_indices() {
    let bytes = font_movie(800, 20480);
    let movie = convert(&bytes).unwrap();
    let font = &movie.fonts[&1];
    assert!(
        font.atlases.len() > 1,
        "fixture must exercise page rollover"
    );
    assert_eq!(font.glyphs.len(), 800);
    for glyph in &font.glyphs {
        let atlas = &font.atlases[glyph.texture_index];
        let [x, y, width, height] = glyph.atlas_rect;
        assert!(x + width < atlas.width && y + height < atlas.height);
        let center = ((y + height / 2) * atlas.width + x + width / 2) as usize * 4;
        assert_eq!(&atlas.rgba[center..center + 4], &[255; 4]);
        assert!((glyph.advance - 64.0).abs() < 0.00001);
    }
    let input = tempfile::tempdir_in("/tmp").unwrap();
    let vfs = fixture_vfs(input.path(), &bytes);
    let output = tempfile::tempdir_in("/tmp").unwrap();
    let cache = build_cache(
        &vfs,
        &[],
        output.path(),
        &Decompiler::Program("unused".into()),
    )
    .unwrap();
    let directory = cache.movies["interface/fixture.swf"].parent().unwrap();
    let resource = std::fs::read_to_string(directory.join("font-1.tres")).unwrap();
    for glyph in &font.glyphs {
        assert!(resource.contains(&format!(
            "glyphs/{}/texture_idx = {}",
            glyph.code, glyph.texture_index
        )));
    }
    assert!(resource.contains("textures/1/image = SubResource(\"atlas_1\")"));
}

#[test]
fn ui_19_excessive_font_page_allocation_returns_a_typed_error() {
    let error = convert(&font_movie(5000, 40960)).unwrap_err();
    assert!(
        matches!(
            &error,
            soe_swf_convert::Error::Invalid(message) if message == "font atlases exceed 256 MiB"
        ),
        "{error}"
    );
}

#[test]
fn ui_19_truncated_sprite_payload_is_rejected_before_conversion() {
    let header = swf::Header::default_with_swf_version(9);
    let mut bytes = Vec::new();
    swf::write::write_swf_raw_tags(&header, &[0xce, 0x09, 1, 0, 1, 0], &mut bytes).unwrap();
    assert!(convert(&bytes)
        .unwrap_err()
        .to_string()
        .contains("truncated tag payload"));
}

#[test]
fn ui_19_bounded_sprite_without_end_or_showframe_retains_its_display_list() {
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 1;
    let mut bytes = Vec::new();
    swf::write::write_swf_raw_tags(
        &header,
        &[
            0xcc, 0x09, 0xe1, 0, 1, 0, 0x86, 0x06, 6, 1, 0, 0xe0, 0, 0, 0x40, 0, 0, 0,
        ],
        &mut bytes,
    )
    .unwrap();
    let movie = convert(&bytes).unwrap();
    let sprite = &movie.timelines[&225];
    assert_eq!(sprite.frames.len(), 1);
    assert_eq!(sprite.frames[0].display_list[0].character, 224);
    assert_eq!(sprite.frames[0].display_list[0].depth, 1);
}

#[test]
fn ui_19_installed_chinese_fonts_keep_every_source_glyph_in_bounded_pages() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let archive = Archive::open(&PathBuf::from(data).join("Skyrim - Interface.bsa")).unwrap();
    let bytes = archive.read("interface/fonts_cn.swf").unwrap();
    let buffer = swf::decompress_swf(&bytes[..]).unwrap();
    let source = swf::parse_swf(&buffer).unwrap();
    let movie = convert(&bytes).unwrap();
    let mut checked = 0;
    for tag in &source.tags {
        let Tag::DefineFont2(source) = tag else {
            continue;
        };
        let font = &movie.fonts[&source.id];
        assert_eq!(font.glyphs.len(), source.glyphs.len());
        assert_eq!(
            font.glyphs.iter().map(|g| g.code).collect::<Vec<_>>(),
            source.glyphs.iter().map(|g| g.code).collect::<Vec<_>>()
        );
        for glyph in &font.glyphs {
            let atlas = &font.atlases[glyph.texture_index];
            let [x, y, w, h] = glyph.atlas_rect;
            assert!(x + w < atlas.width && y + h < atlas.height);
        }
        assert!(font
            .atlases
            .iter()
            .all(|page| page.width <= 1024 && page.height <= 2048));
        eprintln!(
            "UI-19 Chinese font {}: {} glyphs, {} pages",
            source.id,
            font.glyphs.len(),
            font.atlases.len()
        );
        checked += 1;
    }
    assert_eq!(checked, movie.fonts.len());
    assert!(movie.fonts.values().any(|font| font.atlases.len() > 1));
}

#[test]
fn ui_19_skyui_psychosteve_icons_keep_names_and_bounded_sprite_placements() {
    let Some(root) = std::env::var_os("SOE_TEST_SKYUI") else {
        return;
    };
    let archive = Archive::open(&PathBuf::from(root).join("SkyUI_SE.bsa")).unwrap();
    let bytes = archive
        .read("interface/skyui/icons_item_psychosteve.swf")
        .unwrap();
    let buffer = swf::decompress_swf(&bytes[..]).unwrap();
    let movie = convert(&bytes).unwrap();
    let mut reader = swf::read::Reader::new(&buffer.data, buffer.header.version());
    let mut exports = BTreeMap::new();
    let mut labels = vec![];
    let mut sprites = 0;
    while !reader.get_ref().is_empty() {
        let start = reader.get_ref();
        let (code, length) = reader.read_tag_code_and_length().unwrap();
        let payload = &reader.get_ref()[..length];
        if code == 39 {
            let mut sprite = swf::read::Reader::new(payload, buffer.header.version());
            let id = sprite.read_u16().unwrap();
            let count = sprite.read_u16().unwrap();
            let actual = &movie.timelines[&id];
            assert_eq!(actual.frames.len(), count as usize);
            while !sprite.get_ref().is_empty() {
                let tag = sprite.read_tag().unwrap();
                if let Tag::PlaceObject(place) = tag {
                    if let swf::PlaceObjectAction::Place(character) = place.action {
                        assert!(actual
                            .frames
                            .iter()
                            .flat_map(|f| &f.display_list)
                            .any(|item| item.character == character && item.depth == place.depth));
                    }
                }
            }
            sprites += 1;
        } else if code == 56 {
            if let Tag::ExportAssets(entries) =
                swf::read::Reader::new(start, buffer.header.version())
                    .read_tag()
                    .unwrap()
            {
                exports.extend(entries.iter().map(|e| {
                    (
                        e.name.to_string_lossy(swf::SwfStr::encoding_for_version(
                            buffer.header.version(),
                        )),
                        e.id,
                    )
                }));
            }
        } else if code == 43 {
            if let Tag::FrameLabel(label) = swf::read::Reader::new(start, buffer.header.version())
                .read_tag()
                .unwrap()
            {
                labels.push(
                    label
                        .label
                        .to_string_lossy(swf::SwfStr::encoding_for_version(
                            buffer.header.version(),
                        )),
                );
            }
        }
        *reader.get_mut() = &reader.get_ref()[length..];
        if code == 0 {
            break;
        }
    }
    assert!(sprites > 0 && !labels.is_empty());
    assert_eq!(movie.symbols, exports);
    assert_eq!(
        movie.timelines[&0]
            .frames
            .iter()
            .flat_map(|frame| &frame.labels)
            .cloned()
            .collect::<Vec<_>>(),
        labels
    );
}

fn scripts(directory: &Path) -> Vec<String> {
    let mut result = vec![];
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(scripts(&path));
        } else if path.extension().is_some_and(|extension| extension == "as") {
            result.push(std::fs::read_to_string(path).unwrap());
        }
    }
    result
}

#[test]
fn ui_19_installed_all_winning_vanilla_and_skyui_interface_art_converts() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(skyui) = std::env::var_os("SOE_TEST_SKYUI") else {
        return;
    };
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&PathBuf::from(data).join("Skyrim - Interface.bsa"))
        .unwrap();
    vfs.add_archive_path(&PathBuf::from(skyui).join("SkyUI_SE.bsa"))
        .unwrap();
    let mut checked = 0;
    for path in vfs.list("interface") {
        if Path::new(path)
            .extension()
            .is_none_or(|extension| extension != "swf")
        {
            continue;
        }
        let bytes = vfs.read(path).unwrap();
        convert(&bytes).unwrap_or_else(|error| panic!("{path}: {error}"));
        checked += 1;
    }
    eprintln!("UI-19 converted {checked} winning vanilla and SkyUI movies");
    assert!(checked > 3);
}

#[test]
fn ui_19_installed_full_load_order_cache_publishes_all_movies_with_real_ffdec() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(skyui) = std::env::var_os("SOE_TEST_SKYUI") else {
        return;
    };
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&PathBuf::from(data).join("Skyrim - Interface.bsa"))
        .unwrap();
    vfs.add_archive_path(&PathBuf::from(skyui).join("SkyUI_SE.bsa"))
        .unwrap();
    let paths = vfs
        .list("interface")
        .into_iter()
        .filter(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "swf")
        })
        .collect::<Vec<_>>();
    let root = tempfile::tempdir_in("/tmp").unwrap();
    let decompiler = std::env::var_os("SOE_FFDEC_JAR").map_or_else(
        || Decompiler::Jar(FFDEC.into()),
        |jar| Decompiler::Jar(jar.into()),
    );
    let cache = build_cache(
        &vfs,
        &["Skyrim.esm".into(), "SkyUI_SE.esp".into()],
        root.path(),
        &decompiler,
    )
    .unwrap();
    assert_eq!(cache.movies.len(), paths.len());
    for path in paths {
        assert!(
            cache.movies[path].is_file(),
            "{path}: missing native resource"
        );
    }
    eprintln!(
        "UI-19 published {} native movies with real FFDec",
        cache.movies.len()
    );
}

#[test]
fn ui_19_real_ffdec_exports_vanilla_and_skyui_actions_next_to_native_cache() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(skyui) = std::env::var_os("SOE_TEST_SKYUI") else {
        return;
    };
    let mut vanilla = Vfs::new();
    let vanilla_archive = PathBuf::from(data).join("Skyrim - Interface.bsa");
    vanilla.add_archive_path(&vanilla_archive).unwrap();
    let mut modded = Vfs::new();
    modded.add_archive_path(&vanilla_archive).unwrap();
    modded
        .add_archive_path(&PathBuf::from(skyui).join("SkyUI_SE.bsa"))
        .unwrap();
    let decompiler = std::env::var_os("SOE_FFDEC_JAR").map_or_else(
        || Decompiler::Jar(FFDEC.into()),
        |jar| Decompiler::Jar(jar.into()),
    );
    for (source, plugins) in [
        (&vanilla, vec!["Skyrim.esm".into()]),
        (&modded, vec!["Skyrim.esm".into(), "SkyUI_SE.esp".into()]),
    ] {
        let input = tempfile::tempdir_in("/tmp").unwrap();
        std::fs::create_dir(input.path().join("interface")).unwrap();
        for path in [
            "interface/inventorymenu.swf",
            "interface/magicmenu.swf",
            "interface/fonts_en.swf",
        ] {
            std::fs::write(input.path().join(path), source.read(path).unwrap()).unwrap();
        }
        let mut vfs = Vfs::new();
        vfs.add_loose_root(input.path()).unwrap();
        let output = tempfile::tempdir_in("/tmp").unwrap();
        let cache = build_cache(&vfs, &plugins, output.path(), &decompiler).unwrap();
        for path in ["interface/inventorymenu.swf", "interface/magicmenu.swf"] {
            let reference = cache.movies[path].parent().unwrap().join("as2-reference");
            let sources = scripts(&reference);
            assert!(!sources.is_empty());
            assert!(
                sources.iter().any(|script| script.contains("function")),
                "real ActionScript functions missing"
            );
            eprintln!(
                "UI-19 {plugins:?} {path}: {} decompiled sources",
                sources.len()
            );
        }
        assert!(
            build_cache(&vfs, &plugins, output.path(), &decompiler)
                .unwrap()
                .reused
        );
    }
}

#[test]
fn ui_19_godot_loads_paged_font_and_renders_glyphs_from_both_pages() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let input = tempfile::tempdir_in("/tmp").unwrap();
    let vfs = fixture_vfs(input.path(), &font_movie(800, 20480));
    let output = tempfile::tempdir_in("/tmp").unwrap();
    let cache = build_cache(
        &vfs,
        &[],
        output.path(),
        &Decompiler::Program("unused".into()),
    )
    .unwrap();
    let font = cache.movies["interface/fixture.swf"]
        .parent()
        .unwrap()
        .join("font-1.tres");
    let script = output.path().join("ui_19_font.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree
func _initialize():
    create_timer(20).timeout.connect(func(): quit(4))
    call_deferred("check")
func check():
    var font = load(OS.get_environment("SOE_UI_19_FONT")) as FontFile
    if font == null or font.get_texture_count(0, Vector2i(64, 0)) < 2:
        quit(1)
        return
    var control = Control.new()
    root.add_child(control)
    for code in [33, 600]:
        var index = font.get_glyph_index(64, code, 0)
        var page = font.get_glyph_texture_idx(0, Vector2i(64, 0), index)
        if page < 0 or (code == 600 and page == 0):
            quit(2)
            return
        var label = Label.new()
        label.text = String.chr(code)
        label.position = Vector2(40 if code == 33 else 160, 40)
        label.add_theme_font_override("font", font)
        label.add_theme_font_size_override("font_size", 64)
        control.add_child(label)
    await process_frame
    await process_frame
    await RenderingServer.frame_post_draw
    var pixels = root.get_texture().get_image()
    for x in [72, 192]:
        if pixels.get_pixel(x, 100).r < 0.8:
            quit(3)
            return
    print("UI-19 FONT PASS")
    pixels.save_png(OS.get_environment("SOE_UI_19_CAPTURE"))
    quit(0)
"#,
    )
    .unwrap();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let captures = workspace.join("target/ui-19");
    std::fs::create_dir_all(&captures).unwrap();
    let result = Command::new(godot)
        .arg("--path")
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .env("SOE_UI_19_FONT", font)
        .env("SOE_UI_19_CAPTURE", captures.join("font-pages.png"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    std::fs::write(
        captures.join("font-pages.log"),
        format!("{stdout}\n{stderr}"),
    )
    .unwrap();
    assert!(
        result.status.success()
            && stdout.contains("UI-19 FONT PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{}\n{stdout}\n{stderr}",
        result.status
    );
}
