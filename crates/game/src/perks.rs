// SPDX-License-Identifier: GPL-3.0-or-later

//! Player PERK purchases using xEdit's top-level CTDA and NNAM rank chain.
//! Existing actor perk identities remain authoritative. Effect entries have
//! separate conditions and are outside the purchase transaction.
//!
//! DATA Trait, Level and Num Ranks are accepted at any value (PRG-16). Trait
//! and Level are never set on an installed winning PERK record and no source
//! in the corpus describes a runtime consumer for either byte, so they carry
//! no purchase meaning here. Num Ranks is a Creation Kit display field kept
//! in sync with the actual NNAM chain, so the chain walked by [`rank_chain`]
//! is the count that matters, not the byte. Hidden perks purchase the same
//! as shown ones, this API has no UI layer for the hidden flag to affect.
//!
//! DATA also accepts the two truncated encodings wrye-bash's
//! `MelPerkData(MelTruncatedStruct)` declares in `old_versions={'4B', '3B'}`
//! (`Mopy/bash/brec/common_subrecords.py`, reused unmodified for Skyrim in
//! `Mopy/bash/game/skyrim/records.py`): a 4-byte DATA carries Trait, Level,
//! Num Ranks and Playable with Hidden defaulted to 0, and a 3-byte DATA
//! carries only Trait, Level and Num Ranks with Playable and Hidden both
//! defaulted to 0, matching `MelTruncatedStruct`'s zero-padding of the
//! fields a shorter format omits. A record in either shorter, documented
//! encoding is readable, not malformed.
//! See docs/game.md, "Ordinary perk purchases", for the sources read.

use crate::{conditions, Game, World};
use soe_esm::{records::gameplay::Condition, GlobalFormId, Tag};
use std::collections::BTreeSet;

/// A failed purchase leaves perk ownership and the point balance unchanged.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("perk {0} is missing, deleted or malformed")]
    Perk(GlobalFormId),
    #[error("perk {0} is not playable")]
    NotPlayable(GlobalFormId),
    #[error("perk {0} is not the first record in a rank chain")]
    NotRoot(GlobalFormId),
    #[error("perk rank chain contains a cycle at {0}")]
    Cycle(GlobalFormId),
    #[error("all ranks are already owned")]
    Complete,
    #[error("no perk points are available")]
    NoPoints,
    #[error("player actor is unavailable")]
    Player,
    #[error("perk purchase prerequisites are false")]
    Ineligible,
    #[error("perk condition format: {0}")]
    Format(#[from] soe_esm::Error),
    #[error("perk condition: {0}")]
    Condition(#[from] conditions::Error),
}

/// The exact record acquired and its one-based position in the NNAM chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Purchase {
    pub perk: GlobalFormId,
    pub rank: usize,
}

struct Perk {
    playable: bool,
    next: Option<GlobalFormId>,
    conditions: Vec<Condition>,
}

impl Perk {
    fn read(world: &World, id: GlobalFormId) -> Result<Self, Error> {
        let record = world
            .record(id)
            .filter(|r| r.tag == Tag(*b"PERK") && !r.is_deleted())
            .ok_or(Error::Perk(id))?;
        let mut data = None;
        let mut next = None;
        let mut conditions = Vec::new();
        for sub in &record.subrecords {
            match sub.tag.as_bytes() {
                b"PRKE" => break,
                b"DATA" => {
                    if data.is_some() {
                        return Err(Error::Perk(id));
                    }
                    data = Some(pad_legacy_data(&sub.data).ok_or(Error::Perk(id))?);
                }
                b"NNAM" => {
                    if next.is_some() {
                        return Err(Error::Perk(id));
                    }
                    let bytes = sub
                        .data
                        .as_slice()
                        .try_into()
                        .map_err(|_| Error::Perk(id))?;
                    next = Some(u32::from_le_bytes(bytes));
                }
                _ => {
                    Condition::push(record, sub, &mut conditions)?;
                }
            }
        }
        let data = data.ok_or(Error::Perk(id))?;
        let next = next
            .filter(|raw| *raw != 0)
            .map(|raw| world.resolve_from(id, raw).ok_or(Error::Perk(id)))
            .transpose()?;
        Ok(Self {
            playable: data[3] != 0 && record.flags & 4 == 0,
            next,
            conditions,
        })
    }
}

