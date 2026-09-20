// SPDX-License-Identifier: GPL-3.0-or-later

//! Engine defaults for GMSTs omitted from plugin files.
//! SkyEdit's author documented missing GMSTs and editor-ID overrides. Published
//! SkyTweak/Vigor measurements supply combat values, linked in docs/combat.md.
//! Special Loot defaults come from the installed runtime, detailed in game.md.

pub(crate) fn engine_default(name: &str) -> Option<f32> {
    Some(match name.to_ascii_lowercase().as_str() {
        "fdamageweaponmult"
        | "fdamagepcskillmin"
        | "fdamageskillmin"
        | "farmorratingbase"
        | "fdiffmulthpbypcn"
        | "fdiffmulthptopcn"
        | "fspeciallootmaxpclevelmult" => 1.0,
        "fdiffmulthpbypcve" | "fdiffmulthptopcvh" => 2.0,
        "fdiffmulthptopcve" | "fdiffmulthpbypcvh" => 0.5,
        "fdiffmulthpbypce"
        | "fdiffmulthptopch"
        | "fdamagepcskillmax"
        | "fspeciallootmaxzonelevelmult" => 1.5,
        "fdiffmulthptopce" | "fdiffmulthpbypch" => 0.75,
        "fdiffmulthpbypcl" => 0.25,
        "fdiffmulthptopcl" | "fdamageskillmax" => 3.0,
        "farmorratingpcmax" => 1.4,
        "farmorratingmax" => 2.5,
        "farmorscalingfactor" => 0.12,
        "farmorbasefactor" => 0.03,
        "fmaxarmorrating" => 80.0,
        "fspeciallootminpclevelbase"
        | "fspeciallootminzonelevelbase"
        | "fspeciallootmaxpclevelbase"
        | "fspeciallootmaxzonelevelbase" => 0.0,
        "fspeciallootminpclevelmult" | "fspeciallootminzonelevelmult" => 0.5,
        "fspeciallootweighting" => 4.0,
        // Installed runtime Setting data, with the address evidence in combat.md.
        "fbleedoutdefault" => 0.2,
        _ => return None,
    })
}
