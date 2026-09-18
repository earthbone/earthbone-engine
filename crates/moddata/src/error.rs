// SPDX-License-Identifier: GPL-3.0-or-later

//! One error type for every format the crate reads.

use std::path::PathBuf;

/// What went wrong reading a configuration file.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The file could not be opened or read.
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A JSON document did not parse, or was not the shape the format needs.
    #[error("{path}: {why}")]
    Json { path: PathBuf, why: String },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn json(path: impl Into<PathBuf>, why: impl Into<String>) -> Self {
        Self::Json {
            path: path.into(),
            why: why.into(),
        }
    }
}
