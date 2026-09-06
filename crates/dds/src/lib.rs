// SPDX-License-Identifier: GPL-3.0-or-later

//! DDS textures as the engine reads them: the container, the format, and a
//! decoder from every block format Skyrim SE ships (BC1, BC2, BC3, BC4, BC5,
//! BC7) plus uncompressed pixels to RGBA8, mip by mip. The `encode` feature
//! adds the re-encode path the mod manager's `TransformedTexture` step uses,
//! so both sides of the project read and write textures through one crate.
//!
//! Nothing here depends on a renderer. A [`Dds`] borrows the file bytes and
//! hands out levels by layer and mip, and [`Image`] is plain RGBA8.

pub mod decode;
#[cfg(feature = "encode")]
pub mod encode;
pub mod error;
pub mod format;
pub mod header;

pub use error::{Error, Result, Unsupported};
pub use format::{dxgi, ColorSpace, Format, Masks};
pub use header::Header;

/// Where one level's bytes sit and how big it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub layer: u32,
    pub mip: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub offset: usize,
    pub len: usize,
}

/// A decoded level, RGBA8, slices stacked in order for volumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub struct Dds<'a> {
    header: Header,
    format: Format,
    color_space: ColorSpace,
    data: &'a [u8],
    levels: Vec<Level>,
}

