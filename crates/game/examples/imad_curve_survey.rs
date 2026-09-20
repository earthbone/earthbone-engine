// SPDX-License-Identifier: GPL-3.0-or-later
//! Inventory IMAD channels and their IMGS inputs without assigning render semantics.
//!
//! Field layouts follow xEdit wbDefinitionsTES5.pas and Mutagen's
//! ImageSpaceAdapter.xml. UESP IMAD documents normalized key times.

use soe_esm::{GlobalFormId, LoadOrder, Plugin, Record, Tag};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Default)]
struct Channel {
    records: usize,
    multiple_keys: usize,
    non_neutral: usize,
    count_mismatches: usize,
    unordered: usize,
    duplicate_times: usize,
    outside_unit_time: usize,
    examples: Vec<String>,
    outside_examples: Vec<String>,
}

struct Field {
    name: String,
    tag: Tag,
    count_offset: usize,
    color: bool,
    neutral: f32,
}

fn word(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(offset..offset + 4)
            .ok_or("truncated numeric field")?
            .try_into()?,
    ))
}

fn float(data: &[u8], offset: usize) -> Result<f32> {
    let value = f32::from_bits(word(data, offset)?);
    if !value.is_finite() {
        return Err("non-finite numeric field".into());
    }
    Ok(value)
}

fn fields() -> Vec<Field> {
    let mut result = Vec::new();
    for (index, name) in [
        "eye_adapt_speed",
        "bloom_blur_radius",
        "bloom_threshold",
        "bloom_scale",
        "target_lum_min",
        "target_lum_max",
        "sunlight_scale",
        "sky_scale",
        "unused_08",
        "unused_09",
        "unused_0a",
        "unused_0b",
        "unused_0c",
        "unused_0d",
        "unused_0e",
        "unused_0f",
        "unused_10",
        "saturation",
        "brightness",
        "contrast",
        "unused_14",
    ]
    .iter()
    .enumerate()
    {
        for (part, label, neutral) in [(0, "mult", 1.0), (1, "add", 0.0)] {
            result.push(Field {
                name: format!("{name}_{label}"),
                tag: Tag::new(&[index as u8 + part * 0x40, b'I', b'A', b'D']),
                count_offset: 8 + index * 8 + usize::from(part) * 4,
                color: false,
                neutral,
            });
        }
    }
    for (name, tag, offset, color) in [
        ("tint", b"TNAM", 176, true),
        ("blur_radius", b"BNAM", 180, false),
        ("double_vision", b"VNAM", 184, false),
        ("radial_strength", b"RNAM", 188, false),
        ("radial_ramp_up", b"SNAM", 192, false),
        ("radial_start", b"UNAM", 196, false),
        ("dof_strength", b"WNAM", 212, false),
        ("dof_distance", b"XNAM", 216, false),
        ("dof_range", b"YNAM", 220, false),
        ("radial_ramp_down", b"NAM1", 228, false),
        ("radial_down_start", b"NAM2", 232, false),
        ("fade", b"NAM3", 236, true),
        ("motion_blur", b"NAM4", 240, false),
    ] {
        result.push(Field {
            name: name.into(),
            tag: Tag::new(tag),
            count_offset: offset,
            color,
            neutral: 0.0,
        });
    }
    result
}

fn survey_curve(record: &Record, field: &Field, header: &[u8], stats: &mut Channel) -> Result<()> {
    let data = record.find(field.tag).unwrap_or_default();
    let stride = if field.color { 20 } else { 8 };
    if !data.len().is_multiple_of(stride) || record.all(field.tag).count() > 1 {
        return Err(format!("invalid {} curve in {:?}", field.name, record.editor_id()).into());
    }
    let count = data.len() / stride;
    stats.count_mismatches += usize::from(count != word(header, field.count_offset)? as usize);
    stats.records += usize::from(count > 0);
    stats.multiple_keys += usize::from(count > 1);
    let mut previous = None;
    let mut non_neutral = false;
    for key in data.chunks_exact(stride) {
        let time = float(key, 0)?;
        stats.outside_unit_time += usize::from(!(0.0..=1.0).contains(&time));
        stats.unordered += usize::from(previous.is_some_and(|last| last > time));
        stats.duplicate_times += usize::from(previous == Some(time));
        if !(0.0..=1.0).contains(&time) && stats.outside_examples.len() < 3 {
            stats
                .outside_examples
                .push(format!("{}:{time}", record.editor_id().unwrap_or_default()));
        }
        previous = Some(time);
        for offset in (4..stride).step_by(4) {
            let value = float(key, offset)?;
            if !field.color || offset == 16 {
                non_neutral |= value != field.neutral;
            }
        }
    }
    stats.non_neutral += usize::from(non_neutral);
    if non_neutral && stats.examples.len() < 3 {
        stats.examples.push(record.editor_id().unwrap_or_default());
    }
    Ok(())
}

