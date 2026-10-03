// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for the SCR-02 contract audit.

const BACKLOG: &str = include_str!("../../../BACKLOG.csv");
const GAME_DOC: &str = include_str!("../../../docs/game.md");

#[test]
fn equip_item_contract_discrepancy_has_an_owning_backlog_row() {
    let has_owner = BACKLOG.lines().any(|row| {
        row.contains("EquipItem") && row.contains("abPreventRemoval") && row.contains("abSilent")
    });

    assert!(
        has_owner,
        "CK EquipItem prevent-removal and silent-message behavior has no owning backlog row"
    );
    assert!(
        !GAME_DOC.contains("No current backlog row owns the lock and message behavior."),
        "the SCR-02 audit still records the concrete discrepancy as unowned"
    );
}
