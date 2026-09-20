// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed view of an `NPC_` record: the actor base every `ACHR` places.
//! Layout from `wbRecord(NPC_` in `wbDefinitionsTES5.pas` and the UESP page
//! (`engine-refs/uesp-modfileformat/NPC.wikitext`). `ACBS` is the 24 byte
//! configuration block, `DNAM` the 52 byte stats block, and the template
//! flags in `ACBS` say which of those come from the `TPLT` record instead.

use soe_esm::records::Text;
use soe_esm::{Error, Record, Result, Tag};

pub const NPC: Tag = Tag::new(b"NPC_");

pub mod flags {
    pub const FEMALE: u32 = 0x0000_0001;
    pub const ESSENTIAL: u32 = 0x0000_0002;
    pub const RESPAWN: u32 = 0x0000_0008;
    pub const AUTO_CALC_STATS: u32 = 0x0000_0010;
    pub const UNIQUE: u32 = 0x0000_0020;
    pub const PC_LEVEL_MULT: u32 = 0x0000_0080;
    pub const PROTECTED: u32 = 0x0000_0800;
    pub const SUMMONABLE: u32 = 0x0000_4000;
    pub const BLEEDOUT_OVERRIDE: u32 = 0x0004_0000;
    pub const GHOST: u32 = 0x2000_0000;
    pub const INVULNERABLE: u32 = 0x8000_0000;
}

/// Which parts of a templated actor come from the template.
pub mod template {
    pub const TRAITS: u16 = 0x0001;
    pub const STATS: u16 = 0x0002;
    pub const FACTIONS: u16 = 0x0004;
    pub const SPELLS: u16 = 0x0008;
    pub const AI_DATA: u16 = 0x0010;
    pub const AI_PACKAGES: u16 = 0x0020;
    pub const BASE_DATA: u16 = 0x0080;
    pub const INVENTORY: u16 = 0x0100;
    pub const SCRIPT: u16 = 0x0200;
    pub const DEF_PACK_LIST: u16 = 0x0400;
    pub const ATTACK_DATA: u16 = 0x0800;
    pub const KEYWORDS: u16 = 0x1000;
}

