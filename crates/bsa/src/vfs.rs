// SPDX-License-Identifier: GPL-3.0-or-later

//! The game's view of `Data/`: loose files over archives, archives in load
//! order with the later one winning.
//!
//! That is the rule the shipped engine applies and every mod author counts
//! on. A texture replacer ships loose files and expects them to beat the
//! vanilla BSA. A patch archive loaded after another expects to replace its
//! meshes. Nothing here consults the plugin list. The caller adds archives in
//! the order the load order and `sResourceArchiveList` produce, and adds loose
//! roots in the order the mod manager stacks them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::archive::Archive;
use crate::error::{Error, Result};
use crate::normalize;

/// Where a path's bytes come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A file on disk, absolute path.
    Loose(PathBuf),
    /// A record inside one of the added archives, by position in each.
    Archived { archive: usize, record: usize },
}

#[derive(Debug, Default)]
pub struct Vfs {
    archives: Vec<Archive>,
    loose_roots: Vec<PathBuf>,
    /// Normalised path to its winning source.
    map: HashMap<String, Source>,
}

impl Vfs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an archive after every archive already added, so its files beat
    /// theirs. Loose files already registered keep winning.
    pub fn add_archive(&mut self, archive: Archive) -> usize {
        let idx = self.archives.len();
        for (record, path) in archive.files().iter().enumerate() {
            let key = normalize(path);
            match self.map.get(&key) {
                Some(Source::Loose(_)) => {}
                _ => {
                    self.map.insert(
                        key,
                        Source::Archived {
                            archive: idx,
                            record,
                        },
                    );
                }
            }
        }
        self.archives.push(archive);
        idx
    }

    /// Open and add an archive.
    pub fn add_archive_path(&mut self, path: &Path) -> Result<usize> {
        Ok(self.add_archive(Archive::open(path)?))
    }

    /// Register every file under `root` as a loose file. Loose files beat every
    /// archive, and a root added later beats one added earlier.
    pub fn add_loose_root(&mut self, root: &Path) -> Result<usize> {
        let root = root.to_path_buf();
        let mut count = 0;
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir)?.flatten() {
                let path = entry.path();
                let Ok(ty) = entry.file_type() else { continue };
                if ty.is_dir() {
                    stack.push(path);
                    continue;
                }
                let rel = match path.strip_prefix(&root) {
                    Ok(r) => r.to_string_lossy().to_string(),
                    Err(_) => continue,
                };
                self.map.insert(normalize(&rel), Source::Loose(path));
                count += 1;
            }
        }
        self.loose_roots.push(root);
        Ok(count)
    }

    #[must_use]
    pub fn archives(&self) -> &[Archive] {
        &self.archives
    }

    #[must_use]
    pub fn loose_roots(&self) -> &[PathBuf] {
        &self.loose_roots
    }

    #[must_use]
    pub fn resolve(&self, path: &str) -> Option<&Source> {
        self.map.get(&normalize(path))
    }

    #[must_use]
    pub fn exists(&self, path: &str) -> bool {
        self.resolve(path).is_some()
    }

    /// The winning bytes for a data path.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let _span = soe_trace::Span::new("bsa.vfs_read", "bsa");
        match self.resolve(path) {
            Some(Source::Loose(p)) => {
                let _io = soe_trace::LoadScope::new(
                    "loose_read",
                    soe_trace::LoadCause::IoWait,
                    p.display().to_string(),
                );
                Ok(std::fs::read(p)?)
            }
            Some(Source::Archived { archive, record }) => {
                self.archives[*archive].read_record(*record)
            }
            None => Err(Error::NotFound {
                name: path.to_string(),
            }),
        }
    }

    /// Every path the view answers, normalised, in no particular order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Paths under a folder prefix, normalised.
    pub fn list(&self, folder: &str) -> Vec<&str> {
        let mut prefix = normalize(folder);
        if !prefix.is_empty() && !prefix.ends_with('/') {
            prefix.push('/');
        }
        let mut out: Vec<&str> = self
            .map
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .map(String::as_str)
            .collect();
        out.sort_unstable();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::testdata::sse_bsa;

    fn archive(name: &str, files: &[(&str, &str, &[u8])]) -> Archive {
        Archive::from_bytes(Path::new(name), sse_bsa(files)).unwrap()
    }

    #[test]
    fn later_archives_win_and_loose_files_beat_them_all() {
        let mut vfs = Vfs::new();
        vfs.add_archive(archive(
            "Skyrim - Meshes0.bsa",
            &[
                ("meshes", "chair.nif", b"vanilla chair"),
                ("meshes", "table.nif", b"vanilla table"),
            ],
        ));
        vfs.add_archive(archive(
            "Patch.bsa",
            &[("meshes", "chair.nif", b"patched chair")],
        ));
        assert_eq!(vfs.read("meshes/chair.nif").unwrap(), b"patched chair");
        assert_eq!(vfs.read("Meshes\\Table.NIF").unwrap(), b"vanilla table");
        assert_eq!(
            vfs.resolve("meshes/chair.nif"),
            Some(&Source::Archived {
                archive: 1,
                record: 0
            })
        );

        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("Meshes")).unwrap();
        std::fs::write(tmp.path().join("Meshes/Chair.nif"), b"loose chair").unwrap();
        assert_eq!(vfs.add_loose_root(tmp.path()).unwrap(), 1);
        assert_eq!(vfs.read("meshes/chair.nif").unwrap(), b"loose chair");

        // An archive added after the loose root still loses to it.
        vfs.add_archive(archive(
            "Late.bsa",
            &[("meshes", "chair.nif", b"late chair")],
        ));
        assert_eq!(vfs.read("meshes/chair.nif").unwrap(), b"loose chair");
        assert!(matches!(
            vfs.resolve("meshes/chair.nif"),
            Some(Source::Loose(_))
        ));

        assert_eq!(
            vfs.list("meshes"),
            vec!["meshes/chair.nif", "meshes/table.nif"]
        );
        assert_eq!(vfs.len(), 2);
    }

    #[test]
    fn a_later_loose_root_beats_an_earlier_one() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for (dir, text) in [(&a, "from a"), (&b, "from b")] {
            std::fs::create_dir_all(dir.path().join("textures/x")).unwrap();
            std::fs::write(dir.path().join("textures/x/t.dds"), text).unwrap();
        }
        let mut vfs = Vfs::new();
        vfs.add_loose_root(a.path()).unwrap();
        vfs.add_loose_root(b.path()).unwrap();
        assert_eq!(vfs.read("textures/x/t.dds").unwrap(), b"from b");
        assert_eq!(vfs.loose_roots().len(), 2);
    }

    #[test]
    fn a_missing_path_is_not_found_and_an_empty_view_says_so() {
        let vfs = Vfs::new();
        assert!(vfs.is_empty());
        assert!(!vfs.exists("meshes/nothing.nif"));
        assert!(matches!(
            vfs.read("meshes/nothing.nif"),
            Err(Error::NotFound { .. })
        ));
    }
}
