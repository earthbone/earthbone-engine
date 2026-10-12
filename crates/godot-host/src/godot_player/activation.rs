// SPDX-License-Identifier: GPL-3.0-or-later

//! Loose-item prompts and book presentation over Game's activation path.
//! CK Activate and OnActivate define default processing and event delivery.
//! UESP BOOK defines FULL and localized DESC, resolved by World.

use godot::classes::control::SizeFlags;
use godot::classes::RichTextLabel;
use godot::prelude::*;
use soe_esm::GlobalFormId;

use super::PlayerNode;

pub(super) fn prompt(game: &soe_game::Game) -> Option<(String, &'static str, bool)> {
    let mut world = game.world_mut();
    let target = world.crosshair_reference()?;
    if !world.reference_enabled(target) || world.grabbed_reference().is_some() {
        return None;
    }
    let reference = world.reference(target)?;
    let base = reference.base?;
    let item = reference.item.is_some();
    let kind = world.tag_of(base)?;
    let read_only = kind.as_bytes() == b"BOOK" && !world.book_can_be_taken(base);
    let name = world.name_of(target).filter(|name| !name.is_empty())?;
    let actor = world.player();
    let stealing = item && !read_only && world.loose_item_is_stealing(target, actor);
    let action = if stealing {
        "Steal"
    } else if read_only {
        "Read"
    } else if item {
        "Take"
    } else {
        match kind.as_bytes() {
            b"ACTI" => "Activate",
            b"FURN" => "Use",
            b"DOOR" | b"CONT" => "Open",
            _ => return None,
        }
    };
    Some((name, action, stealing))
}

impl PlayerNode {
    pub(super) fn open_book_menu(&mut self, book: GlobalFormId) -> bool {
        let Some(game) = &self.game else {
            return false;
        };
        let world = game.world();
        let title = world.name_of(book).unwrap_or_else(|| "Book".into());
        let text = world.description_of(book).unwrap_or_default();
        drop(world);
        if !self.open_menu(GString::from(&format!("Book: {title}"))) {
            return false;
        }
        let Some(mut content) = self.menu_content() else {
            return false;
        };
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut body = RichTextLabel::new_alloc();
        body.set_name("BookText");
        body.set_v_size_flags(SizeFlags::EXPAND_FILL);
        body.set_text(&book_text(&text));
        content.add_child(&body);
        true
    }
}