/// Skill order in `DNAM`, which is also the actor value name order.
pub const SKILLS: [&str; 18] = [
    "OneHanded",
    "TwoHanded",
    "Marksman",
    "Block",
    "Smithing",
    "HeavyArmor",
    "LightArmor",
    "Pickpocket",
    "Lockpicking",
    "Sneak",
    "Alchemy",
    "Speechcraft",
    "Alteration",
    "Conjuration",
    "Destruction",
    "Illusion",
    "Restoration",
    "Enchanting",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Config {
    pub flags: u32,
    pub magicka_offset: i16,
    pub stamina_offset: i16,
    /// The level, or the level multiplier in thousandths when
    /// `PC_LEVEL_MULT` is set.
    pub level: u16,
    pub calc_min: u16,
    pub calc_max: u16,
    pub speed: u16,
    pub disposition: i16,
    pub template_flags: u16,
    pub health_offset: i16,
    pub bleedout: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    pub skills: [u8; 18],
    pub skill_offsets: [u8; 18],
    pub health: u16,
    pub magicka: u16,
    pub stamina: u16,
    pub far_away_distance: f32,
    pub geared_up_weapons: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Npc {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub name: Option<Text>,
    pub config: Config,
    pub stats: Option<Stats>,
    /// Raw FormIDs, resolve through the load order.
    pub race: Option<u32>,
    pub template: Option<u32>,
    pub class: Option<u32>,
    pub spells: Vec<u32>,
    /// `(faction, rank)`.
    pub factions: Vec<(u32, i8)>,
    /// `(item, count)`.
    pub items: Vec<(u32, i32)>,
    pub keywords: Vec<u32>,
    pub packages: Vec<u32>,
    /// `PRKR` perks, in record order.
    pub perks: Vec<u32>,
    /// `CRIF`, the faction crimes are reported to.
    pub crime_faction: Option<u32>,
    /// `DOFT` and `SOFT`, the default and sleep outfits.
    pub outfit: Option<u32>,
    pub sleep_outfit: Option<u32>,
}

impl Npc {
    pub fn from_record(rec: &Record, localized: bool) -> Result<Npc> {
        if rec.tag != NPC {
            return Err(Error::Invalid(format!("{} is not an NPC_ record", rec.tag)));
        }
        let mut n = Npc {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            name: None,
            config: Config::default(),
            stats: None,
            race: None,
            template: None,
            class: None,
            spells: Vec::new(),
            factions: Vec::new(),
            items: Vec::new(),
            keywords: Vec::new(),
            packages: Vec::new(),
            perks: Vec::new(),
            crime_faction: None,
            outfit: None,
            sleep_outfit: None,
        };
        for sub in &rec.subrecords {
            let d = sub.data.as_slice();
            match sub.tag.as_bytes() {
                b"FULL" => n.name = Some(text(d, localized)),
                b"ACBS" => n.config = config(d)?,
                b"DNAM" => n.stats = Some(stats(d)?),
                b"RNAM" => n.race = Some(u32_at(d, 0, "RNAM")?),
                b"TPLT" => n.template = Some(u32_at(d, 0, "TPLT")?),
                b"CNAM" => n.class = Some(u32_at(d, 0, "CNAM")?),
                b"SPLO" => n.spells.push(u32_at(d, 0, "SPLO")?),
                b"PKID" => n.packages.push(u32_at(d, 0, "PKID")?),
                b"PRKR" => n.perks.push(u32_at(d, 0, "PRKR")?),
                b"CRIF" => n.crime_faction = Some(u32_at(d, 0, "CRIF")?).filter(|f| *f != 0),
                b"DOFT" => n.outfit = Some(u32_at(d, 0, "DOFT")?).filter(|f| *f != 0),
                b"SOFT" => n.sleep_outfit = Some(u32_at(d, 0, "SOFT")?).filter(|f| *f != 0),
                b"SNAM" => n.factions.push((
                    u32_at(d, 0, "SNAM")?,
                    *d.get(4).ok_or_else(|| short("SNAM"))? as i8,
                )),
                b"CNTO" => {
                    let item = u32_at(d, 0, "CNTO")?;
                    let count = i32::from_le_bytes([d[4], d[5], d[6], d[7]]);
                    n.items.push((item, count));
                }
                b"KWDA" => n
                    .keywords
                    .extend(d.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c))),
                _ => {}
            }
        }
        Ok(n)
    }

    pub fn is_essential(&self) -> bool {
        self.config.flags & flags::ESSENTIAL != 0
    }

    pub fn is_protected(&self) -> bool {
        self.config.flags & flags::PROTECTED != 0
    }

    pub fn is_female(&self) -> bool {
        self.config.flags & flags::FEMALE != 0
    }

    pub fn uses_template(&self, part: u16) -> bool {
        self.template.is_some() && self.config.template_flags & part != 0
    }

    /// The level for a player at `player_level`: the fixed level, or the
    /// multiplier applied and clamped to the calc range.
    pub fn level_for(&self, player_level: u32) -> u32 {
        let c = &self.config;
        if c.flags & flags::PC_LEVEL_MULT == 0 {
            return u32::from(c.level).max(1);
        }
        let scaled = (f64::from(player_level) * f64::from(c.level) / 1000.0).round() as u32;
        let lo = u32::from(c.calc_min).max(1);
        let hi = if c.calc_max == 0 {
            u32::MAX
        } else {
            u32::from(c.calc_max)
        };
        scaled.clamp(lo, hi.max(lo))
    }

    /// Base actor values as `(lowercase name, value)`: health, magicka and
    /// stamina from `DNAM` plus the `ACBS` offsets, then the 18 skills. Race
    /// starting values are not read yet, so an actor with no `DNAM` gets
    /// zero for everything.
    pub fn base_values(&self) -> Vec<(String, f32)> {
        let mut out = Vec::with_capacity(21);
        let s = self.stats.unwrap_or_default();
        let c = &self.config;
        out.push((
            "health".into(),
            f32::from(s.health) + f32::from(c.health_offset),
        ));
        out.push((
            "magicka".into(),
            f32::from(s.magicka) + f32::from(c.magicka_offset),
        ));
        out.push((
            "stamina".into(),
            f32::from(s.stamina) + f32::from(c.stamina_offset),
        ));
        for (i, name) in SKILLS.iter().enumerate() {
            out.push((
                name.to_ascii_lowercase(),
                f32::from(s.skills[i]) + f32::from(s.skill_offsets[i]),
            ));
        }
        out
    }

    /// Take the parts the template flags name from `template`, one level.
    pub fn apply_template(&mut self, template: &Npc) {
        let f = self.config.template_flags;
        if f & template::STATS != 0 {
            let keep = self.config.template_flags;
            self.config = template.config;
            self.config.template_flags = keep;
            self.stats = template.stats;
            self.class = template.class;
        }
        if f & template::TRAITS != 0 {
            self.race = template.race.or(self.race);
        }
        if f & template::FACTIONS != 0 {
            self.factions.clone_from(&template.factions);
        }
        if f & template::SPELLS != 0 {
            self.spells.clone_from(&template.spells);
        }
        if f & template::INVENTORY != 0 {
            self.items.clone_from(&template.items);
        }
        if f & template::AI_PACKAGES != 0 {
            self.packages.clone_from(&template.packages);
        }
        if f & template::KEYWORDS != 0 {
            self.keywords.clone_from(&template.keywords);
        }
        if f & template::BASE_DATA != 0 {
            self.name.clone_from(&template.name);
        }
    }
}

