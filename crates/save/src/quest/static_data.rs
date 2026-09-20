// SPDX-License-Identifier: GPL-3.0-or-later

//! Global data type 107, following UESP Quest_Static_Data.wikitext.
//! Its event payload shares the QUST runtime layout. The roles of its arrays
//! remain undocumented, so this reader retains their order without executing them.

use super::{
    bad, counted, fixed_count, read_event, write_counted, write_event, write_len, Counted,
    StoryEvent,
};
use crate::{cursor::Cursor, RefId, Result, Save};

/// The final reference-keyed array in quest static data, with two raw words
/// per entry. Their gameplay meaning is not established by the source.
#[derive(Clone, Debug)]
pub struct StaticEntry {
    pub reference: RefId,
    pub values: Counted<[u32; 2]>,
}

/// Two event arrays, three reference arrays, a reference-keyed table and a
/// trailing byte. All arrays retain file order and unresolved RefIDs.
#[derive(Clone, Debug)]
pub struct QuestStaticData {
    pub events: [Vec<StoryEvent>; 2],
    pub references: [Vec<RefId>; 3],
    pub entries: Counted<StaticEntry>,
    pub unknown: u8,
}

impl Save {
    /// Decode global data type 107 when present.
    ///
    /// # Errors
    /// Repeated blocks or malformed quest static data.
    pub fn quest_static_data(&self) -> Result<Option<QuestStaticData>> {
        let mut blocks = self.table_2.iter().filter(|block| block.kind == 107);
        let Some(block) = blocks.next() else {
            return Ok(None);
        };
        if blocks.next().is_some() {
            return Err(bad("duplicate quest static data blocks"));
        }
        QuestStaticData::read(&block.data).map(Some)
    }
}

impl QuestStaticData {
    /// Decode the entire quest static data block.
    ///
    /// # Errors
    /// Truncation, impossible counts, unknown event tags or trailing bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        let mut r = Cursor::new(data, "quest static data");
        let events = [
            fixed_count(&mut r, 12, read_event)?,
            fixed_count(&mut r, 12, read_event)?,
        ];
        let mut read_refs = || fixed_count(&mut r, 3, |r| r.ref_id());
        let references = [read_refs()?, read_refs()?, read_refs()?];
        let entries = counted(&mut r, 4, |r| {
            Ok(StaticEntry {
                reference: r.ref_id()?,
                values: counted(r, 8, |r| Ok([r.u32()?, r.u32()?]))?,
            })
        })?;
        let unknown = r.u8()?;
        if !r.is_empty() {
            return Err(bad(format!("{} trailing static-data bytes", r.remaining())));
        }
        Ok(Self {
            events,
            references,
            entries,
            unknown,
        })
    }

    /// Re-encode decoded fields, retaining original variable-count widths.
    ///
    /// # Errors
    /// Inconsistent count edits, excessive array lengths or unknown event tags.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for events in &self.events {
            write_len(events.len(), &mut out)?;
            for event in events {
                write_event(event, &mut out)?;
            }
        }
        for references in &self.references {
            write_len(references.len(), &mut out)?;
            for reference in references {
                reference.write(&mut out);
            }
        }
        write_counted(&self.entries, &mut out, |entry, out| {
            entry.reference.write(out);
            write_counted(&entry.values, out, |values, out| {
                for value in values {
                    out.extend_from_slice(&value.to_le_bytes());
                }
                Ok(())
            })
        })?;
        out.push(self.unknown);
        Ok(out)
    }
}
