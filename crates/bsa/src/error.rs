// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

/// Everything that can go wrong reading an archive. Messages name the archive
/// so a failure inside a hundred-archive load order says which one.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// The bytes are not an archive this crate understands, or the tables are
    /// inconsistent with the file size.
    #[error("{message}: {}", path.display())]
    Format { message: String, path: PathBuf },

    #[error("File '{name}' not found in archive")]
    NotFound { name: String },

    /// A record points outside the archive. A truncated download looks like this.
    #[error("File record extends past archive end: {name} (offset={offset}, size={size})")]
    Truncated {
        name: String,
        offset: u64,
        size: u64,
    },

    #[error("{codec} decompression failed for {name}: {source}")]
    Decompress {
        codec: &'static str,
        name: String,
        source: std::io::Error,
    },

    /// Decompression succeeded and produced the wrong number of bytes. The
    /// archive says how long a file is once unpacked, so a mismatch means the
    /// record and the stream disagree. Reported rather than returned short,
    /// because a truncated mesh, texture or script that nothing complains
    /// about is worse than a failed read.
    #[error("{codec} decompressed {name} to {got} bytes, the record declares {want}")]
    DecompressedSize {
        codec: &'static str,
        name: String,
        want: u32,
        got: usize,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn format(message: impl Into<String>, path: &std::path::Path) -> Self {
        Error::Format {
            message: message.into(),
            path: path.to_path_buf(),
        }
    }
}
