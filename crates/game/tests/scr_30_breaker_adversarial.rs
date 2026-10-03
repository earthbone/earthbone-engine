// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::{npc, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call_show_training_menu(game: &mut Game, trainer: GlobalFormId) {
    let script = "Scr30InvalidTrainer";
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .param("trainer", "Actor")
        .ins(
            CallStatic,
            vec![s("Game"), s("ShowTrainingMenu"), ident("unused"), ident("trainer")],
        )
        .ins(Return, vec![])
        .build();
    game.load_script(&ScriptBuilder::new(script, "").function(function).bytes())
        .unwrap();
    let actor = Value::Object(game.object_of_form(trainer));
    let thread = game.vm.call_global(script, "Run", &[actor]).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_30_actor_without_teaching_class_does_not_leave_a_menu_request() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x800,
        "Untrained",
        "Untrained Actor",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    let mut order = LoadOrder::new();
    order.add("InvalidTrainer.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let trainer = game.world_mut().place_at(
        GlobalFormId { file: 0, object: 0x800 },
        player,
        false,
    );

    call_show_training_menu(&mut game, trainer);

    assert!(game.world().menu_requests.is_empty());
    assert_eq!(game.world_mut().take_training_menu_request(), None);
    assert!(game.traces().iter().all(|trace| !trace.contains("error")));
}
