// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use soe_bsa::{Archive, Vfs};
use soe_swf_convert::convert;

const SKYRIM: &str = "/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data";
const SKYUI: &str = "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/mods/SkyUI-5.2SE-6.11";

fn game_vfs() -> Option<Vfs> {
    let data = std::env::var_os("SKYRIM_DATA").map_or_else(|| Some(Path::new(SKYRIM).to_path_buf()), |p| Some(p.into()))?;
    let archive = data.join("Skyrim - Interface.bsa");
    if !archive.exists() { return None; }
    let mut vfs = Vfs::new();
    vfs.add_archive(Archive::open(&archive).ok()?);
    Some(vfs)
}

fn skyui_vfs() -> Option<Vfs> {
    let root = std::env::var_os("SOE_TEST_SKYUI").map_or_else(|| Some(Path::new(SKYUI).to_path_buf()), |p| Some(p.into()))?;
    let archive = root.join("SkyUI_SE.bsa");
    if !archive.exists() { return None; }
    let mut vfs = game_vfs()?;
    vfs.add_archive(Archive::open(&archive).ok()?);
    Some(vfs)
}

#[test]
fn rejects_truncated_and_oversized_swf_headers() {
    assert!(convert(b"FWS\x09\x08").is_err());
    let mut header = b"FWS\x09\0\0\0\0".to_vec();
    header[4..8].copy_from_slice(&(128 * 1024 * 1024u32 + 1).to_le_bytes());
    assert!(convert(&header).is_err());
}

#[test]
fn vanilla_interface_movies_have_art_text_and_embedded_glyphs() {
    let Some(vfs) = game_vfs() else { eprintln!("SKYRIM_DATA unavailable, skipped"); return; };
    for path in ["interface/inventorymenu.swf", "interface/magicmenu.swf", "interface/fonts_en.swf"] {
        let bytes = vfs.read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let movie = convert(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        let instances: usize = movie.timelines.values().flat_map(|t| &t.frames).map(|f| f.display_list.len()).sum();
        let glyphs: usize = movie.fonts.values().map(|f| f.glyphs.len()).sum();
        eprintln!("{path}: symbols={} meshes={} bitmaps={} text={} timelines={} frames={} instances={} fonts={} glyphs={} actions={}",
            movie.symbols.len(), movie.meshes.len(), movie.bitmaps.len(), movie.text_fields.len()+movie.static_text.len(),
            movie.timelines.len(), movie.timelines.values().map(|t| t.frames.len()).sum::<usize>(), instances, movie.fonts.len(), glyphs, movie.action_blocks);
        assert!(!movie.timelines.is_empty(), "{path}: missing timeline");
        assert!(instances > 0, "{path}: display list lost");
        if path.ends_with("fonts_en.swf") { assert!(glyphs > 0, "embedded glyphs missing"); }
        else { assert!(movie.meshes.len()+movie.bitmaps.len()+movie.text_fields.len()+movie.static_text.len() > 0, "{path}: no art or text"); }
    }
}

#[test]
fn skyui_winning_interface_movies_convert_from_later_archive() {
    let Some(vfs) = skyui_vfs() else { eprintln!("SOE_TEST_SKYUI unavailable, skipped"); return; };
    let paths: Vec<_> = vfs.list("interface").into_iter().filter(|p| p.ends_with(".swf")).collect();
    assert!(!paths.is_empty(), "SkyUI archive has no interface SWFs");
    let mut converted = 0;
    for path in paths {
        if !path.contains("skyui") && !path.contains("inventorymenu") && !path.contains("itemcard") { continue; }
        let Some(soe_bsa::Source::Archived { archive: 1, .. }) = vfs.resolve(path) else { continue; };
        let movie = convert(&vfs.read(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(!movie.timelines.is_empty(), "{path}: missing timeline");
        converted += 1;
    }
    assert!(converted > 0, "no SkyUI-winning UI movie was converted");
}

