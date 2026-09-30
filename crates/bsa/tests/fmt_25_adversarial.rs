// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_bsa::{write_bsa, Archive, BsaFile, BsaVersion};

#[test]
fn empty_payload_and_root_file_survive_all_compression_versions() {
    let files = [
        BsaFile {
            path: "empty.bin",
            bytes: b"",
        },
        BsaFile {
            path: r"meshes\zero.nif",
            bytes: b"\0\0\xff\0payload\0",
        },
    ];

    for version in [
        BsaVersion::Oblivion,
        BsaVersion::Fo3Skyrim,
        BsaVersion::SkyrimSe,
    ] {
        let bytes = write_bsa(&files, version).unwrap();
        let archive = Archive::from_bytes(Path::new("edge.bsa"), bytes).unwrap();
        assert_eq!(archive.files().len(), 2, "{version:?}");
        assert_eq!(archive.read("empty.bin").unwrap(), b"", "{version:?}");
        assert_eq!(
            archive.read(r"meshes\zero.nif").unwrap(),
            b"\0\0\xff\0payload\0",
            "{version:?}"
        );
    }
}
