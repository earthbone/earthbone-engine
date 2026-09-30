// SPDX-License-Identifier: GPL-3.0-or-later

//! Named section tables appended after a saved record's version-gated
//! fields. Version 37 is the last `SAVE_VERSION` bump (see
//! `save::SAVE_VERSION`): a row that adds saved state registers a
//! [`Section`] in `save_section_registry` instead, at the level that owns
//! the state, named after the row, for example `"MAG-39"` or
//! `"MAG-39.summons"` for a second section the same row needs.
//!
//! # Format
//!
//! A section table is a `u16` count, then per section a name (`u8`
//! length, UTF-8 bytes), a `u32` byte length, then that many payload
//! bytes. [`write_sections`] and [`read_sections`] work at any level
//! (world, actor, reference) for any `T` with its own small registry of
//! `Section<T>` entries, and read a payload through the same
//! [`FormReader`] context (its [`FormNamespace`](super::save::FormNamespace))
//! the surrounding record uses, so a section that stores a `FormId` resolves
//! it the same way the rest of that record does.

use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::VmError;

use super::save::FormReader;

/// One named section: how to write its payload from `T`, and how to read
/// it back into `T`. `read` receives `T` already built (its defaults, for
/// a value under construction), so a section that is absent from the
/// table leaves the field it would have set at that default.
pub(crate) struct Section<T> {
    pub(crate) name: &'static str,
    pub(crate) write: fn(&T, &mut Writer),
    pub(crate) read: fn(&mut T, &mut FormReader<'_, '_>) -> Result<(), VmError>,
}

/// Write every registered section with a nonempty payload, in registry order.
pub(crate) fn write_sections<T>(w: &mut Writer, value: &T, sections: &[Section<T>]) {
    let payloads: Vec<_> = sections
        .iter()
        .filter_map(|section| {
            let mut payload = Writer::new();
            (section.write)(value, &mut payload);
            (!payload.out.is_empty()).then_some((section, payload.out))
        })
        .collect();
    // Registries are hand-written entries (a handful today, never near
    // 65536) and names are short `&'static str` literals (never near 256
    // bytes), so these casts cannot lose data in practice. See the
    // workspace `cast_possible_truncation` allow in the root `Cargo.toml`.
    w.u16(payloads.len() as u16);
    for (section, payload) in payloads {
        w.u8(section.name.len() as u8);
        w.out.extend_from_slice(section.name.as_bytes());
        w.bytes(&payload);
    }
}

