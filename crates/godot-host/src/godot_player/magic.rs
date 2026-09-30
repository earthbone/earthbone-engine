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
    /// Route an activation press to one resident movable reference while the
    /// player's active concentration effect is Telekinesis, then keep its
    /// host body at the same eye-relative position until World releases it.
    pub(super) fn update_telekinesis(&mut self, pressed: bool) {
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

    fn release_host_telekinesis(&mut self) {
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
        }
        self.telekinesis_distance = None;
    }

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

#[cfg(test)]
mod tests {
    use super::select_telekinesis_target;
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
}

#[godot_api(secondary)]
impl PlayerNode {
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
        let start = self.feet();
        let end = soe_physics::Vec3::new(position[0], position[1], position[2]);
        let line_of_sight_clear = self
            .world
            .as_ref()
            .is_some_and(|world| world.raycast(start, end).is_none());
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
