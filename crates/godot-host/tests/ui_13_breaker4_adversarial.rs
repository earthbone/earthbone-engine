// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::ConfigPage;
use soe_papyrus::asm::{none, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{ArrayRef, Opcode, Value};

const SCRIPT: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x123,
};

#[test]
fn ui_13_color_open_resets_stale_script_parameters_to_skyui_defaults() {
    let mut order = LoadOrder::new();
    order
        .add("Ui13ColorReset.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let on_open = FunctionBuilder::new("OnOptionColorOpen", "None")
        .param("option", "Int")
        .ins(Opcode::Nop, vec![])
        .build();
    game.load_script(
        &ScriptBuilder::new("SKI_ConfigBase", "")
            .variable("_colorParams", "Int[]", none())
            .function(on_open)
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(SCRIPT);
    game.vm.attach(object, "SKI_ConfigBase").unwrap();
    let params = ArrayRef::new(soe_papyrus::value::ElementKind::Int, 2);
    params.items.borrow_mut()[0] = Value::Int(0x123456);
    params.items.borrow_mut()[1] = Value::Int(0xabcdef);
    game.vm
        .set_var(
            object,
            "SKI_ConfigBase",
            "_colorParams",
            Value::Array(params),
        )
        .unwrap();

    let mut page = ConfigPage::new(0);
    let id = page.add_color("Tint", 0x010203, 0).unwrap();
    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    assert_eq!(
        page.color_dialog().map(|dialog| (dialog.start_color, dialog.default_color)),
        Some((-1, -1)),
        "SkyUI resets both values before invoking OnOptionColorOpen"
    );
}
