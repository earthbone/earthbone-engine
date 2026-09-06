// SPDX-License-Identifier: GPL-3.0-or-later

//! Bethesda archive access for the engine: BSA v103 to v105, BA2 GNRL and
//! DX10, and a [`Vfs`] that answers a data path the way the game does, loose
//! file first, then the archives in load order, latest wins.
//!
//! The archive reader is a port of linux-mod-organizer's `src/bsa/parser.rs`
//! (Copyright (C) 2026 Dr. Baklava, GPL-3.0-or-later) with the writer's name
//! hash, an in-memory read API and Windows-1252 names added.

pub mod archive;
pub mod error;
pub mod hash;
pub mod vfs;

pub use archive::{
    extract_archive, extract_file, find_associated_archives, index_archive, index_archive_in,
    is_bsa_or_ba2, read_record, scan_archives, Archive, ArchiveFormat, ArchiveIndex, Ba2Type,
    BsaVersion, DdsChunk, DdsInfo, FileRecord, Scan,
};
pub use error::{Error, Result};
pub use hash::{file_name_hash, folder_hash, tes4_hash};
pub use vfs::{Source, Vfs};

/// The one spelling of a data path every lookup in this crate agrees on:
/// lowercase, forward slashes, no leading separator, no `data/` prefix. The
/// game matches names case-insensitively and plugins write them either way.
pub fn normalize(path: &str) -> String {
    let mut s = path.replace('\\', "/").to_lowercase();
    while s.starts_with('/') {
        s.remove(0);
    }
    if let Some(rest) = s.strip_prefix("data/") {
        s = rest.to_string();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn normalize_folds_case_slashes_and_the_data_prefix() {
        assert_eq!(normalize("Meshes\\Actors\\A.NIF"), "meshes/actors/a.nif");
        assert_eq!(normalize("/Data/textures/t.dds"), "textures/t.dds");
        assert_eq!(normalize("data\\Sound\\x.wav"), "sound/x.wav");
        assert_eq!(normalize("database/x"), "database/x");
    }
}
