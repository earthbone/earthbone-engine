// SPDX-License-Identifier: GPL-3.0-or-later

//! WEAP DATA/DNAM and ARMO DNAM/BOD2/BODT from local xEdit and UESP layouts.
//! These are transient record views, not another authoritative item database.

use std::collections::BTreeSet;

use soe_esm::{cursor::Fields, Error, GlobalFormId, LoadOrder, Record, Tag};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weapon {
    pub damage: u16,
    pub weight: f32,
    pub animation: u8,
    pub speed: f32,
    pub reach: f32,
    pub skill: i32,
    pub resist: i32,
    pub flags: u16,
    pub flags2: u32,
    /// DNAM's trailing Stagger float, 0.0 to 1.0 in vanilla data. Zero means
    /// the weapon never staggers, matching `ProcessTrapHit`'s afStagger
    /// convention for that same 0.0 to 1.0 range.
    pub stagger: f32,
    pub template: Option<u32>,
    pub enchantment: Option<u32>,
}

impl Weapon {
    /// Decode physical weapon inputs without guessing the governing skill from animation.
    ///
    /// # Errors
    /// Wrong/deleted forms, missing or malformed fields, non-finite floats.
    pub fn from_record(record: &Record) -> Result<Self, Error> {
        check_record(record, T(b"WEAP"))?;
        let mut data = Fields::new(record.tag, T(b"DATA"), record.required(T(b"DATA"))?);
        data.exact(10)?;
        data.skip(4)?;
        let weight = number(data.f32()?)?;
        let damage = data.u16()?;
        let mut data = Fields::new(record.tag, T(b"DNAM"), record.required(T(b"DNAM"))?);
        data.exact(100)?;
        let animation = data.u8()?;
        data.skip(3)?;
        let speed = number(data.f32()?)?;
        let reach = number(data.f32()?)?;
        let flags = data.u16()?;
        data.skip(26)?;
        let flags2 = data.u32()?;
        data.skip(32)?;
        let skill = data.i32()?;
        data.skip(8)?;
        let resist = data.i32()?;
        data.skip(4)?;
        let stagger = number(data.f32()?)?;
        Ok(Self {
            damage,
            weight,
            animation,
            speed,
            reach,
            skill,
            resist,
            flags,
            flags2,
            stagger,
            template: link(record, T(b"CNAM"))?,
            enchantment: link(record, T(b"EITM"))?,
        })
    }

