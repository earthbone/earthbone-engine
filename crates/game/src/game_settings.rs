// SPDX-License-Identifier: GPL-3.0-or-later

//! Engine defaults for GMSTs omitted from plugin files.
//! SkyEdit's author documented missing GMSTs and editor-ID overrides. Published
//! SkyTweak/Vigor measurements supply combat values, linked in docs/combat.md.
//! Special Loot defaults come from the installed runtime, detailed in game.md.
//! Block defaults come from UESP's Block page and its talk archive, also
//! linked in docs/combat.md.
//! No local or UESP source states a vanilla stagger threshold or duration,
//! so fStaggerThreshold and fStaggerDuration are engineering estimates,
//! documented as such in docs/combat.md CMB-14.

pub(crate) fn engine_default(name: &str) -> Option<f32> {
    Some(match name.to_ascii_lowercase().as_str() {
        "fdamageweaponmult"
        | "fdamagepcskillmin"
        | "fdamageskillmin"
        | "farmorratingbase"
        | "fdiffmulthpbypcn"
        | "fdiffmulthptopcn"
        | "fspeciallootmaxpclevelmult" => 1.0,
        "fdiffmulthpbypcve" | "fdiffmulthptopcvh" | "fblockskillmult" => 2.0,
        "fdiffmulthptopcve"
        | "fdiffmulthpbypcvh"
        | "fspeciallootminpclevelmult"
        | "fspeciallootminzonelevelmult" => 0.5,
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
        "fshieldbasefactor" => 0.45,
        "fblockmax" => 0.7,
        "fspeciallootminpclevelbase"
        | "fspeciallootminzonelevelbase"
        | "fspeciallootmaxpclevelbase"
        | "fspeciallootmaxzonelevelbase" => 0.0,
        "fspeciallootweighting" => 4.0,
        // fbleedoutdefault comes from installed runtime Setting data, with the
        // address evidence in combat.md.
        "fshieldscalingfactor" | "fbleedoutdefault" => 0.2,
        // Estimated: post-armor damage times the weapon's DNAM Stagger
        // fraction must clear this to stagger, so a dagger (low fraction)
        // needs a heavy hit while a warhammer (high fraction) staggers on
        // an ordinary one. No corpus source pins a vanilla value.
        "fstaggerthreshold" => 3.5,
        // Estimated midpoint of the animation lengths the task pack
        // observed (0.5 to 2.0 seconds). No corpus source pins a vanilla
        // value.
        "fstaggerduration" => 1.2,
        _ => return None,
    })
}
