// SPDX-License-Identifier: GPL-3.0-or-later

//! Tail measurements against the SOEW versioned reader in world/save.rs.

/// Return the number of trailing bytes a versioned reader rejects before the
/// first prefix it accepts. The callback should parse with a fresh fixture.
#[allow(dead_code)]
pub fn version_tail_len(bytes: &[u8], accepts: impl Fn(&[u8]) -> bool) -> usize {
    (0..=bytes.len())
        .find(|cut| accepts(&bytes[..bytes.len() - cut]))
        .expect("a legacy save prefix is accepted")
}

/// Set the old header and cut the tail measured by a fresh versioned reader.
/// Callers remove incompatible fields inside records before calling this.
#[allow(dead_code)]
pub fn truncate_to_version(
    bytes: &mut Vec<u8>,
    version: u16,
    accepts: impl Fn(&[u8]) -> bool,
) -> usize {
    bytes[4..6].copy_from_slice(&version.to_le_bytes());
    let tail = version_tail_len(bytes, accepts);
    bytes.truncate(bytes.len() - tail);
    tail
}
