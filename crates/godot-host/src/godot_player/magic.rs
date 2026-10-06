// SPDX-License-Identifier: GPL-3.0-or-later

//! Native magic menu presenter, mirroring `containers.rs`. SkyUI's magic
//! menu (`skyui/src/ItemMenus/magicmenu.fla`) lists known spells once and
//! lets a hand selection decide which casting slot a click fills, so the
//! two hand buttons stay visible beside the list rather than gating it.

use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_game::combat::Hand;
use soe_physics::{PhysicsWorld, Vec3};

use crate::magic_menu;

use super::PlayerNode;

fn touch_ray(transform: Transform3D, reach: f32) -> Option<(Vector3, Vector3)> {
    if !reach.is_finite() || reach < 0.0 || !transform.origin.is_finite() {
        return None;
    }
    let direction = -transform.basis.col_c();
    if !direction.is_finite() || direction.length_squared() == 0.0 {
        return None;
    }
    let end = transform.origin + direction.normalized() * (reach * crate::units::METRES_PER_UNIT);
    Some((transform.origin, end))
}

fn grab_actor_throw_impulse(direction: Vec3) -> Option<Vector3> {
    let length = direction.length();
    if !length.is_finite() || length <= f32::EPSILON {
        return None;
    }
    let direction = Vector3::new(direction.x, direction.z, -direction.y);
    let impulse = direction / length * 7.0 + Vector3::UP * 1.5;
    impulse.is_finite().then_some(impulse)
}

fn grab_actor_hold_position(eye: Vec3, direction: Vec3) -> Option<Vec3> {
    let length = direction.length();
    if !eye.x.is_finite()
        || !eye.y.is_finite()
        || !eye.z.is_finite()
        || !length.is_finite()
        || length <= f32::EPSILON
    {
        return None;
    }
    Some(eye + direction * (120.0 / length) - Vec3::Z * 72.0)
}

fn touch_target(hit: crate::godot_combat::ContactHit) -> Option<GlobalFormId> {
    match hit {
        crate::godot_combat::ContactHit::Actor(actor) => Some(actor),
        crate::godot_combat::ContactHit::Empty
        | crate::godot_combat::ContactHit::Obstruction(_) => None,
    }
}

const HANDS: [Hand; 2] = [Hand::Left, Hand::Right];

#[derive(Clone, Copy)]
pub(super) struct HandCastInput {
    pub was_held: bool,
    pub held: bool,
    pub pressed: bool,
}

fn select_telekinesis_target(
    eye: Vec3,
    direction: Vec3,
    candidates: impl IntoIterator<Item = (GlobalFormId, Vec3)>,
) -> Option<(GlobalFormId, f32)> {
    candidates
        .into_iter()
        .filter_map(|(reference, position)| {
            let delta = position - eye;
            let distance = delta.length();
            if !(1.0..=super::ACTIVATE_REACH).contains(&distance) {
                return None;
            }
            let along = delta.dot(direction);
            let lateral = (delta - direction * along).length();
            (along > 0.0 && lateral <= 48.0).then_some((reference, along, lateral))
        })
        .min_by(|(_, along_a, lateral_a), (_, along_b, lateral_b)| {
            lateral_a
                .total_cmp(lateral_b)
                .then_with(|| along_a.total_cmp(along_b))
        })
        .map(|(reference, along, _)| (reference, along))
}

impl PlayerNode {
    fn release_host_grab(&mut self, actor: GlobalFormId, direction: Vec3) {
        let impulse = grab_actor_throw_impulse(direction).unwrap_or(Vector3::UP * 1.5);
        self.actor_bodies.throw_grabbed_actor(actor, impulse);
    }

