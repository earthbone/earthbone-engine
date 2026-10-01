// SPDX-License-Identifier: GPL-3.0-or-later

//! Authored HitFrame contacts, following CK equipment semantics and Nemesis
//! animation event tables. Spatial queries return the first obstruction.

#[cfg(feature = "godot")]
use std::collections::HashMap;
#[cfg(feature = "godot")]
use std::hash::BuildHasher;

#[cfg(feature = "godot")]
use godot::builtin::Rid;
use soe_esm::GlobalFormId;
use soe_game::combat::{Error, FlightRequest, Hand, HitResult, MeleeHit};
#[cfg(feature = "godot")]
use soe_game::combat::{FlightMotion, FlightSegment};
use soe_game::Game;
use soe_hkx::behavior::{BehaviorGraph, ClipTrigger, EventInfo, EventProperty, NodeKind};
use soe_hkx::interp::ClipLibrary;
use soe_hkx::skeleton::Skeleton;
use soe_hkx::QsTransform;

use crate::skinned::Affine;

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

/// Armed bow or crossbow release, canceled without consuming ammunition.
#[derive(Debug, Default)]
pub struct RangedRelease {
    shot: Option<(GlobalFormId, GlobalFormId)>,
}

/// One terminal projectile contact, retaining its first-hit identity and point.
#[cfg(feature = "godot")]
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg(feature = "godot")]
pub struct ProjectileContact {
    pub hit: crate::godot_combat::ContactHit,
    pub position: [f32; 3],
}

/// Result of advancing one transient arrow or bolt.
#[cfg(feature = "godot")]
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg(feature = "godot")]
pub enum ProjectileUpdate {
    /// The projectile moved without reaching its configured range.
    Flying { position: [f32; 3] },
    /// The first obstruction or actor stopped the projectile.
    #[cfg(feature = "godot")]
    Contact(ProjectileContact),
    /// The projectile reached its configured maximum range.
    RangeEnded { position: [f32; 3] },
    /// A previously stopped or expired projectile cannot contact again.
    Finished,
}

/// Deterministic, transient flight and one-contact latch for a released shot.
#[cfg(feature = "godot")]
#[derive(Debug, Clone, Copy)]
#[cfg(feature = "godot")]
pub struct ProjectileFlight {
    attacker: GlobalFormId,
    request: FlightRequest,
    motion: FlightMotion,
    finished: bool,
    missed_arrow: Option<[f32; 3]>,
}

#[cfg(feature = "godot")]
impl ProjectileFlight {
    /// Start flight from the host's record-backed release request.
    #[must_use]
    pub fn new(request: FlightRequest) -> Self {
        Self {
            attacker: request.attacker,
            request,
            motion: FlightMotion::new(request),
            finished: false,
            missed_arrow: None,
        }
    }

    /// Place a recoverable arrow after a world obstruction stops this flight.
    ///
    /// The impact is consumed on the first call, even when its AMMO record is
    /// not recoverable, so repeated host finalization cannot create duplicates.
    #[must_use]
    pub fn recover_missed_arrow(&mut self, game: &mut Game) -> Option<GlobalFormId> {
        let position = self.missed_arrow.take()?;
        game.world_mut()
            .place_recovered_arrow(&self.request, position)
    }

    /// Advance once and query the swept segment for its first contact.
    ///
    /// # Errors
    /// An invalid time step, invalid motion or unavailable physics space.
    #[cfg(feature = "godot")]
    pub fn advance<S: BuildHasher>(
        &mut self,
        space: Rid,
        owners: &[&HashMap<Rid, GlobalFormId, S>],
        delta_seconds: f32,
    ) -> Result<ProjectileUpdate, Error> {
        let attacker = self.attacker;
        self.advance_with(delta_seconds, |segment| {
            crate::godot_combat::first_projectile_contact(space, owners, attacker, segment)
        })
    }

