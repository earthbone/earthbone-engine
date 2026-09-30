// SPDX-License-Identifier: GPL-3.0-or-later

//! SPEL/SPIT, ordered EFID/EFIT groups and MGEF/DATA from xEdit and UESP.
//! Numeric enums and flags remain numeric so unfamiliar values are retained.

use super::{check_tag, fields, optional_u32, required, text, Condition, Result, Text};
use crate::{Error, Record, Tag};

/// SPEL SPIT fields. Numeric enum values are preserved.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellData {
    pub base_cost: u32,
    pub flags: u32,
    pub spell_type: u32,
    pub charge_time: f32,
    pub cast_type: u32,
    pub delivery: u32,
    pub cast_duration: f32,
    pub range: f32,
    pub half_cost_perk: u32,
}

/// One EFIT magnitude, area and duration.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectData {
    pub magnitude: f32,
    pub area: u32,
    pub duration: u32,
}

/// One EFID/EFIT group with its own conditions.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub base_effect: u32,
    pub data: EffectData,
    pub conditions: Vec<Condition>,
}

/// Spell metadata and ordered effect definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct Spell {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub name: Option<Text>,
    pub description: Option<Text>,
    pub equip_type: Option<u32>,
    pub data: SpellData,
    pub effects: Vec<Effect>,
}

/// ALCH ENIT fields needed to distinguish ingestible kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct IngestibleData {
    pub value: i32,
    pub flags: u32,
    pub addiction: u32,
    pub addiction_chance: f32,
    pub consume_sound: u32,
}

/// ALCH metadata and ordered effect definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct Ingestible {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub name: Option<Text>,
    pub description: Option<Text>,
    pub data: IngestibleData,
    pub effects: Vec<Effect>,
}

impl Ingestible {
    /// Read this view without changing the source record.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong record type, malformed field lengths or
    /// missing required fields and malformed repeated groups.
    pub fn from_record(rec: &Record, localized: bool) -> Result<Self> {
        check_tag(rec, *b"ALCH")?;
        let mut f = required(rec, *b"ENIT", 20)?;
        let data = IngestibleData {
            value: f.i32()?,
            flags: f.u32()?,
            addiction: f.u32()?,
            addiction_chance: f.f32()?,
            consume_sound: f.u32()?,
        };
        let mut effects = Vec::new();
        let mut pending: Option<(u32, Option<EffectData>, Vec<Condition>)> = None;
        for sub in &rec.subrecords {
            match sub.tag.as_bytes() {
                b"EFID" => {
                    if let Some(previous) = pending.take() {
                        effects.push(finish_effect(previous, Tag::new(b"ALCH"))?);
                    }
                    pending = Some((fields(rec, sub.tag, &sub.data, 4)?.u32()?, None, Vec::new()));
                }
                b"EFIT" => {
                    let current = pending
                        .as_mut()
                        .ok_or_else(|| Error::Invalid("EFIT without EFID".into()))?;
                    if current.1.is_some() {
                        return Err(Error::Invalid("repeated EFIT".into()));
                    }
                    let mut f = fields(rec, sub.tag, &sub.data, 12)?;
                    current.1 = Some(EffectData {
                        magnitude: f.f32()?,
                        area: f.u32()?,
                        duration: f.u32()?,
                    });
                }
                b"CTDA" | b"CIS1" | b"CIS2" => {
                    let current = pending.as_mut().ok_or_else(|| {
                        Error::Invalid("ingestible condition without EFID".into())
                    })?;
                    Condition::push(rec, sub, &mut current.2)?;
                }
                _ => {}
            }
        }
        if let Some(last) = pending {
            effects.push(finish_effect(last, Tag::new(b"ALCH"))?);
        }
        Ok(Self {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            name: text(rec, *b"FULL", localized)?,
            description: text(rec, *b"DESC", localized)?,
            data,
            effects,
        })
    }
}

impl Spell {
    /// Read this view without changing the source record.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong record type, malformed field lengths or
    /// missing required fields and malformed repeated groups.
    pub fn from_record(rec: &Record, localized: bool) -> Result<Self> {
        check_tag(rec, *b"SPEL")?;
        let mut f = required(rec, *b"SPIT", 36)?;
        let data = SpellData {
            base_cost: f.u32()?,
            flags: f.u32()?,
            spell_type: f.u32()?,
            charge_time: f.f32()?,
            cast_type: f.u32()?,
            delivery: f.u32()?,
            cast_duration: f.f32()?,
            range: f.f32()?,
            half_cost_perk: f.u32()?,
        };
        let mut effects = Vec::new();
        let mut pending: Option<(u32, Option<EffectData>, Vec<Condition>)> = None;
        for sub in &rec.subrecords {
            match sub.tag.as_bytes() {
                b"EFID" => {
                    if let Some(previous) = pending.take() {
                        effects.push(finish_effect(previous, Tag::new(b"SPEL"))?);
                    }
                    pending = Some((fields(rec, sub.tag, &sub.data, 4)?.u32()?, None, Vec::new()));
                }
                b"EFIT" => {
                    let current = pending
                        .as_mut()
                        .ok_or_else(|| Error::Invalid("EFIT without EFID".into()))?;
                    if current.1.is_some() {
                        return Err(Error::Invalid("repeated EFIT".into()));
                    }
                    let mut f = fields(rec, sub.tag, &sub.data, 12)?;
                    current.1 = Some(EffectData {
                        magnitude: f.f32()?,
                        area: f.u32()?,
                        duration: f.u32()?,
                    });
                }
                b"CTDA" | b"CIS1" | b"CIS2" => {
                    let current = pending
                        .as_mut()
                        .ok_or_else(|| Error::Invalid("spell condition without EFID".into()))?;
                    Condition::push(rec, sub, &mut current.2)?;
                }
                _ => {}
            }
        }
        if let Some(last) = pending {
            effects.push(finish_effect(last, Tag::new(b"SPEL"))?);
        }
        Ok(Self {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            name: text(rec, *b"FULL", localized)?,
            description: text(rec, *b"DESC", localized)?,
            equip_type: optional_u32(rec, *b"ETYP")?,
            data,
            effects,
        })
    }
}

