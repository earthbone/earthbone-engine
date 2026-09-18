//! Writes one minimal, valid fixture per fuzz target into `fuzz/corpus/<target>/`.
//!
//! Only `soe_bsa`'s tests write synthetic fixtures to disk, and that builder
//! (`archive::testdata`) is `#[cfg(test)] pub(crate)`, so it is not reachable
//! from this separate `fuzz` crate. The other crates build their synthetic
//! fixtures as in-memory byte vectors inside `#[cfg(test)]` modules, also not
//! reachable here. Where a crate exposes an equivalent builder outside
//! `#[cfg(test)]` (dds's `Header::to_bytes`, papyrus's `asm::ScriptBuilder`,
//! esm's `FileHeader::to_record` through `Plugin::new`), this binary uses it.
//! For bsa, havok and nif, no such non-test builder exists, so the bytes
//! below are hand assembled straight from the format read by each parser
//! (cited by source line in the comments), kept minimal on purpose: BSA and
//! havok files whose record and section counts are zero, and an empty NIF
//! block list. See docs/fuzz.md for the corpus rationale.

use std::fs;
use std::path::Path;

fn write_seed(target: &str, name: &str, bytes: &[u8]) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join(target);
    fs::create_dir_all(&dir).expect("create corpus dir");
    fs::write(dir.join(name), bytes).expect("write seed");
}

fn esm_seed() -> Vec<u8> {
    // soe_esm::plugin::FileHeader::to_record + Plugin::new + Plugin::to_bytes
    // is the crate's own non-test builder path for an empty, valid plugin.
    let header = soe_esm::FileHeader::default();
    soe_esm::Plugin::new(header).to_bytes()
}

fn bsa_seed() -> Vec<u8> {
    // crates/bsa/src/archive.rs, parse_bsa: magic "BSA\0", u32 version,
    // u32 folder_record_offset (conventionally 36), u32 archive_flags,
    // u32 folder_count, u32 file_count, u32 total_folder_name_len,
    // u32 total_file_name_len, u32 file_flags. With archive_flags 0 (no
    // BSA_FLAG_INCLUDE_FILE_NAMES) the reader returns a valid empty index
    // right after this 36-byte header.
    let mut b = Vec::with_capacity(36);
    b.extend_from_slice(b"BSA\0");
    b.extend_from_slice(&105u32.to_le_bytes()); // SkyrimSe
    b.extend_from_slice(&36u32.to_le_bytes()); // folder_record_offset
    b.extend_from_slice(&0u32.to_le_bytes()); // archive_flags
    b.extend_from_slice(&0u32.to_le_bytes()); // folder_count
    b.extend_from_slice(&0u32.to_le_bytes()); // file_count
    b.extend_from_slice(&0u32.to_le_bytes()); // total_folder_name_len
    b.extend_from_slice(&0u32.to_le_bytes()); // total_file_name_len
    b.extend_from_slice(&0u32.to_le_bytes()); // file_flags
    b
}

fn nif_seed() -> Vec<u8> {
    // crates/nif/src/header.rs read_header, with block_count 0 so the block
    // and footer sections that follow (crates/nif/src/blocks.rs) are empty.
    let mut b = Vec::new();
    b.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    b.extend_from_slice(&soe_nif::header::VERSION_20_2_0_7.to_le_bytes());
    b.push(1); // endian: little
    b.extend_from_slice(&12u32.to_le_bytes()); // user_version
    b.extend_from_slice(&0u32.to_le_bytes()); // block_count
    b.extend_from_slice(&soe_nif::header::BS_SKYRIM_SE.to_le_bytes()); // bs_version
    b.push(1); // author: ExportString length including NUL
    b.push(0);
    b.push(1); // process_script (bs_version <= 130)
    b.push(0);
    b.push(1); // export_script
    b.push(0);
    // max_filepath is skipped below bs_version 103.
    b.extend_from_slice(&0u16.to_le_bytes()); // block type count
                                              // no block types, no per-block index or sizes (block_count is 0)
    b.extend_from_slice(&0u32.to_le_bytes()); // string_count
    b.extend_from_slice(&0u32.to_le_bytes()); // max_string_length
                                              // no strings
    b.extend_from_slice(&0u32.to_le_bytes()); // group_count
                                              // no groups
    b.extend_from_slice(&0u32.to_le_bytes()); // footer: num_roots
    b
}

