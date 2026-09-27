// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{Record, Tag};
use soe_game::race::AnimationModels;

fn tag(value: [u8; 4]) -> Tag {
    Tag::new(&value)
}

#[test]
fn anm_55_race_markers_select_separate_skeletal_and_behavior_models() {
    let record = Record::new(tag(*b"RACE"), 0x100)
        .sub(tag(*b"MNAM"), vec![])
        .zsub(tag(*b"ANAM"), r"Actors\Human\skeleton.hkx")
        .zsub(tag(*b"MODL"), r"Actors\Human\Behaviors\male.hkx")
        .sub(tag(*b"FNAM"), vec![])
        .zsub(tag(*b"ANAM"), r"Actors\Beast\skeleton.hkx")
        .zsub(tag(*b"MODL"), r"Actors\Beast\Behaviors\female.hkx");
    let models = AnimationModels::from_record(&record).unwrap();

    assert_eq!(
        models.select(false).skeleton.as_deref(),
        Some(r"Actors\Human\skeleton.hkx")
    );
    assert_eq!(
        models.select(false).behavior.as_deref(),
        Some(r"Actors\Human\Behaviors\male.hkx")
    );
    assert_eq!(
        models.select(true).skeleton.as_deref(),
        Some(r"Actors\Beast\skeleton.hkx")
    );
    assert_eq!(
        models.select(true).behavior.as_deref(),
        Some(r"Actors\Beast\Behaviors\female.hkx")
    );
}

#[test]
fn anm_55_missing_sex_models_stay_missing_without_borrowing_other_sex_paths() {
    let record = Record::new(tag(*b"RACE"), 0x100)
        .sub(tag(*b"MNAM"), vec![])
        .zsub(tag(*b"ANAM"), r"Actors\Creature\skeleton.hkx")
        .zsub(tag(*b"MODL"), r"Actors\Creature\Behaviors\graph.hkx");
    let models = AnimationModels::from_record(&record).unwrap();

    assert_eq!(models.select(true).skeleton, None);
    assert_eq!(models.select(true).behavior, None);
    assert_eq!(
        models.select(false).skeleton.as_deref(),
        Some(r"Actors\Creature\skeleton.hkx")
    );
    assert_eq!(
        models.select(false).behavior.as_deref(),
        Some(r"Actors\Creature\Behaviors\graph.hkx")
    );
}
