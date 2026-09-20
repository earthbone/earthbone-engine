// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

//! Invalid input at the saved-state boundary must not create mutable values.

use super::*;

fn encoded(entries: &[(&str, [f32; 4])]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.len(entries.len());
    for (name, components) in entries {
        writer.str(name);
        for value in components {
            writer.f32(*value);
        }
    }
    writer.out
}

#[test]
fn saved_components_reject_duplicates_nonfinite_damage_and_overflow() {
    for components in [
        [100.0, 0.0, 0.0, 1.0],
        [f32::NAN, 0.0, 0.0, 0.0],
        [100.0, f32::INFINITY, 0.0, 0.0],
        [100.0, 0.0, f32::NEG_INFINITY, 0.0],
        [100.0, 0.0, 0.0, f32::NAN],
        [f32::MAX, f32::MAX, 0.0, 0.0],
    ] {
        let bytes = encoded(&[("health", components)]);
        assert!(Values::read(&mut Reader::new(&bytes), 7).is_err());
    }
    let bytes = encoded(&[
        ("Health", [100.0, 0.0, 0.0, 0.0]),
        ("HEALTH", [80.0, 0.0, 0.0, 0.0]),
    ]);
    assert!(Values::read(&mut Reader::new(&bytes), 7).is_err());
}

#[test]
fn each_truncated_component_state_is_rejected() {
    let bytes = encoded(&[("health", [100.0, 20.0, 30.0, -40.0])]);
    for end in 0..bytes.len() {
        assert!(Values::read(&mut Reader::new(&bytes[..end]), 7).is_err());
    }
    let mut reader = Reader::new(&bytes);
    let values = Values::read(&mut reader, 7).unwrap();
    assert_eq!(values.get("HEALTH").current(), 110.0);
    reader.expect_end().unwrap();
}

#[test]
fn old_current_and_base_maps_only_migrate_when_unambiguous() {
    for current in [75.0, 100.0, 125.0] {
        let mut writer = Writer::new();
        writer.len(1);
        writer.str("HEALTH");
        writer.f32(current);
        writer.len(1);
        writer.str("health");
        writer.f32(100.0);
        let result = Values::read(&mut Reader::new(&writer.out), 4);
        if current == 100.0 {
            assert_eq!(result.unwrap().get("health").base(), 100.0);
        } else {
            assert!(result.unwrap_err().to_string().contains("ambiguous"));
        }
    }
}

#[test]
fn every_vanilla_actor_value_round_trips_between_its_index_and_name() {
    for (index, name) in ACTOR_VALUE_NAMES.iter().enumerate() {
        let index = i32::try_from(index).unwrap();
        assert_eq!(actor_value_name(index), Some(*name));
        assert_eq!(actor_value_index(name), Some(index));
        assert_eq!(actor_value_index(&name.to_ascii_uppercase()), Some(index));
    }
    assert_eq!(actor_value_name(-1), None);
    assert_eq!(actor_value_name(164), None);
    assert_eq!(actor_value_index("not-an-actor-value"), None);
}
