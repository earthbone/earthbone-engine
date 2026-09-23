// SPDX-License-Identifier: GPL-3.0-or-later

//! Scoped gameplay views from xEdit's wbDefinitionsTES5.pas and UESP layouts.
//! These read records, not runtime state. See docs/esm.md for field coverage.
//! FormIDs are local to the file supplying the record, including overrides.

mod condition;
mod crafting;
mod dialogue;
mod magic;

pub use condition::{Comparison, Condition};
pub use crafting::{Ingredient, ItemOwnership, Recipe};
pub use dialogue::{
    DialogueBranch, DialogueInfo, DialogueTopic, InfoData, Response, Scene, SceneAction,
    SceneActionData, SceneActor, ScenePhase, TopicSubtype,
};
pub use magic::{
    Effect, EffectData, Ingestible, IngestibleData, MagicEffect, MagicEffectData, Spell, SpellData,
};

use super::Text;
use crate::cursor::{zstring, Fields};
use crate::error::{Error, Result};
use crate::{Record, Tag};

fn check_tag(rec: &Record, expected: [u8; 4]) -> Result<()> {
    if rec.tag != Tag(expected) {
        return Err(Error::Invalid(format!(
            "expected {}, got {}",
            Tag(expected),
            rec.tag
        )));
    }
    Ok(())
}

fn fields<'a>(rec: &Record, tag: Tag, bytes: &'a [u8], size: usize) -> Result<Fields<'a>> {
    let f = Fields::new(rec.tag, tag, bytes);
    f.exact(size)?;
    Ok(f)
}

fn required(rec: &Record, tag: [u8; 4], size: usize) -> Result<Fields<'_>> {
    let tag = Tag(tag);
    let bytes = rec.find(tag).ok_or(Error::MissingSubrecord {
        record: rec.tag,
        tag,
    })?;
    fields(rec, tag, bytes, size)
}

fn text_bytes(rec: &Record, tag: Tag, bytes: &[u8], localized: bool) -> Result<Text> {
    if localized {
        Ok(Text::Localized(fields(rec, tag, bytes, 4)?.u32()?))
    } else {
        Ok(Text::Plain(zstring(bytes)))
    }
}

fn text(rec: &Record, tag: [u8; 4], localized: bool) -> Result<Option<Text>> {
    let tag = Tag(tag);
    rec.find(tag)
        .map(|bytes| text_bytes(rec, tag, bytes, localized))
        .transpose()
}

fn optional_u32(rec: &Record, tag: [u8; 4]) -> Result<Option<u32>> {
    super::form_id(rec, Tag(tag))
}
