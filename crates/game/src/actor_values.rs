// SPDX-License-Identifier: GPL-3.0-or-later

//! Actor-value components follow CommonLib ACTOR_VALUE_MODIFIER and the CK
//! Set/Mod/Force/Damage/RestoreActorValue contracts. Current and maximum are
//! computed from one store, never independently mutable copies.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_papyrus::{
    vm::state::{Reader, Writer},
    VmError,
};

#[cfg(test)]
#[path = "actor_values_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("missing actor {0}")]
    Actor(GlobalFormId),
    #[error("non-finite or overflowing actor value")]
    NonFinite,
    #[error("invalid actor-value component state")]
    State,
}

/// Persistent components of one named actor value.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Components {
    pub(crate) base: f32,
    pub(crate) permanent: f32,
    pub(crate) temporary: f32,
    pub(crate) damage: f32,
}

impl Components {
    pub fn base(self) -> f32 {
        self.base
    }
    pub fn permanent(self) -> f32 {
        self.permanent
    }
    pub fn temporary(self) -> f32 {
        self.temporary
    }
    pub fn damage(self) -> f32 {
        self.damage
    }
    pub fn maximum(self) -> f32 {
        (f64::from(self.base) + f64::from(self.permanent) + f64::from(self.temporary)) as f32
    }
    pub fn current(self) -> f32 {
        (f64::from(self.base)
            + f64::from(self.permanent)
            + f64::from(self.temporary)
            + f64::from(self.damage)) as f32
    }

    pub(crate) fn validate(self) -> Result<(), Error> {
        if [
            self.base,
            self.permanent,
            self.temporary,
            self.damage,
            self.maximum(),
            self.current(),
        ]
        .iter()
        .any(|value| !value.is_finite())
        {
            return Err(Error::NonFinite);
        }
        if self.damage > 0.0 {
            return Err(Error::State);
        }
        Ok(())
    }
}

/// Case-insensitive values, with sorted serialization and no cached totals.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Values {
    pub(crate) entries: BTreeMap<String, Components>,
}

impl Values {
    pub(crate) fn write(&self, writer: &mut Writer) {
        writer.len(self.entries.len());
        for (name, value) in &self.entries {
            writer.str(name);
            writer.f32(value.base);
            writer.f32(value.permanent);
            writer.f32(value.temporary);
            writer.f32(value.damage);
        }
    }

    pub(crate) fn read(reader: &mut Reader<'_>, version: u16) -> Result<Self, VmError> {
        let invalid = || VmError::State("invalid actor-value state".into());
        let mut values = Self::default();
        if version < 7 {
            let read_map = |reader: &mut Reader<'_>| -> Result<BTreeMap<String, f32>, VmError> {
                let mut map = BTreeMap::new();
                for _ in 0..reader.len()? {
                    let name = reader.str()?.to_ascii_lowercase();
                    let value = reader.f32()?;
                    if !value.is_finite() || map.insert(name, value).is_some() {
                        return Err(invalid());
                    }
                }
                Ok(map)
            };
            let current = read_map(reader)?;
            let base = read_map(reader)?;
            for name in current.keys().chain(base.keys()) {
                let current = current.get(name).copied().unwrap_or(0.0);
                let base = base.get(name).copied().unwrap_or(0.0);
                if current != base {
                    return Err(VmError::State(
                        "legacy actor-value modifiers are ambiguous and cannot be restored faithfully"
                            .into(),
                    ));
                }
                values.set_base(name, base).map_err(|_| invalid())?;
            }
        } else {
            for _ in 0..reader.len()? {
                let name = reader.str()?.to_ascii_lowercase();
                let value = Components {
                    base: reader.f32()?,
                    permanent: reader.f32()?,
                    temporary: reader.f32()?,
                    damage: reader.f32()?,
                };
                value.validate().map_err(|_| invalid())?;
                if values.entries.insert(name, value).is_some() {
                    return Err(invalid());
                }
            }
        }
        Ok(values)
    }

    pub fn get(&self, name: &str) -> Components {
        self.entries
            .get(&name.to_ascii_lowercase())
            .copied()
            .unwrap_or_else(|| Components {
                base: if name.eq_ignore_ascii_case("attackdamagemult") {
                    1.0
                } else {
                    0.0
                },
                ..Components::default()
            })
    }

    fn change(
        &mut self,
        name: &str,
        operation: impl FnOnce(&mut Components) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut value = self.get(name);
        operation(&mut value)?;
        value.validate()?;
        self.entries.insert(name.to_ascii_lowercase(), value);
        Ok(())
    }

    /// Change the base while retaining every modifier.
    ///
    /// # Errors
    /// Rejects non-finite input or overflowing totals without changing state.
    pub fn set_base(&mut self, name: &str, amount: f32) -> Result<(), Error> {
        self.change(name, |value| {
            value.base = amount;
            Ok(())
        })
    }

    /// Modify the permanent component, changing both current and maximum.
    ///
    /// # Errors
    /// Rejects non-finite input or overflowing totals without changing state.
    pub fn modify(&mut self, name: &str, amount: f32) -> Result<(), Error> {
        self.change(name, |value| {
            value.permanent = finite(f64::from(value.permanent) + f64::from(amount))?;
            Ok(())
        })
    }

    /// Add or remove an effect's contribution to the temporary component.
    ///
    /// # Errors
    /// Rejects non-finite input or overflowing totals without changing state.
    pub fn modify_temporary(&mut self, name: &str, amount: f32) -> Result<(), Error> {
        self.change(name, |value| {
            value.temporary = finite(f64::from(value.temporary) + f64::from(amount))?;
            Ok(())
        })
    }

    /// Force the current value through the permanent modifier, preserving damage.
    ///
    /// # Errors
    /// Rejects non-finite input or overflowing totals without changing state.
    pub fn force(&mut self, name: &str, amount: f32) -> Result<(), Error> {
        self.change(name, |value| {
            value.permanent = finite(
                f64::from(amount)
                    - f64::from(value.base)
                    - f64::from(value.temporary)
                    - f64::from(value.damage),
            )?;
            Ok(())
        })
    }

    /// Apply damage. CK specifies the same result for either sign of amount.
    ///
    /// # Errors
    /// Rejects non-finite input or overflowing totals without changing state.
    pub fn damage(&mut self, name: &str, amount: f32) -> Result<(), Error> {
        self.change(name, |value| {
            value.damage = finite(f64::from(value.damage) - f64::from(amount).abs())?;
            Ok(())
        })
    }

    /// Heal existing damage without increasing maximum or restoring beyond it.
    ///
    /// # Errors
    /// Rejects non-finite input without changing state.
    pub fn restore(&mut self, name: &str, amount: f32) -> Result<(), Error> {
        if !amount.is_finite() {
            return Err(Error::NonFinite);
        }
        self.change(name, |value| {
            value.damage = (f64::from(value.damage) + f64::from(amount).abs()).min(0.0) as f32;
            Ok(())
        })
    }

    /// CK specifies 1 for zero maximum and CarryWeight, even after damage.
    pub fn percentage(&self, name: &str) -> f32 {
        let value = self.get(name);
        let maximum = value.maximum();
        if name.eq_ignore_ascii_case("carryweight") || maximum == 0.0 {
            1.0
        } else {
            (f64::from(value.current()) / f64::from(maximum)).clamp(0.0, 1.0) as f32
        }
    }
}

fn finite(value: f64) -> Result<f32, Error> {
    let result = value as f32;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(Error::NonFinite)
    }
}
