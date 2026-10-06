// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-24 checks art-only conversion and independent archive provenance.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use soe_godot_host::nordic_theme::{self, Exclusions};
use swf::{FillStyle, Point, PointDelta, ShapeRecord, Tag, Twips};

fn shape(id: u16, color: swf::Color) -> swf::Shape {
    let bounds = swf::Rectangle {
        x_min: Twips::ZERO,
        x_max: Twips::new(200),
        y_min: Twips::ZERO,
        y_max: Twips::new(200),
    };
    swf::Shape {
        version: 3,
        id,
        shape_bounds: bounds,
        edge_bounds: bounds,
        flags: swf::ShapeFlag::empty(),
        styles: swf::ShapeStyles {
            fill_styles: vec![FillStyle::Color(color)],
            line_styles: vec![],
        },
        shape: std::iter::once(ShapeRecord::StyleChange(Box::new(swf::StyleChangeData {
            move_to: Some(Point::new(Twips::ZERO, Twips::ZERO)),
            fill_style_0: Some(1),
            fill_style_1: None,
            line_style: None,
            new_styles: None,
        })))
        .chain(
            [(200, 0), (0, 200), (-200, 0), (0, -200)].map(|(x, y)| ShapeRecord::StraightEdge {
                delta: PointDelta::new(Twips::new(x), Twips::new(y)),
            }),
        )
        .collect(),
    }
}

fn movie(tags: &[Tag<'_>]) -> Vec<u8> {
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 0;
    let mut bytes = Vec::new();
    swf::write_swf(&header, tags, &mut bytes).unwrap();
    bytes
}

fn color(value: u8) -> swf::Color {
    swf::Color {
        r: value,
        g: 100,
        b: 200,
        a: 255,
    }
}

#[test]
fn ui_24_duplicates_in_either_reference_and_renumbered_nested_shapes_are_excluded() {
    let mut exclusions = Exclusions::default();
    for value in [1, 2] {
        exclusions
            .add_movie(&movie(&[
                Tag::DefineShape(Box::new(shape(42, color(value)))),
                Tag::End,
            ]))
            .unwrap();
    }
    let source = movie(&[
        Tag::DefineSprite(swf::Sprite {
            id: 100,
            num_frames: 0,
            tags: vec![Tag::DefineShape(Box::new(shape(1, color(1)))), Tag::End],
        }),
        Tag::DefineShape(Box::new(shape(2, color(2)))),
        Tag::DefineShape(Box::new(shape(3, color(3)))),
        Tag::End,
    ]);
    let art = nordic_theme::own_art(&source, &exclusions).unwrap();
    assert_eq!(art.meshes.keys().copied().collect::<Vec<_>>(), [3]);
}

#[test]
fn ui_24_art_writer_omits_scripts_fonts_symbols_text_buttons_and_layouts() {
    let source = movie(&[
        Tag::DefineShape(Box::new(shape(1, color(1)))),
        Tag::DoAction(&[0]),
        Tag::FrameLabel(swf::FrameLabel {
            label: swf::SwfStr::from_utf8_str("Not a theme layout"),
            is_anchor: false,
        }),
        Tag::End,
    ]);
    let art = nordic_theme::own_art(&source, &Exclusions::default()).unwrap();
    assert!(art.timelines.is_empty() && art.symbols.is_empty() && art.fonts.is_empty());
    assert_eq!(art.action_blocks, 0);
    let output = tempfile::tempdir_in(workspace().join("target")).unwrap();
    soe_swf_convert::write_art(&art, output.path()).unwrap();
    let names: Vec<_> = output
        .path()
        .read_dir()
        .unwrap()
        .map(|p| p.unwrap().file_name())
        .collect();
    assert_eq!(names, ["shape-1.tres"]);
}

#[test]
fn ui_24_malformed_or_truncated_sources_fail_instead_of_bypassing_exclusions() {
    assert!(Exclusions::default().add_movie(b"FWS\x09").is_err());
    let mut bytes = movie(&[Tag::DefineShape(Box::new(shape(1, color(1)))), Tag::End]);
    bytes.truncate(bytes.len() - 5);
    assert!(nordic_theme::own_art(&bytes, &Exclusions::default()).is_err());
}

#[test]
fn ui_24_redefined_art_ids_cannot_replace_an_excluded_definition() {
    let bytes = movie(&[
        Tag::DefineShape(Box::new(shape(1, color(1)))),
        Tag::DefineShape(Box::new(shape(1, color(2)))),
        Tag::End,
    ]);
    assert_eq!(
        nordic_theme::own_art(&bytes, &Exclusions::default())
            .unwrap_err()
            .to_string(),
        "invalid interface art: theme art redefines a character ID"
    );
}

#[test]
fn ui_24_duplicate_bitmaps_and_dependent_shapes_are_excluded_together() {
    fn bitmap(id: u16) -> Tag<'static> {
        Tag::DefineBitsLossless(swf::DefineBitsLossless {
            version: 2,
            id,
            format: swf::BitmapFormat::Rgb32,
            width: 1,
            height: 1,
            data: std::borrow::Cow::Borrowed(&[
                120, 156, 251, 255, 255, 255, 127, 0, 9, 250, 3, 253,
            ]),
        })
    }
    let mut exclusions = Exclusions::default();
    exclusions
        .add_movie(&movie(&[bitmap(80), Tag::End]))
        .unwrap();
    let mut textured = shape(1, color(1));
    textured.styles.fill_styles = vec![FillStyle::Bitmap {
        id: 2,
        matrix: swf::Matrix::IDENTITY,
        is_smoothed: false,
        is_repeating: true,
    }];
    let source = movie(&[Tag::DefineShape(Box::new(textured)), bitmap(2), Tag::End]);
    let control = nordic_theme::own_art(&source, &Exclusions::default()).unwrap();
    assert_eq!(control.bitmaps.len(), 1);
    assert_eq!(control.meshes.len(), 1);
    let art = nordic_theme::own_art(&source, &exclusions).unwrap();
    assert!(art.meshes.is_empty() && art.bitmaps.is_empty());
}

