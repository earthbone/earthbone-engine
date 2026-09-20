// SPDX-License-Identifier: GPL-3.0-or-later

//! Engine defaults for combat GMSTs omitted from plugin files.
//! SkyEdit's author documented missing GMSTs and editor-ID overrides. Published
//! SkyTweak/Vigor measurements supply these values, linked in docs/combat.md.

pub(crate) fn combat_default(name: &str) -> Option<f32> {
    Some(match name.to_ascii_lowercase().as_str() {
        "fdamageweaponmult" | "fdamagepcskillmin" | "fdamageskillmin" | "farmorratingbase"
        | "fdiffmulthpbypcn" | "fdiffmulthptopcn" => 1.0,
        "fdamagepcskillmax" => 1.5,
        "fdamageskillmax" => 3.0,
        "farmorratingpcmax" => 1.4,
        "farmorratingmax" => 2.5,
        "farmorscalingfactor" => 0.12,
        "farmorbasefactor" => 0.03,
        "fmaxarmorrating" => 80.0,
        _ => return None,
    })
}
