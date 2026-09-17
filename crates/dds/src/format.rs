// SPDX-License-Identifier: GPL-3.0-or-later

//! What the pixels are, decided once from the header. Block formats are
//! named. Everything uncompressed is a set of channel masks, which covers
//! RGBA8, BGRA8, BGRX8, BGR8, R8, RG8, luminance and the 16-bit packed
//! formats with one decoder.

use crate::error::{Error, Result, Unsupported};
use crate::header::{pf, Header};

/// Whether the file declares its colour data as sRGB. No conversion is done,
/// the flag is carried for the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    Linear,
    Srgb,
}

/// Channel masks of an uncompressed pixel. A zero alpha mask means opaque.
/// `grey` replicates the red channel into green and blue, for luminance
/// formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Masks {
    pub bits: u32,
    pub r: u32,
    pub g: u32,
    pub b: u32,
    pub a: u32,
    pub grey: bool,
}

impl Masks {
    pub const RGBA8: Masks = Masks {
        bits: 32,
        r: 0xFF,
        g: 0xFF00,
        b: 0xFF_0000,
        a: 0xFF00_0000,
        grey: false,
    };
    pub const BGRA8: Masks = Masks {
        bits: 32,
        r: 0xFF_0000,
        g: 0xFF00,
        b: 0xFF,
        a: 0xFF00_0000,
        grey: false,
    };
    pub const BGRX8: Masks = Masks {
        bits: 32,
        r: 0xFF_0000,
        g: 0xFF00,
        b: 0xFF,
        a: 0,
        grey: false,
    };
    pub const BGR8: Masks = Masks {
        bits: 24,
        r: 0xFF_0000,
        g: 0xFF00,
        b: 0xFF,
        a: 0,
        grey: false,
    };
    pub const R8: Masks = Masks {
        bits: 8,
        r: 0xFF,
        g: 0,
        b: 0,
        a: 0,
        grey: false,
    };
    pub const L8: Masks = Masks {
        bits: 8,
        r: 0xFF,
        g: 0,
        b: 0,
        a: 0,
        grey: true,
    };
    pub const RG8: Masks = Masks {
        bits: 16,
        r: 0xFF,
        g: 0xFF00,
        b: 0,
        a: 0,
        grey: false,
    };
    pub const B5G6R5: Masks = Masks {
        bits: 16,
        r: 0xF800,
        g: 0x07E0,
        b: 0x001F,
        a: 0,
        grey: false,
    };
    pub const B5G5R5A1: Masks = Masks {
        bits: 16,
        r: 0x7C00,
        g: 0x03E0,
        b: 0x001F,
        a: 0x8000,
        grey: false,
    };
    pub const B4G4R4A4: Masks = Masks {
        bits: 16,
        r: 0x0F00,
        g: 0x00F0,
        b: 0x000F,
        a: 0xF000,
        grey: false,
    };

