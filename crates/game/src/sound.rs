// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound playback parameters from winning SOUN, SNDR, SNCT and SOPM records.
//! Layouts follow xEdit wbDefinitionsTES5.pas, UESP's record pages and
//! CommonLibSSE-NG BGSStandardSoundDef/BGSSoundOutput declarations.
//! File, pitch and attenuation draws are supplied by the caller's saved RNG.
//! Conditional descriptors use the shared CTDA evaluator with the initiating
//! source as subject and the descriptor as the FormID-resolution owner.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::cursor::{zstring, Fields};
use soe_esm::records::gameplay::Condition;
use soe_esm::{Error, GlobalFormId, LoadOrder, Record, Result, Tag};
use soe_papyrus::Vm;

use crate::conditions::{self, Context};
use crate::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

#[derive(Clone, Debug, PartialEq)]
pub struct Attenuation {
    pub min_distance: f32,
    pub max_distance: f32,
    pub curve: [u8; 5],
}

impl Attenuation {
    /// Interpolate the five percentage samples across the authored distance range.
    /// Distances use Skyrim units. Invalid distances and positions beyond the range are silent.
    pub fn gain_at_distance(&self, distance: f32) -> f32 {
        if !distance.is_finite() || distance > self.max_distance {
            return 0.0;
        }
        if distance <= self.min_distance || self.max_distance <= self.min_distance {
            return f32::from(self.curve[0]) / 100.0;
        }
        let point = (distance - self.min_distance) / (self.max_distance - self.min_distance) * 4.0;
        let index = (point.floor() as usize).min(3);
        let fraction = point - index as f32;
        (f32::from(self.curve[index]) * (1.0 - fraction)
            + f32::from(self.curve[index + 1]) * fraction)
            / 100.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundOutput {
    /// 0 is positional HRTF, 1 is an authored speaker matrix.
    pub kind: u32,
    pub attenuates: bool,
    pub attenuation: Option<Attenuation>,
    pub speaker_matrix: Option<[u8; 24]>,
    pub reverb_send: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundPlayback {
    pub descriptor: GlobalFormId,
    pub data_path: String,
    pub gain: f32,
    pub pitch: f32,
    pub priority: u8,
    /// Raw LNAM loop mode: 0, 8, 16 or 32.
    pub looping: u8,
    pub output: SoundOutput,
    /// Retained as authored. Resident alternate-sound routing is separate work.
    pub alternate_for: Option<GlobalFormId>,
}

/// Category slider values are caller-owned runtime state. Missing sliders use
/// the record's default. Underwater muting follows every ancestor category.
#[derive(Default)]
pub struct SoundSettings {
    pub category_volumes: BTreeMap<GlobalFormId, f32>,
    pub submerged: bool,
}

/// Parameters derived for an INFO voice file, retaining its category identity.
#[derive(Clone, Debug, PartialEq)]
pub struct DialoguePlayback {
    pub category: GlobalFormId,
    pub gain: f32,
    pub output: SoundOutput,
}

/// Dialogue files use DOBJ's voice category and conversation or ambient output.
/// Reuses SNCT ancestor gains and SOPM parsing without inventing a descriptor.
pub fn prepare_dialogue(
    order: &LoadOrder,
    ambient: bool,
    settings: &SoundSettings,
) -> Result<DialoguePlayback> {
    let defaults = crate::footsteps::Footsteps::new(order);
    let category = defaults
        .default_object(*b"DDSC")?
        .ok_or_else(|| Error::Invalid("missing dialogue voice category DDSC".into()))?;
    let key = if ambient { *b"DOP3" } else { *b"DOP2" };
    let output = defaults
        .default_object(key)?
        .ok_or_else(|| Error::Invalid("missing dialogue output model".into()))?;
    Ok(DialoguePlayback {
        category,
        gain: category_gain(order, Some(category), settings)?,
        output: read_output(order, output)?,
    })
}

/// Gain for a DOBJ-linked sound category, including parent and default sliders.
pub fn default_category_gain(
    order: &LoadOrder,
    key: [u8; 4],
    settings: &SoundSettings,
) -> Result<f32> {
    let category = crate::footsteps::Footsteps::new(order)
        .default_object(key)?
        .ok_or_else(|| Error::Invalid("missing default sound category".into()))?;
    category_gain(order, Some(category), settings)
}

/// Resolve a descriptor into one playback request. Draws select the file,
/// signed frequency variation and additional attenuation, in that order.
/// Uniform variation is an engine policy, not a verified vanilla RNG sequence.
pub fn prepare(
    order: &LoadOrder,
    sound: GlobalFormId,
    draws: [f32; 3],
    settings: &SoundSettings,
) -> Result<SoundPlayback> {
    let descriptor = resolve_descriptor(order, sound)?;
    let conditions = sound_conditions(order, descriptor)?;
    if !conditions.is_empty() {
        return Err(Error::Invalid(format!(
            "sound descriptor {descriptor} requires condition context"
        )));
    }
    prepare_unconditional(order, descriptor, draws, settings)
}

/// Resolve a descriptor for an initiating source. The descriptor's winning
/// record supplies the FormID namespace for CTDA parameters. A false CTDA
/// result means this descriptor has no playable branch for the invocation.
pub fn prepare_for(
    order: &LoadOrder,
    world: &mut World,
    vm: &Vm,
    descriptor: GlobalFormId,
    source: GlobalFormId,
    target: Option<GlobalFormId>,
    draws: [f32; 3],
    settings: &SoundSettings,
) -> Result<SoundPlayback> {
    let descriptor = resolve_descriptor(order, descriptor)?;
    let conditions = sound_conditions(order, descriptor)?;
    if !conditions.is_empty()
        && !conditions::evaluate(
            world,
            vm,
            Context {
                owner: descriptor,
                subject: source,
                target,
                quest: None,
                tempering_enchanted: None,
                tempering_item: None,
                alchemy_making_poison: None,
                alchemy_effect: None,
                magic_item: None,
                dual_cast: None,
                skill_use: None,
                light_level: None,
            },
            &conditions,
        )
        .map_err(|error| Error::Invalid(format!("sound condition at {descriptor}: {error}")))?
    {
        return Err(Error::Invalid(format!(
            "sound descriptor conditions do not match at {descriptor}"
        )));
    }
    prepare_unconditional(order, descriptor, draws, settings)
}

fn prepare_unconditional(
    order: &LoadOrder,
    descriptor: GlobalFormId,
    draws: [f32; 3],
    settings: &SoundSettings,
) -> Result<SoundPlayback> {
    if draws
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(Error::Invalid(
            "sound draws must be finite values in 0 to 1".into(),
        ));
    }
    let (record, file) = record(order, descriptor, T(b"SNDR"))?;
    if field(record, T(b"CNAM"), 4)?.u32()? != 0x1EEF_540A {
        return Err(Error::Invalid(format!(
            "unsupported sound descriptor type at {descriptor}"
        )));
    }
    let files: Vec<_> = record
        .all(T(b"ANAM"))
        .map(zstring)
        .filter(|s| !s.is_empty())
        .collect();
    if files.is_empty() {
        return Err(Error::Invalid(format!("sound {descriptor} has no files")));
    }
    let index = ((draws[0] * files.len() as f32) as usize).min(files.len() - 1);
    let data_path = soe_bsa::normalize(&files[index]);
    let mut values = field(record, T(b"BNAM"), 6)?;
    let shift = values.u8()? as i8;
    let variance = values.u8()?;
    let priority = values.u8()?;
    let db_variance = values.u8()?;
    let attenuation = f32::from(values.u16()?) / 100.0;
    let pitch = 1.0 + (f32::from(shift) + (draws[1] * 2.0 - 1.0) * f32::from(variance)) / 100.0;
    if pitch <= 0.0 {
        return Err(Error::Invalid(format!(
            "sound {descriptor} has nonpositive pitch"
        )));
    }
    let category = link(order, record, file, T(b"GNAM"))?;
    let gain = category_gain(order, category, settings)?
        * 10.0_f32.powf(-(attenuation + draws[2] * f32::from(db_variance)) / 20.0);
    let output = link(order, record, file, T(b"ONAM"))?
        .ok_or_else(|| Error::Invalid(format!("sound {descriptor} has no output model")))?;
    let looping = if record.find(T(b"LNAM")).is_some() {
        let mut length = field(record, T(b"LNAM"), 4)?;
        length.skip(1)?;
        length.u8()?
    } else if record.find(T(b"FNAM")).is_some() {
        if field(record, T(b"FNAM"), 4)?.u32()? & 16 != 0 {
            8
        } else {
            0
        }
    } else {
        0
    };
    if ![0, 8, 16, 32].contains(&looping) {
        return Err(Error::Invalid(format!("unknown sound loop mode {looping}")));
    }
    Ok(SoundPlayback {
        descriptor,
        data_path,
        gain,
        pitch,
        priority,
        looping,
        output: read_output(order, output)?,
        alternate_for: link(order, record, file, T(b"SNAM"))?,
    })
}

fn sound_conditions(order: &LoadOrder, descriptor: GlobalFormId) -> Result<Vec<Condition>> {
    let (record, _) = record(order, descriptor, T(b"SNDR"))?;
    let mut conditions = Vec::new();
    for subrecord in &record.subrecords {
        Condition::push(record, subrecord, &mut conditions)?;
    }
    Ok(conditions)
}

fn category_gain(
    order: &LoadOrder,
    mut id: Option<GlobalFormId>,
    settings: &SoundSettings,
) -> Result<f32> {
    let mut visited = BTreeSet::new();
    let mut gain = 1.0;
    while let Some(category) = id {
        if !visited.insert(category) {
            return Err(Error::Invalid(format!(
                "sound category cycle at {category}"
            )));
        }
        let (record, file) = record(order, category, T(b"SNCT"))?;
        let flags = field(record, T(b"FNAM"), 4)?.u32()?;
        if settings.submerged && flags & 1 != 0 {
            gain = 0.0;
        }
        gain *= f32::from(field(record, T(b"VNAM"), 2)?.u16()?) / 65535.0;
        if flags & 2 != 0 {
            let value = if let Some(value) = settings.category_volumes.get(&category) {
                *value
            } else if record.find(T(b"UNAM")).is_some() {
                f32::from(field(record, T(b"UNAM"), 2)?.u16()?) / 65535.0
            } else {
                1.0
            };
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(Error::Invalid(format!(
                    "invalid category volume at {category}"
                )));
            }
            gain *= value;
        }
        id = link(order, record, file, T(b"PNAM"))?;
    }
    Ok(gain)
}

fn read_output(order: &LoadOrder, id: GlobalFormId) -> Result<SoundOutput> {
    let (record, _) = record(order, id, T(b"SOPM"))?;
    let kind = field(record, T(b"MNAM"), 4)?.u32()?;
    if kind > 1 {
        return Err(Error::Invalid(format!("unknown sound output type {kind}")));
    }
    let (attenuates, reverb_send) = if record.find(T(b"NAM1")).is_some() {
        let mut data = field(record, T(b"NAM1"), 4)?;
        let flags = data.u8()?;
        data.skip(2)?;
        (flags & 1 != 0, f32::from(data.u8()?) / 100.0)
    } else {
        (false, 0.0)
    };
    let attenuation = if record.find(T(b"ANAM")).is_some() {
        let mut data = field(record, T(b"ANAM"), 20)?;
        data.skip(4)?;
        let min_distance = data.f32()?;
        let max_distance = data.f32()?;
        if !min_distance.is_finite()
            || !max_distance.is_finite()
            || min_distance < 0.0
            || max_distance < min_distance
        {
            return Err(Error::Invalid(format!("invalid sound distances at {id}")));
        }
        let mut curve = [0; 5];
        curve.copy_from_slice(data.bytes(5)?);
        Some(Attenuation {
            min_distance,
            max_distance,
            curve,
        })
    } else {
        None
    };
    let speaker_matrix = if record.find(T(b"ONAM")).is_some() {
        let mut matrix = [0; 24];
        matrix.copy_from_slice(field(record, T(b"ONAM"), 24)?.bytes(24)?);
        Some(matrix)
    } else {
        None
    };
    Ok(SoundOutput {
        kind,
        attenuates,
        attenuation,
        speaker_matrix,
        reverb_send,
    })
}

fn record(order: &LoadOrder, id: GlobalFormId, tag: Tag) -> Result<(&Record, usize)> {
    let owner = order
        .winner_ref(id)
        .ok_or_else(|| Error::Invalid(format!("missing {tag} {id}")))?;
    let record = order.record(owner);
    if record.tag != tag || record.is_deleted() {
        return Err(Error::Invalid(format!("expected live {tag} at {id}")));
    }
    Ok((record, owner.file))
}

/// A SOUN marker delegates playback to its required SNDR through SDSC.
/// The FormID is relative to the marker's winning file.
fn resolve_descriptor(order: &LoadOrder, sound: GlobalFormId) -> Result<GlobalFormId> {
    let owner = order
        .winner_ref(sound)
        .ok_or_else(|| Error::Invalid(format!("missing SNDR or SOUN {sound}")))?;
    let record = order.record(owner);
    if record.is_deleted() {
        return Err(Error::Invalid(format!(
            "expected live SNDR or SOUN at {sound}"
        )));
    }
    match record.tag {
        tag if tag == T(b"SNDR") => Ok(sound),
        tag if tag == T(b"SOUN") => link(order, record, owner.file, T(b"SDSC"))?
            .ok_or_else(|| Error::Invalid(format!("sound marker {sound} has no descriptor"))),
        _ => Err(Error::Invalid(format!("expected SNDR or SOUN at {sound}"))),
    }
}

fn field(record: &Record, tag: Tag, size: usize) -> Result<Fields<'_>> {
    let field = Fields::new(record.tag, tag, record.required(tag)?);
    field.exact(size)?;
    Ok(field)
}