fn havok_seed() -> Vec<u8> {
    // crates/havok/src/packfile.rs Packfile::parse: a 64-byte header (two
    // magics, pointer_size 8, little_endian, a 20-byte section_count and
    // section-table geometry, an empty version string, section_offset 0 so
    // the table starts right at byte 64) followed by two 48-byte section
    // headers, `__classnames__` and `__data__`, both zero length so every
    // fixup ordering check (local <= global <= virtual <= exports <= end)
    // holds trivially at all zero.
    const HEADER_SIZE: usize = 64;
    const SECTION_HEADER_SIZE: usize = 48;
    let mut b = vec![0u8; HEADER_SIZE + 2 * SECTION_HEADER_SIZE];
    b[0..4].copy_from_slice(&soe_havok::packfile::MAGIC0.to_le_bytes());
    b[4..8].copy_from_slice(&soe_havok::packfile::MAGIC1.to_le_bytes());
    // user_tag, file_version: leave 0.
    b[16] = 8; // pointer_size
    b[17] = 1; // little_endian
               // padding_option, base_class: leave 0.
    b[20..24].copy_from_slice(&2i32.to_le_bytes()); // section_count
                                                    // contents_section_index/offset and contents_class_name_section_*: leave 0.
                                                    // version_string bytes[40..56]: leave zeroed, read as an empty string.
                                                    // flags bytes[56..60]: leave 0.
                                                    // section_offset bytes[62..64]: leave 0, table starts at HEADER_SIZE.

    let data_start = (HEADER_SIZE + 2 * SECTION_HEADER_SIZE) as u32;
    let write_section = |b: &mut [u8], at: usize, tag: &[u8]| {
        b[at..at + tag.len()].copy_from_slice(tag);
        b[at + 20..at + 24].copy_from_slice(&data_start.to_le_bytes()); // absolute_data_start
                                                                        // local/global/virtual fixups, exports, imports, end offsets: leave 0.
    };
    write_section(&mut b, HEADER_SIZE, b"__classnames__");
    write_section(&mut b, HEADER_SIZE + SECTION_HEADER_SIZE, b"__data__");
    b
}

fn dds_seed() -> Vec<u8> {
    // Same shape as crates/dds/src/lib.rs's own `synthetic()` test helper:
    // a DXT1 (BC1) header for one 4x4 mip, followed by one solid-red block.
    use soe_dds::header::{caps, flags, pf, Header, PixelFormat};
    let header = Header {
        flags: flags::CAPS | flags::HEIGHT | flags::WIDTH | flags::PIXELFORMAT | flags::MIPMAPCOUNT,
        height: 4,
        width: 4,
        pitch_or_linear_size: 0,
        depth: 0,
        mip_map_count: 1,
        pixel_format: PixelFormat {
            flags: pf::FOURCC,
            four_cc: *b"DXT1",
            rgb_bit_count: 0,
            r_mask: 0,
            g_mask: 0,
            b_mask: 0,
            a_mask: 0,
        },
        caps: caps::TEXTURE,
        caps2: 0,
        dx10: None,
    };
    let mut b = header.to_bytes();
    b.extend_from_slice(&[0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0]); // one BC1 block
    b
}

fn pex_seed() -> Vec<u8> {
    // soe_papyrus::asm::ScriptBuilder is the crate's own non-test builder
    // for a minimal, valid compiled script.
    soe_papyrus::asm::ScriptBuilder::new("SoeFuzzSeed", "Form").bytes()
}

fn main() {
    write_seed("fuzz-esm", "seed_default.bin", &esm_seed());
    write_seed("fuzz-bsa", "seed_empty.bin", &bsa_seed());
    write_seed("fuzz-nif", "seed_empty.bin", &nif_seed());
    write_seed("fuzz-havok", "seed_empty.bin", &havok_seed());
    write_seed("fuzz-dds", "seed_bc1.bin", &dds_seed());
    write_seed("fuzz-pex", "seed_default.bin", &pex_seed());
    println!("seeds written");
}