    /// Keep the player's active Grab Actor target at eye level, then throw it
    /// when its effect ends or the actor leaves the resident host set.
    pub(super) fn update_grab_actor(&mut self) {
        let desired = self.game.as_mut().and_then(|game| {
            let player = game.player();
            let mut world = game.world_mut();
            world.contact_actors().into_iter().find_map(|actor| {
                world
                    .grab_actor_effect_on(actor.actor)
                    .filter(|effect| effect.caster == Some(player))
                    .map(|effect| (effect.id, actor.actor))
            })
        });

        if self
            .host_grab_actor
            .is_some_and(|(_, actor)| self.actor_bodies.body(actor).is_none())
        {
            self.host_grab_actor = None;
        }
        if self.host_grab_actor.map(|(_, actor)| actor) != desired.map(|(_, actor)| actor) {
            if let Some((_, actor)) = self.host_grab_actor.take() {
                self.release_host_grab(actor, self.rig.look_direction());
            }
            if let Some((effect, actor)) = desired {
                if self.actor_bodies.grab_actor(actor) {
                    self.host_grab_actor = Some((effect, actor));
                }
            }
        } else if let Some((effect, actor)) = desired {
            self.host_grab_actor = Some((effect, actor));
        }

        let Some((_, actor)) = self.host_grab_actor else {
            return;
        };
        let eye = self.rig.head(self.feet(), self.capsule_height());
        let direction = self.rig.look_direction();
        let Some(feet) = grab_actor_hold_position(eye, direction) else {
            self.host_grab_actor = None;
            self.release_host_grab(actor, direction);
            return;
        };
        let pose = [feet.x, feet.y, feet.z];
        if self.actor_bodies.move_grabbed_actor(actor, pose) {
            if let Some(game) = &mut self.game {
                let rotation = game
                    .world()
                    .ref_placement(actor)
                    .map_or([0.0; 3], |placement| placement.rotation);
                game.world_mut().set_host_actor_pose(actor, pose, rotation);
            }
        } else {
            self.host_grab_actor = None;
            self.release_host_grab(actor, direction);
        }
    }

