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
    /// The transform or property field, kept as written because its grammar
    /// is its own small language of `pos`, `rot`, `scale` and `flags` calls.
    pub properties: String,
    /// Percent, 0 to 100. `ChanceL` makes it location based, which this keeps
    /// as written in `properties` rather than interpreting.
    pub chance: f32,
    pub line: usize,
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
        SwapKind::Properties => (
            Vec::new(),
            fields.get(1).copied().unwrap_or_default().to_string(),
            100.0,
        ),
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
}
