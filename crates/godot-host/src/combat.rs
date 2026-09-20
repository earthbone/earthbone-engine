// SPDX-License-Identifier: GPL-3.0-or-later

//! Authored HitFrame contacts, following CK equipment semantics and Nemesis
//! animation event tables. Spatial queries return the first obstruction.

use soe_esm::GlobalFormId;
use soe_game::combat::{Error, Hand, HitResult, MeleeHit};
use soe_game::Game;
use soe_hkx::behavior::{BehaviorGraph, ClipTrigger, EventInfo, EventProperty, NodeKind};
use soe_hkx::interp::ClipLibrary;

/// Convert decoded combat annotations to the graph's existing trigger model.
/// The Havok clip flag 16 disables annotation conversion. Crop offsets follow
/// the interpreter's local playhead, while authored graph triggers take priority.
pub fn contact_annotation_triggers(graph: &mut BehaviorGraph, clips: &ClipLibrary) {
    for node in &mut graph.nodes {
        let NodeKind::Clip(generator) = &mut node.kind else {
            continue;
        };
        if generator.flags & 16 != 0 {
            continue;
        }
        let Some(clip) = clips.get(&generator.animation_name) else {
            continue;
        };
        for annotation in clip
            .animation
            .annotation_tracks
            .iter()
            .flat_map(|track| &track.annotations)
        {
            if !matches!(
                annotation.text.as_str(),
                "HitFrame" | "preHitFrame" | "weaponSwing" | "attackStop"
            ) {
                continue;
            }
            let time = annotation.time - generator.crop_start_amount_local_time;
            if !time.is_finite()
                || time < 0.0
                || annotation.time > clip.duration() - generator.crop_end_amount_local_time
            {
                continue;
            }
            let id = if let Some(id) = graph
                .data
                .event_names
                .iter()
                .position(|name| name == &annotation.text)
            {
                id
            } else {
                let id = graph.data.event_names.len();
                graph.data.event_names.push(annotation.text.clone());
                graph
                    .data
                    .event_infos
                    .resize(id + 1, EventInfo { flags: 0 });
                id
            };
            let Ok(id) = i32::try_from(id) else { continue };
            if generator.triggers.iter().any(|trigger| {
                trigger.event.id == id
                    && !trigger.relative_to_end_of_clip
                    && trigger.local_time == time
            }) {
                continue;
            }
            generator.triggers.push(ClipTrigger {
                local_time: time,
                event: EventProperty {
                    id,
                    has_payload: false,
                },
                relative_to_end_of_clip: false,
                acyclic: false,
                is_annotation: true,
            });
        }
    }
}

/// One swing's captured equipment, held until its first contact marker.
#[derive(Debug, Clone, Copy)]
struct Swing {
    attacker: GlobalFormId,
    hand: Hand,
    weapon: GlobalFormId,
}

/// Host contact latch. Damage and equipment remain in the shared Game.
#[derive(Debug, Default)]
pub struct MeleeContact {
    swing: Option<Swing>,
}

impl MeleeContact {
    /// Arm one ordinary swing with the weapon currently assigned to its hand.
    /// A second start cannot replace an attack before cancellation or contact.
    ///
    /// # Errors
    /// Missing hand equipment or an already active swing.
    pub fn begin(&mut self, game: &Game, attacker: GlobalFormId, hand: Hand) -> Result<(), Error> {
        if self.swing.is_some() {
            return Err(Error::Unsupported("swing already awaiting contact"));
        }
        let weapon = game
            .world_mut()
            .hand_weapon(attacker, hand)
            .ok_or(Error::Equipment)?;
        self.swing = Some(Swing {
            attacker,
            hand,
            weapon,
        });
        Ok(())
    }

    /// Discard an interrupted swing without making a contact query.
    pub fn cancel(&mut self) {
        self.swing = None;
    }

    /// Consume a named animation event. Only HitFrame can query and apply damage.
    /// The query returns the first obstruction's actor identity, or None for
    /// empty space and non-actor obstructions. It must exclude the attacker.
    ///
    /// # Errors
    /// Changed hand equipment, a failed query or rejected damage. The contact
    /// opportunity is consumed on errors and misses as well as successful hits.
    pub fn event(
        &mut self,
        game: &mut Game,
        name: &str,
        query: impl FnOnce(GlobalFormId) -> Result<Option<GlobalFormId>, Error>,
    ) -> Result<Option<HitResult>, Error> {
        if name == "attackStop" {
            self.cancel();
            return Ok(None);
        }
        if name != "HitFrame" {
            return Ok(None);
        }
        let Some(swing) = self.swing.take() else {
            return Ok(None);
        };
        if game.world_mut().hand_weapon(swing.attacker, swing.hand) != Some(swing.weapon) {
            return Err(Error::Equipment);
        }
        let Some(target) = query(swing.attacker)? else {
            return Ok(None);
        };
        game.melee_hit(MeleeHit {
            attacker: swing.attacker,
            target,
            weapon: swing.weapon,
        })
        .map(Some)
    }
}
