// SPDX-License-Identifier: GPL-3.0-or-later

//! Special Edition selection math and conversion, documented in game.md.
//! CK's LeveledItem page identifies the settings. The installed runtime
//! establishes the unsigned random range, rounding and nesting rules.

use soe_esm::Error;
use soe_papyrus::Value;

use super::World;

pub(super) struct SpecialLoot {
    minimum: [f32; 4],
    maximum: [f32; 4],
    weighting: f32,
}

impl SpecialLoot {
    pub(super) fn read(world: &World) -> Result<Self, Error> {
        let setting = |name: &str| match world.game_setting(name) {
            Some(Value::Float(value)) if value.is_finite() => Ok(value),
            _ => Err(Error::Invalid(format!(
                "invalid Special Loot setting {name}"
            ))),
        };
        Ok(Self {
            minimum: [
                setting("fSpecialLootMinPCLevelMult")?,
                setting("fSpecialLootMinPCLevelBase")?,
                setting("fSpecialLootMinZoneLevelBase")?,
                setting("fSpecialLootMinZoneLevelMult")?,
            ],
            maximum: [
                setting("fSpecialLootMaxPCLevelMult")?,
                setting("fSpecialLootMaxPCLevelBase")?,
                setting("fSpecialLootMaxZoneLevelBase")?,
                setting("fSpecialLootMaxZoneLevelMult")?,
            ],
            weighting: setting("fSpecialLootWeighting")?,
        })
    }

    pub(super) fn level(&self, player: u32, input: u32, draw: u32) -> i32 {
        let bound = |parts: [f32; 4]| {
            player as f32 * parts[0] + parts[1] + parts[2] + input as f32 * parts[3]
        };
        let minimum = bound(self.minimum);
        let maximum = bound(self.maximum);
        // The runtime converts the unsigned draw to float before scaling.
        let unit = draw as f32 * (1.0 / 4_294_967_296.0);
        let used = minimum + (maximum - minimum) * unit.powf(self.weighting);
        // CVTTSS2SI truncates toward zero, with INT_MIN for invalid conversion.
        if (-2_147_483_648.0..2_147_483_648.0).contains(&used) {
            used as i32
        } else {
            i32::MIN
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SpecialLoot;

    #[test]
    fn special_loot_unsigned_draw_and_fractional_weight_have_known_quantiles() {
        let mut special = SpecialLoot {
            minimum: [0.0, 10.0, 0.0, 0.0],
            maximum: [0.0, 30.0, 0.0, 0.0],
            weighting: 0.5,
        };
        assert_eq!(special.level(1, 1, 0), 10);
        assert_eq!(special.level(1, 1, 1 << 30), 20);
        assert_eq!(special.level(1, 1, 1 << 31), 24);
        assert_eq!(special.level(1, 1, u32::MAX), 30);
        special.weighting = 2.0;
        assert_eq!(special.level(1, 1, 1 << 31), 15);
        special.weighting = -1.0;
        assert_eq!(special.level(1, 1, 0), i32::MIN);
    }
}
