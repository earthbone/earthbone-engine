// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_bsa::{Archive, Vfs};
use soe_swf_convert::{build_cache, convert, Decompiler};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use swf::{PlaceObjectAction, Tag};

fn paged_font_movie() -> Vec<u8> {
    let edge = 20_480;
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
        name: swf::SwfStr::from_utf8_str("UI-19 adversarial fixture"),
        language: swf::Language::Unknown,
        layout: Some(swf::FontLayout {
            ascent: 16384,
            descent: 4096,
            leading: 0,
            kerning: vec![],
        }),
        glyphs: (0..800)
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
        flags: swf::FontFlag::HAS_WIDE_CODES | swf::FontFlag::HAS_LAYOUT,
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

fn has_actionscript(directory: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            has_actionscript(&path)
        } else {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("as"))
        }
    })
}

#[test]
fn ui_19_installed_fonts_keep_glyph_pages_bounded_and_indexed() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let archive = Archive::open(&PathBuf::from(data).join("Skyrim - Interface.bsa")).unwrap();
    for name in ["interface/fonts_en.swf", "interface/fonts_cn.swf"] {
        let bytes = archive.read(name).unwrap();
        let source = swf::decompress_swf(bytes.as_slice()).unwrap();
        let parsed = swf::parse_swf(&source).unwrap();
        let movie = convert(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let fonts: Vec<_> = parsed
            .tags
            .iter()
            .filter_map(|tag| match tag {
                Tag::DefineFont2(font) => Some(font),
                _ => None,
            })
            .collect();
        assert!(!fonts.is_empty(), "{name}: no source fonts");
        assert_eq!(
            fonts.len(),
            movie.fonts.len(),
            "{name}: lost font definition"
        );
        for source in fonts {
            let font = &movie.fonts[&source.id];
            assert_eq!(
                font.glyphs
                    .iter()
                    .map(|glyph| glyph.code)
                    .collect::<Vec<_>>(),
                source
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.code)
                    .collect::<Vec<_>>(),
                "{name}: font {} glyph order differs",
                source.id
            );
            for glyph in &font.glyphs {
                let atlas = font.atlases.get(glyph.texture_index).unwrap_or_else(|| {
                    panic!("{name}: font {} glyph has missing atlas", source.id)
                });
                let [x, y, width, height] = glyph.atlas_rect;
                assert!(x + width < atlas.width && y + height < atlas.height);
            }
            assert!(font.atlases.iter().all(|page| {
                page.width <= 1024
                    && page.height <= 2048
                    && page.rgba.len() == (page.width * page.height * 4) as usize
            }));
        }
    }
}

#[test]
fn ui_19_skyui_psychosteve_icons_convert_without_losing_authored_art() {
    let Some(root) = std::env::var_os("SOE_TEST_SKYUI") else {
        return;
    };
    let archive = Archive::open(&PathBuf::from(root).join("SkyUI_SE.bsa")).unwrap();
    let bytes = archive
        .read("interface/skyui/icons_item_psychosteve.swf")
        .unwrap();
    let buffer = swf::decompress_swf(bytes.as_slice()).unwrap();
    let converted = convert(&bytes).expect("SkyUI psychosteve icon SWF should convert");
    let mut reader = swf::read::Reader::new(&buffer.data, buffer.header.version());
    let mut expected_symbols = BTreeMap::new();
    while !reader.get_ref().is_empty() {
        let start = reader.get_ref();
        let (code, length) = reader.read_tag_code_and_length().unwrap();
        if code == 56 {
            if let Tag::ExportAssets(entries) =
                swf::read::Reader::new(start, buffer.header.version())
                    .read_tag()
                    .unwrap()
            {
                expected_symbols.extend(entries.iter().map(|entry| {
                    (
                        entry
                            .name
                            .to_string_lossy(swf::SwfStr::encoding_for_version(
                                buffer.header.version(),
                            )),
                        entry.id,
                    )
                }));
            }
        }
        *reader.get_mut() = &reader.get_ref()[length..];
        if code == 0 {
            break;
        }
    }
    assert_eq!(converted.symbols, expected_symbols, "icon exports differ");
    assert!(
        converted
            .timelines
            .values()
            .any(|timeline| !timeline.frames.is_empty()),
        "icon sprite timelines were lost"
    );
}