fn finish_effect(
    (base_effect, data, conditions): (u32, Option<EffectData>, Vec<Condition>),
    record: Tag,
) -> Result<Effect> {
    Ok(Effect {
        base_effect,
        data: data.ok_or(Error::MissingSubrecord {
            record,
            tag: Tag::new(b"EFIT"),
        })?,
        conditions,
    })
}

/// Selected gameplay fields of the 152-byte DATA. See docs/esm.md for omissions.
#[derive(Debug, Clone, PartialEq)]
pub struct MagicEffectData {
    pub flags: u32,
    pub base_cost: f32,
    pub associated_item: u32,
    pub magic_skill: i32,
    pub resistance: i32,
    pub counter_effect_count: u16,
    pub taper_weight: f32,
    pub minimum_skill: u32,
    pub area: u32,
    pub casting_time: f32,
    pub taper_curve: f32,
    pub taper_duration: f32,
    pub second_actor_value_weight: f32,
    pub archetype: u32,
    pub primary_actor_value: i32,
    pub projectile: u32,
    pub explosion: u32,
    pub cast_type: u32,
    pub delivery: u32,
    pub second_actor_value: i32,
    pub skill_usage_multiplier: f32,
    pub dual_cast_scale: f32,
    pub equip_ability: u32,
    pub perk: u32,
    pub ai_score: f32,
    pub ai_delay_time: f32,
}

/// MGEF metadata and the scoped gameplay fields in its DATA subrecord.
#[derive(Debug, Clone, PartialEq)]
pub struct MagicEffect {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub name: Option<Text>,
    pub description: Option<Text>,
    pub data: MagicEffectData,
    pub keywords: Vec<u32>,
    pub counter_effects: Vec<u32>,
    pub conditions: Vec<Condition>,
}

impl MagicEffect {
    /// Read this view without changing the source record.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong record type, malformed field lengths or
    /// missing required fields and malformed repeated groups.
    pub fn from_record(rec: &Record, localized: bool) -> Result<Self> {
        check_tag(rec, *b"MGEF")?;
        let mut f = required(rec, *b"DATA", 152)?;
        let flags = f.u32()?;
        let base_cost = f.f32()?;
        let associated_item = f.u32()?;
        let magic_skill = f.i32()?;
        let resistance = f.i32()?;
        let counter_effect_count = f.u16()?;
        f.skip(2)?;
        let _casting_light = f.u32()?;
        let taper_weight = f.f32()?;
        f.skip(8)?;
        let minimum_skill = f.u32()?;
        let area = f.u32()?;
        let casting_time = f.f32()?;
        let taper_curve = f.f32()?;
        let taper_duration = f.f32()?;
        let second_actor_value_weight = f.f32()?;
        let archetype = f.u32()?;
        let primary_actor_value = f.i32()?;
        let projectile = f.u32()?;
        let explosion = f.u32()?;
        let cast_type = f.u32()?;
        let delivery = f.u32()?;
        let second_actor_value = f.i32()?;
        f.skip(12)?;
        let skill_usage_multiplier = f.f32()?;
        let _dual_cast_art = f.u32()?;
        let dual_cast_scale = f.f32()?;
        f.skip(12)?;
        let equip_ability = f.u32()?;
        f.skip(4)?;
        let perk = f.u32()?;
        f.skip(4)?;
        let ai_score = f.f32()?;
        let ai_delay_time = f.f32()?;
        Ok(Self {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            name: text(rec, *b"FULL", localized)?,
            description: text(rec, *b"DNAM", localized)?,
            data: MagicEffectData {
                flags,
                base_cost,
                associated_item,
                magic_skill,
                resistance,
                counter_effect_count,
                taper_weight,
                minimum_skill,
                area,
                casting_time,
                taper_curve,
                taper_duration,
                second_actor_value_weight,
                archetype,
                primary_actor_value,
                projectile,
                explosion,
                cast_type,
                delivery,
                second_actor_value,
                skill_usage_multiplier,
                dual_cast_scale,
                equip_ability,
                perk,
                ai_score,
                ai_delay_time,
            },
            keywords: super::super::form_id_list(rec, Tag::new(b"KWDA"))?,
            counter_effects: super::super::form_id_list(rec, Tag::new(b"ESCE"))?,
            conditions: Condition::all(rec)?,
        })
    }
}