    /// Advance with a contact query, allowing deterministic headless tests.
    ///
    /// # Errors
    /// The time step, motion or supplied contact query is invalid.
    pub fn advance_with(
        &mut self,
        delta_seconds: f32,
        query: impl FnOnce(FlightSegment) -> Result<Option<ProjectileContact>, Error>,
    ) -> Result<ProjectileUpdate, Error> {
        if self.finished {
            return Ok(ProjectileUpdate::Finished);
        }
        let previous_motion = self.motion;
        let Some(segment) = self.motion.advance(delta_seconds)? else {
            return Ok(ProjectileUpdate::Flying {
                position: self.motion.position(),
            });
        };
        let contact = match query(segment) {
            Ok(contact) => contact,
            Err(error) => {
                self.motion = previous_motion;
                return Err(error);
            }
        };
        if let Some(contact) = contact {
            self.motion.stop_at(contact.position);
            self.finished = true;
            if matches!(contact.hit, crate::godot_combat::ContactHit::Obstruction(_)) {
                self.missed_arrow = Some(contact.position);
            }
            return Ok(ProjectileUpdate::Contact(contact));
        }
        if !self.motion.is_active() {
            self.finished = true;
            return Ok(ProjectileUpdate::RangeEnded {
                position: self.motion.position(),
            });
        }
        Ok(ProjectileUpdate::Flying {
            position: self.motion.position(),
        })
    }
}

impl RangedRelease {
    /// Arm the actor's currently equipped bow or crossbow.
    ///
    /// # Errors
    /// Missing ranged equipment or ammunition.
    pub fn begin(&mut self, game: &mut Game, attacker: GlobalFormId) -> Result<(), Error> {
        if self.shot.is_some() {
            return Err(Error::Unsupported("projectile release already armed"));
        }
        let equipment = game.world_mut().ranged_equipment(attacker)?;
        self.shot = Some((attacker, equipment.weapon));
        Ok(())
    }

    /// Cancel a draw before its release event.
    pub fn cancel(&mut self) {
        self.shot = None;
    }

    /// Consume the armed shot only when the authored bow release event arrives.
    ///
    /// # Errors
    /// Equipment changed or the game rejected the release transaction.
    pub fn event(
        &mut self,
        game: &mut Game,
        name: &str,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<Option<FlightRequest>, Error> {
        if name == "attackStop" || name == "bowCancel" {
            self.cancel();
            return Ok(None);
        }
        if name != "bowRelease" {
            return Ok(None);
        }
        let Some((attacker, weapon)) = self.shot.take() else {
            return Ok(None);
        };
        game.release_projectile(attacker, weapon, origin, direction)
            .map(Some)
    }
}

impl MeleeContact {
    /// Arm the player's right-hand swing for a newly pressed attack action.
    ///
    /// The host supplies a press edge. Following held frames pass `false`,
    /// so they cannot replace an armed swing.
    ///
    /// # Errors
    /// The player's right hand has no usable melee weapon, or another swing
    /// is still armed.
    pub fn begin_from_attack_press(
        &mut self,
        game: &mut Game,
        pressed: bool,
    ) -> Result<bool, Error> {
        if !pressed {
            return Ok(false);
        }
        let player = game.player();
        self.begin(game, player, Hand::Right)?;
        Ok(true)
    }

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

    /// The armed swing's attacker and hand, so a host can read that hand's
    /// weapon bone from the attacker's pose before the contact marker.
    #[must_use]
    pub fn armed(&self) -> Option<(GlobalFormId, Hand)> {
        self.swing.map(|swing| (swing.attacker, swing.hand))
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
            // Shield-bash gesture detection is not wired up on the host side
            // yet. Every contact this bridge delivers is an ordinary weapon
            // swing until that lands. See docs/combat.md CMB-20.
            is_bash: false,
            // `begin` arms ordinary swings only. The host has no power
            // attack trigger yet, so no stamina cost applies here.
            is_power_attack: false,
        })
        .map(Some)
    }
}

/// Skeleton nodes a hand's weapon hangs from, most specific first. The
/// vanilla skeleton parents `WEAPON` under the right hand and `SHIELD` under
/// the left, the nodes activeragdoll's `GetWeaponNode` resolves
/// (`engine-refs/activeragdoll/src/utils.cpp`). The hand bones follow as a
/// fallback for skeletons without those nodes.
#[must_use]
pub fn weapon_bones(hand: Hand) -> &'static [&'static str] {
    match hand {
        Hand::Right => &["WEAPON", "Weapon", "NPC R Hand [RHnd]"],
        Hand::Left => &["SHIELD", "Shield", "NPC L Hand [LHnd]"],
    }
}

