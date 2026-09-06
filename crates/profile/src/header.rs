// SPDX-License-Identifier: GPL-3.0-or-later

//! Just enough of a plugin to order it: its masters and its flags.
//!
//! The engine decides a file's kind from both the header flags and the
//! extension. `.esm` is a master whatever the flags say, `.esl` is a master
//! and light whatever the flags say, and an `.esp` is whatever its flags say
//! (an ESM-flagged `.esp` loads with the masters, an ESL-flagged `.esp` takes
//! a light slot in its plugin position). Only the TES4 record is read, so a
//! Skyrim.esm header costs a few kilobytes, not the file.

use std::io::Read;
use std::path::{Path, PathBuf};

use soe_esm::Plugin;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginHeader {
    /// File name as found on disk, case preserved.
    pub name: String,
    pub path: PathBuf,
    pub masters: Vec<String>,
    pub master_flag: bool,
    pub light_flag: bool,
    pub localized: bool,
}

fn ext_is(name: &str, ext: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// Whether a file name is one the engine would consider a plugin at all.
pub fn is_plugin_name(name: &str) -> bool {
    ext_is(name, "esm") || ext_is(name, "esp") || ext_is(name, "esl")
}

impl PluginHeader {
    pub fn read(path: &Path) -> Result<PluginHeader> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let io = |source| Error::Io {
            path: path.to_path_buf(),
            source,
        };
        let mut file = std::fs::File::open(path).map_err(io)?;
        let mut head = [0u8; 24];
        file.read_exact(&mut head).map_err(|_| Error::Truncated {
            path: path.to_path_buf(),
        })?;
        if &head[..4] != b"TES4" {
            // Not a plugin at all: let the parser name what it found.
            return Self::from_bytes(&name, path, &head);
        }
        let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let mut bytes = head.to_vec();
        file.take(size as u64)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes.len() < 24 + size {
            return Err(Error::Truncated {
                path: path.to_path_buf(),
            });
        }
        Self::from_bytes(&name, path, &bytes)
    }

    /// From the bytes of the TES4 record (a whole file works too, the groups
    /// after the header are parsed and dropped).
    pub fn from_bytes(name: &str, path: &Path, bytes: &[u8]) -> Result<PluginHeader> {
        let plugin = Plugin::parse(bytes).map_err(|source| Error::Header {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(PluginHeader {
            name: name.to_string(),
            path: path.to_path_buf(),
            masters: plugin.header.masters.clone(),
            master_flag: plugin.header.is_master(),
            light_flag: plugin.header.is_light(),
            localized: plugin.header.is_localized(),
        })
    }

    /// Loads with the masters, before every plain plugin.
    pub fn is_master(&self) -> bool {
        self.master_flag || ext_is(&self.name, "esm") || ext_is(&self.name, "esl")
    }

    /// Takes a light slot instead of a top-byte one.
    pub fn is_light(&self) -> bool {
        self.light_flag || ext_is(&self.name, "esl")
    }

    /// The name without its extension, which names the plugin's INI and
    /// archives.
    pub fn stem(&self) -> &str {
        Path::new(&self.name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::plugin::{flags, FileHeader};

    fn bytes(masters: &[&str], flags: u32) -> Vec<u8> {
        Plugin::new(FileHeader {
            masters: masters.iter().map(|m| m.to_string()).collect(),
            flags,
            ..Default::default()
        })
        .to_bytes()
    }

    #[test]
    fn reads_only_the_header_of_a_file_with_more_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Mod.esp");
        let mut data = bytes(&["Skyrim.esm", "Update.esm"], 0);
        data.extend_from_slice(b"GRUPgarbage that is not parsed because only the header is read");
        std::fs::write(&path, &data).unwrap();
        let h = PluginHeader::read(&path).unwrap();
        assert_eq!(h.name, "Mod.esp");
        assert_eq!(h.masters, vec!["Skyrim.esm", "Update.esm"]);
        assert!(!h.is_master());
        assert!(!h.is_light());
        assert_eq!(h.stem(), "Mod");
    }

    #[test]
    fn the_extension_and_the_flags_both_decide_the_kind() {
        let dir = tempfile::tempdir().unwrap();
        let plain = bytes(&[], 0);
        let cases = [
            ("A.esm", 0, true, false),
            ("B.esl", 0, true, true),
            ("C.esp", flags::MASTER, true, false),
            ("D.esp", flags::LIGHT_MASTER, false, true),
            ("E.ESP", flags::MASTER | flags::LIGHT_MASTER, true, true),
        ];
        for (name, fl, master, light) in cases {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes(&[], fl)).unwrap();
            let h = PluginHeader::read(&path).unwrap();
            assert_eq!(h.is_master(), master, "{name}");
            assert_eq!(h.is_light(), light, "{name}");
        }
        assert!(is_plugin_name("x.ESL"));
        assert!(!is_plugin_name("x.bsa"));
        let _ = plain;
    }

    #[test]
    fn truncated_and_foreign_files_are_typed_errors() {
        let dir = tempfile::tempdir().unwrap();
        let short = dir.path().join("short.esp");
        std::fs::write(&short, b"TES4").unwrap();
        assert!(matches!(PluginHeader::read(&short), Err(Error::Truncated { .. })));
        let cut = dir.path().join("cut.esp");
        let full = bytes(&["Skyrim.esm"], 0);
        std::fs::write(&cut, &full[..full.len() - 3]).unwrap();
        assert!(matches!(PluginHeader::read(&cut), Err(Error::Truncated { .. })));
        let foreign = dir.path().join("foreign.esp");
        std::fs::write(&foreign, b"BSA\0this is not a plugin at all, padding padding").unwrap();
        assert!(matches!(PluginHeader::read(&foreign), Err(Error::Header { .. })));
        assert!(matches!(
            PluginHeader::read(&dir.path().join("absent.esp")),
            Err(Error::Io { .. })
        ));
    }
}
