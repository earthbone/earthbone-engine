// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Check emission gating and slot-two selection from nifly Shaders.hpp and local UESP texture slots.

use soe_godot_host::mesh::MaterialDesc;
use soe_nif::blocks::{flags1, flags2};

#[test]
fn rnd_24_emission_flags_do_not_activate_an_unrelated_slot_two_texture() {
    let desc = MaterialDesc {
        glow: Some("textures/actors/hair/specular.dds".into()),
        emissive_color: [0.8, 0.4, 0.2],
        emissive_multiple: 2.0,
        shader_flags_1: flags1::OWN_EMIT,
        ..Default::default()
    };
    assert!(desc.emits_light());
    assert_eq!(desc.glow_texture(), None);
}

#[test]
fn rnd_24_glow_map_uses_only_flagged_slot_two_and_needs_authored_emission() {
    let no_emission = MaterialDesc {
        glow: Some("textures/candle_g.dds".into()),
        shader_flags_2: flags2::GLOW_MAP,
        ..Default::default()
    };
    assert!(!no_emission.emits_light());
    assert_eq!(no_emission.glow_texture(), Some("textures/candle_g.dds"));

    let unrelated_slot = MaterialDesc {
        glow: Some("textures/candle_g.dds".into()),
        emissive_color: [1.0, 1.0, 1.0],
        emissive_multiple: 1.0,
        ..Default::default()
    };
    assert!(!unrelated_slot.emits_light());
    assert_eq!(unrelated_slot.glow_texture(), None);
}