/// Find the armed weapon node in an animated model pose and convert its
/// transform to Godot axes and metres. The pose is the same model-space buffer
/// used to skin the actor, before inverse bind transforms are applied.
///
/// # Errors
/// The rig has no weapon or hand node, or the pose is missing that bone.
pub fn posed_weapon_bone(
    skeleton: &Skeleton,
    model: &[QsTransform],
    hand: Hand,
) -> Result<Affine, Error> {
    let index = weapon_bones(hand)
        .iter()
        .find_map(|name| skeleton.bone_index(name))
        .ok_or(Error::Unsupported(
            "rig has no weapon bone for the swing hand",
        ))?;
    let pose = model
        .get(index)
        .ok_or(Error::Unsupported("animated weapon bone has no pose"))?;
    Ok(Affine::from_qs(pose).to_godot())
}

/// A swing's blade as a segment from the weapon node's origin along its
/// blade axis, `reach` long, in whatever unit the caller works in.
///
/// activeragdoll measures the blade along the weapon node's local +Y
/// (`ForwardVector`, the matrix's second column). Skyrim +Y maps to Godot
/// -Z under `units::SkyrimPos::to_godot`, so a Godot caller passes the node's
/// negated basis Z column as `blade_axis`.
///
/// # Errors
/// Non-finite input, a zero axis, or a non-positive reach.
pub fn weapon_segment(
    origin: [f32; 3],
    blade_axis: [f32; 3],
    reach: f32,
) -> Result<([f32; 3], [f32; 3]), Error> {
    let length = blade_axis.iter().map(|c| c * c).sum::<f32>().sqrt();
    if !origin.iter().all(|c| c.is_finite())
        || !length.is_finite()
        || length <= f32::EPSILON
        || !reach.is_finite()
        || reach <= 0.0
    {
        return Err(Error::Number);
    }
    let tip = [
        origin[0] + blade_axis[0] / length * reach,
        origin[1] + blade_axis[1] / length * reach,
        origin[2] + blade_axis[2] / length * reach,
    ];
    Ok((origin, tip))
}

/// What one synchronization pass has to do to the host's actor bodies.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct BodySyncPlan {
    /// Actors without a body, or whose shape no longer fits their scale.
    pub create: Vec<soe_game::world::ContactActor>,
    /// Actors that keep their body and only move.
    pub moved: Vec<soe_game::world::ContactActor>,
    /// Bodies whose actor left the resident cells, was disabled or unloaded.
    pub removed: Vec<GlobalFormId>,
}

/// Scale difference below which a body keeps its shape.
const SCALE_TOLERANCE: f32 = 1e-4;

/// Diff the bodies a host holds, keyed by actor with the scale their shape
/// was built for, against the actors the shared World reports now. A scale
/// change removes and recreates the body, since a capsule's size is fixed
/// at creation.
#[must_use]
pub fn plan_body_sync<S: std::hash::BuildHasher>(
    current: &std::collections::HashMap<GlobalFormId, f32, S>,
    wanted: &[soe_game::world::ContactActor],
) -> BodySyncPlan {
    let mut plan = BodySyncPlan::default();
    let mut seen = std::collections::HashSet::with_capacity(wanted.len());
    for actor in wanted {
        if !seen.insert(actor.actor) {
            continue;
        }
        match current.get(&actor.actor) {
            Some(scale) if (scale - actor.scale).abs() <= SCALE_TOLERANCE => {
                plan.moved.push(*actor);
            }
            Some(_) => {
                plan.removed.push(actor.actor);
                plan.create.push(*actor);
            }
            None => plan.create.push(*actor),
        }
    }
    let mut gone: Vec<GlobalFormId> = current
        .keys()
        .filter(|form| !seen.contains(*form))
        .copied()
        .collect();
    gone.sort();
    plan.removed.extend(gone);
    plan
}