    /// Route an activation press to one resident movable reference while the
    /// player's active concentration effect is Telekinesis, then keep its
    /// host body at the same eye-relative position until World releases it.
    pub(super) fn update_telekinesis(&mut self, pressed: bool) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Magic);
        let player = self.game.as_mut().map(soe_game::Game::player);
        let Some(player) = player else {
            self.release_host_telekinesis();
            return;
        };
        let world_target = self
            .game
            .as_ref()
            .and_then(|game| game.world().telekinesis_reference(player));
        if let Some(target) = self.host_telekinesis_target {
            if world_target != Some(target) {
                self.release_host_telekinesis();
            }
        }
        if world_target.is_none() && pressed {
            self.try_grab_telekinesis_target(player);
        }
        let Some(target) = self.host_telekinesis_target else {
            return;
        };
        if self
            .game
            .as_ref()
            .is_none_or(|game| game.world().telekinesis_reference(player) != Some(target))
        {
            self.release_host_telekinesis();
            return;
        }
        let distance = self.telekinesis_distance.unwrap_or(180.0);
        let eye = self.rig.head(self.feet(), self.capsule_height());
        let direction = self.rig.look_direction();
        let position = [
            eye.x + direction.x * distance,
            eye.y + direction.y * distance,
            eye.z + direction.z * distance,
        ];
        if let Some(streamer) = self.game_streamer.as_mut() {
            if !streamer
                .bind_mut()
                .move_telekinesis_reference(target, position)
            {
                self.release_host_telekinesis();
            }
        } else {
            self.release_host_telekinesis();
        }
    }

    fn try_grab_telekinesis_target(&mut self, player: GlobalFormId) {
        let eye = self.rig.head(self.feet(), self.capsule_height());
        let direction = self.rig.look_direction();
        let (Some(game), Some(streamer)) = (&mut self.game, self.game_streamer.as_mut()) else {
            return;
        };
        let candidates = streamer.bind().movable_reference_poses();
        let mut references = Vec::new();
        for pose in candidates.iter_shared() {
            let Some(file_value) = pose.get("file") else {
                continue;
            };
            let Ok(file) = file_value.try_to::<i64>() else {
                continue;
            };
            let Some(object_value) = pose.get("object") else {
                continue;
            };
            let Ok(object) = object_value.try_to::<i64>() else {
                continue;
            };
            let Some(position_value) = pose.get("position") else {
                continue;
            };
            let Ok(position) = position_value.try_to::<Vector3>() else {
                continue;
            };
            let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
                continue;
            };
            let reference = GlobalFormId { file, object };
            references.push((reference, Vec3::new(position.x, position.y, position.z)));
        }
        let Some((target, distance)) = select_telekinesis_target(eye, direction, references) else {
            return;
        };
        if !game.world_mut().grab_telekinesis_reference(player, target) {
            return;
        }
        if streamer.bind_mut().grab_telekinesis_reference(target) {
            self.host_telekinesis_target = Some(target);
            self.telekinesis_distance = Some(distance);
            let world = game.shared_world();
            world
                .borrow_mut()
                .form_event(target, "OnGrab", Vec::new(), &mut |name, class| {
                    game.vm.create_object_of(name, class)
                });
        } else {
            let _ = game.world_mut().release_telekinesis_reference(player);
            self.telekinesis_distance = None;
        }
    }

    pub(super) fn release_host_telekinesis(&mut self) {
        if let Some(target) = self.host_telekinesis_target.take() {
            if let Some(game) = &mut self.game {
                let world = game.shared_world();
                world.borrow_mut().form_event(
                    target,
                    "OnRelease",
                    Vec::new(),
                    &mut |name, class| game.vm.create_object_of(name, class),
                );
            }
            if let Some(streamer) = self.game_streamer.as_mut() {
                streamer.bind_mut().release_telekinesis_reference(target);
            }
        } else if let Some(streamer) = self.game_streamer.as_mut() {
            streamer.bind_mut().release_grabbed_telekinesis_reference();
        }
        self.telekinesis_distance = None;
    }

    pub(super) fn route_hand_casts(&mut self, left: HandCastInput, right: HandCastInput) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Magic);
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let player = game.player();
        for (hand, input) in [(Hand::Left, left), (Hand::Right, right)] {
            if input.pressed || (input.held && !input.was_held) {
                if let Err(error) = game.begin_hand_cast(player, hand, None) {
                    soe_trace::error!(target: "host::magic", system = "host::magic", "PlayerNode: hand cast rejected: {error}");
                }
            }
            if !input.held && (input.was_held || input.pressed) {
                let _ = game.end_hand_cast(player, hand);
            }
        }
    }

    fn magic_content(&mut self) -> bool {
        self.menu.title() == Some("Magic") && self.present_magic_items()
    }
}