fn link(order: &LoadOrder, record: &Record, file: usize, tag: Tag) -> Result<Option<GlobalFormId>> {
    if record.find(tag).is_none() {
        return Ok(None);
    }
    let raw = field(record, tag, 4)?.u32()?;
    Ok((raw != 0).then(|| order.resolve(file, raw)))
}

#[cfg(test)]
mod dialogue_parameters {
    use super::*;
    use soe_esm::{FileHeader, Plugin};

    fn order() -> LoadOrder {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(T(b"SNCT"), 0x800)
                .sub(b"FNAM", 2_u32.to_le_bytes())
                .sub(b"VNAM", 65535_u16.to_le_bytes())
                .sub(b"UNAM", 32768_u16.to_le_bytes()),
        );
        plugin.push_top(
            Record::new(T(b"SNCT"), 0x801)
                .sub(b"FNAM", 0_u32.to_le_bytes())
                .sub(b"VNAM", 45875_u16.to_le_bytes())
                .sub(b"PNAM", 0x800_u32.to_le_bytes()),
        );
        plugin.push_top(
            Record::new(T(b"SOPM"), 0x802)
                .sub(b"NAM1", [0, 0, 0, 0])
                .sub(b"MNAM", 1_u32.to_le_bytes())
                .sub(b"ONAM", [0; 24]),
        );
        let mut entries = Vec::new();
        for (key, value) in [(b"DDSC", 0x801_u32), (b"MDSC", 0x801), (b"DOP2", 0x802)] {
            entries.extend_from_slice(key);
            entries.extend_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(Record::new(T(b"DOBJ"), 0x31).sub(b"DNAM", entries));
        let mut order = LoadOrder::new();
        order.add("Skyrim.esm", plugin).unwrap();
        order
    }

