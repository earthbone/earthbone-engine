// SPDX-License-Identifier: GPL-3.0-or-later

//! LTEX: a landscape texture. `LAND` quadrants name one as a base texture or
//! an alpha layer (see `land.rs`), and a repeated `GNAM` here names the
//! `GRAS` records the engine may scatter wherever that texture is painted.
//! Field layout from `TES5Edit/Core/wbDefinitionsTES5.pas`
//! (`wbRecord(LTEX, ...)`), cross checked against OpenMW's
//! `components/esm4/loadltex.cpp`, which reads the same `GNAM` as a
//! repeated single FormID subrecord, one per grass entry.

use crate::error::Result;
use crate::plugin::Record;
use crate::tag::{self, Tag};

use super::{form_id, form_id_list};

#[derive(Debug, Clone, PartialEq)]
pub struct LandTexture {
    pub form_id: u32,
    pub editor_id: Option<String>,
    /// TNAM: the texture set this uses. FormID as written in the file.
    pub texture_set: Option<u32>,
    /// GNAM, repeated: GRAS FormIDs this texture may grow, as written in the
    /// file, in record order.
    pub grasses: Vec<u32>,
}

impl LandTexture {
    pub fn from_record(rec: &Record) -> Result<LandTexture> {
        debug_assert_eq!(rec.tag, tag::LTEX);
        let texture_set = form_id(rec, Tag::new(b"TNAM"))?;
        let grasses = form_id_list(rec, Tag::new(b"GNAM"))?;
        Ok(LandTexture {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            texture_set,
            grasses,
        })
    }
}
