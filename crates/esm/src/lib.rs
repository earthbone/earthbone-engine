// SPDX-License-Identifier: GPL-3.0-or-later

//! Load-order aware reader for TES4 family plugins, targeting Skyrim SE.
//!
//! Three layers:
//!
//! 1. `plugin`: one file as a group tree of records, each record its list of
//!    subrecords. Compressed records are inflated, `XXXX` size markers are
//!    folded away, and every record can be written back.
//! 2. `load_order`: several plugins in order, with FormIDs resolved through
//!    each file's master list into `GlobalFormId`s, and a form table where
//!    the last file to carry a record wins.
//! 3. `records`: typed views of CELL, REFR, WRLD, LAND, NAVM and STAT built
//!    from a record on demand.
//! 4. `cell_index`: cell to placed objects, land and navmeshes, and
//!    worldspace grid to cell, built once from the winning placements.
//!
//! ```no_run
//! use soe_esm::{LoadOrder, Plugin, records::cell::Cell, tag};
//!
//! let mut order = LoadOrder::new();
//! for name in ["Skyrim.esm", "Update.esm"] {
//!     let bytes = std::fs::read(format!("Data/{name}")).unwrap();
//!     order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
//! }
//! for (id, rec) in order.winners_of(tag::CELL) {
//!     let cell = Cell::from_record(rec, order.file(id.file as usize).plugin.header.is_localized()).unwrap();
//!     println!("{id} {:?} interior={}", cell.editor_id, cell.is_interior());
//! }
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod cell_index;
pub mod cursor;
pub mod error;
pub mod load_order;
pub mod plugin;
pub mod records;
pub mod tag;

pub use cell_index::{CellIndex, GridCell};
pub use error::{Error, Result};
pub use load_order::{GlobalFormId, LoadOrder, LoadedFile, RecordRef, Slot};
pub use plugin::{Entry, FileHeader, Group, GroupKind, Placement, Plugin, Record, Subrecord};
pub use tag::Tag;

/// Read and parse one plugin file.
pub fn read_plugin(path: impl AsRef<std::path::Path>) -> std::io::Result<Result<Plugin>> {
    let bytes = std::fs::read(path)?;
    Ok(Plugin::parse(&bytes))
}