fn book_text(text: &str) -> String {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let tag = rest[start + 1..start + end].trim().to_ascii_lowercase();
        if matches!(
            tag.as_str(),
            "br" | "br/" | "br /" | "p" | "/p" | "pagebreak"
        ) {
            output.push('\n');
        }
        rest = &rest[start + end + 1..];
    }
    output.push_str(rest);
    output
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Current item name, action and ownership marker from the shared ray target.
    #[func]
    pub fn activation_prompt(&self) -> VarDictionary {
        let mut result = VarDictionary::new();
        if let Some((name, action, stealing)) = self.game.as_ref().and_then(prompt) {
            result.set("name", name);
            result.set("action", action);
            result.set("stealing", stealing);
        }
        result
    }

    /// Attach a counting Papyrus OnActivate observer for a host interaction check.
    #[func]
    pub fn debug_watch_activation(&mut self, file: i32, object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let Some(game) = &mut self.game else {
            return false;
        };
        let target = GlobalFormId { file, object };
        if game.world_mut().reference(target).is_none() {
            return false;
        }
        use soe_papyrus::asm::{i, ident, none, FunctionBuilder, ScriptBuilder};
        use soe_papyrus::Opcode::{IAdd, Return};
        let bytes = ScriptBuilder::new("SoeActivationObserver", "ObjectReference")
            .variable("activations", "Int", i(0))
            .function(
                FunctionBuilder::new("OnActivate", "None")
                    .param("activator", "ObjectReference")
                    .ins(IAdd, vec![ident("activations"), ident("activations"), i(1)])
                    .ins(Return, vec![none()])
                    .build(),
            )
            .bytes();
        if game.load_script(&bytes).is_err() {
            return false;
        }
        let target = game.object_of_form(target);
        game.vm.attach(target, "SoeActivationObserver").is_ok()
    }

    /// Read pickup and observer results from the shared World and Papyrus VM.
    #[func]
    pub fn activation_reference_status(&mut self, file: i32, object: i64) -> VarDictionary {
        let mut result = VarDictionary::new();
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return result;
        };
        let Some(game) = &mut self.game else {
            return result;
        };
        let target = GlobalFormId { file, object };
        let player = game.player();
        let mut world = game.world_mut();
        let Some(reference) = world.reference(target) else {
            return result;
        };
        let (base, enabled) = (reference.base, reference.enabled);
        result.set("enabled", enabled);
        if let Some(base) = base {
            result.set("base_file", i64::from(base.file));
            result.set("base_object", i64::from(base.object));
            result.set("count", world.item_count(player, base));
            let stolen = world.reference(player).map_or(0, |reference| {
                reference
                    .inventory
                    .stacks(base)
                    .iter()
                    .filter(|stack| stack.extra(33).is_some())
                    .map(|stack| stack.count())
                    .sum::<i32>()
            });
            result.set("owned_count", stolen);
        }
        drop(world);
        let target = game.object_of_form(target);
        result.set(
            "activations",
            game.vm
                .get_var(target, "SoeActivationObserver", "activations")
                .map_or(0, |value| value.as_int()),
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hst_26_untakeable_book_prompts_read_without_stealing() {
        let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
        let mut data = [0; 16];
        data[0] = 2;
        plugin.push_top(
            soe_esm::Record::new(soe_esm::Tag(*b"BOOK"), 42)
                .zsub(b"FULL", "Read me")
                .sub(b"DATA", data),
        );
        plugin.push_top(
            soe_esm::Record::new(soe_esm::tag::REFR, 43)
                .sub(b"NAME", 42_u32.to_le_bytes())
                .sub(b"DATA", [0; 24])
                .sub(b"XOWN", 50_u32.to_le_bytes()),
        );
        let mut order = soe_esm::LoadOrder::new();
        order.add("Skyrim.esm", plugin).unwrap();
        let game = soe_game::Game::new(order);
        game.world_mut().set_crosshair_reference(Some(GlobalFormId {
            file: 0,
            object: 43,
        }));
        assert_eq!(prompt(&game), Some(("Read me".into(), "Read", false)));
    }

    #[test]
    fn hst_26_book_text_retains_words_line_breaks_and_entities() {
        assert_eq!(
            book_text("<font face='x'>A&amp;B<br>Page<pagebreak>Next</font>"),
            "A&B\nPage\nNext"
        );
        assert_eq!(book_text("A <unfinished"), "A <unfinished");
    }

    #[test]
    fn hst_26_prompt_names_items_stealing_books_and_activators_and_hides_disabled() {
        for (kind, expected) in [
            (b"MISC", "Take"),
            (b"WEAP", "Take"),
            (b"ARMO", "Take"),
            (b"ALCH", "Take"),
            (b"INGR", "Take"),
            (b"KEYM", "Take"),
            (b"BOOK", "Take"),
            (b"ACTI", "Activate"),
            (b"FURN", "Use"),
        ] {
            let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
            plugin.push_top(
                soe_esm::Record::new(soe_esm::Tag(*kind), 42).zsub(b"FULL", "Object name"),
            );
            plugin.push_top(
                soe_esm::Record::new(soe_esm::tag::REFR, 43)
                    .sub(b"NAME", 42_u32.to_le_bytes())
                    .sub(b"DATA", [0; 24]),
            );
            let mut order = soe_esm::LoadOrder::new();
            order.add("Skyrim.esm", plugin).unwrap();
            let game = soe_game::Game::new(order);
            let target = GlobalFormId {
                file: 0,
                object: 43,
            };
            game.world_mut().set_crosshair_reference(Some(target));
            assert_eq!(prompt(&game), Some(("Object name".into(), expected, false)));
            if kind != b"ACTI" && kind != b"FURN" {
                game.world_mut().reference(target).unwrap().owner = Some(GlobalFormId {
                    file: 0,
                    object: 50,
                });
                assert_eq!(prompt(&game), Some(("Object name".into(), "Steal", true)));
            }
            game.world_mut()
                .reference(target)
                .unwrap()
                .activation_blocked = true;
            assert!(
                prompt(&game).is_some(),
                "blocked references retain their prompt"
            );
            game.world_mut().reference(target).unwrap().enabled = false;
            assert_eq!(prompt(&game), None);
        }
    }
}
