// SPDX-License-Identifier: GPL-3.0-or-later

//! Container controls read the same inventory rows as the player menu and
//! move an item through the shared game's notified transaction.

use godot::classes::{Button, Label};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use crate::inventory_menu::{self, Category};

use super::PlayerNode;

impl PlayerNode {
    pub(super) fn open_barter_menu(&mut self, merchant: GlobalFormId) -> bool {
        let title = self
            .game
            .as_ref()
            .and_then(|game| game.world().name_of(merchant))
            .map_or_else(|| "Barter".to_owned(), |name| format!("Barter: {name}"));
        if !self.open_menu(GString::from(&title)) {
            return false;
        }
        let goods = self.game.as_mut().and_then(|game| {
            let mut factions: Vec<_> = game
                .world()
                .actor_ref(merchant)
                .map(|actor| actor.factions.keys().copied().collect())
                .unwrap_or_default();
            factions.sort();
            factions.into_iter().find_map(|faction| {
                game.resolve_vendor_service(faction, merchant)
                    .ok()
                    .flatten()
                    .map(|service| {
                        if !service.available {
                            return Vec::new();
                        }
                        service
                            .merchandise
                            .iter()
                            .map(|(item, count)| (crate::item_menu::item(game, *item, count), 0))
                            .collect::<Vec<_>>()
                    })
            })
        });
        let rows = goods.unwrap_or_default();
        self.present_items("barter", &rows)
    }

    pub(super) fn open_gift_menu(&mut self) -> bool {
        let Some(request) = self
            .game
            .as_ref()
            .and_then(soe_game::Game::gift_menu_request)
        else {
            return false;
        };
        let title = self
            .game
            .as_ref()
            .and_then(|game| game.world().name_of(request.giver))
            .unwrap_or_else(|| "Gift".into());
        if !self.open_menu(GString::from(&format!("Gift: {title}"))) {
            return false;
        }
        self.gift_menu_content()
    }

    fn gift_menu_content(&mut self) -> bool {
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        let Some(request) = game.gift_menu_request() else {
            return false;
        };
        let entries = inventory_menu::gift_entries(game, &request);
        let Some(mut content) = self.menu_content() else {
            return false;
        };
        let mut rows = self.item_menu_rows(&mut content, entries.first().map(|entry| entry.item));
        let base = self.base().clone();
        let _guard = self.base_mut();
        let action = if request.source == request.giver {
            "Give"
        } else {
            "Take"
        };
        for entry in entries {
            let mut button = Button::new_alloc();
            button.set_text(&format!("{action} {} x{}", entry.name, entry.count));
            let callback = base.callable("gift_menu_transfer").bindv(&varray![
                i64::from(entry.item.file),
                i64::from(entry.item.object),
            ]);
            super::item_card::connect_preview(&base, &mut button, entry.item);
            button.connect("pressed", &callback);
            rows.add_child(&button);
        }
        if rows.get_child_count() == 0 {
            let mut empty = Label::new_alloc();
            empty.set_text("No matching items.");
            rows.add_child(&empty);
        }
        true
    }

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
        self.container_menu = Some((holder, category));
        if !self.present_container_items(holder) {
            return false;
        }
        if let Some(mut view) = self
            .menu_content()
            .and_then(|content| content.get_node_or_null("ItemMenu"))
        {
            let _guard = self.base_mut();
            view.call("select_category", &[category.label().to_variant()]);
        }
        true
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Container identities in the current cell for host interaction tools.
    #[func]
    pub fn gameplay_containers(&mut self) -> Array<VarDictionary> {
        let (Some(game), Some(cell)) = (&mut self.game, self.loaded_cell) else {
            return Array::new();
        };
        let mut world = game.world_mut();
        let references = world.cell_references(cell);
        let mut result = Array::new();
        for reference in references {
            let Some(base) = world.base_of(reference) else {
                continue;
            };
            if world.tag_of(base) != Some(soe_esm::Tag(*b"CONT")) {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set("file", i64::from(reference.file));
            row.set("object", i64::from(reference.object));
            row.set(
                "name",
                world
                    .name_of(reference)
                    .unwrap_or_else(|| "Container".into())
                    .as_str(),
            );
            result.push(&row);
        }
        result
    }

    /// Activate a container through the same lock and script path as the crosshair.
    #[func]
    pub fn open_container_inventory(&mut self, file: i64, object: i64) -> bool {
        let (Ok(file), Ok(object), Some(game)) = (
            u16::try_from(file),
            u32::try_from(object),
            self.game.as_ref(),
        ) else {
            return false;
        };
        let holder = GlobalFormId { file, object };
        let container = {
            let mut world = game.world_mut();
            world
                .base_of(holder)
                .is_some_and(|base| world.tag_of(base) == Some(soe_esm::Tag(*b"CONT")))
        };
        container && self.activate_game_reference(holder) && self.container_menu.is_some()
    }

    /// Transfer one selected gift stack through Game's inventory transaction.
    #[func]
    pub fn gift_menu_transfer(&mut self, file: i64, object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let item = GlobalFormId { file, object };
        let moved = self
            .game
            .as_mut()
            .map_or(0, |game| game.gift_menu_transfer(item, 1));
        moved > 0 && self.gift_menu_content()
    }

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
