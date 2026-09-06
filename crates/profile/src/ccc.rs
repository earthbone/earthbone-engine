// SPDX-License-Identifier: GPL-3.0-or-later

//! What loads before the user's list has a say: the five hardcoded masters,
//! then the Creation Club files named in `Skyrim.ccc` in the game folder,
//! in the order that file gives them. A name that is not on disk is simply
//! not loaded, the shipped `Skyrim.ccc` names creations the install may not
//! own.

use std::collections::HashSet;
use std::path::Path;

/// Loaded first, in this order, whatever `plugins.txt` says.
pub const HARDCODED_SE: &[&str] = &[
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

/// One name per line, blank lines dropped, CRLF tolerated.
pub fn parse_ccc(text: &str) -> Vec<String> {
    text.strip_prefix('\u{feff}')
        .unwrap_or(text)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// An absent or unreadable list is an empty one, it must not stop a launch.
pub fn read_ccc(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|t| parse_ccc(&t))
        .unwrap_or_default()
}

/// Hardcoded masters followed by the creations, first mention winning.
pub fn early_loading(ccc: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    HARDCODED_SE
        .iter()
        .map(|s| (*s).to_string())
        .chain(ccc.iter().cloned())
        .filter(|n| seen.insert(n.to_ascii_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_blank_lines_and_duplicates_are_handled() {
        let list = parse_ccc("ccA.esm\r\n\r\nccB.esl\r\nSkyrim.esm\r\n  \r\nccA.esm\r\n");
        assert_eq!(list, vec!["ccA.esm", "ccB.esl", "Skyrim.esm", "ccA.esm"]);
        let early = early_loading(&list);
        assert_eq!(early.len(), HARDCODED_SE.len() + 2);
        assert_eq!(early[0], "Skyrim.esm");
        assert_eq!(early[5], "ccA.esm");
        assert_eq!(early[6], "ccB.esl");
    }

    #[test]
    fn a_missing_list_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_ccc(&dir.path().join("Skyrim.ccc")).is_empty());
    }
}
