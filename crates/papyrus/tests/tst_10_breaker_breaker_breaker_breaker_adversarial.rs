#![allow(clippy::expect_used)]

#[path = "tst_10_champollion.rs"]
mod champollion_harness;

#[test]
fn tst_10_return_type_mismatch_is_reported_against_champollion() {
    let loaded = champollion_harness::champollion_functions(
        ".object Example\n.function Tick\n.return Int\n.paramTable\n.endParamTable\n.code\nRETURN 0\n.endCode\n.endFunction ;Tick\n.endObject\n",
    );
    let decoded = champollion_harness::champollion_functions(
        ".object Example\n.function Tick\n.return String\n.paramTable\n.endParamTable\n.code\nRETURN 0\n.endCode\n.endFunction ;Tick\n.endObject\n",
    );

    assert!(
        !champollion_harness::differences(&loaded, &decoded).is_empty(),
        "Champollion emits return types, so a return-type mismatch must be reported"
    );
}