    #[test]
    fn dialogue_and_music_honor_parent_slider_settings_without_a_descriptor() {
        let order = order();
        let mut settings = SoundSettings::default();
        settings.category_volumes.insert(
            GlobalFormId {
                file: 0,
                object: 0x800,
            },
            0.25,
        );
        let voice = prepare_dialogue(&order, false, &settings).unwrap();
        assert_eq!(
            voice.category,
            GlobalFormId {
                file: 0,
                object: 0x801
            }
        );
        assert!((voice.gain - 0.175).abs() < 0.0001);
        assert_eq!(
            default_category_gain(&order, *b"MDSC", &settings).unwrap(),
            voice.gain
        );
        assert!(!voice.output.attenuates);
    }

    #[test]
    fn dialogue_rejects_missing_ambient_output_and_invalid_sliders() {
        let order = order();
        let mut settings = SoundSettings::default();
        assert!(prepare_dialogue(&order, true, &settings).is_err());
        settings.category_volumes.insert(
            GlobalFormId {
                file: 0,
                object: 0x800,
            },
            2.0,
        );
        assert!(prepare_dialogue(&order, false, &settings).is_err());
    }
}

#[cfg(test)]
mod dialogue_curve {
    use super::Attenuation;

    #[test]
    fn voice_curve_preserves_near_level_and_authored_quarter_points() {
        let curve = Attenuation {
            min_distance: 70.0,
            max_distance: 3000.0,
            curve: [100, 50, 20, 5, 0],
        };
        assert_eq!(curve.gain_at_distance(0.0), 1.0);
        assert_eq!(curve.gain_at_distance(70.0), 1.0);
        assert!((curve.gain_at_distance(140.0) - 0.9522184).abs() < 1e-6);
        for (distance, gain) in [(802.5, 0.5), (1535.0, 0.2), (2267.5, 0.05), (3000.0, 0.0)] {
            assert!((curve.gain_at_distance(distance) - gain).abs() < 1e-6);
        }
        assert_eq!(curve.gain_at_distance(3001.0), 0.0);
        assert_eq!(curve.gain_at_distance(f32::NAN), 0.0);
    }
}
