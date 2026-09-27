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

use crate::magic_menu;

use super::PlayerNode;

const HANDS: [Hand; 2] = [Hand::Left, Hand::Right];

fn hand_label(hand: Hand) -> &'static str {
    match hand {
        Hand::Left => "Left Hand",
        Hand::Right => "Right Hand",
    }
}

impl PlayerNode {
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
