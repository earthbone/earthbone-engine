// SPDX-License-Identifier: GPL-3.0-or-later

//! The 124-byte DDS header and its DX10 extension, as Microsoft documents
//! them. Reading validates only what later stages depend on, so a file with
//! odd flags but a sane size still opens, the way the game's own loader
//! tolerates the output of a decade of texture tools.

use crate::error::{Error, Result};

pub const MAGIC: &[u8; 4] = b"DDS ";
pub const HEADER_SIZE: usize = 124;
pub const DX10_SIZE: usize = 20;

pub mod flags {
    pub const CAPS: u32 = 0x1;
    pub const HEIGHT: u32 = 0x2;
    pub const WIDTH: u32 = 0x4;
    pub const PITCH: u32 = 0x8;
    pub const PIXELFORMAT: u32 = 0x1000;
    pub const MIPMAPCOUNT: u32 = 0x2_0000;
    pub const LINEARSIZE: u32 = 0x8_0000;
    pub const DEPTH: u32 = 0x80_0000;
}

pub mod pf {
    pub const ALPHAPIXELS: u32 = 0x1;
    pub const ALPHA: u32 = 0x2;
    pub const FOURCC: u32 = 0x4;
    pub const RGB: u32 = 0x40;
    pub const YUV: u32 = 0x200;
    pub const LUMINANCE: u32 = 0x2_0000;
    pub const BUMPDUDV: u32 = 0x8_0000;
}

