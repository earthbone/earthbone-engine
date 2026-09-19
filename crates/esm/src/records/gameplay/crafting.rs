// SPDX-License-Identifier: GPL-3.0-or-later

//! COBJ fields from xEdit wbRecord(COBJ), wbCNTONoReach and wbCOED.

use super::{check_tag, fields, optional_u32, Condition, Result};
use crate::{Error, Record};

/// Optional COED data for one ingredient.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemOwnership {
    pub owner: u32,
    /// Global FormID for an NPC owner, signed rank bits for a faction owner.
    pub global_or_rank: u32,
    pub condition: f32,
}

/// One CNTO entry, without merging repeated item IDs.
#[derive(Debug, Clone, PartialEq)]
pub struct Ingredient {
    pub item: u32,
    /// CNTO is signed in xEdit. Preserve negative values rather than wrapping.
    pub count: i32,
    pub ownership: Option<ItemOwnership>,
}

/// A COBJ recipe definition. Missing output fields stay absent.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub form_id: u32,
    pub editor_id: Option<String>,
    /// COCT as stored, not used to allocate or truncate the ingredient list.
    pub declared_ingredient_count: Option<u32>,
    pub ingredients: Vec<Ingredient>,
    pub conditions: Vec<Condition>,
    /// Missing in several shipped records. Absence is not a craftable output.
    pub created_object: Option<u32>,
    pub workbench_keyword: Option<u32>,
    pub created_count: Option<u16>,
}

impl Recipe {
    /// Read this view without changing the source record.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong record type, malformed field lengths or
    /// missing required fields and malformed repeated groups.
    pub fn from_record(rec: &Record) -> Result<Self> {
        check_tag(rec, *b"COBJ")?;
        let mut ingredients: Vec<Ingredient> = Vec::new();
        for sub in &rec.subrecords {
            match sub.tag.as_bytes() {
                b"CNTO" => {
                    let mut f = fields(rec, sub.tag, &sub.data, 8)?;
                    ingredients.push(Ingredient {
                        item: f.u32()?,
                        count: f.i32()?,
                        ownership: None,
                    });
                }
                b"COED" => {
                    let mut f = fields(rec, sub.tag, &sub.data, 12)?;
                    let item = ingredients
                        .last_mut()
                        .ok_or_else(|| Error::Invalid("COED without CNTO".into()))?;
                    if item.ownership.is_some() {
                        return Err(Error::Invalid("repeated COED".into()));
                    }
                    item.ownership = Some(ItemOwnership {
                        owner: f.u32()?,
                        global_or_rank: f.u32()?,
                        condition: f.f32()?,
                    });
                }
                _ => {}
            }
        }
        Ok(Self {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            declared_ingredient_count: optional_u32(rec, *b"COCT")?,
            ingredients,
            conditions: Condition::all(rec)?,
            created_object: optional_u32(rec, *b"CNAM")?,
            workbench_keyword: optional_u32(rec, *b"BNAM")?,
            created_count: rec
                .find(crate::Tag::new(b"NAM1"))
                .map(|bytes| fields(rec, crate::Tag::new(b"NAM1"), bytes, 2)?.u16())
                .transpose()?,
        })
    }
}