fn config(d: &[u8]) -> Result<Config> {
    if d.len() < 24 {
        return Err(short("ACBS"));
    }
    let u16_at = |i: usize| u16::from_le_bytes([d[i], d[i + 1]]);
    Ok(Config {
        flags: u32::from_le_bytes([d[0], d[1], d[2], d[3]]),
        magicka_offset: u16_at(4) as i16,
        stamina_offset: u16_at(6) as i16,
        level: u16_at(8),
        calc_min: u16_at(10),
        calc_max: u16_at(12),
        speed: u16_at(14),
        disposition: u16_at(16) as i16,
        template_flags: u16_at(18),
        health_offset: u16_at(20) as i16,
        bleedout: u16_at(22),
    })
}

fn stats(d: &[u8]) -> Result<Stats> {
    if d.len() < 52 {
        return Err(short("DNAM"));
    }
    let mut s = Stats::default();
    s.skills.copy_from_slice(&d[0..18]);
    s.skill_offsets.copy_from_slice(&d[18..36]);
    let u16_at = |i: usize| u16::from_le_bytes([d[i], d[i + 1]]);
    s.health = u16_at(36);
    s.magicka = u16_at(38);
    s.stamina = u16_at(40);
    s.far_away_distance = f32::from_le_bytes([d[44], d[45], d[46], d[47]]);
    s.geared_up_weapons = d[48];
    Ok(s)
}

fn u32_at(d: &[u8], i: usize, tag: &str) -> Result<u32> {
    if d.len() < i + 4 {
        return Err(short(tag));
    }
    Ok(u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]))
}

fn short(tag: &str) -> Error {
    Error::Invalid(format!("NPC_: {tag} is too short"))
}

