// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Source-backed adversarial coverage for SCR-02 Actor argument binding.

use soe_papyrus::Value;

#[test]
fn scr_02_set_allow_flying_ex_uses_first_argument_and_returns_none() {
    // CK declares abAllowed first, followed by the crash and search options.
    let mut game = crate::natives_dlc::game();
    let actor = crate::natives_dlc::obj(&mut game, "VampireRef");
    let actor_id = crate::natives_dlc::form(&game, "VampireRef");

    assert_eq!(
        crate::natives_dlc::call(
            &mut game,
            &actor,
            "SetAllowFlyingEx",
            &[Value::Bool(false), Value::Bool(true), Value::Bool(true)],
        ),
        Value::None
    );
    assert!(!game.world_mut().actor(actor_id).unwrap().allow_flying);

    assert_eq!(
        crate::natives_dlc::call(
            &mut game,
            &actor,
            "SetAllowFlyingEx",
            &[Value::Bool(true), Value::Bool(false), Value::Bool(false)],
        ),
        Value::None
    );
    assert!(game.world_mut().actor(actor_id).unwrap().allow_flying);
}
