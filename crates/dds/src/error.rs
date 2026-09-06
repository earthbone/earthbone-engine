// SPDX-License-Identifier: GPL-3.0-or-later

use thiserror::Error;

/// A format the reader recognises but does not decode, named precisely so a
/// caller can report it or route it elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsupported {
    /// A DX10 header with a DXGI format outside the decoded set.
    Dxgi(u32),
    /// A legacy FourCC outside the decoded set, such as the D3DFMT numbers
    /// some tools write for float formats.
    FourCc([u8; 4]),
    /// BC6H, the half-float HDR format, which has no RGBA8 meaning.
    Bc6h,
    /// A pixel format described by masks the generic path cannot read: a bit
    /// count other than 8, 16, 24 or 32, or a mask wider than the pixel.
    Masks { bits: u32, r: u32, g: u32, b: u32, a: u32 },
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dxgi(n) => write!(f, "DXGI format {n}"),
            Self::FourCc(c) => write!(f, "FourCC {:?}", String::from_utf8_lossy(c)),
            Self::Bc6h => write!(f, "BC6H half-float"),
            Self::Masks { bits, r, g, b, a } => write!(
                f,
                "{bits}-bit pixels with masks r={r:#x} g={g:#x} b={b:#x} a={a:#x}"
            ),
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("not a DDS file: bad magic")]
    NotDds,
    #[error("bad DDS header: {0}")]
    BadHeader(&'static str),
    #[error("unsupported texture format: {0}")]
    Unsupported(Unsupported),
    #[error("truncated: level data needs {needed} bytes, file has {have}")]
    Truncated { needed: usize, have: usize },
    #[error("no such level: layer {layer} mip {mip}")]
    NoSuchLevel { layer: u32, mip: u32 },
    #[error("image too large: {width} by {height} by {depth}")]
    TooLarge { width: u32, height: u32, depth: u32 },
    #[cfg(feature = "encode")]
    #[error("encode: {0}")]
    Encode(String),
}

pub type Result<T> = std::result::Result<T, Error>;