#[test]
fn ui_24_bundle_has_declared_resources_and_required_permission_and_gap_credits() {
    let theme = workspace().join("godot/ui/theme");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(theme.join("manifest.json")).unwrap()).unwrap();
    let provenance: serde_json::Value =
        serde_json::from_slice(&std::fs::read(theme.join("nordic/provenance.json")).unwrap())
            .unwrap();
    let declared: std::collections::BTreeSet<_> = manifest
        .as_object()
        .unwrap()
        .values()
        .map(|v| {
            workspace()
                .join("godot")
                .join(v.as_str().unwrap().strip_prefix("res://").unwrap())
        })
        .collect();
    for path in manifest
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_str().unwrap())
    {
        assert!(
            workspace()
                .join("godot")
                .join(path.strip_prefix("res://").unwrap())
                .is_file(),
            "{path}: missing theme art"
        );
        if let Some(nordic) = path.strip_prefix("res://ui/theme/nordic/") {
            assert!(
                provenance["elements"][nordic].is_object(),
                "{path}: missing provenance"
            );
        }
    }
    let credits = std::fs::read_to_string(theme.join("CREDITS")).unwrap();
    for required in [
        "outobugi",
        "dopesan",
        "2.4.1",
        "2021-08-14",
        "2026-10-06",
        "https://www.nexusmods.com/skyrimspecialedition/mods/49881",
        "Permissions are open to use my art as you wish",
        "Lorc",
        "CC BY 3.0",
        "https://creativecommons.org/licenses/by/3.0/",
    ] {
        assert!(credits.contains(required), "missing credit: {required}");
    }
    fn check_files(path: &Path, declared: &std::collections::BTreeSet<PathBuf>) {
        for entry in path.read_dir().unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                check_files(&path, declared);
                continue;
            }
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            assert!(
                ["", "gd", "uid", "json", "svg", "tres", "import"].contains(&extension),
                "unexpected bundled file {}",
                path.display()
            );
            let name = path.file_name().unwrap().to_str().unwrap();
            if matches!(extension, "tres" | "svg") {
                assert!(
                    declared.contains(&path),
                    "undeclared theme asset {}",
                    path.display()
                );
            }
            assert!(
                !name.starts_with("movie.") && !name.starts_with("font-"),
                "layout or font was bundled"
            );
        }
    }
    check_files(&theme, &declared);
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn ui_24_nordic_install_extracts_only_art_absent_from_skyui_and_vanilla() {
    let (Some(nordic), Some(skyui), Some(data)) = (
        std::env::var_os("SOE_TEST_NORDIC"),
        std::env::var_os("SOE_TEST_SKYUI"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        eprintln!("SOE_TEST_NORDIC, SOE_TEST_SKYUI or SKYRIM_DATA unset, skipped");
        return;
    };
    let nordic = PathBuf::from(nordic);
    let skyui = soe_bsa::Archive::open(&PathBuf::from(skyui).join("SkyUI_SE.bsa")).unwrap();
    let vanilla =
        soe_bsa::Archive::open(&PathBuf::from(data).join("Skyrim - Interface.bsa")).unwrap();
    let output = tempfile::tempdir_in(workspace().join("target")).unwrap();
    let manifest = nordic_theme::extract(&nordic, &skyui, &vanilla, output.path()).unwrap();
    let elements = manifest["elements"].as_object().unwrap();
    assert!(!elements.is_empty(), "no author's art extracted");
    let mut count = 0;
    for path in [
        "icons_category_psychosteve",
        "inventorylists",
        "itemcard",
        "mapmarkerart",
    ] {
        let source = nordic.join(format!("interface/skyui/{path}.swf"));
        let bytes = std::fs::read(source).unwrap();
        let mut exclusions = Exclusions::default();
        exclusions.add_movie(&bytes).unwrap();
        let rechecked = nordic_theme::own_art(&bytes, &exclusions).unwrap();
        assert!(rechecked.meshes.is_empty() && rechecked.bitmaps.is_empty());
        for file in output.path().join(path).read_dir().unwrap() {
            let file = file.unwrap();
            let key = format!("{path}/{}", file.file_name().to_str().unwrap());
            assert!(elements.contains_key(&key), "uncredited art {key}");
            assert_eq!(file.path().extension().unwrap(), "tres");
            count += 1;
        }
    }
    assert_eq!(count, elements.len());
    let reference_hashes: std::collections::BTreeSet<_> = [&skyui, &vanilla]
        .into_iter()
        .flat_map(|archive| {
            archive
                .files()
                .iter()
                .filter(|path| {
                    Path::new(path).extension().is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("swf")
                            || extension.eq_ignore_ascii_case("gfx")
                    })
                })
                .flat_map(|path| art_hashes(&archive.read(path).unwrap()))
        })
        .collect();
    for element in elements.values() {
        assert!(
            !reference_hashes.contains(element["sha256"].as_str().unwrap()),
            "reference art escaped exclusion"
        );
    }
    let bundle = workspace().join("godot/ui/theme/nordic");
    let bundled: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("provenance.json")).unwrap()).unwrap();
    for (path, provenance) in bundled["elements"].as_object().unwrap() {
        assert_eq!(
            provenance, &manifest["elements"][path],
            "{path}: changed source art"
        );
        assert_eq!(
            std::fs::read(bundle.join(path)).unwrap(),
            std::fs::read(output.path().join(path)).unwrap(),
            "{path}: resource changed after extraction"
        );
    }
    assert!(nordic_theme::extract(&nordic, &skyui, &vanilla, output.path()).is_err());
}

