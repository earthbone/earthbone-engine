// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_game::play::{self, Route};

#[test]
fn ply_03_short_multibyte_vm_message_does_not_panic_or_match_error_prefix() {
    let mut game = super::ply_02_routes::game(false);
    game.vm.log.push(game.vm.time(), "é");

    let report = play::run(&mut game, &Route::parse("expect-no-vm-error").unwrap());

    assert!(report.passed(), "{}", report.format());
}
