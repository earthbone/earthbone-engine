// SPDX-License-Identifier: GPL-3.0-or-later

//! Device translation and event state, independent of Godot.
//!
//! Codes come from CommonLibSSE-NG's BSWin32MouseDevice and
//! BSWin32GamepadDevice, corroborated by SKSE InputMap.cpp. Host indices come
//! from godot-docs/classes/class_@globalscope.rst. These small tables translate
//! between those existing interfaces. No device layout is inferred from bits.

use std::collections::{BTreeMap, BTreeSet};

use super::{godot_key_for, Action, Binding, Bindings};

/// Radial stick dead zone. The remaining range is scaled back to zero to one.
pub const DEAD_ZONE: f32 = 0.2;
pub const TRIGGER_THRESHOLD: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadControl {
    Button(i32),
    Trigger(i32),
}

#[must_use]
pub fn godot_mouse_for(code: u32) -> Option<i32> {
    Some(match code {
        0 => 1,
        1 => 2,
        2 => 3,
        3 => 8,
        4 => 9,
        8 => 4,
        9 => 5,
        // Godot exposes two side buttons, not Bethesda's five.
        _ => return None,
    })
}

#[must_use]
pub fn godot_pad_for(code: u32) -> Option<PadControl> {
    Some(match code {
        0x0001 => PadControl::Button(11),
        0x0002 => PadControl::Button(12),
        0x0004 => PadControl::Button(13),
        0x0008 => PadControl::Button(14),
        0x0010 => PadControl::Button(6),
        0x0020 => PadControl::Button(4),
        0x0040 => PadControl::Button(7),
        0x0080 => PadControl::Button(8),
        0x0100 => PadControl::Button(9),
        0x0200 => PadControl::Button(10),
        0x1000 => PadControl::Button(0),
        0x2000 => PadControl::Button(1),
        0x4000 => PadControl::Button(2),
        0x8000 => PadControl::Button(3),
        0x0009 => PadControl::Trigger(4),
        0x000a => PadControl::Trigger(5),
        _ => return None,
    })
}

#[derive(Debug, Default)]
struct Pad {
    buttons: BTreeSet<i32>,
    axes: [f32; 6],
}

impl Pad {
    fn down(&self, code: u32) -> bool {
        match godot_pad_for(code) {
            Some(PadControl::Button(index)) => self.buttons.contains(&index),
            Some(PadControl::Trigger(index)) => self.axes[index as usize] > TRIGGER_THRESHOLD,
            None => false,
        }
    }

    fn stick(&self, binding: &Binding) -> (f32, f32) {
        let mut result = (0.0, 0.0);
        for chord in &binding.0 {
            let Some((last, modifiers)) = chord.0.split_last() else {
                continue;
            };
            let first = match last {
                0x000b => 0,
                0x000c => 2,
                _ => continue,
            };
            if modifiers.iter().all(|code| self.down(*code)) {
                result = strongest(result, radial(self.axes[first], self.axes[first + 1]));
            }
        }
        result
    }
}

fn strongest(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    if b.0.hypot(b.1) > a.0.hypot(a.1) {
        b
    } else {
        a
    }
}

fn radial(x: f32, y: f32) -> (f32, f32) {
    let length = x.hypot(y);
    if length <= DEAD_ZONE {
        return (0.0, 0.0);
    }
    let scale = (length.min(1.0) - DEAD_ZONE) / ((1.0 - DEAD_ZONE) * length);
    (x * scale, y * scale)
}

/// Raw device state and pending action edges between simulation ticks.
/// Each pad owns its buttons, so two pads cannot complete one chord.
#[derive(Debug, Default)]
pub struct Devices {
    keys: BTreeSet<i64>,
    mouse: BTreeSet<i32>,
    pads: BTreeMap<i32, Pad>,
    held: BTreeSet<Action>,
    pending: BTreeSet<Action>,
}

impl Devices {
    pub fn key(&mut self, code: i64, down: bool, bindings: &Bindings) {
        if down {
            self.keys.insert(code);
        } else {
            self.keys.remove(&code);
        }
        self.observe(bindings);
    }

    pub fn mouse_button(&mut self, index: i32, down: bool, bindings: &Bindings) {
        set_button(&mut self.mouse, index, down);
        self.observe(bindings);
        // A wheel press is a pulse even when the platform sends no release.
        if (4..=5).contains(&index) {
            self.mouse.remove(&index);
            self.held = self.actions(bindings);
        }
    }

    pub fn pad_button(&mut self, device: i32, index: i32, down: bool, bindings: &Bindings) {
        set_button(
            &mut self.pads.entry(device).or_default().buttons,
            index,
            down,
        );
        self.observe(bindings);
    }

    pub fn pad_axis(&mut self, device: i32, axis: i32, value: f32, bindings: &Bindings) {
        if let Some(slot) = self
            .pads
            .entry(device)
            .or_default()
            .axes
            .get_mut(axis as usize)
        {
            *slot = if value.is_finite() {
                value.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.observe(bindings);
        }
    }

    pub fn disconnect(&mut self, device: i32, bindings: &Bindings) {
        self.pads.remove(&device);
        // A press already received remains valid even if its pad disconnects.
        self.held = self.actions(bindings);
    }

    /// Changing a binding is not a new press. Clear edges from the old map.
    pub fn rebind(&mut self, bindings: &Bindings) {
        self.held = self.actions(bindings);
        self.pending.clear();
    }

    #[must_use]
    pub fn held(&self, action: Action) -> bool {
        self.held.contains(&action)
    }

    pub fn take_pressed(&mut self) -> BTreeSet<Action> {
        std::mem::take(&mut self.pending)
    }

    #[must_use]
    pub fn movement(&self, bindings: &Bindings) -> (f32, f32) {
        let (x, y) = self.stick(&bindings.movement);
        (x, -y)
    }

    #[must_use]
    pub fn look(&self, bindings: &Bindings) -> (f32, f32) {
        self.stick(&bindings.look)
    }

    fn stick(&self, binding: &Binding) -> (f32, f32) {
        self.pads
            .values()
            .fold((0.0, 0.0), |best, pad| strongest(best, pad.stick(binding)))
    }

    fn actions(&self, bindings: &Bindings) -> BTreeSet<Action> {
        Action::all()
            .into_iter()
            .filter(|a| {
                bindings.held(*a, &|code| {
                    godot_key_for(code).is_some_and(|key| self.keys.contains(&key))
                }) || bindings.mouse(*a).is_some_and(|b| {
                    b.held(&|code| {
                        godot_mouse_for(code).is_some_and(|index| self.mouse.contains(&index))
                    })
                }) || bindings
                    .gamepad(*a)
                    .is_some_and(|b| self.pads.values().any(|pad| b.held(&|code| pad.down(code))))
            })
            .collect()
    }

    fn observe(&mut self, bindings: &Bindings) {
        let now = self.actions(bindings);
        self.pending.extend(now.difference(&self.held).copied());
        self.held = now;
    }
}

fn set_button(buttons: &mut BTreeSet<i32>, index: i32, down: bool) {
    if down {
        buttons.insert(index);
    } else {
        buttons.remove(&index);
    }
}

#[cfg(test)]
mod tests;