#[test]
fn ui_19_godot_loads_font_cache_with_multiple_atlas_pages() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let bytes = paged_font_movie();
    let movie = convert(&bytes).unwrap();
    let glyph = movie.fonts[&1]
        .glyphs
        .iter()
        .find(|glyph| glyph.code == 600)
        .unwrap();
    assert_eq!(glyph.texture_index, 1);
    let (font_id, code, page) = (1, glyph.code, glyph.texture_index);
    let input = tempfile::tempdir_in("/tmp").unwrap();
    std::fs::create_dir(input.path().join("interface")).unwrap();
    std::fs::write(input.path().join("interface/fixture.swf"), bytes).unwrap();
    let mut vfs = Vfs::new();
    vfs.add_loose_root(input.path()).unwrap();
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
        .join(format!("font-{font_id}.tres"));
    let script = output.path().join("ui_19_paged_font.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree
func _initialize():
    call_deferred("check")
func check():
    var font = load(OS.get_environment("SOE_UI_19_FONT")) as FontFile
    if font == null or font.get_texture_count(0, Vector2i(64, 0)) <= int(OS.get_environment("SOE_UI_19_PAGE")):
        quit(1)
        return
    var index = font.get_glyph_index(64, int(OS.get_environment("SOE_UI_19_CODE")), 0)
    if font.get_glyph_texture_idx(0, Vector2i(64, 0), index) != int(OS.get_environment("SOE_UI_19_PAGE")):
        quit(2)
        return
    print("UI-19 PAGED FONT PASS")
    quit(0)
"#,
    )
    .unwrap();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(workspace.join("godot"))
        .arg("--script")
        .arg(script)
        .env("SOE_UI_19_FONT", font)
        .env("SOE_UI_19_CODE", code.to_string())
        .env("SOE_UI_19_PAGE", page.to_string())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success() && stdout.contains("UI-19 PAGED FONT PASS"),
        "{}\n{stdout}\n{stderr}",
        result.status
    );
}

#[test]
fn ui_19_root_swf_requires_a_well_formed_end_tag() {
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 1;
    let mut missing_end = Vec::new();
    swf::write::write_swf_raw_tags(&header, &[0x40, 0x00], &mut missing_end).unwrap();

    header.num_frames = 0;
    let mut end_with_payload = Vec::new();
    swf::write::write_swf_raw_tags(&header, &[0x01, 0x00, 0xff], &mut end_with_payload).unwrap();

    assert!(
        convert(&missing_end).is_err() && convert(&end_with_payload).is_err(),
        "root SWF accepted missing End or End with a payload"
    );
}

#[test]
fn ui_19_full_winning_load_order_builds_cache_and_keeps_real_as2_sources() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(skyui) = std::env::var_os("SOE_TEST_SKYUI") else {
        return;
    };
    let jar = std::env::var_os("SOE_FFDEC_JAR").map_or_else(
        || {
            PathBuf::from(
                "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/SkyUI-Community/tools/FFDec/ffdec-cli.jar",
            )
        },
        PathBuf::from,
    );
    if !jar.is_file() {
        return;
    }
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&PathBuf::from(data).join("Skyrim - Interface.bsa"))
        .unwrap();
    vfs.add_archive_path(&PathBuf::from(skyui).join("SkyUI_SE.bsa"))
        .unwrap();
    let paths: Vec<_> = vfs
        .list("interface")
        .into_iter()
        .filter(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("swf"))
        })
        .collect();
    let output = tempfile::tempdir_in("/tmp").unwrap();
    let cache = build_cache(
        &vfs,
        &["Skyrim.esm".into(), "SkyUI_SE.esp".into()],
        output.path(),
        &Decompiler::Jar(jar),
    )
    .unwrap();
    assert_eq!(cache.movies.len(), paths.len());
    for path in &paths {
        assert!(
            cache.movies[*path].is_file(),
            "{path}: missing native movie"
        );
    }
    for path in ["interface/inventorymenu.swf", "interface/magicmenu.swf"] {
        let source = cache.movies[path].parent().unwrap().join("as2-reference");
        assert!(
            has_actionscript(&source),
            "{path}: no FFDec source retained"
        );
    }
}

