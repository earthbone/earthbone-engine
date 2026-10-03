// SPDX-License-Identifier: GPL-3.0-or-later

//! LTEX: a landscape texture. `LAND` quadrants name one as a base texture or
//! an alpha layer (see `land.rs`), and a repeated `GNAM` here names the
//! `GRAS` records the engine may scatter wherever that texture is painted.
//! `MNAM` names the MATT material footsteps resolve on painted terrain, which
//! carries no Havok material of its own.
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
    /// MNAM: the MATT material type footsteps and impacts resolve on this
    /// texture. FormID as written in the file.
    pub material: Option<u32>,
    /// HNAM friction byte, when the two byte struct is present.
    pub friction: Option<u8>,
    /// HNAM restitution byte, when the two byte struct is present.
    pub restitution: Option<u8>,
    /// SNAM: the texture specular exponent.
    pub specular_exponent: Option<u8>,
    /// INAM: the Is Snow boolean, a `u32` in the file.
    pub is_snow: bool,
}

impl LandTexture {
    pub fn from_record(rec: &Record) -> Result<LandTexture> {
        debug_assert_eq!(rec.tag, tag::LTEX);
        let texture_set = form_id(rec, Tag::new(b"TNAM"))?;
        let grasses = form_id_list(rec, Tag::new(b"GNAM"))?;
        let material = form_id(rec, Tag::new(b"MNAM"))?;
        let havok = rec.find(Tag::new(b"HNAM")).filter(|data| data.len() >= 2);
        let specular_exponent = rec
            .find(Tag::new(b"SNAM"))
            .and_then(|data| data.first().copied());
        let is_snow = rec
            .find(Tag::new(b"INAM"))
            .and_then(|data| data.get(..4))
            .is_some_and(|bytes| bytes != [0; 4]);
        Ok(LandTexture {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            texture_set,
            grasses,
            material,
            friction: havok.map(|data| data[0]),
            restitution: havok.map(|data| data[1]),
            specular_exponent,
            is_snow,
        })
    }
}
