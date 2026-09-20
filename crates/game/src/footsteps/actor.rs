// SPDX-License-Identifier: GPL-3.0-or-later

//! Footstep sets use the same active body parts as rendering.

use super::{Footsteps, T};
use crate::appearance::ActorAppearance;
use crate::World;
use soe_esm::{GlobalFormId, Result};
use std::collections::BTreeSet;

impl World {
    /// Resolve live skin, equipment and race changes on each event.
    pub fn footstep_sets(&mut self, actor: GlobalFormId) -> Result<Vec<GlobalFormId>> {
        let appearance = self.actor_appearance(actor)?;
        Footsteps::new(self.order()).appearance_sets(&appearance)
    }
}

impl Footsteps<'_> {
    /// Select distinct surviving sets in FormID order, then the default fallback.
    pub fn actor_sets(
        &self,
        base: Option<GlobalFormId>,
        current_race: Option<GlobalFormId>,
        equipped: &BTreeSet<GlobalFormId>,
    ) -> Result<Vec<GlobalFormId>> {
        self.appearance_sets(&ActorAppearance::from_order(
            self.order,
            base,
            current_race,
            equipped,
        )?)
    }

    fn appearance_sets(&self, appearance: &ActorAppearance) -> Result<Vec<GlobalFormId>> {
        let mut sets: BTreeSet<_> = appearance
            .parts
            .iter()
            .filter_map(|part| part.footsteps)
            .collect();
        if sets.is_empty() {
            if let Some(default) = self.default_object(*b"DFTS")? {
                self.record(default, T(b"FSTS"))?;
                sets.insert(default);
            }
        }
        Ok(sets.into_iter().collect())
    }
}
