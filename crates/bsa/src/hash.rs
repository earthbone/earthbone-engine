// SPDX-License-Identifier: GPL-3.0-or-later
// Ported from linux-mod-organizer `src/wabbajack/bsa_writer.rs`,
// Copyright (C) 2026 Dr. Baklava (cd4u2b0z), GPL-3.0-or-later.

//! The TES4 name hash used by BSA v103 to v105 folder and file records.
//!
//! The archives store this hash next to every name and the game looks names up
//! by it, so a reader that wants to verify an archive, and a writer that wants
//! the game to find its files, must compute the same value bit for bit. The
//! reference is Wabbajack's `GetBSAHash`, quirks included: the middle loop stops
//! two bytes short of the end of the name, and the extension marks live in the
//! low word.

/// Hash of a lowercase name and its extension (with the leading dot).
#[must_use]
pub fn tes4_hash(name: &str, ext: &str) -> u64 {
    let name = name.to_lowercase();
    let ext = ext.to_lowercase();
    let (n, _, _) = encoding_rs::WINDOWS_1252.encode(&name);
    let (ext_bytes, _, _) = encoding_rs::WINDOWS_1252.encode(&ext);
    let n = n.as_ref();
    if n.is_empty() {
        return 0;
    }
    let len = n.len();
    let hash_bytes = [
        n[len - 1],
        if len < 3 { 0 } else { n[len - 2] },
        len as u8,
        n[0],
    ];
    let mut hash1 = u32::from_le_bytes(hash_bytes);
    match ext.as_str() {
        ".kf" => hash1 |= 0x80,
        ".nif" => hash1 |= 0x8000,
        ".dds" => hash1 |= 0x8080,
        ".wav" => hash1 |= 0x8000_0000,
        _ => {}
    }
    let mut hash2: u32 = 0;
    let mut i = 1;
    while i + 2 < len {
        hash2 = hash2.wrapping_mul(0x1003f).wrapping_add(u32::from(n[i]));
        i += 1;
    }
    let mut hash3: u32 = 0;
    for &c in ext_bytes.as_ref() {
        hash3 = hash3.wrapping_mul(0x1003f).wrapping_add(u32::from(c));
    }
    (u64::from(hash2.wrapping_add(hash3)) << 32) + u64::from(hash1)
}

/// Hash of a file name without its folder, extension split off at the last dot.
#[must_use]
pub fn file_name_hash(file_name: &str) -> u64 {
    let file_name = file_name.replace('/', "\\");
    match file_name.rfind('.') {
        Some(i) if i > 0 => tes4_hash(&file_name[..i], &file_name[i..]),
        _ => tes4_hash(&file_name, ""),
    }
}

/// Hash of a folder path. Stored with backslashes and no extension part.
#[must_use]
pub fn folder_hash(folder: &str) -> u64 {
    tes4_hash(&folder.replace('/', "\\"), "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_marks_land_in_the_low_word() {
        assert_eq!(tes4_hash("ab", ".nif") & 0xff00, 0x8000);
        assert_eq!(
            tes4_hash("abc", ".nif") & 0xff00,
            0x8000 | ((b'b' as u64) << 8)
        );
        assert_eq!(tes4_hash("abc", ".kf") & 0xff, b'c' as u64 | 0x80);
        assert_eq!(tes4_hash("abc", ".wav") >> 31 & 1, 1);
        assert_eq!(tes4_hash("", ".nif"), 0);
    }

    #[test]
    fn the_middle_loop_skips_the_last_two_bytes() {
        // "abcd": hash2 covers only 'b'. "abzd" must therefore differ, "abcz"
        // and "abcd" must agree in the high word.
        let high = |s: &str| tes4_hash(s, "") >> 32;
        assert_eq!(high("abcd"), high("abcz"));
        assert_ne!(high("abcd"), high("azcd"));
        assert_eq!(high("abcd"), b'b' as u64);
    }

    #[test]
    fn names_are_case_folded_and_slashes_normalised() {
        assert_eq!(folder_hash("Meshes/Actors"), folder_hash("meshes\\actors"));
        assert_eq!(file_name_hash("Body.NIF"), tes4_hash("body", ".nif"));
        assert_eq!(file_name_hash(".hidden"), tes4_hash(".hidden", ""));
        assert_eq!(file_name_hash("noext"), tes4_hash("noext", ""));
    }

    #[test]
    fn fmt_14_windows_1252_names_match_the_reference() {
        assert_eq!(file_name_hash("María_F.fuz"), 0x690E_0782_6D07_5F66);
    }
}