impl Game {
    /// Preview the first unowned rank starting at a chain's first PERK record.
    /// Script grants can leave gaps. Existing ownership is retained in that case.
    ///
    /// # Errors
    ///
    /// Rejects non-root inputs, invalid chains, exhausted points, completed
    /// chains, non-playable ranks and false prerequisites.
    pub fn check_perk_purchase(&mut self, root: GlobalFormId) -> Result<Purchase, Error> {
        let mut world = self.world.borrow_mut();
        let chain = rank_chain(&world, root)?;
        if world.perk_points <= 0 {
            return Err(Error::NoPoints);
        }
        let player = world.player();
        let owned = &world.actor(player).ok_or(Error::Player)?.perks;
        let (index, (id, perk)) = chain
            .iter()
            .enumerate()
            .find(|(_, (id, _))| !owned.contains(id))
            .ok_or(Error::Complete)?;
        if !perk.playable {
            return Err(Error::NotPlayable(*id));
        }
        let context = conditions::Context {
            owner: *id,
            subject: player,
            target: None,
            quest: None,
            tempering_enchanted: None,
        };
        if !conditions::evaluate(&mut world, &self.vm, context, &perk.conditions)? {
            return Err(Error::Ineligible);
        }
        Ok(Purchase {
            perk: *id,
            rank: index + 1,
        })
    }

    /// Acquire one rank and spend one player perk point after rechecking eligibility.
    /// Lower rank identities remain owned. This does not execute perk effects.
    ///
    /// # Errors
    ///
    /// Returns the same errors as check_perk_purchase without spending points.
    pub fn purchase_perk(&mut self, root: GlobalFormId) -> Result<Purchase, Error> {
        let purchase = self.check_perk_purchase(root)?;
        let mut world = self.world.borrow_mut();
        let player = world.player();
        world
            .actor(player)
            .ok_or(Error::Player)?
            .perks
            .insert(purchase.perk);
        world.perk_points -= 1;
        Ok(purchase)
    }
}

fn rank_chain(world: &World, root: GlobalFormId) -> Result<Vec<(GlobalFormId, Perk)>, Error> {
    let mut seen = BTreeSet::new();
    let mut chain = Vec::new();
    let mut next = Some(root);
    while let Some(id) = next {
        if !seen.insert(id) {
            return Err(Error::Cycle(id));
        }
        let perk = Perk::read(world, id)?;
        next = perk.next;
        chain.push((id, perk));
    }
    // CONTEXT: The API accepts a tree node's first rank, preventing a caller
    // from skipping earlier ranks by passing a later NNAM target as a root.
    for (id, record) in world.order().winners_of(Tag(*b"PERK")) {
        if record.is_deleted() {
            continue;
        }
        let raw = record
            .find(Tag(*b"NNAM"))
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes);
        if raw.is_some_and(|raw| raw != 0 && world.resolve_from(id, raw) == Some(root)) {
            return Err(Error::NotRoot(root));
        }
    }
    Ok(chain)
}

/// Accepts PERK DATA's full 5-byte encoding (Trait, Level, Num Ranks,
/// Playable, Hidden) plus the two truncated encodings wrye-bash's
/// `MelPerkData` declares as `old_versions={'4B', '3B'}`: a 4-byte DATA
/// omits Hidden and a 3-byte DATA omits Playable and Hidden too. Missing
/// trailing bytes are zero-padded, matching `MelTruncatedStruct`'s default.
/// Any other length is not a documented encoding and is rejected.
fn pad_legacy_data(bytes: &[u8]) -> Option<[u8; 5]> {
    if bytes.len() < 3 || bytes.len() > 5 {
        return None;
    }
    let mut padded = [0u8; 5];
    padded[..bytes.len()].copy_from_slice(bytes);
    Some(padded)
}
