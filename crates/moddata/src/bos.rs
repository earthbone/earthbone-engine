// SPDX-License-Identifier: GPL-3.0-or-later

//! Base Object Swapper, `*_SWAP.ini`.
//!
//! This one is shaped unlike the other two. `Manager::LoadForms` sets
//! `SetAllowKeyOnly`, so a line has no `=` and the whole line is the payload,
//! and it walks named sections instead of the anonymous one. Four section
//! names matter:
//!
//! - `[Forms]`, swap one base object for another everywhere it is placed.
//! - `[References]`, swap the base of one placed reference.
//! - `[Transforms]` and `[Properties]`, override a reference's properties
//!   without swapping anything.
//!
//! A section name may carry conditions after a `|`, as in
//! `[Forms|Riverwood,-WhiterunDragonsreach]`, and a condition written with a
//! leading `-` excludes. Conditions name a location, cell, keyword or region
//! by editor id, and the plugin tests them against the reference's
//! surroundings at swap time.
//!
//! Either side of a swap may be a comma separated set, which the plugin picks
//! from at random.

use crate::form::FormRef;
use crate::ini::{is_valid_entry, split_list, Ini};
use std::path::Path;

/// What a section does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapKind {
    /// Swap a base object wherever it appears.
    Forms,
    /// Swap the base of one placed reference.
    References,
    /// Override properties, swapping nothing.
    Properties,
}

/// One condition on a section header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    /// Written `-`, so the swap applies everywhere except here.
    pub exclude: bool,
    /// A location, cell, keyword or region, by editor id or form.
    pub target: FormRef,
}

/// One line inside a section.
#[derive(Debug, Clone, PartialEq)]
pub struct Swap {
    /// The base object or reference being changed.
    pub from: Vec<FormRef>,
    /// What replaces it. Empty for a properties-only entry.
    pub to: Vec<FormRef>,
    /// The transform or property field, parsed by [`Properties`] in the game
    /// crate after resolution.
    pub properties: String,
    /// Percent, 0 to 100, for selecting a swap replacement.
    pub chance: f32,
    pub line: usize,
}

/// Parsed transform and record-flag overrides from a BOS property field.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Properties {
    /// Position values in game units, with relative mode adding to placement.
    pub position: Option<Point3>,
    /// Rotation values in radians, with relative mode adding to placement.
    pub rotation: Option<Point3>,
    /// Scale value, multiplied by default or used directly when absolute.
    pub scale: Option<Scale>,
    /// Record flag bits to set.
    pub flags_set: u32,
    /// Record flag bits to clear.
    pub flags_clear: u32,
    /// Optional chance call controlling the whole property set.
    pub chance: Option<PropertyChance>,
}

/// Chance mode attached to a BOS properties field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropertyChance {
    /// Chance to apply the properties, from 0 to 100 percent.
    pub percent: f32,
    /// Whether the chance seed combines location and base identities.
    pub location_based: bool,
    /// Optional seed included in the deterministic selection.
    pub seed: u64,
}

/// Three values, each optionally represented by an inclusive `min/max` range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point3 {
    /// One `[minimum, maximum]` range for each axis.
    pub values: [[f32; 2]; 3],
    /// Add sampled values to existing placement when true.
    pub relative: bool,
}

/// A scale value or range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    /// `[minimum, maximum]` scale range.
    pub values: [f32; 2],
    /// Replace the current scale when true, otherwise multiply it.
    pub absolute: bool,
}

impl Properties {
    /// Parses recognized `pos`, `rot`, `scale` and `flags` calls.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        for call in split_calls(text) {
            let Some((name, body)) = call.split_once('(') else {
                continue;
            };
            let Some(body) = body.strip_suffix(')') else {
                continue;
            };
            let name = name.trim();
            let (name, mode) = match name.chars().last() {
                Some(mode @ ('R' | 'r' | 'A' | 'a' | 'C' | 'c' | 'L' | 'l')) => {
                    (&name[..name.len() - mode.len_utf8()], mode)
                }
                _ => (name, '\0'),
            };
            let body = body.trim();
            match name.trim().to_ascii_lowercase().as_str() {
                "pos" | "rot" => {
                    let values: Vec<_> = body.split(',').map(parse_range).collect();
                    if values.len() != 3 || values.iter().any(Option::is_none) {
                        continue;
                    }
                    let mut values = values.into_iter().flatten();
                    let mut point = [
                        values.next().unwrap_or([0.0; 2]),
                        values.next().unwrap_or([0.0; 2]),
                        values.next().unwrap_or([0.0; 2]),
                    ];
                    if name.trim().eq_ignore_ascii_case("rot") {
                        for range in &mut point {
                            range[0] = range[0].to_radians();
                            range[1] = range[1].to_radians();
                        }
                    }
                    let property = Point3 {
                        values: point,
                        relative: mode == 'R' || mode == 'r',
                    };
                    if name.trim().eq_ignore_ascii_case("pos") {
                        out.position = Some(property);
                    } else {
                        out.rotation = Some(property);
                    }
                }
                "scale" => {
                    if let Some(values) = parse_range(body) {
                        out.scale = Some(Scale {
                            values,
                            absolute: mode == 'A' || mode == 'a',
                        });
                    }
                }
                "flags" => {
                    let clear = mode == 'C' || mode == 'c';
                    let mut bits = 0_u32;
                    for token in body.split(',') {
                        let token = token.trim().trim_start_matches("0x");
                        if let Ok(value) = u32::from_str_radix(token, 16) {
                            bits |= value;
                        }
                    }
                    if clear {
                        out.flags_clear |= bits;
                    } else {
                        out.flags_set |= bits;
                    }
                }
                "chance" => {
                    let mut values = body.split(',').map(str::trim);
                    let Some(Ok(percent)) = values.next().map(str::parse::<f32>) else {
                        continue;
                    };
                    if !percent.is_finite() {
                        continue;
                    }
                    let seed = values
                        .next()
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or_default();
                    out.chance = Some(PropertyChance {
                        percent: percent.clamp(0.0, 100.0),
                        location_based: mode == 'L' || mode == 'l',
                        seed,
                    });
                }
                _ => {}
            }
        }
        out
    }
}