fn links(order: &LoadOrder, signature: Tag, tag: Tag) -> Result<()> {
    let mut records = 0;
    let mut linked = 0;
    let mut targets = BTreeSet::new();
    for (id, record) in order.winners_of(signature) {
        if record.flags & 0x20 != 0 {
            continue;
        }
        records += 1;
        let Some(data) = record.find(tag) else {
            continue;
        };
        let file = order.winner_ref(id).ok_or("missing winning owner")?.file;
        for offset in (0..data.len()).step_by(4) {
            let raw = word(data, offset)?;
            if raw == 0 {
                continue;
            }
            let target = order.resolve(file, raw);
            if order
                .winner(target)
                .is_none_or(|rec| rec.tag != Tag::new(b"IMGS"))
            {
                return Err(format!("{id} refers to missing IMGS {target}").into());
            }
            linked += 1;
            targets.insert(target);
        }
    }
    println!(
        "{signature} records={records} {tag} links={linked} distinct_IMGS={}",
        targets.len()
    );
    Ok(())
}

fn main() -> Result<()> {
    let path = std::env::var("SKYRIM_DATA")?;
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = std::fs::read(std::path::Path::new(&path).join(name))?;
        order.add(name, Plugin::parse(&bytes)?)?;
    }
    let mut channels: BTreeMap<String, Channel> = BTreeMap::new();
    let mut flags = BTreeMap::<u32, usize>::new();
    let mut records = 0;
    let mut animated = 0;
    let mut radial_target = 0;
    let mut dof_target = 0;
    for (id, record) in order.winners_of(Tag::new(b"IMAD")) {
        if record.flags & 0x20 != 0 {
            continue;
        }
        records += 1;
        let header = record.required(Tag::new(b"DNAM"))?;
        if header.len() != 244 || word(header, 0)? > 1 {
            return Err(format!("invalid IMAD header {id}").into());
        }
        animated += usize::from(word(header, 0)? != 0);
        radial_target += usize::from(word(header, 200)? != 0);
        dof_target += usize::from(header[224] != 0);
        *flags.entry(word(header, 224)?).or_default() += 1;
        println!(
            "IMAD {id} {} animated={} duration={} radial_target={} dof_flags={:#06x}",
            record.editor_id().unwrap_or_default(),
            word(header, 0)?,
            float(header, 4)?,
            word(header, 200)?,
            word(header, 224)?
        );
        for field in fields() {
            survey_curve(
                record,
                &field,
                header,
                channels.entry(field.name.clone()).or_default(),
            )?;
        }
    }
    println!("IMAD records={records} animated={animated} radial_target={radial_target} dof_target={dof_target} dof_flags={flags:?}");
    for (name, stats) in channels {
        println!("CHANNEL {name} records={} multiple_keys={} non_neutral={} count_mismatches={} unordered={} duplicate_times={} outside_unit_time={} examples={:?} outside_examples={:?}", stats.records, stats.multiple_keys, stats.non_neutral, stats.count_mismatches, stats.unordered, stats.duplicate_times, stats.outside_unit_time, stats.examples, stats.outside_examples);
    }
    let images: Vec<(GlobalFormId, &Record)> = order
        .winners_of(Tag::new(b"IMGS"))
        .filter(|(_, record)| record.flags & 0x20 == 0)
        .collect();
    println!("IMGS records={}", images.len());
    for (id, record) in images {
        let hdr = record.find(Tag::new(b"HNAM")).unwrap_or_default();
        let values: Vec<_> = (0..hdr.len())
            .step_by(4)
            .map(|offset| float(hdr, offset))
            .collect::<Result<_>>()?;
        println!(
            "IMGS {id} {} HDR={values:?}",
            record.editor_id().unwrap_or_default()
        );
    }
    links(&order, Tag::new(b"CELL"), Tag::new(b"XCIM"))?;
    links(&order, Tag::new(b"WTHR"), Tag::new(b"IMSP"))?;
    Ok(())
}
