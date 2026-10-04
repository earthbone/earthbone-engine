// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! A valid BSA fixture proves that the host profile path selects the chosen
//! locale, retains loose-file precedence, and hands a voice line to `soe-audio`.

use soe_audio::{fuz, SoundSource};
use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::plugin::FileHeader;
use soe_esm::Plugin;
use soe_godot_host::audio::{to_i16_bytes, to_mono};
use soe_godot_host::data_source::{vfs_from_loadout, DataSource, DataSourceError};
use soe_profile::{ArchiveRef, Ini, Layer, LoadOrder, Loadout, Settings};

fn bsa(file: &str, bytes: &[u8]) -> Vec<u8> {
    write_bsa(&[BsaFile { path: file, bytes }], BsaVersion::SkyrimSe).unwrap()
}

fn wave(sample: i16) -> Vec<u8> {
    let mut out = b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x44\xAC\0\0\x88\x58\x01\0\x02\0\x10\0data\x02\0\0\0".to_vec();
    out.extend_from_slice(&sample.to_le_bytes());
    out
}

fn loadout(language: &str, roots: Vec<std::path::PathBuf>, archives: Vec<ArchiveRef>) -> Loadout {
    let mut settings = Settings::new();
    settings.apply(
        &Layer::Default,
        &Ini::parse(&format!("[General]\nsLanguage={language}\n")),
    );
    Loadout {
        data_roots: roots,
        load_order: LoadOrder::default(),
        settings,
        ini_dir: None,
        archives,
    }
}

#[test]
fn french_voice_archive_and_later_loose_root_win_then_decode() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("Data");
    let mod_root = temp.path().join("Mod");
    std::fs::create_dir_all(&data).unwrap();
    let english = fuz::write(1, &[], &wave(100));
    let french = fuz::write(1, &[], &wave(200));
    std::fs::write(
        data.join("Skyrim - Voices_en0.bsa"),
        bsa("sound/voice/archive.fuz", &english),
    )
    .unwrap();
    std::fs::write(
        data.join("Skyrim - Voices_fr0.bsa"),
        bsa("sound/voice/archive.fuz", &french),
    )
    .unwrap();
    std::fs::create_dir_all(data.join("sound/voice")).unwrap();
    std::fs::write(
        data.join("sound/voice/loose.fuz"),
        fuz::write(1, &[], &wave(300)),
    )
    .unwrap();
    std::fs::create_dir_all(mod_root.join("sound/voice")).unwrap();
    let loose = fuz::write(1, &[], &wave(400));
    std::fs::write(mod_root.join("sound/voice/loose.fuz"), &loose).unwrap();
    let archives = vec![
        ArchiveRef {
            name: "Skyrim - Voices_en0.bsa".into(),
            path: Some(data.join("Skyrim - Voices_en0.bsa")),
            owner: None,
        },
        ArchiveRef {
            name: "Skyrim - Voices_fr0.bsa".into(),
            path: Some(data.join("Skyrim - Voices_fr0.bsa")),
            owner: None,
        },
    ];
    let vfs = vfs_from_loadout(&loadout("FRENCH", vec![data, mod_root], archives)).unwrap();

    assert_eq!(vfs.read("sound/voice/archive.fuz").unwrap(), french);
    assert_eq!(vfs.read("sound/voice/loose.fuz").unwrap(), loose);
    let voice = SoundSource::new(&vfs)
        .read("sound/voice/archive.fuz")
        .unwrap();
    assert_eq!(voice.pcm.samples.len(), 1);
    assert!(voice.pcm.samples[0] > 0.005);
    let mono = to_mono(&voice.pcm);
    assert_eq!(to_i16_bytes(&mono).len(), 2);
}

#[test]
fn missing_selected_locale_does_not_fall_back_to_english() {
    let temp = tempfile::tempdir().unwrap();
    let english = temp.path().join("Skyrim - Voices_en0.bsa");
    std::fs::write(
        &english,
        bsa("sound/voice/archive.fuz", &fuz::write(1, &[], &wave(100))),
    )
    .unwrap();
    let archives = vec![
        ArchiveRef {
            name: "Skyrim - Voices_en0.bsa".into(),
            path: Some(english),
            owner: None,
        },
        ArchiveRef {
            name: "Skyrim - Voices_fr0.bsa".into(),
            path: None,
            owner: None,
        },
    ];
    let error = vfs_from_loadout(&loadout(
        "FRENCH",
        vec![temp.path().to_path_buf()],
        archives,
    ))
    .unwrap_err();
    assert!(
        matches!(error, DataSourceError::MissingVoiceArchive(name) if name == "Skyrim - Voices_fr0.bsa")
    );
}

#[test]
fn data_source_open_uses_the_exact_data_directory_without_voice_assets() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("custom-data");
    std::fs::create_dir_all(&data).unwrap();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            data.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }

    DataSource::open(&data).expect("base masters without an INI or voice archives open");
}