fn split_calls(text: &str) -> Vec<&str> {
    let (mut calls, mut start, mut depth) = (Vec::new(), 0, 0_i32);
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                calls.push(text[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start < text.len() {
        calls.push(text[start..].trim());
    }
    calls
}

fn parse_range(text: &str) -> Option<[f32; 2]> {
    let mut values = text.trim().split('/');
    let min = values.next()?.trim().parse::<f32>().ok()?;
    let max = values
        .next()
        .map(str::trim)
        .map(str::parse::<f32>)
        .transpose()
        .ok()?
        .unwrap_or(min);
    if values.next().is_some() || !min.is_finite() || !max.is_finite() {
        return None;
    }
    Some([min.min(max), min.max(max)])
}

/// One section and everything it asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct SwapSection {
    pub kind: SwapKind,
    pub conditions: Vec<Condition>,
    pub swaps: Vec<Swap>,
}

/// Every swap one file asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct SwapFile {
    pub path: std::path::PathBuf,
    pub sections: Vec<SwapSection>,
}

impl SwapFile {
    /// Reads one `*_SWAP.ini`.
    ///
    /// # Errors
    ///
    /// [`crate::Error::Io`] when the file cannot be read.
    pub fn read(path: impl AsRef<Path>) -> crate::Result<Self> {
        let ini = Ini::read(path)?;
        Ok(Self::from_ini(&ini))
    }

    /// Reads an already parsed file.
    pub fn from_ini(ini: &Ini) -> Self {
        let mut sections = Vec::new();
        for section in &ini.sections {
            let (name, conditions) = split_header(&section.name);
            let Some(kind) = parse_kind(name) else {
                continue;
            };
            let swaps = section
                .entries
                .iter()
                .map(|e| {
                    // Key only, so the payload is the key, but an author who
                    // writes an `=` should still be understood.
                    let text = if e.value.is_empty() {
                        e.key.clone()
                    } else {
                        format!("{}={}", e.key, e.value)
                    };
                    parse_swap(&text, kind, e.line)
                })
                .collect();
            sections.push(SwapSection {
                kind,
                conditions,
                swaps,
            });
        }
        Self {
            path: ini.path.clone(),
            sections,
        }
    }

    /// Every swap in the file, ignoring which section carried it.
    pub fn swaps(&self) -> impl Iterator<Item = &Swap> {
        self.sections.iter().flat_map(|s| s.swaps.iter())
    }
}

/// `Forms|A,-B` splits into the name and its conditions.
fn split_header(name: &str) -> (&str, Vec<Condition>) {
    match name.split_once('|') {
        Some((head, rest)) => {
            let conditions = split_list(rest, ',')
                .iter()
                .map(|t| match t.strip_prefix('-') {
                    Some(r) => Condition {
                        exclude: true,
                        target: FormRef::parse(r),
                    },
                    None => Condition {
                        exclude: false,
                        target: FormRef::parse(t),
                    },
                })
                .collect();
            (head.trim(), conditions)
        }
        None => (name.trim(), Vec::new()),
    }
}

fn parse_kind(name: &str) -> Option<SwapKind> {
    match name.to_ascii_lowercase().as_str() {
        "forms" => Some(SwapKind::Forms),
        "references" => Some(SwapKind::References),
        "transforms" | "properties" => Some(SwapKind::Properties),
        _ => None,
    }
}

