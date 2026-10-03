// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::LoadOrder;
use soe_game::Game;

const CONTRACTS: &str = include_str!("fixtures/scr_02_ck_contracts.txt");

#[test]
fn every_registered_actor_and_effect_method_has_a_nonempty_ck_contract() {
    let game = Game::new(LoadOrder::new());
    for (class, method) in game.vm.natives.method_bindings() {
        if class != "actor" && class != "activemagiceffect" {
            continue;
        }
        let found = CONTRACTS.lines().any(|line| {
            let mut fields = line.splitn(3, '|');
            fields
                .next()
                .is_some_and(|value| value.eq_ignore_ascii_case(&class))
                && fields.next().is_some_and(|source| !source.is_empty())
                && fields.next().is_some_and(|declaration| {
                    declaration.contains('(')
                        && declaration.split_once('(').is_some_and(|(head, _)| {
                            head.split_whitespace()
                                .last()
                                .is_some_and(|name| name.eq_ignore_ascii_case(&method))
                        })
                })
        });
        assert!(found, "{class}.{method} lacks a source-backed declaration");
    }
}
