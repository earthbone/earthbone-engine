// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient item cards over the shared World, following local UESP item
//! layouts and SkyUI ItemCard.as. Armor ground models use MOD2/MOD4, never
//! the repeated MODL armature FormIDs. No menu owns inventory state.

use std::collections::BTreeSet;

use soe_esm::{cursor::Fields, Error, GlobalFormId, Record, Tag};
use soe_game::combat::records::Armor;
use soe_game::world::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// The resolved name presented by every item list and card.
pub fn display_name(world: &World, item: GlobalFormId) -> String {
    world
        .name_of(item)
        .filter(|name| !name.trim().is_empty() && !name.starts_with("<string "))
        .unwrap_or_else(|| "Unnamed item".into())
}

/// Selected item's authored statistics and inventory ground model.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemCard {
    pub name: String,
    pub model: Option<String>,
    pub weight: f32,
    pub value: u32,
    pub damage: Option<f32>,
    pub armor: Option<f32>,
}

impl ItemCard {
    /// Read a card from winning records, following weapon and armor templates.
    ///
    /// # Errors
    /// Rejects absent/deleted forms, template cycles and malformed numeric fields.
    pub fn read(world: &mut World, item: GlobalFormId) -> Result<Self, Error> {
        let female = world
            .player_character_traits()
            .is_some_and(|(_, female)| female);
        let child = world
            .record(item)
            .filter(|record| !record.is_deleted())
            .ok_or_else(|| Error::Invalid(format!("missing item {item}")))?;
        let name = display_name(world, item);
        let mut physical = child;
        let mut owner = item;
        if matches!(child.tag.as_bytes(), b"WEAP" | b"ARMO") {
            let template = if child.tag == T(b"ARMO") {
                T(b"TNAM")
            } else {
                T(b"CNAM")
            };
            let mut seen = BTreeSet::new();
            loop {
                if !seen.insert(owner) {
                    return Err(Error::Invalid(format!("item template cycle at {owner}")));
                }
                let Some(bytes) = physical.find(template) else {
                    break;
                };
                let mut field = Fields::new(physical.tag, template, bytes);
                field.exact(4)?;
                let raw = field.u32()?;
                if raw == 0 {
                    break;
                }
                owner = world
                    .resolve_from(owner, raw)
                    .ok_or_else(|| Error::Invalid("unresolved item template".into()))?;
                physical = world
                    .record(owner)
                    .filter(|record| record.tag == child.tag && !record.is_deleted())
                    .ok_or_else(|| Error::Invalid("invalid item template".into()))?;
            }
        }
        let (weight, value, damage, armor) = match child.tag.as_bytes() {
            b"WEAP" => {
                let mut field = data(physical, T(b"DATA"), 10)?;
                let value = field.u32()?;
                let weight = nonnegative(field.f32()?)?;
                (weight, value, Some(f32::from(field.u16()?)), None)
            }
            b"ARMO" => {
                let view = Armor::from_sources(child, physical)?;
                let rating = if view.kind == Some(2) {
                    0.0
                } else {
                    view.rating
                };
                (
                    view.weight.unwrap_or(0.0),
                    view.value.unwrap_or(0).max(0) as u32,
                    None,
                    Some(rating),
                )
            }
            b"AMMO" => {
                let bytes = child.required(T(b"DATA"))?;
                let mut field = Fields::new(child.tag, T(b"DATA"), bytes);
                if !matches!(bytes.len(), 16 | 20) {
                    return Err(Error::Invalid("AMMO DATA must be 16 or 20 bytes".into()));
                }
                field.skip(8)?;
                let damage = nonnegative(field.f32()?)?;
                let value = field.u32()?;
                let weight = if field.remaining() == 0 {
                    0.0
                } else {
                    nonnegative(field.f32()?)?
                };
                (weight, value, Some(damage), None)
            }
            b"LIGH" => {
                let mut field = data(child, T(b"DATA"), 48)?;
                field.skip(40)?;
                let value = field.u32()?;
                (nonnegative(field.f32()?)?, value, None, None)
            }
            b"ALCH" => {
                let weight = nonnegative(data(child, T(b"DATA"), 4)?.f32()?)?;
                let mut field = Fields::new(child.tag, T(b"ENIT"), child.required(T(b"ENIT"))?);
                (weight, field.u32()?, None, None)
            }
            b"BOOK" => {
                let mut field = data(child, T(b"DATA"), 16)?;
                field.skip(8)?;
                let value = field.u32()?;
                (nonnegative(field.f32()?)?, value, None, None)
            }
            b"MISC" | b"INGR" | b"KEYM" | b"SLGM" | b"SCRL" | b"APPA" => {
                let mut field = data(child, T(b"DATA"), 8)?;
                let value = field.u32()?;
                (nonnegative(field.f32()?)?, value, None, None)
            }
            _ => {
                return Err(Error::Invalid(format!(
                    "unsupported item card {}",
                    child.tag
                )))
            }
        };
        let model = if physical.tag == T(b"ARMO") {
            let bytes = if female {
                physical
                    .find(T(b"MOD4"))
                    .or_else(|| physical.find(T(b"MOD2")))
            } else {
                physical
                    .find(T(b"MOD2"))
                    .or_else(|| physical.find(T(b"MOD4")))
            };
            bytes.map(soe_esm::cursor::zstring)
        } else {
            physical.find(T(b"MODL")).map(soe_esm::cursor::zstring)
        }
        .filter(|path| !path.is_empty());
        Ok(Self {
            name,
            model,
            weight,
            value,
            damage,
            armor,
        })
    }

    /// Text shown below the model, with type-specific combat statistics.
    pub fn statistics(&self) -> String {
        let mut text = format!("Weight  {}\nValue  {}", self.weight, self.value);
        if let Some(damage) = self.damage {
            text.push_str(&format!("\nDamage  {damage}"));
        }
        if let Some(armor) = self.armor {
            text.push_str(&format!("\nArmor  {armor}"));
        }
        text
    }
}

fn data(record: &Record, tag: Tag, size: usize) -> Result<Fields<'_>, Error> {
    let fields = Fields::new(record.tag, tag, record.required(tag)?);
    fields.exact(size)?;
    Ok(fields)
}

fn nonnegative(value: f32) -> Result<f32, Error> {
    if !value.is_finite() || value < 0.0 {
        return Err(Error::Invalid("invalid item card number".into()));
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