fn parse_swap(text: &str, kind: SwapKind, line: usize) -> Swap {
    let fields: Vec<&str> = text.split('|').map(str::trim).collect();
    let from = parse_set(fields.first().copied().unwrap_or_default());
    // A properties section has no swap target, so field 1 is the transform.
    let (to, properties, chance) = match kind {
        SwapKind::Properties => {
            let properties = fields
                .iter()
                .skip(1)
                .take(2)
                .copied()
                .filter(|field| !field.is_empty())
                .collect::<Vec<_>>()
                .join(",");
            (Vec::new(), properties, 100.0)
        }
        _ => (
            parse_set(fields.get(1).copied().unwrap_or_default()),
            fields.get(2).copied().unwrap_or_default().to_string(),
            parse_chance(fields.get(3).copied().unwrap_or_default()),
        ),
    };
    Swap {
        from,
        to,
        properties,
        chance,
        line,
    }
}

fn parse_set(text: &str) -> Vec<FormRef> {
    split_list(text, ',')
        .iter()
        .map(|t| FormRef::parse(t))
        .collect()
}

fn parse_chance(text: &str) -> f32 {
    if is_valid_entry(text) {
        text.parse().unwrap_or(100.0)
    } else {
        100.0
    }
}

#[cfg(test)]
mod tests {
    use super::{SwapFile, SwapKind};
    use crate::form::FormRef;
    use crate::ini::Ini;

    fn parse(text: &str) -> SwapFile {
        SwapFile::from_ini(&Ini::parse("t_SWAP.ini", text))
    }

    #[test]
    fn a_form_swap_reads_both_sides() {
        let f = parse("[Forms]\n0x01~A.esp|0x02~B.esp\n");
        assert_eq!(f.sections[0].kind, SwapKind::Forms);
        let s = &f.sections[0].swaps[0];
        assert_eq!(
            s.from[0],
            FormRef::InFile {
                object: 1,
                file: "A.esp".to_string()
            }
        );
        assert_eq!(s.to.len(), 1);
    }

    #[test]
    fn either_side_may_be_a_set() {
        let f = parse("[Forms]\n0x01~A.esp,0x02~A.esp|0x03~B.esp,0x04~B.esp\n");
        let s = &f.sections[0].swaps[0];
        assert_eq!(s.from.len(), 2);
        assert_eq!(s.to.len(), 2);
    }

    #[test]
    fn a_conditional_header_splits_into_name_and_conditions() {
        let f = parse("[Forms|Riverwood,-WhiterunDragonsreach]\na|b\n");
        let sec = &f.sections[0];
        assert_eq!(sec.kind, SwapKind::Forms);
        assert_eq!(sec.conditions.len(), 2);
        assert!(!sec.conditions[0].exclude);
        assert!(sec.conditions[1].exclude);
    }

    #[test]
    fn properties_and_chance_ride_on_the_end() {
        let f = parse("[Forms]\na|b|scale(2)|25\n");
        let s = &f.sections[0].swaps[0];
        assert_eq!(s.properties, "scale(2)");
        assert!((s.chance - 25.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_transforms_section_has_no_swap_target() {
        let f = parse("[Transforms]\n0x01~A.esp|scale(1.5)\n");
        let sec = &f.sections[0];
        assert_eq!(sec.kind, SwapKind::Properties);
        assert!(sec.swaps[0].to.is_empty());
        assert_eq!(sec.swaps[0].properties, "scale(1.5)");
    }

    #[test]
    fn references_are_their_own_kind() {
        let f = parse("[References]\na|b\n");
        assert_eq!(f.sections[0].kind, SwapKind::References);
    }

    #[test]
    fn an_unknown_section_is_ignored() {
        let f = parse("[Wibble]\na|b\n[Forms]\nc|d\n");
        assert_eq!(f.sections.len(), 1);
        assert_eq!(f.sections[0].kind, SwapKind::Forms);
    }

    #[test]
    fn every_swap_is_reachable_without_walking_sections() {
        let f = parse("[Forms]\na|b\nc|d\n[References]\ne|f\n");
        assert_eq!(f.swaps().count(), 3);
    }

    #[test]
    fn mod_03_property_language_models_transform_modes_ranges_and_flags() {
        let properties = super::Properties::parse(
            "posR(1/3,2,4),rot(0,0,90),scaleA(1.5/2),flags(0x10,20),flagsC(0x40),ChanceL(75,12)",
        );
        assert_eq!(properties.position.unwrap().values[0], [1.0, 3.0]);
        assert!(properties.position.unwrap().relative);
        assert!(
            (properties.rotation.unwrap().values[2][0] - std::f32::consts::FRAC_PI_2).abs() < 1e-6
        );
        assert_eq!(properties.scale.unwrap().values, [1.5, 2.0]);
        assert!(properties.scale.unwrap().absolute);
        assert_eq!(properties.flags_set, 0x30);
        assert_eq!(properties.flags_clear, 0x40);
        assert_eq!(
            properties.chance,
            Some(super::PropertyChance {
                percent: 75.0,
                location_based: true,
                seed: 12,
            })
        );
    }
}