fn data_dir() -> Option<PathBuf> {
    let path = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    path.join("Skyrim - Interface.bsa").exists().then_some(path)
}

fn vanilla_vfs() -> Option<Vfs> {
    let data = data_dir()?;
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&data.join("Skyrim - Interface.bsa"))
        .ok()?;
    Some(vfs)
}

fn skyui_vfs() -> Option<Vfs> {
    let root = std::env::var_os("SOE_TEST_SKYUI").map(PathBuf::from)?;
    let mut vfs = vanilla_vfs()?;
    vfs.add_archive_path(&root.join("SkyUI_SE.bsa")).ok()?;
    Some(vfs)
}

fn visit<'src, 'buf>(tags: &'src [Tag<'buf>], output: &mut Vec<&'src Tag<'buf>>) {
    for tag in tags {
        output.push(tag);
        if let Tag::DefineSprite(sprite) = tag {
            visit(&sprite.tags, output);
        }
    }
}

fn input(path: &str) -> Option<Vec<u8>> {
    let vfs = vanilla_vfs()?;
    vfs.read(path).ok()
}

#[test]
fn ui_19_installed_font3_codes_advances_and_metrics_match_swf_tags() {
    let Some(bytes) = input("interface/fonts_en.swf") else {
        eprintln!("SKYRIM_DATA unavailable, skipped");
        return;
    };
    let buffer = swf::decompress_swf(bytes.as_slice()).expect("decompress fonts_en.swf");
    let source_movie = swf::parse_swf(&buffer).expect("parse fonts_en.swf");
    let mut tags = Vec::new();
    visit(&source_movie.tags, &mut tags);
    let movie = convert(&bytes).expect("fonts_en.swf should convert");
    let expected: BTreeMap<_, _> = tags
        .iter()
        .filter_map(|tag| match *tag {
            Tag::DefineFont2(font) => Some((font.id, font)),
            _ => None,
        })
        .collect();
    assert!(!expected.is_empty(), "source SWF has no DefineFont2/3 tags");
    assert_eq!(
        movie.fonts.len(),
        expected.len(),
        "a parsed font definition was lost"
    );
    for (id, source) in expected {
        let actual = &movie.fonts[&id];
        assert_eq!(actual.version, source.version, "font {id} version");
        assert_eq!(
            actual.name,
            source
                .name
                .to_string_lossy(swf::SwfStr::encoding_for_version(movie.version)),
            "font {id} name"
        );
        assert_eq!(
            actual.glyphs.len(),
            source.glyphs.len(),
            "font {id} glyph count"
        );
        assert_eq!(
            actual
                .glyphs
                .iter()
                .map(|glyph| glyph.code)
                .collect::<Vec<_>>(),
            source
                .glyphs
                .iter()
                .map(|glyph| glyph.code)
                .collect::<Vec<_>>(),
            "font {id} glyph code order"
        );
        let unit_scale = if source.version == 3 {
            64.0 / 20_480.0
        } else {
            64.0 / 1_024.0
        };
        for (actual_glyph, source_glyph) in actual.glyphs.iter().zip(&source.glyphs) {
            assert!(
                (actual_glyph.advance - source_glyph.advance as f64 * unit_scale).abs() < 0.001,
                "font {id} glyph {} advance",
                actual_glyph.code
            );
        }
        for atlas in &actual.atlases {
            assert_eq!(
                atlas.rgba.len(),
                (atlas.width * atlas.height * 4) as usize,
                "font {id} atlas is truncated"
            );
        }
    }
}