#[cfg(test)]
mod tests {
    use super::{grab_actor_hold_position, grab_actor_throw_impulse, select_telekinesis_target};
    use soe_esm::GlobalFormId;
    use soe_physics::Vec3;

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 1, object }
    }

    #[test]
    fn mag_41_selects_the_nearest_resident_body_on_the_eye_ray() {
        let target = select_telekinesis_target(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            [
                (id(3), Vec3::new(120.0, 40.0, 0.0)),
                (id(2), Vec3::new(80.0, 20.0, 0.0)),
                (id(1), Vec3::new(80.0, 60.0, 0.0)),
                (id(4), Vec3::new(-20.0, 0.0, 0.0)),
                (id(5), Vec3::new(181.0, 0.0, 0.0)),
            ],
        );
        assert_eq!(target, Some((id(2), 80.0)));
    }

    #[test]
    fn mag_66_throw_impulse_follows_the_look_direction_with_upward_lift() {
        let impulse = grab_actor_throw_impulse(Vec3::new(3.0, 0.0, 0.0)).unwrap();
        assert!((impulse.x - 7.0).abs() < 1e-5);
        assert!((impulse.y - 1.5).abs() < 1e-5);
        assert_eq!(impulse.z, 0.0);
    }

    #[test]
    fn mag_66_held_actor_stays_a_fixed_eye_relative_distance() {
        let eye = Vec3::new(100.0, 200.0, 300.0);
        let held = grab_actor_hold_position(eye, Vec3::new(2.0, 0.0, 0.0)).unwrap();
        assert_eq!(held, Vec3::new(220.0, 200.0, 228.0));
    }

    #[test]
    fn mag_66_held_actor_pose_rejects_non_finite_camera_state() {
        assert!(grab_actor_hold_position(Vec3::ZERO, Vec3::ZERO).is_none());
        assert!(
            grab_actor_hold_position(Vec3::new(f32::NAN, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0))
                .is_none()
        );
    }

    #[test]
    fn mag_66_throw_impulse_rejects_degenerate_look_directions() {
        assert!(grab_actor_throw_impulse(Vec3::ZERO).is_none());
        assert!(grab_actor_throw_impulse(Vec3::new(f32::NAN, 0.0, 0.0)).is_none());
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Cast a self-delivered spell through the shared runtime world.
    #[func]
    pub fn cast_spell_on_self(&mut self, spell_file: i32, spell_object: i64) -> bool {
        let (Ok(file), Ok(object), Some(game)) = (
            u16::try_from(spell_file),
            u32::try_from(spell_object),
            self.game.as_mut(),
        ) else {
            return false;
        };
        let spell = GlobalFormId { file, object };
        let player = game.player();
        let shared = game.shared_world();
        let result = shared.borrow_mut().cast_spell(
            spell,
            player,
            Some(player),
            Some(player),
            &mut |name, class| game.vm.create_object_of(name, class),
        );
        match result {
            Ok(effects) => !effects.is_empty(),
            Err(error) => {
                soe_trace::error!(target: "host::magic", system = "host::magic", "PlayerNode: self cast rejected: {error}");
                false
            }
        }
    }

    /// Cast a touch spell at the first eligible actor under the camera reticle.
    /// The ray is capped by the spell's authored SPIT reach and static bodies
    /// block actors behind them.
    #[func]
    pub fn cast_touch_spell(&mut self, spell_file: i32, spell_object: i64) -> bool {
        let (Ok(file), Ok(object), Some(transform), Some(world_3d)) = (
            u16::try_from(spell_file),
            u32::try_from(spell_object),
            self.camera
                .as_ref()
                .map(|camera| camera.get_global_transform()),
            self.base().get_world_3d(),
        ) else {
            return false;
        };
        let space = world_3d.get_space();
        if !space.is_valid() {
            return false;
        }
        if !self.sync_actor_bodies() {
            return false;
        }
        let spell = GlobalFormId { file, object };
        let Some(game) = &mut self.game else {
            return false;
        };
        let player = game.player();
        let definition = match game.world_mut().spell(spell) {
            Ok(definition) if definition.data.delivery == 1 => definition,
            _ => return false,
        };
        let reach = definition.data.range;
        if !reach.is_finite() || reach < 0.0 {
            return false;
        }
        let Some((origin, end)) = touch_ray(transform, reach) else {
            return false;
        };
        let owners = [&self.melee_bodies, self.actor_bodies.owners()];
        let hit = match crate::godot_combat::first_contact(space, &owners, player, origin, end) {
            Ok(hit) => hit,
            Err(error) => {
                soe_trace::error!(target: "host::magic", system = "host::magic", "PlayerNode: touch target query failed: {error}");
                return false;
            }
        };
        let Some(target) = touch_target(hit) else {
            return false;
        };
        let target_eligible = {
            let mut shared = game.world_mut();
            shared
                .reference(target)
                .is_some_and(|reference| reference.enabled && !reference.deleted)
                && shared.actor(target).is_some_and(|actor| !actor.dead)
        };
        if !target_eligible {
            return false;
        }
        let shared = game.shared_world();
        let result = shared.borrow_mut().cast_spell(
            spell,
            player,
            Some(player),
            Some(target),
            &mut |name, class| game.vm.create_object_of(name, class),
        );
        match result {
            Ok(_) => true,
            Err(error) => {
                soe_trace::error!(target: "host::magic", system = "host::magic", "PlayerNode: touch cast rejected: {error}");
                false
            }
        }
    }

    fn location_target(&self, reach: f32) -> Option<[f32; 3]> {
        if !reach.is_finite() || reach < 0.0 {
            return None;
        }
        let world = self.world.as_ref()?;
        let start = self.rig.head(self.feet(), self.capsule_height());
        let direction = self.rig.look_direction();
        let end = start + direction * reach;
        let hit = world.raycast(start, end)?;
        Some([hit.point.x, hit.point.y, hit.point.z])
    }

    /// Return the live static-world selection point in Godot metres.
    /// An empty array means the camera ray did not hit loaded geometry.
    #[func]
    pub fn selected_location_position(&self) -> PackedVector3Array {
        let Some([x, y, z]) = self.location_target(4096.0) else {
            return PackedVector3Array::new();
        };
        let position = crate::units::SkyrimPos::new(x, y, z).to_godot();
        [Vector3::new(position.x, position.y, position.z)]
            .into_iter()
            .collect()
    }

    /// Cast a target-location spell at the live point selected by the camera ray.
    #[func]
    pub fn cast_location_spell_at_look(&mut self, spell_file: i32, spell_object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(spell_file), u32::try_from(spell_object))
        else {
            return false;
        };
        let spell = GlobalFormId { file, object };
        let reach = self
            .game
            .as_mut()
            .and_then(|game| game.world_mut().spell(spell).ok())
            .filter(|definition| definition.data.delivery == 4)
            .map(|definition| definition.data.range);
        let Some(reach) = reach else {
            return false;
        };
        let Some([x, y, z]) = self.location_target(reach) else {
            return false;
        };
        self.cast_location_spell(
            spell_file,
            spell_object,
            f64::from(x),
            f64::from(y),
            f64::from(z),
        )
    }

    /// Cast a target-location spell at a selected Skyrim world position.
    /// Static world geometry blocks the cast when it intersects the segment
    /// from the player to the selected point.
    #[func]
    pub fn cast_location_spell(
        &mut self,
        spell_file: i32,
        spell_object: i64,
        x: f64,
        y: f64,
        z: f64,
    ) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(spell_file), u32::try_from(spell_object))
        else {
            return false;
        };
        let position = [x as f32, y as f32, z as f32];
        if !position.iter().all(|value| value.is_finite()) {
            return false;
        }
        let start = self.rig.head(self.feet(), self.capsule_height());
        let end = Vec3::new(position[0], position[1], position[2]);
        let line_of_sight_clear = self.world.as_ref().is_some_and(|world| {
            world
                .raycast(start, end)
                .is_none_or(|hit| hit.fraction >= 1.0 - 1e-4)
        });
        let Some(game) = &mut self.game else {
            return false;
        };
        let spell = GlobalFormId { file, object };
        let caster = game.player();
        let shared = game.shared_world();
        let result = shared.borrow_mut().cast_spell_at_location(
            spell,
            caster,
            Some(caster),
            position,
            line_of_sight_clear,
            &mut |name, class| game.vm.create_object_of(name, class),
        );
        match result {
            Ok(_) => true,
            Err(error) => {
                soe_trace::error!(target: "host::magic", system = "host::magic", "PlayerNode: {error}");
                false
            }
        }
    }

    /// Resolve one projectile's contacted actor body and apply the aimed spell.
    /// The projectile caller should discard its projectile after this contact.
    #[func]
    pub fn aimed_spell_projectile_contact(
        &mut self,
        spell_file: i32,
        spell_object: i64,
        projectile_instance_id: i64,
        contact_body: Rid,
    ) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(spell_file), u32::try_from(spell_object))
        else {
            return false;
        };
        let Some(target) = self.actor_bodies.owners().get(&contact_body).copied() else {
            return false;
        };
        let Some(game) = &mut self.game else {
            return false;
        };
        let spell = GlobalFormId { file, object };
        let caster = game.player();
        let shared = game.shared_world();
        self.aimed_projectile_contacts
            .contact(projectile_instance_id, || {
                let result = shared.borrow_mut().cast_spell_on_projectile_contact(
                    spell,
                    caster,
                    Some(caster),
                    target,
                    &mut |name, class| game.vm.create_object_of(name, class),
                );
                match result {
                    Ok(_) => true,
                    Err(error) => {
                        soe_trace::error!(target: "host::magic", system = "host::magic", "PlayerNode: {error}");
                        false
                    }
                }
            })
    }

    /// Release the one-shot contact guard when a projectile node is freed.
    #[func]
    pub fn aimed_spell_projectile_finished(&mut self, projectile_instance_id: i64) {
        self.aimed_projectile_contacts
            .forget(projectile_instance_id);
    }

    /// Open the shared player's known-spell list.
    #[func]
    pub fn open_magic_menu(&mut self) -> bool {
        if self.game.is_none() || !self.open_menu("Magic".into()) {
            return false;
        }
        self.magic_hand = Hand::Right;
        if self.magic_content() {
            true
        } else {
            self.close_menu();
            false
        }
    }

    /// Pick the hand a spell selection sends to next, then redraw the list.
    #[func]
    pub fn select_magic_hand(&mut self, index: i64) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        let Some(hand) = HANDS.get(index).copied() else {
            return false;
        };
        if self.menu.title() != Some("Magic") {
            return false;
        }
        self.magic_hand = hand;
        self.magic_content()
    }

    /// Equip a listed known spell to the active hand, then refresh the list.
    #[func]
    pub fn equip_magic_spell(&mut self, file: i64, object: i64) -> bool {
        if self.menu.title() != Some("Magic") {
            return false;
        }
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let spell = GlobalFormId { file, object };
        let hand = self.magic_hand;
        let changed = self
            .game
            .as_mut()
            .is_some_and(|game| magic_menu::equip_spell(game, hand, spell));
        if changed {
            self.magic_content();
        }
        changed
    }
}

