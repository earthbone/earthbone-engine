// SPDX-License-Identifier: GPL-3.0-or-later

use thiserror::Error;

/// A codec this build cannot decode, named precisely so a caller can report
/// it or route the bytes elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsupported {
    /// A WMA bitstream with no decoder compiled in. The `xwma-ffmpeg`
    /// feature adds one.
    Wma { format_tag: u16 },
    /// An Ogg Vorbis stream with the `vorbis` feature off.
    Vorbis,
    /// An Opus stream with the `opus` feature off or an unsupported mapping.
    Opus,
    /// A `WAVEFORMATEX` tag the crate does not recognise at all.
    WaveFormat(u16),
    /// A file extension the source resolver has no reader for.
    Extension(String),
    /// LIP flags outside the version 1 little-endian variable-target layout.
    LipFlags(u32),
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wma { format_tag } => {
                write!(
                    f,
                    "WMA bitstream (format tag {format_tag:#06x}), no decoder in this build"
                )
            }
            Self::Vorbis => write!(f, "Ogg Vorbis, the vorbis feature is off"),
            Self::Opus => write!(
                f,
                "Opus, the opus feature is off or the mapping is unsupported"
            ),
            Self::WaveFormat(tag) => write!(f, "WAVEFORMATEX tag {tag:#06x}"),
            Self::Extension(e) => write!(f, "no reader for extension {e:?}"),
            Self::LipFlags(flags) => write!(f, "LIP body flags {flags:#010x}"),
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("not a FUZ file: bad magic")]
    NotFuz,
    #[error("FUZ version {0} is not supported, only 1 is")]
    FuzVersion(u32),
    #[error("not a RIFF file: bad magic")]
    NotRiff,
    #[error("RIFF form is {0:?}, expected XWMA")]
    NotXwma(String),
    #[error("{what} is truncated: needs {needed} bytes, has {have}")]
    Truncated {
        what: &'static str,
        needed: usize,
        have: usize,
    },
    #[error("malformed {what}: {why}")]
    Malformed { what: &'static str, why: String },
    #[error("missing the {0} chunk")]
    MissingChunk(&'static str),
    #[error("unsupported: {0}")]
    Unsupported(Unsupported),
    #[error("decoder: {0}")]
    Decode(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
