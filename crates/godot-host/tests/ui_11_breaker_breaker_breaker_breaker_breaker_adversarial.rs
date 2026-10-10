#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_godot_host::mcm_menu::ConfigPage;
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, IAdd};
use soe_papyrus::Value;

const CONFIG_QUEST: u32 = 0x800;

fn config_game() -> Game {
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

fn script_var(game: &mut Game, name: &str) -> Value {
    let object = game.object_of_form(GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    });
    game.vm.get_var(object, "ConfigFixture", name).unwrap()
}

#[test]
fn ui_11_signed_papyrus_option_ids_reject_pages_above_int_range() {
    let mut largest = ConfigPage::new(0x7f_ffff);
    assert_eq!(largest.add_text("Last valid page", "", 0), Some(0x7fff_ff00));

    let mut out_of_range = ConfigPage::new(0x80_0000);
    assert_eq!(out_of_range.add_text("Not representable", "", 0), None);
}

#[test]
fn ui_11_full_page_keeps_its_last_slot_and_rejects_repeated_overflow() {
    let mut page = ConfigPage::new(4);
    for slot in 0..128 {
        assert_eq!(page.add_text("Entry", "", 0), Some(0x400 + slot));
    }

    assert_eq!(page.rows().len(), 128);
    assert_eq!(page.add_toggle("Overflow", true, 0), None);
    assert_eq!(page.add_text("Still full", "", 0), None);
    assert_eq!(page.rows().len(), 128);
}

#[test]
fn ui_11_out_of_range_selection_does_not_dispatch_or_change_the_option() {
    let mut game = config_game();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(1);
    let id = page.add_text("Status", "Ready", 0).unwrap();
    let before = page.rows().to_vec();

    assert!(!page.select(&mut game, form, 0x1ff));
    assert_eq!(page.rows(), before);
    assert_eq!(script_var(&mut game, "selection_count"), Value::Int(0));
    assert_eq!(script_var(&mut game, "selected_option"), Value::Int(-1));
    assert!(page.select(&mut game, form, id));
    assert_eq!(script_var(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(script_var(&mut game, "selected_option"), Value::Int(0x100));
}
