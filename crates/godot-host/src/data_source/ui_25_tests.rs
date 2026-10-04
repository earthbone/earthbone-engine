// SPDX-License-Identifier: GPL-3.0-or-later

//! UI-25 startup localization through the source's selected language and VFS.

use super::*;
use soe_esm::{FileHeader, Plugin};

fn table(text: &str) -> Vec<u8> {
    let mut bytes = [
        1_u32.to_le_bytes(),
        (text.len() as u32 + 1).to_le_bytes(),
        7_u32.to_le_bytes(),
        0_u32.to_le_bytes(),
    ]
    .concat();
    bytes.extend_from_slice(text.as_bytes());
    bytes.push(0);
    bytes
}

fn source(root: &Path) -> DataSource {
    let mut plugin = Plugin::new(FileHeader {
        flags: soe_esm::plugin::flags::LOCALIZED,
        ..FileHeader::default()
    });
    plugin.push_top(Record::new(Tag(*b"MISC"), 1).sub(Tag(*b"FULL"), 7_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Items.esm", plugin).unwrap();
    let mut vfs = Vfs::new();
    vfs.add_loose_root(root).unwrap();
    DataSource {
        order: Arc::new(order),
        vfs: Arc::new(vfs),
        threads: 1,
        workers: WorkerPools::default(),
        textures: TextureCache::default(),
        profile_settings: soe_profile::Settings::default(),
        profile_ini_dir: None,
        language: "french".into(),
    }
}

#[test]
fn ui_25_startup_uses_selected_language_instead_of_english() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("Strings")).unwrap();
    std::fs::write(
        root.path().join("Strings/Items_english.strings"),
        table("Apple"),
    )
    .unwrap();
    std::fs::write(
        root.path().join("Strings/Items_french.strings"),
        table("Pomme"),
    )
    .unwrap();
    let source = source(root.path());
    let game = source.localized_game().unwrap();
    assert_eq!(
        game.world()
            .name_of(GlobalFormId { file: 0, object: 1 })
            .as_deref(),
        Some("Pomme")
    );
}

#[test]
fn ui_25_startup_reports_malformed_selected_language_table() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("Strings")).unwrap();
    std::fs::write(root.path().join("Strings/Items_french.strings"), [0_u8; 3]).unwrap();
    let source = source(root.path());
    let Err(error) = source.localized_game() else {
        panic!("malformed table accepted");
    };
    assert!(matches!(error, DataSourceError::Localization(_)));
    assert!(error.to_string().contains("Items_french.strings"));
}
