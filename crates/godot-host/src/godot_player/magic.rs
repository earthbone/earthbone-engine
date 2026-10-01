// SPDX-License-Identifier: GPL-3.0-or-later

//! Native magic menu presenter, mirroring `containers.rs`. SkyUI's magic
//! menu (`skyui/src/ItemMenus/magicmenu.fla`) lists known spells once and
//! lets a hand selection decide which casting slot a click fills, so the
//! two hand buttons stay visible beside the list rather than gating it.

use godot::classes::control::SizeFlags;
use godot::classes::{Button, Label, ScrollContainer, VBoxContainer};
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

fn hand_label(hand: Hand) -> &'static str {
    match hand {
        Hand::Left => "Left Hand",
        Hand::Right => "Right Hand",
    }
}

impl PlayerNode {
    pub(super) fn route_hand_casts(&mut self, left: HandCastInput, right: HandCastInput) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let player = game.player();
        for (hand, input) in [(Hand::Left, left), (Hand::Right, right)] {
            if input.pressed || (input.held && !input.was_held) {
                if let Err(error) = game.begin_hand_cast(player, hand, None) {
                    godot_error!("PlayerNode: hand cast rejected: {error}");
                }
            }
            if !input.held && (input.was_held || input.pressed) {
                let _ = game.end_hand_cast(player, hand);
            }
        }
    }

    fn magic_content(&mut self) -> bool {
        if self.menu.title() != Some("Magic") {
            return false;
        }
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let player = game.player();
        let entries = magic_menu::entries(game, player);
        let Some(mut content) = self.menu_content() else {
            return false;
        };
        let hand = self.magic_hand;
        let base = self.base().clone();
        let _guard = self.base_mut();
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut rows = VBoxContainer::new_alloc();
        scroll.add_child(&rows);
        content.add_child(&scroll);
        for (index, candidate) in HANDS.into_iter().enumerate() {
            let mut button = Button::new_alloc();
            let mark = if candidate == hand { " *" } else { "" };
            button.set_text(&format!("{}{}", hand_label(candidate), mark));
            let callback = base
                .callable("select_magic_hand")
                .bindv(&varray![index as i64]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        let mut heading = Label::new_alloc();
        heading.set_text(&format!("Known spells ({})", entries.len()));
        rows.add_child(&heading);
        for entry in entries {
            let mut button = Button::new_alloc();
            let equipped = match (entry.left_equipped, entry.right_equipped) {
                (true, true) => " [Left, Right]",
                (true, false) => " [Left]",
                (false, true) => " [Right]",
                (false, false) => "",
            };
            button.set_text(&format!("{} ({} cost){}", entry.name, entry.cost, equipped));
            let callback = base.callable("equip_magic_spell").bindv(&varray![
                i64::from(entry.spell.file),
                i64::from(entry.spell.object),
            ]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        true
    }
}

#[godot_api(secondary)]
impl PlayerNode {
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
                godot_error!("PlayerNode: touch target query failed: {error}");
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
                godot_error!("PlayerNode: touch cast rejected: {error}");
                false
            }
        }
    }

    fn location_target(&self) -> Option<[f32; 3]> {
        let world = self.world.as_ref()?;
        let start = self.rig.head(self.feet(), self.capsule_height());
        let direction = self.rig.look_direction();
        let end = start + direction * 4096.0;
        let hit = world.raycast(start, end)?;
        Some([hit.point.x, hit.point.y, hit.point.z])
    }

    /// Return the live static-world selection point in Godot metres.
    /// An empty array means the camera ray did not hit loaded geometry.
    #[func]
    pub fn selected_location_position(&self) -> PackedVector3Array {
        let Some([x, y, z]) = self.location_target() else {
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
        let Some([x, y, z]) = self.location_target() else {
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
                godot_error!("PlayerNode: {error}");
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
                        godot_error!("PlayerNode: {error}");
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
