// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared body-part selection from live NPC_, RACE, ARMO and ARMA records.
//! Layouts follow xEdit, armor-parent matching follows CommonLib's
//! TESObjectARMA::IsValidRace. Equal priority uses the existing footstep
//! policy of later addon and sorted equipment order.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::cursor::{zstring, Fields};
use soe_esm::{Error, GlobalFormId, LoadOrder, Record, Result, Tag};

use crate::combat::records::Armor;
use crate::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// One surviving addon, with slots numbered from Skyrim body part 30.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmorPart {
    pub armor: GlobalFormId,
    pub addon: GlobalFormId,
    pub authored_slots: u32,
    pub visible_slots: u32,
    pub model: Option<String>,
    pub first_person_model: Option<String>,
    pub footsteps: Option<GlobalFormId>,
}

/// A derived view for rendering and sound, never carried inventory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActorAppearance {
    pub race: Option<GlobalFormId>,
    pub female: bool,
    pub skin: Option<GlobalFormId>,
    pub parts: Vec<ArmorPart>,
}

/// Editable player head-part and morph selection stored with the player actor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerAppearance {
    pub head_parts: BTreeMap<u32, GlobalFormId>,
    pub morphs: BTreeMap<String, f32>,
}

#[derive(Default)]
struct Traits {
    race: Option<GlobalFormId>,
    skin: Option<GlobalFormId>,
    female: bool,
}

