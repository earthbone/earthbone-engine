// SPDX-License-Identifier: GPL-3.0-or-later

#[test]
fn prf_27_process_levels_keep_distinct_report_slots() {
    let slots: Vec<_> = soe_game::ai::metrics::ProcessLevel::ALL
        .into_iter()
        .map(|level| level as usize)
        .collect();
    assert_eq!(
        slots,
        [0, 1, 2, 3],
        "process levels need separate report slots"
    );
    let names: Vec<_> = soe_game::ai::metrics::ProcessLevel::ALL
        .into_iter()
        .map(soe_game::ai::metrics::ProcessLevel::name)
        .collect();
    assert_eq!(names, ["high", "middle_high", "middle_low", "low"]);
}
