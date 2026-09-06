// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: not a plugin: {source}")]
    Header {
        path: PathBuf,
        #[source]
        source: soe_esm::Error,
    },
    #[error("{path}: shorter than a TES4 header")]
    Truncated { path: PathBuf },
    #[error("{count} regular plugins are active, the engine has slots for {max}")]
    TooManyPlugins { count: usize, max: usize },
    #[error("{count} light plugins are active, the engine has slots for {max}")]
    TooManyLight { count: usize, max: usize },
}

pub type Result<T> = std::result::Result<T, Error>;
