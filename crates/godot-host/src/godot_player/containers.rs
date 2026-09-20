// SPDX-License-Identifier: GPL-3.0-or-later

//! Container controls read the same inventory rows as the player menu and
//! move an item through the shared game's notified transaction.

use godot::classes::control::SizeFlags;
use godot::classes::{Button, Label, ScrollContainer, VBoxContainer};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use crate::inventory_menu::{self, Category};

use super::PlayerNode;

impl PlayerNode {
    pub(super) fn open_container_menu(&mut self, holder: GlobalFormId) -> bool {
        let title = self
            .game
            .as_ref()
            .and_then(|game| game.world().name_of(holder))
            .unwrap_or_else(|| "Container".into());
        if !self.open_menu(GString::from(&title)) {
            return false;
        }
        self.container_menu = Some((holder, Category::All));
        if self.container_content(holder, Category::All) {
            true
        } else {
            self.close_menu();
            false
        }
    }

    fn container_content(&mut self, holder: GlobalFormId, category: Category) -> bool {
        if !self.menu.is_open() || self.container_menu.is_none() {
            return false;
        }
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        let entries = inventory_menu::entries(game, holder, category);
        let Some(mut content) = self.menu_content() else {
            return false;
        };
        self.container_menu = Some((holder, category));
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
        for (index, category) in Category::ALL.into_iter().enumerate() {
            let mut button = Button::new_alloc();
            button.set_text(category.label());
            let callback = base
                .callable("container_category")
                .bindv(&varray![index as i64]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        let mut heading = Label::new_alloc();
        heading.set_text(&format!("{} ({})", category.label(), entries.len()));
        rows.add_child(&heading);
        for entry in entries {
            let mut button = Button::new_alloc();
            button.set_text(&format!("Take {} x{}", entry.name, entry.count));
            let callback = base.callable("take_container_item").bindv(&varray![
                i64::from(holder.file),
                i64::from(holder.object),
                i64::from(entry.item.file),
                i64::from(entry.item.object),
            ]);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        true
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Select a category in the open container's form-backed item list.
    #[func]
    pub fn container_category(&mut self, index: i64) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        let Some(category) = Category::ALL.get(index).copied() else {
            return false;
        };
        let Some((holder, _)) = self.container_menu else {
            return false;
        };
        self.menu.is_open() && self.container_content(holder, category)
    }

    /// Take one listed base-form item and refresh the same container category.
    #[func]
    pub fn take_container_item(
        &mut self,
        holder_file: i64,
        holder_object: i64,
        item_file: i64,
        item_object: i64,
    ) -> bool {
        let (Ok(holder_file), Ok(holder_object), Ok(item_file), Ok(item_object)) = (
            u16::try_from(holder_file),
            u32::try_from(holder_object),
            u16::try_from(item_file),
            u32::try_from(item_object),
        ) else {
            return false;
        };
        let holder = GlobalFormId {
            file: holder_file,
            object: holder_object,
        };
        let item = GlobalFormId {
            file: item_file,
            object: item_object,
        };
        let Some((open_holder, category)) = self.container_menu else {
            return false;
        };
        if !self.menu.is_open() || open_holder != holder {
            return false;
        }
        let moved = self.game.as_mut().map_or(0, |game| {
            let player = game.player();
            game.transfer_item(holder, item, 1, player)
        });
        if moved == 0 {
            return false;
        }
        self.container_content(holder, category)
    }
}
