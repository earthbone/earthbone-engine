// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The real archives, when `SKYRIM_DATA` points at a Skyrim SE `Data/` folder.
//! Skips cleanly otherwise. Indexes every archive, reads one file out of each
//! and checks a few files vanilla is known to ship, so a format regression
//! shows up against the real thing and not only against synthetic fixtures.

use std::path::PathBuf;

use soe_bsa::{scan_archives, Archive, ArchiveFormat, BsaVersion, Vfs};

fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    if dir.join("Skyrim.esm").is_file() {
        Some(dir)
    } else {
        eprintln!("SKYRIM_DATA={} has no Skyrim.esm, skipping", dir.display());
        None
    }
}

#[test]
fn every_vanilla_archive_indexes_and_reads() {
    let Some(dir) = data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping the real-archive test");
        return;
    };
    let scan = scan_archives(&dir).unwrap();
    assert!(
        scan.failed.is_empty(),
        "archives that failed to index: {:?}",
        scan.failed
    );
    assert!(
        scan.archives.len() >= 10,
        "found {} archives",
        scan.archives.len()
    );

    let mut total_files = 0usize;
    for (path, index) in &scan.archives {
        assert_eq!(
            index.format,
            ArchiveFormat::Bsa(BsaVersion::SkyrimSe),
            "{}",
            path.display()
        );
        assert_eq!(
            index.files.len(),
            index.file_count as usize,
            "{}",
            path.display()
        );
        total_files += index.files.len();

        // Reading one file from each archive exercises both the LZ4 frame
        // path and the stored path across all of them.
        let archive = Archive::open(path).unwrap();
        let bytes = archive.read_record(0).unwrap();
        assert!(!bytes.is_empty(), "{} record 0 is empty", path.display());
        let record = &archive.index().records[0];
        if record.compressed && record.uncompressed_size > 0 {
            assert_eq!(
                bytes.len(),
                record.uncompressed_size as usize,
                "{}",
                path.display()
            );
        }
        eprintln!(
            "{}: {} files, first {} ({} bytes)",
            path.file_name().unwrap().to_string_lossy(),
            index.files.len(),
            record.path,
            bytes.len()
        );
    }
    eprintln!("total files across archives: {total_files}");
    assert!(
        total_files > 100_000,
        "vanilla ships well over 100k files, saw {total_files}"
    );
}

#[test]
fn the_vfs_answers_vanilla_paths_case_insensitively() {
    let Some(dir) = data_dir() else {
        return;
    };
    let mut vfs = Vfs::new();
    for name in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Textures0.bsa",
        "Skyrim - Interface.bsa",
        "Skyrim - Misc.bsa",
    ] {
        vfs.add_archive_path(&dir.join(name)).unwrap();
    }
    // Files every Skyrim SE install has, spelled the way plugins spell them.
    let nif = vfs.read("Meshes\\Clutter\\Bucket01.nif").unwrap();
    assert_eq!(&nif[..20], b"Gamebryo File Format");
    // Texture archives set the embed-names flag, so this exercises that path.
    let dds = vfs.read("Textures/Actors/Dog/Dog.dds").unwrap();
    assert_eq!(&dds[..4], b"DDS ");
    let gfx = vfs.read("interface/exported/hudmenu.gfx").unwrap();
    assert!(
        &gfx[..3] == b"CFX" || &gfx[..3] == b"GFX",
        "{:?}",
        &gfx[..3]
    );
    // Skyrim PEX files are big-endian, magic FA57C0DE.
    let script = vfs.read("scripts/actor.pex").unwrap();
    assert_eq!(&script[..4], &[0xFA, 0x57, 0xC0, 0xDE], "PEX magic");
    assert!(vfs.len() > 50_000);
}
