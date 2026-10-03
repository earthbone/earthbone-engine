// SPDX-License-Identifier: GPL-3.0-or-later

//! Convert load-order interface art into Godot resources, without executing SWF.
//!
//! SWF parsing uses Ruffle's `swf` crate. Edge assembly follows Ruffle's
//! `render/src/shape_utils.rs`. Resource encoding follows Godot 4.4's
//! `scene/resources/{mesh,font,image_texture}.cpp` and `rendering_server.h`.
//! JPEXS FFDec supplies ActionScript source outside the runtime.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod bitmap;
mod cache;
mod convert;
mod geometry;
mod resource;

pub use cache::{build_cache, Cache, Decompiler};
pub use convert::{convert, Font, Frame, Glyph, Instance, Movie, TextField, Timeline};
pub use geometry::{Mesh, Paint, Surface};

/// Conversion failures retain the asset or operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The SWF parser rejected the input.
    #[error("SWF: {0}")]
    Swf(String),
    /// A source structure cannot describe valid art.
    #[error("invalid interface art: {0}")]
    Invalid(String),
    /// Tessellation could not produce a mesh.
    #[error("tessellation: {0}")]
    Tessellation(String),
    /// Bitmap decoding failed.
    #[error("bitmap: {0}")]
    Image(#[from] image::ImageError),
    /// Reading or writing a file failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Reading a winning VFS asset failed.
    #[error("{0}")]
    Bsa(#[from] soe_bsa::Error),
    /// A cache manifest could not be encoded or read.
    #[error("cache manifest: {0}")]
    Json(#[from] serde_json::Error),
    /// The external ActionScript decompiler failed.
    #[error("AS2 decompilation: {0}")]
    Decompiler(String),
    /// An asset failed, with its normalized data path.
    #[error("{path}: {source}")]
    Asset {
        /// Winning SWF path.
        path: String,
        /// Underlying failure.
        source: Box<Error>,
    },
}
