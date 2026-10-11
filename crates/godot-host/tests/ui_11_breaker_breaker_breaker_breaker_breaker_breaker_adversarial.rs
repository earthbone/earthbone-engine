#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OptionKind, OPTION_FLAG_DISABLED};
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, IAdd};
use soe_papyrus::Value;

const CONFIG_QUEST: u32 = 0x800;

fn fixture() -> Game {
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
    game.load_script(
        &ScriptBuilder::new("ConfigFixture", "Quest")
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
            .bytes(),
    )
    .unwrap();
    game.start();
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
fn ui_11_empty_text_and_unchecked_toggle_keep_authored_values_and_order() {
    let mut page = ConfigPage::new(1);
    let text = page.add_text("", "", 0).unwrap();
    let toggle = page.add_toggle("設定", false, 0).unwrap();

    assert_eq!(text, 0x100);
    assert_eq!(toggle, 0x101);
    assert_eq!(
        page.rows()[0].kind,
        OptionKind::Text {
            value: String::new()
        }
    );
    assert_eq!(
        page.rows()[1].kind,
        OptionKind::Toggle { checked: false }
    );
    assert_eq!(page.rows()[0].label, "");
    assert_eq!(page.rows()[1].label, "設定");
}

#[test]
fn ui_11_enabled_updates_are_idempotent_and_do_not_alias_another_page() {
    let mut page = ConfigPage::new(2);
    let id = page.add_toggle("Toggle", true, 0x06).unwrap();

    assert!(page.set_enabled(id, false));
    assert!(page.set_enabled(id, false));
    assert_eq!(page.rows()[0].flags, 0x07);
    assert!(!page.set_enabled(0x101, true));
    assert_eq!(page.rows()[0].flags, 0x07);
    assert!(page.set_enabled(id, true));
    assert_eq!(page.rows()[0].flags, 0x06);
}

#[test]
fn ui_11_hidden_disabled_and_unknown_ids_never_call_the_script() {
    let mut game = fixture();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(1);
    let hidden = page.add_text("Hidden", "value", 0x02).unwrap();
    let disabled = page
        .add_toggle("Disabled", true, OPTION_FLAG_DISABLED)
        .unwrap();
    let available = page.add_text("Available", "", 0).unwrap();

    assert!(!page.select(&mut game, form, hidden));
    assert!(!page.select(&mut game, form, disabled));
    assert!(!page.select(&mut game, form, 0x1ff));
    assert_eq!(variable(&mut game, "selection_count"), Value::Int(0));
    assert!(page.select(&mut game, form, available));
    assert_eq!(variable(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(variable(&mut game, "selected_option"), Value::Int(0x102));
}

#[test]
fn ui_11_repeated_selection_sends_one_page_strided_callback_each_time() {
    let mut game = fixture();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(0x1234);
    let id = page.add_toggle("Enabled", false, 0).unwrap();

    assert_eq!(id, 0x1234_00);
    assert!(page.select(&mut game, form, id));
    assert_eq!(variable(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(variable(&mut game, "selected_option"), Value::Int(0x1234_00));
    assert!(page.select(&mut game, form, id));
    assert_eq!(variable(&mut game, "selection_count"), Value::Int(2));
}