enum Records<'a> {
    World(&'a World),
    Order(&'a LoadOrder),
}

impl World {
    /// Resolve current race, sex, skin and equipment from authoritative state.
    pub fn actor_appearance(&mut self, actor: GlobalFormId) -> Result<ActorAppearance> {
        let base = self.reference(actor).and_then(|r| r.base);
        let traits_source = if base.is_some() {
            Some(self.actor_component_source(actor, crate::npc::template::TRAITS)?)
        } else {
            None
        };
        let traits = if let Some(source) = traits_source {
            let distributed_skin = base.and_then(|base| {
                self.mod_data
                    .for_base(base)
                    .iter()
                    .rev()
                    .find(|distribution| {
                        distribution.record_type == soe_moddata::spid::RecordType::Skin
                    })
                    .map(|distribution| distribution.what)
            });
            Traits {
                race: self.actor_base_race(source)?,
                skin: distributed_skin.or(self.skin(source)?),
                female: self.npc_record(source)?.is_female(),
            }
        } else {
            Traits::default()
        };
        let state = self
            .actor(actor)
            .ok_or_else(|| Error::Invalid(format!("appearance requires an actor: {actor}")))?;
        let race = state.race;
        let equipped = state.equipped.clone();
        Records::World(self).assemble(&traits, race, &equipped)
    }
}

impl ActorAppearance {
    /// Read plugin records when no runtime world is available.
    pub fn from_order(
        order: &LoadOrder,
        base: Option<GlobalFormId>,
        current_race: Option<GlobalFormId>,
        equipped: &BTreeSet<GlobalFormId>,
    ) -> Result<Self> {
        let records = Records::Order(order);
        let traits = base
            .map(|id| records.traits(id))
            .transpose()?
            .unwrap_or_default();
        records.assemble(&traits, current_race, equipped)
    }
}

impl Records<'_> {
    fn order(&self) -> &LoadOrder {
        match self {
            Self::World(world) => world.order(),
            Self::Order(order) => order,
        }
    }

    fn record(&self, id: GlobalFormId, tag: Tag) -> Result<&Record> {
        let record = match self {
            Self::World(world) => world.record(id),
            Self::Order(order) => order.winner(id),
        }
        .ok_or_else(|| Error::Invalid(format!("missing {tag} {id}")))?;
        if record.tag != tag || record.is_deleted() {
            return Err(Error::Invalid(format!(
                "expected live {tag} at {id}, got {}",
                record.tag
            )));
        }
        Ok(record)
    }

    fn link(&self, owner: GlobalFormId, raw: u32) -> Option<GlobalFormId> {
        if raw == 0 {
            return None;
        }
        match self {
            Self::World(world) => world.resolve_from(owner, raw),
            Self::Order(order) => order
                .winner_ref(owner)
                .map(|file| order.resolve(file.file, raw)),
        }
    }

    fn record_link(
        &self,
        record: &Record,
        owner: GlobalFormId,
        tag: Tag,
    ) -> Result<Option<GlobalFormId>> {
        let Some(bytes) = record.find(tag) else {
            return Ok(None);
        };
        let mut field = Fields::new(record.tag, tag, bytes);
        field.exact(4)?;
        Ok(self.link(owner, field.u32()?))
    }

    fn assemble(
        &self,
        traits: &Traits,
        current_race: Option<GlobalFormId>,
        equipped: &BTreeSet<GlobalFormId>,
    ) -> Result<ActorAppearance> {
        let race = current_race.or(traits.race);
        let mut out = ActorAppearance {
            race,
            female: traits.female,
            ..Default::default()
        };
        let mut slots = BTreeMap::new();
        let mut candidates = Vec::new();
        if let Some(race) = race {
            let races = self.armor_races(race)?;
            let record = self.record(race, T(b"RACE"))?;
            out.skin = match traits.skin {
                Some(skin) => Some(skin),
                None => self.record_link(record, race, T(b"WNAM"))?,
            };
            if let Some(skin) = out.skin {
                self.armor_parts(skin, &races, traits.female, &mut slots, &mut candidates)?;
            }
            for armor in equipped {
                if self
                    .order()
                    .winner(*armor)
                    .is_some_and(|r| r.tag == T(b"ARMO"))
                {
                    self.armor_parts(*armor, &races, traits.female, &mut slots, &mut candidates)?;
                }
            }
        }
        for (slot, (_, index)) in slots {
            candidates[index].visible_slots |= 1 << slot;
        }
        out.parts = candidates
            .into_iter()
            .filter(|part| part.visible_slots != 0)
            .collect();
        Ok(out)
    }

    fn traits(&self, mut base: GlobalFormId) -> Result<Traits> {
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(base) {
                return Err(Error::Invalid(format!(
                    "NPC_ trait template cycle at {base}"
                )));
            }
            let record = self.record(base, T(b"NPC_"))?;
            let file = base;
            let (female, inherited) = if let Some(bytes) = record.find(T(b"ACBS")) {
                Fields::new(record.tag, T(b"ACBS"), bytes).exact(24)?;
                let flags = Fields::new(record.tag, T(b"ACBS"), bytes).u32()?;
                let template = u16::from_le_bytes([bytes[18], bytes[19]]);
                (
                    flags & crate::npc::flags::FEMALE != 0,
                    template & crate::npc::template::TRAITS != 0,
                )
            } else {
                (false, false)
            };
            if inherited {
                if let Some(template) = self.record_link(record, file, T(b"TPLT"))? {
                    base = template;
                    continue;
                }
            }
            return Ok(Traits {
                race: self.record_link(record, file, T(b"RNAM"))?,
                skin: self.record_link(record, file, T(b"WNAM"))?,
                female,
            });
        }
    }

    fn armor_races(&self, mut race: GlobalFormId) -> Result<BTreeSet<GlobalFormId>> {
        let mut races = BTreeSet::new();
        loop {
            if !races.insert(race) {
                return Err(Error::Invalid(format!("RACE armor-parent cycle at {race}")));
            }
            let record = self.record(race, T(b"RACE"))?;
            let file = race;
            let Some(parent) = self.record_link(record, file, T(b"RNAM"))? else {
                return Ok(races);
            };
            race = parent;
        }
    }

    fn armor_parts(
        &self,
        armor: GlobalFormId,
        races: &BTreeSet<GlobalFormId>,
        female: bool,
        parts: &mut BTreeMap<u32, (u8, usize)>,
        candidates: &mut Vec<ArmorPart>,
    ) -> Result<()> {
        let sources = Armor::sources(self.order(), armor)?;
        let record = self.record(sources.inherited, T(b"ARMO"))?;
        let file = sources.inherited;
        for bytes in record.all(T(b"MODL")) {
            let mut field = Fields::new(record.tag, T(b"MODL"), bytes);
            field.exact(4)?;
            let Some(addon) = self.link(file, field.u32()?) else {
                continue;
            };
            let record = self.record(addon, T(b"ARMA"))?;
            let owner = addon;
            let mut matches = self
                .record_link(record, owner, T(b"RNAM"))?
                .is_some_and(|race| races.contains(&race));
            for bytes in record.all(T(b"MODL")) {
                let mut field = Fields::new(record.tag, T(b"MODL"), bytes);
                field.exact(4)?;
                matches |= self
                    .link(owner, field.u32()?)
                    .is_some_and(|r| races.contains(&r));
            }
            if !matches {
                continue;
            }
            let data = record.required(T(b"DNAM"))?;
            Fields::new(record.tag, T(b"DNAM"), data).exact(12)?;
            let priority = data[usize::from(female)];
            let set = self.record_link(record, owner, T(b"SNDD"))?;
            if let Some(set) = set {
                self.record(set, T(b"FSTS"))?;
            }
            let slots = slots(record)?;
            let index = candidates.len();
            candidates.push(ArmorPart {
                armor,
                addon,
                authored_slots: slots,
                visible_slots: 0,
                model: model(record, if female { *b"MOD3" } else { *b"MOD2" }),
                first_person_model: model(record, if female { *b"MOD5" } else { *b"MOD4" }),
                footsteps: set,
            });
            for slot in 0..32 {
                if slots & (1 << slot) != 0
                    && parts.get(&slot).is_none_or(|part| part.0 <= priority)
                {
                    // Silent addons also replace covered skin parts.
                    parts.insert(slot, (priority, index));
                }
            }
        }
        Ok(())
    }
}

fn slots(record: &Record) -> Result<u32> {
    for (tag, sizes) in [(T(b"BOD2"), &[4, 8][..]), (T(b"BODT"), &[8, 12][..])] {
        if let Some(data) = record.find(tag) {
            if !sizes.contains(&data.len()) {
                return Err(Error::Invalid(format!(
                    "{} {tag} has invalid length",
                    record.tag
                )));
            }
            return Fields::new(record.tag, tag, data).u32();
        }
    }
    Ok(0)
}

fn model(record: &Record, tag: [u8; 4]) -> Option<String> {
    record
        .find(T(&tag))
        .map(zstring)
        .filter(|path| !path.is_empty())
}