    /// Decode only the links needed to follow a weapon template chain.
    ///
    /// Template children may omit combat fields supplied by their terminal
    /// template, so callers must not require DATA or DNAM before following CNAM.
    ///
    /// # Errors
    /// Wrong/deleted forms or malformed links.
    pub fn template_links(record: &Record) -> Result<(Option<u32>, Option<u32>), Error> {
        check_record(record, T(b"WEAP"))?;
        Ok((link(record, T(b"CNAM"))?, link(record, T(b"EITM"))?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Armor {
    pub value: Option<i32>,
    pub weight: Option<f32>,
    pub rating: f32,
    pub slots: u32,
    /// 0 light, 1 heavy, 2 clothing, absent in early BODT records.
    pub kind: Option<u32>,
    pub template: Option<u32>,
    pub enchantment: Option<u32>,
    /// BIDS, the impact data set a shield bash plays. UESP's ARMO layout
    /// (lines 64 to 68) documents BIDS/BAMT as shield-only fields, absent
    /// from every other armor piece. See docs/combat.md CMB-20.
    pub bash_impact: Option<u32>,
    /// BAMT, the alternate material a shield bash reports to its impact effect.
    pub bash_material: Option<u32>,
}

/// The records that supply an armor's Creation Kit template fields.
///
/// `child` supplies FULL, DATA value and EITM. `inherited` supplies every
/// other ARMO field after following TNAM to its terminal source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmorSources {
    pub child: GlobalFormId,
    pub inherited: GlobalFormId,
}

impl Armor {
    /// Resolve a template chain in each winning record's defining-file namespace.
    ///
    /// # Errors
    /// Missing, deleted, malformed, non-ARMO or cyclic template records.
    pub fn sources(order: &LoadOrder, child: GlobalFormId) -> Result<ArmorSources, Error> {
        let mut inherited = child;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(inherited) {
                return Err(Error::Invalid(format!(
                    "ARMO template cycle at {inherited}"
                )));
            }
            let owner = order
                .winner_ref(inherited)
                .ok_or_else(|| Error::Invalid(format!("missing ARMO template {inherited}")))?;
            let record = order.record(owner);
            check_record(record, T(b"ARMO"))?;
            let Some(template) = link(record, T(b"TNAM"))? else {
                return Ok(ArmorSources { child, inherited });
            };
            inherited = order.resolve(owner.file, template);
        }
    }

    /// Decode a non-templated armor input record.
    ///
    /// # Errors
    /// Wrong/deleted forms, missing or malformed fields.
    pub fn from_record(record: &Record) -> Result<Self, Error> {
        Self::from_sources(record, record)
    }

    /// Decode child value/enchantment fields and inherited physical properties.
    ///
    /// # Errors
    /// Wrong/deleted forms, missing or malformed fields, non-finite weights.
    pub fn from_sources(child: &Record, inherited: &Record) -> Result<Self, Error> {
        check_record(child, T(b"ARMO"))?;
        check_record(inherited, T(b"ARMO"))?;
        let value = armor_data(child)?.map(|data| data.0);
        let weight = armor_data(inherited)?.map(|data| data.1);
        let mut rating = Fields::new(inherited.tag, T(b"DNAM"), inherited.required(T(b"DNAM"))?);
        rating.exact(4)?;
        // UESP documents the runtime's low-16-bit rating, stored in hundredths.
        let rating = (rating.u32()? & 0xffff) as f32 / 100.0;
        let (tag, bytes) = if let Some(bytes) = inherited.find(T(b"BOD2")) {
            (T(b"BOD2"), bytes)
        } else {
            (T(b"BODT"), inherited.required(T(b"BODT"))?)
        };
        let mut body = Fields::new(inherited.tag, tag, bytes);
        let slots = body.u32()?;
        if tag == T(b"BODT") {
            body.skip(4)?;
        }
        let kind = if body.remaining() == 0 {
            None
        } else {
            Some(body.u32()?)
        };
        if body.remaining() != 0 {
            return Err(Error::Invalid("excess armor body data".into()));
        }
        Ok(Self {
            value,
            weight,
            rating,
            slots,
            kind,
            template: link(child, T(b"TNAM"))?,
            enchantment: link(child, T(b"EITM"))?,
            // BIDS/BAMT follow the TNAM template chain like the rest of a
            // shield's physical fields, so they come from `inherited` rather
            // than `child`. TES5Edit's WEAP layout (lines 10444-10445) shows
            // the same two subrecords on WEAP for hand-to-hand bash sources,
            // out of scope here since this row restricts bashing to shields.
            bash_impact: link(inherited, T(b"BIDS"))?,
            bash_material: link(inherited, T(b"BAMT"))?,
        })
    }

    /// Decode only the links needed to follow an armor template chain.
    ///
    /// Template children may omit combat fields supplied by their terminal
    /// template, so callers must not require DNAM or BOD2/BODT before following
    /// TNAM.
    ///
    /// # Errors
    /// Wrong/deleted forms or malformed links.
    pub fn template_links(record: &Record) -> Result<(Option<u32>, Option<u32>), Error> {
        check_record(record, T(b"ARMO"))?;
        Ok((link(record, T(b"TNAM"))?, link(record, T(b"EITM"))?))
    }
}

fn check_record(record: &Record, tag: Tag) -> Result<(), Error> {
    if record.tag != tag || record.is_deleted() {
        return Err(Error::Invalid(format!("expected non-deleted {tag}")));
    }
    Ok(())
}

fn number(value: f32) -> Result<f32, Error> {
    if !value.is_finite() || value < 0.0 {
        return Err(Error::Invalid("invalid combat record float".into()));
    }
    Ok(value)
}

fn armor_data(record: &Record) -> Result<Option<(i32, f32)>, Error> {
    let Some(bytes) = record.find(T(b"DATA")) else {
        return Ok(None);
    };
    let mut data = Fields::new(record.tag, T(b"DATA"), bytes);
    data.exact(8)?;
    Ok(Some((data.i32()?, number(data.f32()?)?)))
}

fn link(record: &Record, tag: Tag) -> Result<Option<u32>, Error> {
    let Some(bytes) = record.find(tag) else {
        return Ok(None);
    };
    let mut field = Fields::new(record.tag, tag, bytes);
    field.exact(4)?;
    Ok(Some(field.u32()?).filter(|id| *id != 0))
}