/// Read a section table into `value`. A section absent from the table
/// leaves `value` untouched by this call. An unknown name is an error
/// naming the section and `level` (the caller's word for this table, for
/// example `"world"`, `"actor"` or `"reference"`), matching the unknown
/// version error's shape. A truncated payload and a payload its reader
/// does not fully consume are both errors too.
pub(crate) fn read_sections<T>(
    r: &mut FormReader<'_, '_>,
    level: &str,
    value: &mut T,
    sections: &[Section<T>],
) -> Result<(), VmError> {
    let n = r.u16()?;
    for _ in 0..n {
        let name_len = usize::from(r.u8()?);
        let name = std::str::from_utf8(r.take(name_len)?)
            .map_err(|_| VmError::State(format!("non UTF-8 section name at {level}")))?
            .to_string();
        let payload = r
            .bytes()
            .map_err(|error| VmError::State(format!("section {name} at {level}: {error}")))?;
        let Some(section) = sections.iter().find(|s| s.name == name) else {
            return Err(VmError::State(format!("unknown section {name} at {level}")));
        };
        let mut sub_reader = Reader::new(payload);
        let mut sub = FormReader::new(&mut sub_reader, r.namespace());
        (section.read)(value, &mut sub)
            .map_err(|error| VmError::State(format!("section {name} at {level}: {error}")))?;
        sub.expect_end()
            .map_err(|_| VmError::State(format!("section {name} at {level} left unread bytes")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::save::FormNamespace;
    use super::*;

    #[derive(Debug, Default, PartialEq)]
    struct Fixture {
        a: u32,
        b: u16,
    }

    fn write_a(v: &Fixture, w: &mut Writer) {
        w.u32(v.a);
    }
    fn read_a(v: &mut Fixture, r: &mut FormReader<'_, '_>) -> Result<(), VmError> {
        v.a = r.u32()?;
        Ok(())
    }
    fn write_b(v: &Fixture, w: &mut Writer) {
        w.u16(v.b);
    }
    fn read_b(v: &mut Fixture, r: &mut FormReader<'_, '_>) -> Result<(), VmError> {
        v.b = r.u16()?;
        Ok(())
    }
    /// Leaves a byte unread, to exercise the "left unread bytes" error.
    fn read_b_partial(_v: &mut Fixture, r: &mut FormReader<'_, '_>) -> Result<(), VmError> {
        r.u8()?;
        Ok(())
    }

    fn both_sections() -> Vec<Section<Fixture>> {
        vec![
            Section {
                name: "a",
                write: write_a,
                read: read_a,
            },
            Section {
                name: "b",
                write: write_b,
                read: read_b,
            },
        ]
    }

    fn form_reader<'reader, 'data>(r: &'reader mut Reader<'data>) -> FormReader<'reader, 'data> {
        FormReader::new(r, FormNamespace::Current)
    }

    #[test]
    fn a_round_trip_restores_every_registered_section() {
        let value = Fixture { a: 7, b: 9 };
        let sections = both_sections();
        let mut w = Writer::new();
        write_sections(&mut w, &value, &sections);

        let mut restored = Fixture::default();
        let mut r = Reader::new(&w.out);
        let mut fr = form_reader(&mut r);
        read_sections(&mut fr, "fixture", &mut restored, &sections).unwrap();
        assert_eq!(restored, value);
    }

    #[test]
    fn a_section_absent_from_the_table_leaves_its_field_at_the_default() {
        let value = Fixture { a: 7, b: 9 };
        let write_only_a = vec![Section {
            name: "a",
            write: write_a,
            read: read_a,
        }];
        let mut w = Writer::new();
        write_sections(&mut w, &value, &write_only_a);

        let mut restored = Fixture { a: 0, b: 42 };
        let mut r = Reader::new(&w.out);
        let mut fr = form_reader(&mut r);
        read_sections(&mut fr, "fixture", &mut restored, &both_sections()).unwrap();
        assert_eq!(restored.a, 7);
        assert_eq!(restored.b, 42);
    }

    #[test]
    fn an_unknown_section_name_names_the_section_and_the_level() {
        let value = Fixture { a: 1, b: 2 };
        let mut w = Writer::new();
        write_sections(&mut w, &value, &both_sections());

        let mut restored = Fixture::default();
        let mut r = Reader::new(&w.out);
        let mut fr = form_reader(&mut r);
        let only_a = vec![Section {
            name: "a",
            write: write_a,
            read: read_a,
        }];
        let err = read_sections(&mut fr, "reference", &mut restored, &only_a).unwrap_err();
        assert!(err.to_string().contains("unknown section b at reference"));
    }

    #[test]
    fn a_truncated_payload_is_rejected() {
        let value = Fixture { a: 1, b: 2 };
        let mut w = Writer::new();
        write_sections(&mut w, &value, &both_sections());
        w.out.pop();

        let mut restored = Fixture::default();
        let mut r = Reader::new(&w.out);
        let mut fr = form_reader(&mut r);
        assert!(read_sections(&mut fr, "fixture", &mut restored, &both_sections()).is_err());
    }

    #[test]
    fn unread_payload_bytes_are_rejected() {
        let value = Fixture { a: 1, b: 2 };
        let mut w = Writer::new();
        write_sections(&mut w, &value, &both_sections());

        let mut restored = Fixture::default();
        let mut r = Reader::new(&w.out);
        let mut fr = form_reader(&mut r);
        let partial = vec![
            Section {
                name: "a",
                write: write_a,
                read: read_a,
            },
            Section {
                name: "b",
                write: write_b,
                read: read_b_partial,
            },
        ];
        let err = read_sections(&mut fr, "fixture", &mut restored, &partial).unwrap_err();
        assert!(err.to_string().contains("left unread bytes"));
    }
}