pub mod caps {
    pub const COMPLEX: u32 = 0x8;
    pub const TEXTURE: u32 = 0x1000;
    pub const MIPMAP: u32 = 0x40_0000;
    pub const CUBEMAP: u32 = 0x200;
    pub const CUBEMAP_ALL_FACES: u32 = 0xFC00;
    pub const VOLUME: u32 = 0x20_0000;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelFormat {
    pub flags: u32,
    pub four_cc: [u8; 4],
    pub rgb_bit_count: u32,
    pub r_mask: u32,
    pub g_mask: u32,
    pub b_mask: u32,
    pub a_mask: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dx10 {
    pub dxgi_format: u32,
    /// 2 for 1D, 3 for 2D, 4 for 3D, as D3D10_RESOURCE_DIMENSION.
    pub dimension: u32,
    pub misc_flag: u32,
    pub array_size: u32,
    pub misc_flags2: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub flags: u32,
    pub height: u32,
    pub width: u32,
    pub pitch_or_linear_size: u32,
    pub depth: u32,
    pub mip_map_count: u32,
    pub pixel_format: PixelFormat,
    pub caps: u32,
    pub caps2: u32,
    pub dx10: Option<Dx10>,
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl Header {
    /// Parse the magic, header and DX10 extension. Returns the header and
    /// the offset of the first level's data.
    pub fn parse(bytes: &[u8]) -> Result<(Header, usize)> {
        if bytes.len() < 4 || &bytes[..4] != MAGIC {
            return Err(Error::NotDds);
        }
        if bytes.len() < 4 + HEADER_SIZE {
            return Err(Error::BadHeader("file shorter than the header"));
        }
        let h = &bytes[4..4 + HEADER_SIZE];
        if u32_at(h, 0) != HEADER_SIZE as u32 {
            return Err(Error::BadHeader("header size is not 124"));
        }
        let pfo = 72;
        if u32_at(h, pfo) != 32 {
            return Err(Error::BadHeader("pixel format size is not 32"));
        }
        let pixel_format = PixelFormat {
            flags: u32_at(h, pfo + 4),
            four_cc: [h[pfo + 8], h[pfo + 9], h[pfo + 10], h[pfo + 11]],
            rgb_bit_count: u32_at(h, pfo + 12),
            r_mask: u32_at(h, pfo + 16),
            g_mask: u32_at(h, pfo + 20),
            b_mask: u32_at(h, pfo + 24),
            a_mask: u32_at(h, pfo + 28),
        };
        let mut header = Header {
            flags: u32_at(h, 4),
            height: u32_at(h, 8),
            width: u32_at(h, 12),
            pitch_or_linear_size: u32_at(h, 16),
            depth: u32_at(h, 20),
            mip_map_count: u32_at(h, 24),
            pixel_format,
            caps: u32_at(h, 104),
            caps2: u32_at(h, 108),
            dx10: None,
        };
        let mut offset = 4 + HEADER_SIZE;
        if header.pixel_format.flags & pf::FOURCC != 0 && &header.pixel_format.four_cc == b"DX10" {
            if bytes.len() < offset + DX10_SIZE {
                return Err(Error::BadHeader("DX10 header missing"));
            }
            let d = &bytes[offset..offset + DX10_SIZE];
            header.dx10 = Some(Dx10 {
                dxgi_format: u32_at(d, 0),
                dimension: u32_at(d, 4),
                misc_flag: u32_at(d, 8),
                array_size: u32_at(d, 12),
                misc_flags2: u32_at(d, 16),
            });
            offset += DX10_SIZE;
        }
        if header.width == 0 || header.height == 0 {
            return Err(Error::BadHeader("zero width or height"));
        }
        Ok((header, offset))
    }

    /// Serialize back to bytes, magic included. The inverse of `parse` for
    /// every field it reads.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + HEADER_SIZE + DX10_SIZE);
        out.extend_from_slice(MAGIC);
        let w = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        w(&mut out, HEADER_SIZE as u32);
        w(&mut out, self.flags);
        w(&mut out, self.height);
        w(&mut out, self.width);
        w(&mut out, self.pitch_or_linear_size);
        w(&mut out, self.depth);
        w(&mut out, self.mip_map_count);
        for _ in 0..11 {
            w(&mut out, 0);
        }
        let p = &self.pixel_format;
        w(&mut out, 32);
        w(&mut out, p.flags);
        out.extend_from_slice(&p.four_cc);
        w(&mut out, p.rgb_bit_count);
        w(&mut out, p.r_mask);
        w(&mut out, p.g_mask);
        w(&mut out, p.b_mask);
        w(&mut out, p.a_mask);
        w(&mut out, self.caps);
        w(&mut out, self.caps2);
        w(&mut out, 0);
        w(&mut out, 0);
        w(&mut out, 0);
        if let Some(d) = &self.dx10 {
            w(&mut out, d.dxgi_format);
            w(&mut out, d.dimension);
            w(&mut out, d.misc_flag);
            w(&mut out, d.array_size);
            w(&mut out, d.misc_flags2);
        }
        debug_assert_eq!(
            out.len(),
            4 + HEADER_SIZE + if self.dx10.is_some() { DX10_SIZE } else { 0 }
        );
        out
    }

    /// Mip levels per face, at least one. A zero count means "no mips".
    pub fn mip_count(&self) -> u32 {
        if self.flags & flags::MIPMAPCOUNT != 0 || self.caps & caps::MIPMAP != 0 {
            self.mip_map_count.max(1)
        } else {
            1
        }
    }

    pub fn is_cubemap(&self) -> bool {
        self.caps2 & caps::CUBEMAP != 0
            || self.dx10.as_ref().is_some_and(|d| d.misc_flag & 0x4 != 0)
    }

    /// Faces present for a cube map. Legacy files may carry fewer than six.
    pub fn face_count(&self) -> u32 {
        if !self.is_cubemap() {
            return 1;
        }
        if self.dx10.is_some() {
            return 6;
        }
        let faces = (self.caps2 & caps::CUBEMAP_ALL_FACES) >> 10;
        faces.count_ones().max(1)
    }

    /// Layers in the file: array size times faces.
    pub fn layer_count(&self) -> u32 {
        let arrays = self.dx10.as_ref().map(|d| d.array_size.max(1)).unwrap_or(1);
        arrays * self.face_count()
    }

    pub fn depth(&self) -> u32 {
        if self.flags & flags::DEPTH != 0 || self.caps2 & caps::VOLUME != 0 {
            self.depth.max(1)
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_round_trips_with_and_without_dx10() {
        let mut h = Header {
            flags: flags::CAPS
                | flags::HEIGHT
                | flags::WIDTH
                | flags::PIXELFORMAT
                | flags::MIPMAPCOUNT,
            height: 8,
            width: 16,
            pitch_or_linear_size: 0,
            depth: 0,
            mip_map_count: 5,
            pixel_format: PixelFormat {
                flags: pf::FOURCC,
                four_cc: *b"DXT1",
                rgb_bit_count: 0,
                r_mask: 0,
                g_mask: 0,
                b_mask: 0,
                a_mask: 0,
            },
            caps: caps::TEXTURE | caps::MIPMAP,
            caps2: 0,
            dx10: None,
        };
        let (back, off) = Header::parse(&h.to_bytes()).unwrap();
        assert_eq!(back, h);
        assert_eq!(off, 128);
        assert_eq!(back.mip_count(), 5);
        assert_eq!(back.layer_count(), 1);
        h.pixel_format.four_cc = *b"DX10";
        h.dx10 = Some(Dx10 {
            dxgi_format: 98,
            dimension: 3,
            misc_flag: 0,
            array_size: 1,
            misc_flags2: 0,
        });
        let (back, off) = Header::parse(&h.to_bytes()).unwrap();
        assert_eq!(back, h);
        assert_eq!(off, 148);
    }

    #[test]
    fn bad_magic_and_short_files_are_typed() {
        assert!(matches!(Header::parse(b"PNG\0"), Err(Error::NotDds)));
        assert!(matches!(
            Header::parse(b"DDS \0\0"),
            Err(Error::BadHeader(_))
        ));
        let mut bytes = vec![0u8; 128];
        bytes[..4].copy_from_slice(MAGIC);
        bytes[4] = 100;
        assert!(matches!(
            Header::parse(&bytes),
            Err(Error::BadHeader("header size is not 124"))
        ));
    }

    #[test]
    fn cube_faces_are_counted_from_caps2() {
        let mut h = Header {
            flags: 0,
            height: 4,
            width: 4,
            pitch_or_linear_size: 0,
            depth: 0,
            mip_map_count: 0,
            pixel_format: PixelFormat {
                flags: 0,
                four_cc: [0; 4],
                rgb_bit_count: 32,
                r_mask: 0,
                g_mask: 0,
                b_mask: 0,
                a_mask: 0,
            },
            caps: 0,
            caps2: caps::CUBEMAP | caps::CUBEMAP_ALL_FACES,
            dx10: None,
        };
        assert_eq!(h.face_count(), 6);
        h.caps2 = caps::CUBEMAP | 0x400 | 0x800;
        assert_eq!(h.face_count(), 2);
        assert_eq!(h.mip_count(), 1);
        assert_eq!(h.depth(), 1);
    }
}