    fn validate(self) -> Result<Masks> {
        let ok_bits = matches!(self.bits, 8 | 16 | 24 | 32);
        // `bits` is the file's `rgb_bit_count` field, and the shift below
        // runs before the `ok_bits` check that rejects it, so a value
        // outside 8/16/24/32 (up to u32::MAX) must not reach a bare
        // `1u32 << bits`: any shift of 32 or more panics. `checked_shl`
        // turns that case into `None`, mapped to a harmless `u32::MAX`
        // limit. `ok_bits` still fails on it further down, so the overall
        // call still returns `Unsupported`.
        let limit = if self.bits == 32 {
            u32::MAX
        } else {
            1u32.checked_shl(self.bits).map_or(u32::MAX, |v| v - 1)
        };
        let fits = [self.r, self.g, self.b, self.a]
            .iter()
            .all(|m| *m & !limit == 0);
        if ok_bits && fits && (self.r | self.g | self.b | self.a) != 0 {
            Ok(self)
        } else {
            Err(Error::Unsupported(Unsupported::Masks {
                bits: self.bits,
                r: self.r,
                g: self.g,
                b: self.b,
                a: self.a,
            }))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Bc1,
    Bc2,
    Bc3,
    Bc4 { signed: bool },
    Bc5 { signed: bool },
    Bc7,
    Uncompressed(Masks),
}

impl Format {
    /// Decide the format and colour space from a parsed header.
    pub fn from_header(h: &Header) -> Result<(Format, ColorSpace)> {
        let p = &h.pixel_format;
        if let Some(d) = &h.dx10 {
            return from_dxgi(d.dxgi_format);
        }
        if p.flags & pf::FOURCC != 0 {
            let f = match &p.four_cc {
                b"DXT1" => Format::Bc1,
                b"DXT2" | b"DXT3" => Format::Bc2,
                b"DXT4" | b"DXT5" => Format::Bc3,
                b"ATI1" | b"BC4U" => Format::Bc4 { signed: false },
                b"BC4S" => Format::Bc4 { signed: true },
                b"ATI2" | b"BC5U" => Format::Bc5 { signed: false },
                b"BC5S" => Format::Bc5 { signed: true },
                other => return Err(Error::Unsupported(Unsupported::FourCc(*other))),
            };
            return Ok((f, ColorSpace::Linear));
        }
        let masks = if p.flags & pf::RGB != 0 {
            Masks {
                bits: p.rgb_bit_count,
                r: p.r_mask,
                g: p.g_mask,
                b: p.b_mask,
                a: if p.flags & pf::ALPHAPIXELS != 0 {
                    p.a_mask
                } else {
                    0
                },
                grey: false,
            }
        } else if p.flags & pf::LUMINANCE != 0 {
            Masks {
                bits: p.rgb_bit_count,
                r: p.r_mask,
                g: 0,
                b: 0,
                a: if p.flags & pf::ALPHAPIXELS != 0 {
                    p.a_mask
                } else {
                    0
                },
                grey: true,
            }
        } else if p.flags & pf::ALPHA != 0 {
            Masks {
                bits: p.rgb_bit_count,
                r: 0,
                g: 0,
                b: 0,
                a: p.a_mask,
                grey: false,
            }
        } else {
            // BUMPDUDV and flagless files both describe themselves by masks.
            Masks {
                bits: p.rgb_bit_count,
                r: p.r_mask,
                g: p.g_mask,
                b: p.b_mask,
                a: p.a_mask,
                grey: false,
            }
        };
        Ok((Format::Uncompressed(masks.validate()?), ColorSpace::Linear))
    }

    /// Bytes per 4 by 4 block for the block formats.
    pub fn block_size(&self) -> Option<usize> {
        match self {
            Format::Bc1 | Format::Bc4 { .. } => Some(8),
            Format::Bc2 | Format::Bc3 | Format::Bc5 { .. } | Format::Bc7 => Some(16),
            Format::Uncompressed(_) => None,
        }
    }

    /// Bytes one slice of a level occupies at this size. `width` and
    /// `height` come straight from the file header, so a plain multiply
    /// here can overflow `usize` on a hostile file (fuzz find,
    /// fuzz/artifacts/fuzz-dds/crash-1ebc502b360b3a4c38e1a6b05b2bac7b05629455).
    /// Saturating keeps the result a `usize` the caller's
    /// `checked_mul`/`checked_add` can still reject as `Error::TooLarge`,
    /// instead of panicking here.
    pub fn slice_bytes(&self, width: u32, height: u32) -> usize {
        match self.block_size() {
            Some(b) => (width as usize)
                .div_ceil(4)
                .saturating_mul((height as usize).div_ceil(4))
                .saturating_mul(b),
            None => {
                let Format::Uncompressed(m) = self else {
                    unreachable!()
                };
                (width as usize)
                    .saturating_mul(height as usize)
                    .saturating_mul(m.bits as usize / 8)
            }
        }
    }

    pub fn name(&self) -> String {
        match self {
            Format::Bc1 => "BC1".into(),
            Format::Bc2 => "BC2".into(),
            Format::Bc3 => "BC3".into(),
            Format::Bc4 { signed } => format!("BC4{}", if *signed { " snorm" } else { "" }),
            Format::Bc5 { signed } => format!("BC5{}", if *signed { " snorm" } else { "" }),
            Format::Bc7 => "BC7".into(),
            Format::Uncompressed(m) if *m == Masks::RGBA8 => "RGBA8".into(),
            Format::Uncompressed(m) if *m == Masks::BGRA8 => "BGRA8".into(),
            Format::Uncompressed(m) if *m == Masks::BGRX8 => "BGRX8".into(),
            Format::Uncompressed(m) if *m == Masks::BGR8 => "BGR8".into(),
            Format::Uncompressed(m) if *m == Masks::R8 => "R8".into(),
            Format::Uncompressed(m) if *m == Masks::L8 => "L8".into(),
            Format::Uncompressed(m) => format!("{}-bit masks", m.bits),
        }
    }
}

/// DXGI_FORMAT values the reader knows. Numbers from dxgiformat.h.
pub mod dxgi {
    pub const R8G8B8A8_UNORM: u32 = 28;
    pub const R8G8B8A8_UNORM_SRGB: u32 = 29;
    pub const R8G8_UNORM: u32 = 49;
    pub const R8_UNORM: u32 = 61;
    pub const A8_UNORM: u32 = 65;
    pub const BC1_UNORM: u32 = 71;
    pub const BC1_UNORM_SRGB: u32 = 72;
    pub const BC2_UNORM: u32 = 74;
    pub const BC2_UNORM_SRGB: u32 = 75;
    pub const BC3_UNORM: u32 = 77;
    pub const BC3_UNORM_SRGB: u32 = 78;
    pub const BC4_UNORM: u32 = 80;
    pub const BC4_SNORM: u32 = 81;
    pub const BC5_UNORM: u32 = 83;
    pub const BC5_SNORM: u32 = 84;
    pub const B5G6R5_UNORM: u32 = 85;
    pub const B5G5R5A1_UNORM: u32 = 86;
    pub const B8G8R8A8_UNORM: u32 = 87;
    pub const B8G8R8X8_UNORM: u32 = 88;
    pub const B8G8R8A8_UNORM_SRGB: u32 = 91;
    pub const B8G8R8X8_UNORM_SRGB: u32 = 93;
    pub const BC6H_UF16: u32 = 95;
    pub const BC6H_SF16: u32 = 96;
    pub const BC7_UNORM: u32 = 98;
    pub const BC7_UNORM_SRGB: u32 = 99;
    pub const B4G4R4A4_UNORM: u32 = 115;
}

fn from_dxgi(n: u32) -> Result<(Format, ColorSpace)> {
    use ColorSpace::{Linear, Srgb};
    let (f, cs) = match n {
        dxgi::BC1_UNORM => (Format::Bc1, Linear),
        dxgi::BC1_UNORM_SRGB => (Format::Bc1, Srgb),
        dxgi::BC2_UNORM => (Format::Bc2, Linear),
        dxgi::BC2_UNORM_SRGB => (Format::Bc2, Srgb),
        dxgi::BC3_UNORM => (Format::Bc3, Linear),
        dxgi::BC3_UNORM_SRGB => (Format::Bc3, Srgb),
        dxgi::BC4_UNORM => (Format::Bc4 { signed: false }, Linear),
        dxgi::BC4_SNORM => (Format::Bc4 { signed: true }, Linear),
        dxgi::BC5_UNORM => (Format::Bc5 { signed: false }, Linear),
        dxgi::BC5_SNORM => (Format::Bc5 { signed: true }, Linear),
        dxgi::BC7_UNORM => (Format::Bc7, Linear),
        dxgi::BC7_UNORM_SRGB => (Format::Bc7, Srgb),
        dxgi::R8G8B8A8_UNORM => (Format::Uncompressed(Masks::RGBA8), Linear),
        dxgi::R8G8B8A8_UNORM_SRGB => (Format::Uncompressed(Masks::RGBA8), Srgb),
        dxgi::B8G8R8A8_UNORM => (Format::Uncompressed(Masks::BGRA8), Linear),
        dxgi::B8G8R8A8_UNORM_SRGB => (Format::Uncompressed(Masks::BGRA8), Srgb),
        dxgi::B8G8R8X8_UNORM => (Format::Uncompressed(Masks::BGRX8), Linear),
        dxgi::B8G8R8X8_UNORM_SRGB => (Format::Uncompressed(Masks::BGRX8), Srgb),
        dxgi::R8_UNORM => (Format::Uncompressed(Masks::R8), Linear),
        dxgi::A8_UNORM => (
            Format::Uncompressed(Masks {
                bits: 8,
                r: 0,
                g: 0,
                b: 0,
                a: 0xFF,
                grey: false,
            }),
            Linear,
        ),
        dxgi::R8G8_UNORM => (Format::Uncompressed(Masks::RG8), Linear),
        dxgi::B5G6R5_UNORM => (Format::Uncompressed(Masks::B5G6R5), Linear),
        dxgi::B5G5R5A1_UNORM => (Format::Uncompressed(Masks::B5G5R5A1), Linear),
        dxgi::B4G4R4A4_UNORM => (Format::Uncompressed(Masks::B4G4R4A4), Linear),
        dxgi::BC6H_UF16 | dxgi::BC6H_SF16 => return Err(Error::Unsupported(Unsupported::Bc6h)),
        other => return Err(Error::Unsupported(Unsupported::Dxgi(other))),
    };
    Ok((f, cs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{Dx10, PixelFormat};

    fn header(pfmt: PixelFormat, dx10: Option<Dx10>) -> Header {
        Header {
            flags: 0,
            height: 4,
            width: 4,
            pitch_or_linear_size: 0,
            depth: 0,
            mip_map_count: 0,
            pixel_format: pfmt,
            caps: 0,
            caps2: 0,
            dx10,
        }
    }

    fn fourcc(c: [u8; 4]) -> PixelFormat {
        PixelFormat {
            flags: pf::FOURCC,
            four_cc: c,
            rgb_bit_count: 0,
            r_mask: 0,
            g_mask: 0,
            b_mask: 0,
            a_mask: 0,
        }
    }

    #[test]
    fn legacy_fourccs_map_to_block_formats() {
        for (c, f) in [
            (b"DXT1", Format::Bc1),
            (b"DXT3", Format::Bc2),
            (b"DXT5", Format::Bc3),
            (b"ATI1", Format::Bc4 { signed: false }),
            (b"ATI2", Format::Bc5 { signed: false }),
            (b"BC5S", Format::Bc5 { signed: true }),
        ] {
            assert_eq!(
                Format::from_header(&header(fourcc(*c), None)).unwrap(),
                (f, ColorSpace::Linear)
            );
        }
        let err = Format::from_header(&header(fourcc(*b"DXT9"), None)).unwrap_err();
        assert!(matches!(err, Error::Unsupported(Unsupported::FourCc(c)) if &c == b"DXT9"));
    }

    #[test]
    fn dxgi_formats_carry_srgb_and_refuse_bc6h() {
        let dx = |n| {
            header(
                fourcc(*b"DX10"),
                Some(Dx10 {
                    dxgi_format: n,
                    dimension: 3,
                    misc_flag: 0,
                    array_size: 1,
                    misc_flags2: 0,
                }),
            )
        };
        assert_eq!(
            Format::from_header(&dx(dxgi::BC7_UNORM_SRGB)).unwrap(),
            (Format::Bc7, ColorSpace::Srgb)
        );
        assert_eq!(
            Format::from_header(&dx(dxgi::B8G8R8A8_UNORM)).unwrap(),
            (Format::Uncompressed(Masks::BGRA8), ColorSpace::Linear)
        );
        assert!(matches!(
            Format::from_header(&dx(dxgi::BC6H_UF16)),
            Err(Error::Unsupported(Unsupported::Bc6h))
        ));
        assert!(matches!(
            Format::from_header(&dx(10)),
            Err(Error::Unsupported(Unsupported::Dxgi(10)))
        ));
    }

    #[test]
    fn masked_formats_are_read_and_bad_masks_refused() {
        let rgb = PixelFormat {
            flags: pf::RGB | pf::ALPHAPIXELS,
            four_cc: [0; 4],
            rgb_bit_count: 32,
            r_mask: 0xFF_0000,
            g_mask: 0xFF00,
            b_mask: 0xFF,
            a_mask: 0xFF00_0000,
        };
        assert_eq!(
            Format::from_header(&header(rgb, None)).unwrap().0,
            Format::Uncompressed(Masks::BGRA8)
        );
        let lum = PixelFormat {
            flags: pf::LUMINANCE,
            four_cc: [0; 4],
            rgb_bit_count: 8,
            r_mask: 0xFF,
            g_mask: 0,
            b_mask: 0,
            a_mask: 0,
        };
        assert_eq!(
            Format::from_header(&header(lum, None)).unwrap().0,
            Format::Uncompressed(Masks::L8)
        );
        let odd = PixelFormat {
            flags: pf::RGB,
            four_cc: [0; 4],
            rgb_bit_count: 12,
            r_mask: 0xF,
            g_mask: 0xF0,
            b_mask: 0xF00,
            a_mask: 0,
        };
        assert!(matches!(
            Format::from_header(&header(odd, None)),
            Err(Error::Unsupported(Unsupported::Masks { bits: 12, .. }))
        ));
        let wide = PixelFormat {
            flags: pf::RGB,
            four_cc: [0; 4],
            rgb_bit_count: 16,
            r_mask: 0xFF_0000,
            g_mask: 0,
            b_mask: 0,
            a_mask: 0,
        };
        assert!(matches!(
            Format::from_header(&header(wide, None)),
            Err(Error::Unsupported(Unsupported::Masks { .. }))
        ));
    }

    /// Fuzz find (fuzz/artifacts/fuzz-dds/crash-423907005bf0954c640ce6cd3d8131b6da3567f4):
    /// `rgb_bit_count` is read straight from the file and reaches
    /// `Masks::validate` before its `matches!(bits, 8 | 16 | 24 | 32)` check
    /// runs. A count outside that set and at or above 32 shifted `1u32` by
    /// 32 or more, which panics. `validate` now uses `checked_shl`, so an
    /// out-of-range count is refused as `Unsupported::Masks` like any other
    /// bad bit count, not a panic.
    #[test]
    fn a_huge_bit_count_is_refused_not_shifted_into_overflow() {
        let huge = PixelFormat {
            flags: pf::RGB,
            four_cc: [0; 4],
            rgb_bit_count: u32::MAX,
            r_mask: 0xFF,
            g_mask: 0,
            b_mask: 0,
            a_mask: 0,
        };
        assert!(matches!(
            Format::from_header(&header(huge, None)),
            Err(Error::Unsupported(Unsupported::Masks {
                bits: u32::MAX,
                ..
            }))
        ));
    }

    #[test]
    fn level_sizes_round_up_to_blocks() {
        assert_eq!(Format::Bc1.slice_bytes(1, 1), 8);
        assert_eq!(Format::Bc7.slice_bytes(5, 4), 32);
        assert_eq!(Format::Bc3.slice_bytes(256, 256), 65536);
        assert_eq!(Format::Uncompressed(Masks::BGR8).slice_bytes(3, 2), 18);
    }

    /// Fuzz find (fuzz/artifacts/fuzz-dds/crash-1ebc502b360b3a4c38e1a6b05b2bac7b05629455):
    /// `width` and `height` come straight from the file header, and
    /// `slice_bytes` multiplied them by the bytes per pixel with a plain
    /// `*`. Dimensions near `u32::MAX` overflow `usize` there, which
    /// panics, before `Dds::parse`'s own `checked_mul`/`checked_add` ever
    /// gets a chance to refuse the file as too large. `slice_bytes` now
    /// saturates instead, for both the block and uncompressed branches, so
    /// the overflow surfaces as a saturated `usize::MAX` the caller's
    /// checked arithmetic turns into `Error::TooLarge`.
    #[test]
    fn huge_dimensions_saturate_instead_of_overflowing() {
        assert_eq!(
            Format::Uncompressed(Masks::RGBA8).slice_bytes(u32::MAX, u32::MAX),
            usize::MAX
        );
        assert_eq!(Format::Bc7.slice_bytes(u32::MAX, u32::MAX), usize::MAX);
    }
}