#[test]
fn ui_19_installed_shapes_bitmaps_text_and_sprite_frames_match_tag_ids() {
    let Some(bytes) = input("interface/inventorymenu.swf") else {
        eprintln!("SKYRIM_DATA unavailable, skipped");
        return;
    };
    let buffer = swf::decompress_swf(bytes.as_slice()).expect("decompress inventorymenu.swf");
    let source_movie = swf::parse_swf(&buffer).expect("parse inventorymenu.swf");
    let mut tags = Vec::new();
    visit(&source_movie.tags, &mut tags);
    let movie = convert(&bytes).expect("inventorymenu.swf should convert");
    let ids = |predicate: fn(&Tag<'_>) -> Option<u16>| {
        tags.iter()
            .filter_map(|tag| predicate(tag))
            .collect::<BTreeSet<_>>()
    };
    let shapes = ids(|tag| {
        if let Tag::DefineShape(shape) = tag {
            Some(shape.id)
        } else {
            None
        }
    });
    let bitmaps = ids(|tag| match tag {
        Tag::DefineBits { id, .. } | Tag::DefineBitsJpeg2 { id, .. } => Some(*id),
        Tag::DefineBitsJpeg3(bitmap) => Some(bitmap.id),
        Tag::DefineBitsLossless(bitmap) => Some(bitmap.id),
        _ => None,
    });
    let edit_text = ids(|tag| {
        if let Tag::DefineEditText(text) = tag {
            Some(text.id())
        } else {
            None
        }
    });
    let static_text = ids(|tag| match tag {
        Tag::DefineText(text) | Tag::DefineText2(text) => Some(text.id),
        _ => None,
    });
    assert_eq!(
        movie.meshes.keys().copied().collect::<BTreeSet<_>>(),
        shapes,
        "shape IDs differ from parser tags"
    );
    assert_eq!(
        movie.bitmaps.keys().copied().collect::<BTreeSet<_>>(),
        bitmaps,
        "bitmap IDs differ from parser tags"
    );
    assert_eq!(
        movie.text_fields.keys().copied().collect::<BTreeSet<_>>(),
        edit_text,
        "edit text IDs differ from parser tags"
    );
    assert_eq!(
        movie.static_text.keys().copied().collect::<BTreeSet<_>>(),
        static_text,
        "static text IDs differ from parser tags"
    );
    let declared_frames = tags
        .iter()
        .filter(|tag| matches!(tag, Tag::ShowFrame))
        .count();
    assert!(declared_frames > 0, "source menu has no frames");
    assert!(
        movie
            .timelines
            .values()
            .map(|timeline| timeline.frames.len())
            .sum::<usize>()
            >= declared_frames,
        "sprite frames were dropped"
    );
}

#[test]
fn ui_19_installed_display_list_keeps_authored_names_labels_and_placements() {
    for path in ["interface/inventorymenu.swf", "interface/magicmenu.swf"] {
        let Some(bytes) = input(path) else {
            eprintln!("SKYRIM_DATA unavailable, skipped");
            return;
        };
        let buffer = swf::decompress_swf(bytes.as_slice()).expect("decompress installed menu");
        let source_movie = swf::parse_swf(&buffer).expect("parse installed menu");
        let mut tags = Vec::new();
        visit(&source_movie.tags, &mut tags);
        let movie = convert(&bytes).expect("installed menu should convert");
        let expected_names: BTreeSet<_> = tags
            .iter()
            .filter_map(|tag| match *tag {
                Tag::PlaceObject(place) if matches!(place.action, PlaceObjectAction::Place(_)) => {
                    place.name.map(|name| {
                        name.to_string_lossy(swf::SwfStr::encoding_for_version(movie.version))
                    })
                }
                _ => None,
            })
            .collect();
        let actual_names: BTreeSet<_> = movie
            .timelines
            .values()
            .flat_map(|timeline| &timeline.frames)
            .flat_map(|frame| &frame.display_list)
            .filter_map(|instance| instance.name.clone())
            .collect();
        assert!(
            !expected_names.is_empty(),
            "source menu has no named placements"
        );
        assert!(
            expected_names.is_subset(&actual_names),
            "some authored instance names are absent"
        );
        let expected_labels: BTreeSet<_> = tags
            .iter()
            .filter_map(|tag| {
                if let Tag::FrameLabel(label) = *tag {
                    Some(
                        label
                            .label
                            .to_string_lossy(swf::SwfStr::encoding_for_version(movie.version)),
                    )
                } else {
                    None
                }
            })
            .collect();
        let actual_labels: BTreeSet<_> = movie
            .timelines
            .values()
            .flat_map(|timeline| &timeline.frames)
            .flat_map(|frame| &frame.labels)
            .cloned()
            .collect();
        assert_eq!(
            actual_labels, expected_labels,
            "authored frame labels differ"
        );
        let expected_transforms: Vec<_> = tags
            .iter()
            .filter_map(|tag| match *tag {
                Tag::PlaceObject(place) if matches!(place.action, PlaceObjectAction::Place(_)) => {
                    place.matrix.map(|matrix| {
                        let mut transform = [
                            matrix.a.to_f64(),
                            matrix.b.to_f64(),
                            matrix.c.to_f64(),
                            matrix.d.to_f64(),
                            matrix.tx.get() as f64,
                            matrix.ty.get() as f64,
                        ];
                        transform[4] /= 20.0;
                        transform[5] /= 20.0;
                        transform
                    })
                }
                _ => None,
            })
            .collect();
        let actual_transforms: Vec<_> = movie
            .timelines
            .values()
            .flat_map(|timeline| &timeline.frames)
            .flat_map(|frame| &frame.display_list)
            .map(|instance| instance.transform)
            .collect();
        for expected in expected_transforms {
            assert!(
                actual_transforms.iter().any(|actual| actual
                    .iter()
                    .zip(expected)
                    .all(|(left, right)| (left - right).abs() < 0.000_001)),
                "authored placement transform was lost: {expected:?}"
            );
        }
    }
}