#[cfg(test)]
mod mag_16_tests {
    use super::*;
    use crate::godot_combat::ContactHit;

    #[test]
    fn mag_16_touch_target_accepts_actor_hits_and_rejects_empty_or_obstructed_hits() {
        let actor = GlobalFormId {
            file: 1,
            object: 0x123,
        };
        assert_eq!(touch_target(ContactHit::Actor(actor)), Some(actor));
        assert_eq!(touch_target(ContactHit::Empty), None);
        assert_eq!(touch_target(ContactHit::Obstruction(Rid::Invalid)), None);
    }

    #[test]
    fn mag_16_touch_ray_uses_authored_reach_independent_of_camera_scale() {
        let transform = Transform3D::new(
            Basis::from_cols(Vector3::RIGHT, Vector3::UP, Vector3::new(0.0, 0.0, 4.0)),
            Vector3::ZERO,
        );
        let (start, end) = touch_ray(transform, 128.0).expect("finite reach makes a ray");
        assert!((start.distance_to(end) - 128.0 * crate::units::METRES_PER_UNIT).abs() < 0.0001);
        assert!(end.z < start.z, "camera forward points down negative Z");
    }

    #[test]
    fn mag_16_touch_ray_rejects_invalid_authored_reach() {
        let transform = Transform3D::IDENTITY;
        assert!(touch_ray(transform, -1.0).is_none());
        assert!(touch_ray(transform, f32::NAN).is_none());
    }
}
