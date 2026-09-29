// SPDX-License-Identifier: MIT

//! Read-only world views for CommonLib's setting, actor-state and world-singleton declarations.
//! Type names and numeric states follow the local CommonLibSSE-NG headers.

use crate::form::FormId;

#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Integer(i32),
    Float(f32),
    String(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub name: String,
    pub value: SettingValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SitSleepState {
    Normal = 0,
    WantToSit = 1,
    WaitingForSitAnimation = 2,
    Sitting = 3,
    WantToStand = 4,
    WantToSleep = 5,
    WaitingForSleepAnimation = 6,
    Sleeping = 7,
    WantToWake = 8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    pub hour: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReanimateEffect {
    pub id: u64,
    pub spell: FormId,
    pub effect: FormId,
    pub target: FormId,
    pub magnitude: f32,
    pub elapsed: f64,
    pub duration: Option<f64>,
}

/// Read-only access to live runtime values. Unsupported engine hooks return a
/// reason instead of manufacturing an address or silently doing nothing.
pub trait RuntimeState {
    fn setting(&self, name: &str) -> Option<Setting>;
    fn sky(&self) -> Sky;
    fn reanimate_effect(&self, id: u64) -> Option<ReanimateEffect>;
    fn sit_sleep_state(&self, actor: FormId) -> Result<SitSleepState, UnsupportedWorldHook>;
    fn hook(&self, name: &str) -> UnsupportedWorldHook;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedWorldHook {
    pub name: &'static str,
    pub reason: &'static str,
}