#[test]
fn ui_19_cache_rebuilds_for_new_winning_bytes_and_preserves_as2_reference() {
    let Some(data) = data_dir() else {
        eprintln!("SKYRIM_DATA unavailable, skipped");
        return;
    };
    let source = data.join("Skyrim - Interface.bsa");
    let mut source_vfs = Vfs::new();
    source_vfs
        .add_archive_path(&source)
        .expect("open interface archive");
    let first = source_vfs
        .read("interface/magicmenu.swf")
        .expect("read vanilla menu");
    let second = source_vfs
        .read("interface/inventorymenu.swf")
        .expect("read second menu");
    let temp = tempfile::tempdir_in("/tmp").expect("temporary test root");
    let loose = temp.path().join("loose");
    std::fs::create_dir_all(loose.join("interface")).expect("create loose interface");
    let override_file = loose.join("interface/magicmenu.swf");
    std::fs::write(&override_file, &first).expect("write initial loose winner");
    std::fs::write(loose.join("interface/inventorymenu.swf"), &second)
        .expect("write second interface movie");
    let mut vfs = Vfs::new();
    vfs.add_loose_root(&loose).expect("add loose override");
    let root = temp.path().join("outside-checkout");
    let fake = temp.path().join("ffdec");
    std::fs::write(&fake, "#!/bin/sh\nwhile [ $# -gt 0 ]; do if [ \"$1\" = script ]; then shift; out=$1; break; fi; shift; done\nmkdir -p \"$out\"\ncat >/dev/null\nprintf 'reference source' >\"$out/menu.as\"\n").expect("write fake decompiler");
    let mut permissions = std::fs::metadata(&fake).expect("stat fake").permissions();
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake, permissions).expect("make fake decompiler executable");
    let decompiler = Decompiler::Program(fake);
    let first_cache =
        build_cache(&vfs, &["Skyrim.esm".into()], &root, &decompiler).expect("publish cache");
    assert!(!first_cache.reused);
    let movie_path = &first_cache.movies["interface/magicmenu.swf"];
    assert!(movie_path.is_file(), "movie resource is missing");
    let source_dir = movie_path
        .parent()
        .expect("movie resource directory")
        .join("as2-reference");
    assert!(
        source_dir.join("menu.as").is_file(),
        "AS2 source reference was not retained"
    );
    let reused =
        build_cache(&vfs, &["Skyrim.esm".into()], &root, &decompiler).expect("reuse cache");
    assert!(
        reused.reused,
        "identical load order should reuse generation"
    );
    let reordered = build_cache(
        &vfs,
        &["Skyrim.esm".into(), "SkyUI.esp".into()],
        &root,
        &decompiler,
    )
    .expect("rebuild changed load order");
    assert!(
        !reordered.reused,
        "changed load order must produce a new generation"
    );
    assert_ne!(first_cache.directory, reordered.directory);
    std::fs::write(&override_file, &second).expect("replace winning loose SWF");
    let rebuilt = build_cache(&vfs, &["Skyrim.esm".into()], &root, &decompiler)
        .expect("rebuild changed input");
    assert!(
        !rebuilt.reused,
        "changed winning bytes must produce a new generation"
    );
    assert_ne!(first_cache.directory, rebuilt.directory);
}

