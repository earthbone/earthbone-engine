// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Source-backed adversarial coverage for the SCR-02 Actor contract audit.

use soe_papyrus::Value;

#[test]
fn scr_02_enable_ai_default_reenables_after_disable() {
    // CK EnableAI declares abEnable = true and returns None.
    let mut game = crate::natives_dlc::game();
    let actor = crate::natives_dlc::obj(&mut game, "VampireRef");
    let actor_id = crate::natives_dlc::form(&game, "VampireRef");

    assert_eq!(
        crate::natives_dlc::call(&mut game, &actor, "EnableAI", &[Value::Bool(false)]),
        Value::None
    );
    assert!(game.world().actor(actor_id).unwrap().ai_disabled);

    assert_eq!(
        crate::natives_dlc::call(&mut game, &actor, "EnableAI", &[]),
        Value::None
    );
    assert!(!game.world().actor(actor_id).unwrap().ai_disabled);

    assert_eq!(
        crate::natives_dlc::call(&mut game, &actor, "EnableAI", &[Value::Bool(false)]),
        Value::None
    );
    assert!(game.world().actor(actor_id).unwrap().ai_disabled);
    assert_eq!(
        crate::natives_dlc::call(&mut game, &actor, "EnableAI", &[Value::Bool(true)]),
        Value::None
    );
    assert!(!game.world().actor(actor_id).unwrap().ai_disabled);
}
