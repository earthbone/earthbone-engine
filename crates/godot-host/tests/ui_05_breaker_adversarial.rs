// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;

const CONFIG: u32 = 0x800;
const CHILD: &str = "Ui05ExecutableChild";

fn fixture() -> crate::compiled::Fixture {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new(CHILD)],
        ..Default::default()
    };
    plugin.push_top(
        quest::build_record(CONFIG, "Ui05HostConfigQuest", quest_flags::START_GAME_ENABLED)
            .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
    );
    let mut base = ScriptBuilder::new("SKI_ConfigBase", "Quest")
        .auto_property("ModName", "String", s(""))
        .auto_property("Pages", "String[]", soe_papyrus::asm::none())
        .function(
            FunctionBuilder::new("OnInit", "None")
                .ins(CallMethod, vec![s("OnConfigInit"), ident("self"), ident("::nonevar")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OpenConfig", "None")
                .ins(
                    CallMethod,
                    vec![s("SetPage"), ident("self"), ident("::nonevar"), s(""), i(-1)],
                )
                .ins(CallMethod, vec![s("OnConfigOpen"), ident("self"), ident("::nonevar")])
                .build(),
        )
        .function(
            FunctionBuilder::new("SetPage", "None")
                .param("page", "String")
                .param("index", "Int")
                .ins(
                    CallMethod,
                    vec![s("OnPageReset"), ident("self"), ident("::nonevar"), ident("page")],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("CloseConfig", "None")
                .ins(CallMethod, vec![s("OnConfigClose"), ident("self"), ident("::nonevar")])
                .build(),
        );
    let base_bytes = base.bytes();
    let mut child = ScriptBuilder::new(CHILD, "SKI_ConfigBase")
        .variable("init_count", "Int", i(0))
        .variable("register_count", "Int", i(0))
        .variable("reset_count", "Int", i(0))
        .variable("open_count", "Int", i(0))
        .variable("close_count", "Int", i(0))
        .variable("sequence", "Int", i(0))
        .variable("last_page", "String", s("unset"))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .local("pages", "String[]")
                .ins(IAdd, vec![ident("init_count"), ident("init_count"), i(1)])
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Example Mod")])
                .ins(ArrayCreate, vec![ident("pages"), i(2)])
                .ins(ArraySetElement, vec![ident("pages"), i(0), s("General")])
                .ins(ArraySetElement, vec![ident("pages"), i(1), s("Advanced")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("pages")])
                .build(),
        );
    for (event, count, digit) in [
        ("OnConfigRegister", "register_count", 0),
        ("OnPageReset", "reset_count", 1),
        ("OnConfigOpen", "open_count", 2),
        ("OnConfigClose", "close_count", 3),
    ] {
        let mut callback = FunctionBuilder::new(event, "None").ins(
            IAdd,
            vec![ident(count), ident(count), i(1)],
        );
        if event == "OnPageReset" {
            callback = callback
                .param("page", "String")
                .ins(Assign, vec![ident("last_page"), ident("page")]);
        }
        if digit != 0 {
            callback = callback
                .ins(IMul, vec![ident("sequence"), ident("sequence"), i(10)])
                .ins(IAdd, vec![ident("sequence"), ident("sequence"), i(digit)]);
        }
        child = child.function(callback.build());
    }
    crate::compiled::Fixture {
        plugin,
        scripts: vec![("SKI_ConfigBase".into(), base_bytes), (CHILD.into(), child.bytes())],
    }
}

#[path = "../../game/tests/support/ui_05_fixture.rs"]
mod compiled;

#[test]
fn ui_05_godot_renders_declared_pages_in_script_order() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("UI-05 host check skipped: SOE_GODOT_BIN is unset");
        return;
    };
    let fixture = fixture();
    let mut game = Game::new({
        let mut order = soe_esm::LoadOrder::new();
        order.add("Ui05Config.esm", fixture.plugin.clone()).unwrap();
        order
    });
    for (_, bytes) in &fixture.scripts {
        game.load_script(bytes).unwrap();
    }
    assert!(game.start().scripts_missing.is_empty());
    let directory = std::env::temp_dir().join(format!("soe-ui05-host-{}", std::process::id()));
    fixture.write(&directory);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(godot)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "res://ui_05_mcm_lifecycle_test.gd",
        ])
        .current_dir(&root)
        .env("SOE_UI_05_FIXTURE_DIR", &directory)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{}\n{stdout}\n{stderr}", output.status);
    assert!(stdout.contains("PASS ui_05 windowed MCM lifecycle"), "{stdout}\n{stderr}");
}