#[test]
fn ui_19_skyui_archive_winners_convert_and_keep_source_definitions() {
    let Some(vfs) = skyui_vfs() else {
        eprintln!("SOE_TEST_SKYUI or installed vanilla data unavailable, skipped");
        return;
    };
    let paths: Vec<_> = vfs
        .list("interface")
        .into_iter()
        .filter(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("swf"))
                && (path.contains("skyui")
                    || path.contains("inventorymenu")
                    || path.contains("itemcard"))
        })
        .collect();
    let winners: Vec<_> = paths
        .into_iter()
        .filter(|path| {
            matches!(
                vfs.resolve(path),
                Some(soe_bsa::Source::Archived { archive: 1, .. })
            )
        })
        .collect();
    assert!(
        !winners.is_empty(),
        "SkyUI archive supplied no winning interface movies"
    );
    for path in winners {
        let bytes = vfs.read(path).expect("read winning SkyUI movie");
        let buffer = swf::decompress_swf(bytes.as_slice()).expect("decompress winning SWF");
        let movie = convert(&bytes)
            .unwrap_or_else(|error| panic!("convert winning SkyUI SWF {path}: {error}"));
        assert_eq!(
            movie.timelines[&0].frames.len(),
            buffer.header.num_frames() as usize,
            "{path}: root frames differ from SWF header"
        );
        let Ok(source) = swf::parse_swf(&buffer) else {
            // CONTEXT: SkyUI icon sprites omit End, which the bounded converter accepts.
            let mut reader = swf::read::Reader::new(&buffer.data, buffer.header.version());
            let mut sprites = 0;
            while !reader.get_ref().is_empty() {
                let (code, length) = reader.read_tag_code_and_length().unwrap();
                let payload = &reader.get_ref()[..length];
                if code == 39 {
                    let id = u16::from_le_bytes(payload[..2].try_into().unwrap());
                    let frames = u16::from_le_bytes(payload[2..4].try_into().unwrap());
                    assert_eq!(
                        movie.timelines[&id].declared_frames, frames,
                        "{path}: sprite {id}"
                    );
                    assert_eq!(
                        movie.timelines[&id].frames.len(),
                        frames as usize,
                        "{path}: sprite {id} frames"
                    );
                    sprites += 1;
                }
                *reader.get_mut() = &reader.get_ref()[length..];
                if code == 0 {
                    break;
                }
            }
            assert!(sprites > 0, "{path}: no bounded sprite definitions checked");
            continue;
        };
        assert!(!movie.timelines.is_empty(), "{path}: root timeline missing");
        let expected_symbols: BTreeSet<_> = source
            .tags
            .iter()
            .filter_map(|tag| {
                if let Tag::SymbolClass(entries) = tag {
                    Some(entries.len())
                } else {
                    None
                }
            })
            .collect();
        if !expected_symbols.is_empty() {
            assert!(
                !movie.symbols.is_empty(),
                "{path}: SymbolClass names were lost"
            );
        }
    }
}

#[test]
fn ui_19_failed_swf_conversion_leaves_no_published_generation() {
    let temp = tempfile::tempdir_in("/tmp").expect("temporary test root");
    let loose = temp.path().join("loose");
    std::fs::create_dir_all(loose.join("interface")).expect("create loose interface");
    std::fs::write(loose.join("interface/broken.swf"), b"not an swf").expect("write broken file");
    let mut vfs = Vfs::new();
    vfs.add_loose_root(&loose).expect("add loose files");
    let root = temp.path().join("cache");
    let result = build_cache(
        &vfs,
        &[],
        &root,
        &Decompiler::Program(Path::new("unused").to_path_buf()),
    );
    assert!(result.is_err(), "malformed SWF should fail conversion");
    let generations = std::fs::read_dir(root)
        .expect("cache root exists")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .count();
    assert_eq!(
        generations, 0,
        "failed conversion published a cache generation"
    );
}
