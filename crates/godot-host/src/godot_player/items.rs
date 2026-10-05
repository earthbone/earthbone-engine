// SPDX-License-Identifier: GPL-3.0-or-later

//! SkyUI-informed native Control presentation, with actions resolved in Game.

use godot::classes::{Control, GDScript};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use super::PlayerNode;
use crate::item_menu::{self, Row};

fn dictionary(row: &Row, side: i64) -> VarDictionary {
    let mut data = VarDictionary::new();
    data.set("id", row.form.to_string());
    data.set("file", i64::from(row.form.file));
    data.set("object", i64::from(row.form.object));
    data.set("name", row.name.clone());
    data.set("category", row.category.clone());
    data.set("icon", row.icon.clone());
    data.set("count", row.count);
    data.set("equipped", row.equipped);
    data.set("favorite", row.favorite);
    data.set("value", row.value);
    data.set(
        "weight",
        &row.weight
            .map_or_else(Variant::nil, |value| value.to_variant()),
    );
    data.set(
        "damage",
        &row.damage
            .map_or_else(Variant::nil, |value| i64::from(value).to_variant()),
    );
    data.set(
        "armor",
        &row.armor
            .map_or_else(Variant::nil, |value| value.to_variant()),
    );
    data.set(
        "cost",
        &row.cost
            .map_or_else(Variant::nil, |value| i64::from(value).to_variant()),
    );
    data.set("description", row.description.clone());
    data.set("side", side);
    data
}

fn form(data: &VarDictionary) -> Option<GlobalFormId> {
    Some(GlobalFormId {
        file: u16::try_from(data.get("file")?.try_to::<i64>().ok()?).ok()?,
        object: u32::try_from(data.get("object")?.try_to::<i64>().ok()?).ok()?,
    })
}

impl PlayerNode {
    fn item_art_cache(&mut self) -> String {
        if let Some(cache) = &self.interface_cache {
            return cache.clone();
        }
        if let Ok(cache) = std::env::var("SOE_INTERFACE_CACHE") {
            self.interface_cache = Some(cache.clone());
            return cache;
        }
        let Some(source) = self.data_source.as_ref() else {
            return String::new();
        };
        let root = std::env::var_os("SOE_INTERFACE_CACHE_ROOT").map_or_else(
            || std::path::PathBuf::from("/tmp/soe-interface-cache"),
            std::path::PathBuf::from,
        );
        let plugins = source
            .order()
            .files()
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>();
        let cache = match soe_swf_convert::build_cache(
            source.vfs(),
            &plugins,
            &root,
            &soe_swf_convert::Decompiler::from_env(),
        ) {
            Ok(cache) => cache.directory.to_string_lossy().into_owned(),
            Err(error) => {
                godot_error!("PlayerNode: interface art cache failed: {error}");
                String::new()
            }
        };
        self.interface_cache = Some(cache.clone());
        cache
    }

    pub(super) fn present_items(&mut self, mode: &str, rows: &[(Row, i64)]) -> bool {
        let Some(mut content) = self.menu_content() else {
            return false;
        };
        let mut data = VarArray::new();
        for (row, side) in rows {
            data.push(&dictionary(row, *side).to_variant());
        }
        if let Some(mut view) = content.get_node_or_null("ItemMenu") {
            let _guard = self.base_mut();
            view.call("replace_rows", &[data.to_variant()]);
            return true;
        }
        let script = godot::tools::try_load::<GDScript>("res://ui/item_menu.gd");
        let Ok(script) = script else {
            godot_error!("PlayerNode: native item menu script is unavailable");
            return false;
        };
        let cache = self.item_art_cache();
        let base = self.base().clone();
        let _guard = self.base_mut();
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut view = Control::new_alloc();
        view.set_script(&script);
        view.connect("activate_item", &base.callable("item_menu_activate"));
        view.connect("favorite_item", &base.callable("item_menu_favorite"));
        content.add_child(&view);
        view.call(
            "configure",
            &[mode.to_variant(), data.to_variant(), cache.to_variant()],
        );
        true
    }

    pub(super) fn present_inventory_items(&mut self) -> bool {
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        let player = game.world_mut().player();
        let rows = item_menu::inventory(game, player)
            .into_iter()
            .map(|row| (row, 0))
            .collect::<Vec<_>>();
        self.present_items("inventory", &rows)
    }

