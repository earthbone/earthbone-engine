// SPDX-License-Identifier: GPL-3.0-or-later

#[test]
fn debug_and_effect_shader_audit_has_a_contract_and_disposition_for_each_binding() {
    let audit = include_str!("../../../docs/reference-audit.md");
    for binding in [
        "Debug.Trace`",
        "Debug.TraceUser`",
        "Debug.TraceConditional`",
        "Debug.TraceStack`",
        "Debug.Notification`",
        "Debug.MessageBox`",
        "Debug.SendAnimationEvent`",
        "Debug.OpenUserLog`",
        "Debug.QuitGame`",
        "Debug.CenterOnCell`",
        "Debug.CenterOnCellAndWait`",
        "Debug.CloseUserLog`",
        "Debug.DumpAliasData`",
        "Debug.DebugChannelNotify`",
        "Debug.SetGodMode`",
        "Debug.ToggleAI`",
        "Debug.ToggleCollisions`",
        "Debug.StartScriptProfiling`",
        "Debug.StopScriptProfiling`",
        "Debug.StartStackProfiling`",
        "Debug.StopStackProfiling`",
        "Debug.TakeScreenshot`",
        "Debug.SetFootIK`",
        "Debug.ShowRefPosition`",
        "Debug.PlayerMoveToAndWait`",
        "Debug.MoveToCell`",
        "EffectShader.Play`",
        "EffectShader.Stop`",
    ] {
        assert!(
            audit.contains(binding),
            "missing source contract row for {binding}"
        );
    }
    assert!(audit.contains("SCR-24 owns Papyrus behavior"));
    assert!(audit.contains("MAG-14 owns host shader lifetime and rendering"));
    let backlog = include_str!("../../../BACKLOG.csv");
    assert!(backlog.contains("\"SCR-24\""));
    assert!(backlog.contains("\"MAG-14\""));
}
