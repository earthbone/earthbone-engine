// SPDX-License-Identifier: GPL-3.0-or-later

//! Reader for Skyrim Special Edition NIF files (Gamebryo 20.2.0.7, user
//! version 12, Bethesda stream 100), with Skyrim LE (stream 83) and the
//! geometry side of Fallout 4 (stream 130) along for the ride.
//!
//! The crate has two layers. [`blocks`] is the file as written: one struct
//! per block type the mesh path needs, everything else kept as raw bytes.
//! [`mesh`] is what a renderer wants: a flat list of [`mesh::Mesh`] with
//! positions, normals, tangents, UVs, colors, indices, skin weights and a
//! [`mesh::Material`] decoded from the shader and alpha properties.
//!
//! ```no_run
//! let bytes = std::fs::read("meshes/clutter/bucket01.nif").unwrap();
//! let file = soe_nif::parse(&bytes).unwrap();
//! for mesh in file.meshes().unwrap() {
//!     println!("{} {} triangles", mesh.name, mesh.triangle_count());
//! }
//! ```
//!
//! nif.xml from the niftools project is the format authority this reader
//! follows, and OpenMW's `components/nif` the worked example.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod blocks;
pub mod error;
pub mod header;
pub mod mesh;
pub mod stream;
pub mod types;
pub mod vertex;

pub use error::{Error, Result};
pub use header::Header;
pub use mesh::{Material, Mesh};
pub use types::{BlockRef, Bound, Mat33, Transform, Vec3};

use blocks::Block;

/// A parsed file: header, blocks in file order, and the root references
/// from the footer.
#[derive(Debug, Clone, PartialEq)]
pub struct NifFile {
    pub header: Header,
    pub blocks: Vec<Block>,
    pub roots: Vec<BlockRef>,
}

impl NifFile {
    pub fn get(&self, r: BlockRef) -> Option<&Block> {
        r.get().and_then(|i| self.blocks.get(i))
    }

    /// The name of a node or shape block, `None` if the reference is not
    /// one of those.
    pub fn node_name(&self, r: BlockRef) -> Option<String> {
        match self.get(r)? {
            Block::Node(n) => Some(n.av.name().to_string()),
            Block::TriShape(t) => Some(t.av.name().to_string()),
            Block::BsTriShape(t) => Some(t.av.name().to_string()),
            _ => None,
        }
    }

    /// Every mesh reachable from the roots, see [`mesh::meshes`].
    pub fn meshes(&self) -> Result<Vec<Mesh>> {
        mesh::meshes(self)
    }

    /// Type names of every block, useful when a file fails to yield meshes.
    pub fn block_types(&self) -> Vec<&str> {
        self.blocks.iter().map(Block::type_name).collect()
    }
}

/// Parse a whole file from memory.
pub fn parse(bytes: &[u8]) -> Result<NifFile> {
    let mut s = stream::Stream::new(bytes);
    let header = header::read_header(&mut s)?;
    let blocks = blocks::read_blocks(&mut s, &header)?;
    let roots = blocks::read_footer(&mut s)?;
    for r in &roots {
        if let Some(i) = r.get() {
            if i >= blocks.len() {
                return Err(Error::BadRef {
                    block: blocks.len(),
                    reference: r.0,
                });
            }
        }
    }
    Ok(NifFile {
        header,
        blocks,
        roots,
    })
}