fn art_hashes(bytes: &[u8]) -> Vec<String> {
    fn visit(mut tags: &[u8], version: u8, hashes: &mut Vec<String>) {
        while !tags.is_empty() {
            let mut reader = swf::read::Reader::new(tags, version);
            let (code, size) = reader.read_tag_code_and_length().unwrap();
            let payload = &reader.get_ref()[..size];
            if code == 0 {
                break;
            }
            if code == 39 {
                visit(&payload[4..], version, hashes);
            }
            if matches!(code, 2 | 22 | 32 | 83 | 6 | 20 | 21 | 35 | 36 | 90) {
                let mut hash = Sha256::new();
                hash.update(code.to_le_bytes());
                hash.update(&payload[2..]);
                hashes.push(format!("{:x}", hash.finalize()));
            }
            tags = &reader.get_ref()[size..];
        }
    }
    let source = swf::decompress_swf(bytes).unwrap();
    let mut hashes = Vec::new();
    visit(&source.data, source.header.version(), &mut hashes);
    hashes
}

fn scene(cache: Option<&Path>, capture_name: &str) {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipped");
        return;
    };
    let capture = workspace().join("target").join(capture_name);
    let output = Command::new(godot)
        .args(["--path"])
        .arg(workspace().join("godot"))
        .args(["--resolution", "640x360", "--script", "ui_24_theme_test.gd"])
        .env("SOE_UI_24_CACHE", cache.unwrap_or_else(|| Path::new("")))
        .env("SOE_UI_24_CAPTURE", &capture)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("UI-24 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
    assert!(capture.is_file());
}

#[test]
fn ui_24_scene_every_category_item_type_panel_and_marker_has_theme_art() {
    scene(None, "ui-24-default.png");
}

#[test]
fn ui_24_skyui_install_art_wins_over_the_default_theme_in_a_scene() {
    let Some(skyui) = std::env::var_os("SOE_TEST_SKYUI") else {
        eprintln!("SOE_TEST_SKYUI unset, skipped");
        return;
    };
    let archive = soe_bsa::Archive::open(&PathBuf::from(skyui).join("SkyUI_SE.bsa")).unwrap();
    let cache = tempfile::tempdir_in(workspace().join("target")).unwrap();
    let mut movies = serde_json::Map::new();
    for name in [
        "icons_item_psychosteve",
        "icons_category_psychosteve",
        "inventorylists",
        "itemcard",
    ] {
        let path = format!("interface/skyui/{name}.swf");
        let movie = soe_swf_convert::convert(&archive.read(&path).unwrap()).unwrap();
        let output = cache.path().join(name);
        soe_swf_convert::write_art(&movie, &output).unwrap();
        std::fs::write(
            output.join("movie.json"),
            serde_json::to_vec(&movie).unwrap(),
        )
        .unwrap();
        movies.insert(path, format!("{name}/movie.tres").into());
    }
    std::fs::write(
        cache.path().join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({"movies":movies})).unwrap(),
    )
    .unwrap();
    scene(Some(cache.path()), "ui-24-skyui.png");
}