fn text(d: &[u8], localized: bool) -> Text {
    if localized && d.len() == 4 {
        Text::Localized(u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
    } else {
        Text::Plain(crate::quest::zstring(d))
    }
}

/// Builder for synthetic actor bases, used by the tests here and in the
/// integration suite.
pub fn build_record(
    form_id: u32,
    editor_id: &str,
    name: &str,
    config: Config,
    stats: Stats,
) -> Record {
    let mut acbs = Vec::with_capacity(24);
    acbs.extend_from_slice(&config.flags.to_le_bytes());
    acbs.extend_from_slice(&config.magicka_offset.to_le_bytes());
    acbs.extend_from_slice(&config.stamina_offset.to_le_bytes());
    acbs.extend_from_slice(&config.level.to_le_bytes());
    acbs.extend_from_slice(&config.calc_min.to_le_bytes());
    acbs.extend_from_slice(&config.calc_max.to_le_bytes());
    acbs.extend_from_slice(&config.speed.to_le_bytes());
    acbs.extend_from_slice(&config.disposition.to_le_bytes());
    acbs.extend_from_slice(&config.template_flags.to_le_bytes());
    acbs.extend_from_slice(&config.health_offset.to_le_bytes());
    acbs.extend_from_slice(&config.bleedout.to_le_bytes());
    let mut dnam = vec![0u8; 52];
    dnam[0..18].copy_from_slice(&stats.skills);
    dnam[18..36].copy_from_slice(&stats.skill_offsets);
    dnam[36..38].copy_from_slice(&stats.health.to_le_bytes());
    dnam[38..40].copy_from_slice(&stats.magicka.to_le_bytes());
    dnam[40..42].copy_from_slice(&stats.stamina.to_le_bytes());
    dnam[44..48].copy_from_slice(&stats.far_away_distance.to_le_bytes());
    dnam[48] = stats.geared_up_weapons;
    Record::new(NPC, form_id)
        .zsub(Tag::new(b"EDID"), editor_id)
        .zsub(Tag::new(b"FULL"), name)
        .sub(Tag::new(b"ACBS"), acbs)
        .sub(Tag::new(b"DNAM"), dnam)
}

pub fn with_faction(rec: Record, faction: u32, rank: i8) -> Record {
    let mut d = faction.to_le_bytes().to_vec();
    d.push(rank as u8);
    d.extend_from_slice(&[0, 0, 0]);
    rec.sub(Tag::new(b"SNAM"), d)
}

pub fn with_spell(rec: Record, spell: u32) -> Record {
    rec.sub(Tag::new(b"SPLO"), spell.to_le_bytes())
}

pub fn with_item(rec: Record, item: u32, count: i32) -> Record {
    let mut d = item.to_le_bytes().to_vec();
    d.extend_from_slice(&count.to_le_bytes());
    rec.sub(Tag::new(b"CNTO"), d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Record {
        let config = Config {
            flags: flags::ESSENTIAL | flags::PROTECTED | flags::PC_LEVEL_MULT,
            magicka_offset: -10,
            stamina_offset: 5,
            level: 1500,
            calc_min: 5,
            calc_max: 30,
            speed: 100,
            disposition: 0,
            template_flags: 0,
            health_offset: 20,
            bleedout: 0,
        };
        let mut stats = Stats {
            health: 100,
            magicka: 50,
            stamina: 80,
            far_away_distance: 2.5,
            geared_up_weapons: 1,
            ..Default::default()
        };
        stats.skills[0] = 25;
        stats.skill_offsets[0] = 3;
        stats.skills[9] = 40;
        let rec = build_record(0x900, "TestNpc", "Tester", config, stats)
            .sub(Tag::new(b"RNAM"), 0x13745u32.to_le_bytes())
            .sub(Tag::new(b"TPLT"), 0x901u32.to_le_bytes());
        let rec = with_faction(rec, 0x1234, -1);
        let rec = with_spell(rec, 0x2222);
        with_item(rec, 0x3333, 7)
    }

    #[test]
    fn an_npc_view_reads_config_stats_and_lists() {
        let n = Npc::from_record(&sample(), false).unwrap();
        assert_eq!(n.name, Some(Text::Plain("Tester".into())));
        assert!(n.is_essential() && n.is_protected() && !n.is_female());
        assert_eq!(n.race, Some(0x13745));
        assert_eq!(n.template, Some(0x901));
        assert_eq!(n.factions, vec![(0x1234, -1)]);
        assert_eq!(n.spells, vec![0x2222]);
        assert_eq!(n.items, vec![(0x3333, 7)]);
        let s = n.stats.unwrap();
        assert_eq!((s.health, s.magicka, s.stamina), (100, 50, 80));
        assert_eq!(s.far_away_distance, 2.5);
        let values = n.base_values();
        assert_eq!(values[0], ("health".into(), 120.0));
        assert_eq!(values[1], ("magicka".into(), 40.0));
        assert_eq!(values[2], ("stamina".into(), 85.0));
        assert_eq!(values[3], ("onehanded".into(), 28.0));
        assert_eq!(values[12], ("sneak".into(), 40.0));
    }

    #[test]
    fn level_follows_the_player_when_the_mult_flag_is_set() {
        let n = Npc::from_record(&sample(), false).unwrap();
        assert_eq!(n.level_for(1), 5, "clamped to calc min");
        assert_eq!(n.level_for(10), 15);
        assert_eq!(n.level_for(50), 30, "clamped to calc max");
        let mut fixed = n.clone();
        fixed.config.flags &= !flags::PC_LEVEL_MULT;
        fixed.config.level = 12;
        assert_eq!(fixed.level_for(50), 12);
    }

    #[test]
    fn a_template_supplies_the_flagged_parts_only() {
        let mut child = Npc::from_record(&sample(), false).unwrap();
        child.config.template_flags = template::STATS | template::SPELLS;
        let mut tmpl = child.clone();
        tmpl.config.level = 40;
        tmpl.config.flags = 0;
        tmpl.stats = Some(Stats {
            health: 999,
            ..Default::default()
        });
        tmpl.spells = vec![0x7777];
        tmpl.factions = vec![(0x9999, 0)];
        tmpl.race = Some(0x1);
        child.apply_template(&tmpl);
        assert_eq!(child.stats.unwrap().health, 999);
        assert_eq!(child.config.level, 40);
        assert_eq!(
            child.config.template_flags,
            template::STATS | template::SPELLS,
            "own flags kept"
        );
        assert_eq!(child.spells, vec![0x7777]);
        assert_eq!(child.factions, vec![(0x1234, -1)], "factions not flagged");
        assert_eq!(child.race, Some(0x13745), "traits not flagged");
    }

    #[test]
    fn malformed_blocks_are_refused() {
        let rec = Record::new(NPC, 1).sub(Tag::new(b"ACBS"), vec![0u8; 10]);
        assert!(Npc::from_record(&rec, false).is_err());
        let rec = Record::new(NPC, 1).sub(Tag::new(b"DNAM"), vec![0u8; 20]);
        assert!(Npc::from_record(&rec, false).is_err());
        let rec = Record::new(NPC, 1).sub(Tag::new(b"SNAM"), vec![0u8; 4]);
        assert!(Npc::from_record(&rec, false).is_err());
        let rec = Record::new(Tag::new(b"STAT"), 1);
        assert!(Npc::from_record(&rec, false).is_err());
        let rec = Record::new(NPC, 1);
        let n = Npc::from_record(&rec, false).unwrap();
        assert_eq!(
            n.base_values()[0].1,
            0.0,
            "no DNAM means zero, race defaults are not read"
        );
    }
}
