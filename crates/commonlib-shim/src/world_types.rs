// SPDX-License-Identifier: MIT

//! Narrow views of world state represented by the round-2 game runtime.

use crate::form::FormId;

#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WornState {
    pub worn: bool,
    pub worn_left: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazardState {
    pub reference: FormId,
    pub base: FormId,
    pub position: crate::ni::NiPoint3,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HighProcessState {
    pub actor: FormId,
    pub in_combat: bool,
    pub combat_target: Option<FormId>,
    pub staggered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitData {
    pub attacker: FormId,
    pub target: FormId,
    pub weapon: FormId,
    pub weapon_damage: f32,
    pub armor_reduction: f32,
    pub health_damage: f32,
    pub blocked: bool,
    pub killed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedHook {
    pub type_name: &'static str,
    pub hook_name: &'static str,
    pub reason: &'static str,
}

/// Return the explicit boundary decision for operations that need Skyrim's
/// executable, mutable inventory extras, or an unimplemented simulation.
pub fn unsupported_hook(type_name: &str, hook_name: &str) -> UnsupportedHook {
    let (type_name, hook_name, reason) = match (type_name, hook_name) {
        ("ExtraWorn", "attach") => (
            "ExtraWorn",
            "attach",
            "extra-data mutation is not exposed by the read-only shim",
        ),
        ("GameSettingCollection", "singleton") => (
            "GameSettingCollection",
            "singleton",
            "executable singleton lookup has no target",
        ),
        ("GameSettingCollection", "write") => (
            "GameSettingCollection",
            "write",
            "game-setting writes are not exposed by this binding",
        ),
        ("Hazard", "tick") => ("Hazard", "tick", "hazard simulation is not implemented"),
        ("Hazard", "finish_load") => (
            "Hazard",
            "finish_load",
            "engine load callbacks have no target",
        ),
        ("HighProcessData", "headtrack") => (
            "HighProcessData",
            "headtrack",
            "head tracking process state is not represented",
        ),
        ("HighProcessData", "animation") => (
            "HighProcessData",
            "animation",
            "animation sequencer state is not represented",
        ),
        ("HitData", "dispatch") => (
            "HitData",
            "dispatch",
            "hit event dispatch is owned by game combat execution",
        ),
        ("HitData", "relocation") => (
            "HitData",
            "relocation",
            "executable hit relocations are unsupported",
        ),
        _ => (
            "CommonLib",
            "unknown",
            "hook is outside the implemented source-level boundary",
        ),
    };
    UnsupportedHook {
        type_name,
        hook_name,
        reason,
    }
}