    pub(super) fn present_magic_items(&mut self) -> bool {
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        let rows = item_menu::magic(game)
            .into_iter()
            .map(|row| (row, 0))
            .collect::<Vec<_>>();
        self.present_items("magic", &rows)
    }

    pub(super) fn present_container_items(&mut self, holder: GlobalFormId) -> bool {
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        let player = game.world_mut().player();
        let mut rows: Vec<_> = item_menu::inventory(game, holder)
            .into_iter()
            .map(|row| (row, 0))
            .collect();
        rows.extend(
            item_menu::inventory(game, player)
                .into_iter()
                .map(|row| (row, 1)),
        );
        self.present_items("container", &rows)
    }

    fn present_favorite_items(&mut self) -> bool {
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        let player = game.world_mut().player();
        let mut rows = item_menu::inventory(game, player);
        rows.extend(item_menu::magic(game));
        let rows = rows
            .into_iter()
            .filter(|row| row.favorite)
            .map(|row| (row, 0))
            .collect::<Vec<_>>();
        self.present_items("favorites", &rows)
    }

    fn refresh_item_menu(&mut self) -> bool {
        if let Some((merchant, faction)) = self.barter_menu {
            let Some(game) = self.game.as_mut() else {
                return false;
            };
            let rows = item_menu::barter(game, faction, merchant);
            return self.present_items("barter", &rows);
        }
        if let Some((holder, _)) = self.container_menu {
            return self.present_container_items(holder);
        }
        match self.menu.title() {
            Some("Inventory") => self.present_inventory_items(),
            Some("Magic") => self.present_magic_items(),
            Some("Favorites") => self.present_favorite_items(),
            _ => false,
        }
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Open the existing actor favorite set through the native presenter.
    #[func]
    pub fn open_favorites_menu(&mut self) -> bool {
        if self.game.is_none() || !self.open_menu("Favorites".into()) {
            return false;
        }
        self.present_favorite_items()
    }

    /// Resolve a presenter action against the currently open shared Game.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn item_menu_activate(&mut self, data: VarDictionary, amount: i64, side: i64) -> bool {
        let Some(item) = form(&data) else {
            return false;
        };
        let Ok(amount) = i32::try_from(amount) else {
            return false;
        };
        if !self.menu.is_open() || amount <= 0 || !matches!(side, 0 | 1) {
            return false;
        }
        let title = self.menu.title().unwrap_or_default().to_owned();
        let holder = self.container_menu.map(|(holder, _)| holder);
        let barter = self.barter_menu;
        let hand = self
            .menu_content()
            .and_then(|content| content.get_node_or_null("ItemMenu"))
            .map_or(1, |view| view.get_meta("hand").try_to::<i64>().unwrap_or(1));
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let success = if let Some((merchant, faction)) = barter {
            item_menu::trade(game, faction, merchant, item, amount, side)
        } else if let Some(holder) = holder {
            let player = game.player();
            let (source, destination) = if side == 0 {
                (holder, player)
            } else {
                (player, holder)
            };
            game.transfer_item(source, item, amount, destination) > 0
        } else if matches!(title.as_str(), "Magic" | "Favorites")
            && game.world().tag_of(item) == Some(soe_esm::Tag(*b"SPEL"))
        {
            let hand = if hand == 0 {
                soe_game::combat::Hand::Left
            } else {
                soe_game::combat::Hand::Right
            };
            crate::magic_menu::equip_spell(game, hand, item)
        } else if matches!(title.as_str(), "Inventory" | "Favorites") {
            crate::inventory_menu::toggle_equip(game, item)
        } else {
            false
        };
        success && self.refresh_item_menu()
    }

    /// Favorite actions mutate the existing SHM-13 actor state and save section.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn item_menu_favorite(&mut self, data: VarDictionary) -> bool {
        if !matches!(self.menu.title(), Some("Inventory" | "Magic" | "Favorites")) {
            return false;
        }
        let Some(item) = form(&data) else {
            return false;
        };
        self.game
            .as_ref()
            .is_some_and(|game| item_menu::toggle_favorite(game, item))
            && self.refresh_item_menu()
    }
}