impl<'a> Dds<'a> {
    /// Parse the header, decide the format and lay out every level. The
    /// file must hold every level it declares, so a truncated file is
    /// refused here rather than at the first short mip.
    pub fn parse(bytes: &'a [u8]) -> Result<Dds<'a>> {
        let (header, mut offset) = Header::parse(bytes)?;
        let (format, color_space) = Format::from_header(&header)?;
        let mut levels = Vec::new();
        for layer in 0..header.layer_count() {
            for mip in 0..header.mip_count() {
                let width = (header.width >> mip).max(1);
                let height = (header.height >> mip).max(1);
                let depth = (header.depth() >> mip).max(1);
                let len = format
                    .slice_bytes(width, height)
                    .checked_mul(depth as usize)
                    .ok_or(Error::TooLarge { width, height, depth })?;
                levels.push(Level { layer, mip, width, height, depth, offset, len });
                offset = offset.checked_add(len).ok_or(Error::TooLarge { width, height, depth })?;
            }
        }
        if bytes.len() < offset {
            return Err(Error::Truncated { needed: offset, have: bytes.len() });
        }
        Ok(Dds { header, format, color_space, data: bytes, levels })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn color_space(&self) -> ColorSpace {
        self.color_space
    }

    pub fn width(&self) -> u32 {
        self.header.width
    }

    pub fn height(&self) -> u32 {
        self.header.height
    }

    pub fn mip_count(&self) -> u32 {
        self.header.mip_count()
    }

    pub fn layer_count(&self) -> u32 {
        self.header.layer_count()
    }

    pub fn levels(&self) -> &[Level] {
        &self.levels
    }

    pub fn level(&self, layer: u32, mip: u32) -> Result<Level> {
        self.levels
            .iter()
            .copied()
            .find(|l| l.layer == layer && l.mip == mip)
            .ok_or(Error::NoSuchLevel { layer, mip })
    }

    /// The still-encoded bytes of one level.
    pub fn level_bytes(&self, layer: u32, mip: u32) -> Result<&'a [u8]> {
        let l = self.level(layer, mip)?;
        Ok(&self.data[l.offset..l.offset + l.len])
    }

    /// Decode one level to RGBA8.
    pub fn decode(&self, layer: u32, mip: u32) -> Result<Image> {
        let l = self.level(layer, mip)?;
        let bytes = &self.data[l.offset..l.offset + l.len];
        let slice_len = self.format.slice_bytes(l.width, l.height);
        let mut rgba = Vec::with_capacity(l.width as usize * l.height as usize * l.depth as usize * 4);
        for z in 0..l.depth as usize {
            rgba.extend(decode::decode_slice(self.format, l.width, l.height, &bytes[z * slice_len..][..slice_len])?);
        }
        Ok(Image { width: l.width, height: l.height, depth: l.depth, rgba })
    }

    /// Every mip of one layer, base first.
    pub fn decode_chain(&self, layer: u32) -> Result<Vec<Image>> {
        (0..self.mip_count()).map(|m| self.decode(layer, m)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use header::{caps, flags, pf, Dx10, PixelFormat};

    /// A DX10-headed BC7 file with `mips` levels of solid mode-6 blocks.
    fn synthetic(fourcc: &[u8; 4], dxgi: Option<u32>, w: u32, h: u32, mips: u32, block: &[u8]) -> Vec<u8> {
        let header = Header {
            flags: flags::CAPS | flags::HEIGHT | flags::WIDTH | flags::PIXELFORMAT | flags::MIPMAPCOUNT,
            height: h,
            width: w,
            pitch_or_linear_size: 0,
            depth: 0,
            mip_map_count: mips,
            pixel_format: PixelFormat { flags: pf::FOURCC, four_cc: *fourcc, rgb_bit_count: 0, r_mask: 0, g_mask: 0, b_mask: 0, a_mask: 0 },
            caps: caps::TEXTURE | caps::MIPMAP,
            caps2: 0,
            dx10: dxgi.map(|d| Dx10 { dxgi_format: d, dimension: 3, misc_flag: 0, array_size: 1, misc_flags2: 0 }),
        };
        let mut out = header.to_bytes();
        for m in 0..mips {
            let lw = (w >> m).max(1).div_ceil(4);
            let lh = (h >> m).max(1).div_ceil(4);
            for _ in 0..lw * lh {
                out.extend_from_slice(block);
            }
        }
        out
    }

    #[test]
    fn a_bc1_mip_chain_decodes_level_by_level() {
        // Solid red block: c0 = c1 = red in 565, all indices 0.
        let block = [0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0];
        let bytes = synthetic(b"DXT1", None, 8, 4, 4, &block);
        let dds = Dds::parse(&bytes).unwrap();
        assert_eq!(dds.format(), Format::Bc1);
        assert_eq!(dds.mip_count(), 4);
        let sizes: Vec<_> = dds.levels().iter().map(|l| (l.width, l.height, l.len)).collect();
        assert_eq!(sizes, vec![(8, 4, 16), (4, 2, 8), (2, 1, 8), (1, 1, 8)]);
        let chain = dds.decode_chain(0).unwrap();
        assert_eq!(chain.len(), 4);
        assert_eq!(chain[0].rgba.len(), 8 * 4 * 4);
        assert!(chain[3].rgba == [255, 0, 0, 255]);
        assert!(matches!(dds.level(0, 4), Err(Error::NoSuchLevel { layer: 0, mip: 4 })));
    }

    #[test]
    fn a_truncated_file_is_refused_at_parse() {
        let block = [0u8; 16];
        let mut bytes = synthetic(b"DX10", Some(dxgi::BC7_UNORM_SRGB), 4, 4, 2, &block);
        let whole = Dds::parse(&bytes).unwrap();
        assert_eq!(whole.color_space(), ColorSpace::Srgb);
        bytes.pop();
        let err = Dds::parse(&bytes).unwrap_err();
        assert!(matches!(err, Error::Truncated { .. }), "{err}");
    }

    #[test]
    fn unsupported_formats_name_themselves() {
        let bytes = synthetic(b"DX10", Some(dxgi::BC6H_UF16), 4, 4, 1, &[0u8; 16]);
        let err = Dds::parse(&bytes).unwrap_err();
        assert_eq!(err.to_string(), "unsupported texture format: BC6H half-float");
        let bytes = synthetic(b"DXT9", None, 4, 4, 1, &[0u8; 16]);
        assert!(matches!(Dds::parse(&bytes), Err(Error::Unsupported(Unsupported::FourCc(_)))));
    }

    #[test]
    fn a_cube_map_has_six_layers_of_chains() {
        let block = [0u8; 8];
        let mut bytes = synthetic(b"ATI1", None, 4, 4, 2, &block);
        // Five more faces, each two levels of one block.
        for _ in 0..5 {
            bytes.extend_from_slice(&block);
            bytes.extend_from_slice(&block);
        }
        let caps2_at = 4 + 108;
        bytes[caps2_at..caps2_at + 4].copy_from_slice(&(caps::CUBEMAP | caps::CUBEMAP_ALL_FACES).to_le_bytes());
        let dds = Dds::parse(&bytes).unwrap();
        assert_eq!(dds.layer_count(), 6);
        assert_eq!(dds.levels().len(), 12);
        assert_eq!(dds.level(5, 1).unwrap().offset, bytes.len() - 8);
    }
}
