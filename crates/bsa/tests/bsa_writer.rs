// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_bsa::{write_bsa, Archive, BsaFile, BsaVersion};

#[test]
fn fmt_25_written_archives_reload_with_identical_names_and_bytes() {
    let contents = [
        (r"Meshes\Actors\A.nif", b"first mesh".as_slice()),
        (r"textures\mar\María.dds", b"texture payload".as_slice()),
        ("root.txt", b"root payload".as_slice()),
        (r"Meshes\Actors\B.nif", b"second mesh".as_slice()),
    ];
    let files: Vec<_> = contents
        .iter()
        .map(|(path, bytes)| BsaFile { path, bytes })
        .collect();

    for version in [
        BsaVersion::Oblivion,
        BsaVersion::Fo3Skyrim,
        BsaVersion::SkyrimSe,
    ] {
        let bytes = write_bsa(&files, version).unwrap();
        let archive = Archive::from_bytes(Path::new("roundtrip.bsa"), bytes).unwrap();
        assert_eq!(archive.files().len(), contents.len());
        let mut expected_names: Vec<_> = contents
            .iter()
            .map(|(path, _)| path.replace('\\', "/"))
            .collect();
        let mut actual_names = archive.files().to_vec();
        expected_names.sort();
        actual_names.sort();
        assert_eq!(actual_names, expected_names, "{version:?}");
        for (path, expected) in contents {
            assert_eq!(archive.read(path).unwrap(), expected, "{version:?}: {path}");
        }
        assert!(archive
            .index()
            .records
            .iter()
            .all(|record| record.compressed));
    }
}

#[test]
fn fmt_25_archive_order_is_hash_sorted_and_independent_of_input_order() {
    let contents = [
        (r"zeta\last.dat", b"z".as_slice()),
        (r"alpha\second.dat", b"2".as_slice()),
        (r"alpha\first.dat", b"1".as_slice()),
        ("root.dat", b"r".as_slice()),
    ];
    let files: Vec<_> = contents
        .iter()
        .map(|(path, bytes)| BsaFile { path, bytes })
        .collect();
    let reversed: Vec<_> = files.iter().rev().copied().collect();

    let first = write_bsa(&files, BsaVersion::SkyrimSe).unwrap();
    let second = write_bsa(&reversed, BsaVersion::SkyrimSe).unwrap();
    assert_eq!(first, second);

    let archive = Archive::from_bytes(Path::new("order.bsa"), first).unwrap();
    let mut expected: Vec<_> = contents
        .iter()
        .map(|(path, _)| {
            (
                soe_bsa::folder_hash(path.rsplit_once('\\').map_or("", |(d, _)| d)),
                *path,
            )
        })
        .collect();
    expected.sort_by_key(|(hash, path)| {
        let name = path.rsplit_once('\\').map_or(*path, |(_, name)| name);
        (*hash, soe_bsa::file_name_hash(name), name.to_lowercase())
    });
    let actual: Vec<_> = archive
        .files()
        .iter()
        .map(|path| {
            let normalized = path.replace('/', "\\");
            let folder = normalized.rsplit_once('\\').map_or("", |(dir, _)| dir);
            (soe_bsa::folder_hash(folder), normalized)
        })
        .collect();
    assert_eq!(
        actual
            .iter()
            .map(|(_, path)| path.as_str())
            .collect::<Vec<_>>(),
        expected.iter().map(|(_, path)| *path).collect::<Vec<_>>()
    );
}

#[test]
fn fmt_25_writer_rejects_unrepresentable_and_duplicate_names() {
    let unrepresentable = [BsaFile {
        path: "meshes/hand_🫠.nif",
        bytes: b"data",
    }];
    assert!(write_bsa(&unrepresentable, BsaVersion::SkyrimSe).is_err());

    let absolute = [BsaFile {
        path: r"C:\meshes\chair.nif",
        bytes: b"data",
    }];
    assert!(write_bsa(&absolute, BsaVersion::SkyrimSe).is_err());

    let duplicate = [
        BsaFile {
            path: r"meshes\Chair.nif",
            bytes: b"one",
        },
        BsaFile {
            path: "Meshes/CHAIR.NIF",
            bytes: b"two",
        },
    ];
    assert!(write_bsa(&duplicate, BsaVersion::SkyrimSe).is_err());
}
