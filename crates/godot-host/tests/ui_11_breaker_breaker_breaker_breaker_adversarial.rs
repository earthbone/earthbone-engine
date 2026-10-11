#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OPTION_FLAG_DISABLED};
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, IAdd};
use soe_papyrus::Value;

const CONFIG_QUEST: u32 = 0x800;
const HIDDEN: u32 = 0x02;

fn fixture() -> Game {
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("ConfigFixture")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        quest::build_record(
            CONFIG_QUEST,
            "ConfigFixtureQuest",
            quest_flags::START_GAME_ENABLED,
        )
        .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("ConfigFixture.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("ConfigFixture", "Quest")
        .variable("selection_count", "Int", i(0))
        .variable("selected_option", "Int", i(-1))
        .function(
            FunctionBuilder::new("OnOptionSelect", "None")
                .param("option", "Int")
                .ins(
                    IAdd,
                    vec![ident("selection_count"), ident("selection_count"), i(1)],
                )
                .ins(Assign, vec![ident("selected_option"), ident("option")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    game.start();
    assert_eq!(form.object, CONFIG_QUEST);
    game
}

fn variable(game: &mut Game, name: &str) -> Value {
    let object = game.object_of_form(GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    });
    game.vm.get_var(object, "ConfigFixture", name).unwrap()
}

#[test]
fn ui_11_adversarial_visible_page_ids_follow_skyui_one_based_page_numbers() {
    let mut page = ConfigPage::new(1);
    assert_eq!(page.add_text("Status", "Ready", 0), Some(0x100));
    assert_eq!(page.add_toggle("Enabled", true, 0), Some(0x101));
    let mut later_page = ConfigPage::new(2);
    assert_eq!(later_page.add_text("Status", "Ready", 0), Some(0x200));
}

#[test]
fn ui_11_adversarial_disabled_and_hidden_rows_never_dispatch() {
    let mut game = fixture();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(1);
    let disabled = page
        .add_toggle("Disabled", true, OPTION_FLAG_DISABLED)
        .unwrap();
    let hidden = page.add_text("Hidden", "secret", HIDDEN).unwrap();
    assert!(!page.select(&mut game, form, disabled));
    assert!(!page.select(&mut game, form, hidden));
    assert_eq!(variable(&mut game, "selection_count"), Value::Int(0));
}

#[test]
fn ui_11_adversarial_enable_toggle_preserves_hidden_and_other_flags() {
    let mut page = ConfigPage::new(1);
    let id = page.add_toggle("Enabled", false, HIDDEN | 0x04).unwrap();
    assert!(page.set_enabled(id, true));
    assert_eq!(page.rows()[0].flags, HIDDEN | 0x04);
    assert!(!page.rows()[0].visible());
    assert!(page.set_enabled(id, false));
    assert_eq!(page.rows()[0].flags, HIDDEN | 0x04 | OPTION_FLAG_DISABLED);
    assert!(!page.set_enabled(0x1ff, true));
}

#[test]
fn ui_11_adversarial_callback_uses_page_strided_id_once_per_selection() {
    let mut game = fixture();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(3);
    let id = page.add_text("Status", "Ready", 0).unwrap();
    assert_eq!(id, 0x300);
    assert!(page.select(&mut game, form, id));
    assert!(page.select(&mut game, form, id));
    assert_eq!(variable(&mut game, "selection_count"), Value::Int(2));
    assert_eq!(variable(&mut game, "selected_option"), Value::Int(0x300));
}

#[test]
fn ui_11_adversarial_missing_form_does_not_dispatch_or_mutate_rows() {
    let mut game = fixture();
    let missing = GlobalFormId {
        file: 0,
        object: 0x999,
    };
    let mut page = ConfigPage::new(1);
    let id = page.add_toggle("Enabled", true, 0).unwrap();
    let before = page.rows().to_vec();
    assert!(!page.select(&mut game, missing, id));
    assert_eq!(page.rows(), before);
    assert_eq!(variable(&mut game, "selection_count"), Value::Int(0));
}
