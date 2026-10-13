// SPDX-License-Identifier: GPL-3.0-or-later

//! Barter pricing from UESP Skyrim Speech Prices. PERK entry points 8 and 60
//! follow local UESP PERK and xEdit. SkyUI BarterDataSetter rounds per item.

use soe_esm::GlobalFormId;
use soe_papyrus::Value;

use crate::{perks, Game};

/// Direction of an exchange from the player's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

/// Current price factors, calculated from the shared actor and record state.
#[derive(Debug, Clone, Copy)]
pub struct Prices {
    buy: f32,
    sell: f32,
}

/// Invalid settings, actor values or price perk data prevent a quote.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid barter setting {0}")]
    Setting(&'static str),
    #[error("invalid barter actor values")]
    ActorValues,
    #[error("barter price is outside the gold quantity range")]
    Price,
    #[error("barter perk: {0}")]
    Perk(#[from] perks::DispatchError),
}

impl Prices {
    /// Round one item's adjusted value before multiplying by quantity.
    ///
    /// # Errors
    /// Rejects negative values and prices outside the inventory quantity range.
    pub fn unit_price(self, value: i32, side: Side) -> Result<i32, Error> {
        if value < 0 {
            return Err(Error::Price);
        }
        let multiplier = match side {
            Side::Buy => self.buy,
            Side::Sell => self.sell,
        };
        let price = (value as f32 * multiplier).round();
        if !price.is_finite() || f64::from(price) > f64::from(i32::MAX) {
            return Err(Error::Price);
        }
        Ok(match side {
            Side::Buy => (price as i32).max(1),
            Side::Sell => price as i32,
        })
    }
}

impl Game {
    /// Read Speech, Fortify Barter, GMST overrides and conditional price perks.
    /// The merchant is the second perk condition tab, including Allure's sex test.
    ///
    /// # Errors
    /// Invalid numeric inputs and malformed or unsupported price perks.
    pub fn barter_prices(&self, merchant: GlobalFormId) -> Result<Prices, Error> {
        let mut world = self.world_mut();
        let player = world.player();
        let speech = world.actor_value(player, "speechcraft");
        let equipment = world.actor_value(player, "speechcraftmodifier") / 100.0;
        let potion = world.actor_value(player, "speechcraftpowermodifier") / 100.0;
        if ![speech, equipment, potion]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(Error::ActorValues);
        }
        let setting = |name: &'static str| match world.game_setting(name) {
            Some(Value::Float(value)) if value.is_finite() && value > 0.0 => Ok(value),
            _ => Err(Error::Setting(name)),
        };
        let minimum = setting("fBarterMin")?;
        let maximum = setting("fBarterMax")?;
        let buy_min = setting("fBarterBuyMin")?;
        let sell_max = setting("fBarterSellMax")?;
        let factor = maximum - (maximum - minimum) * speech.clamp(0.0, 100.0) / 100.0;
        let context = perks::EntryPointContext {
            tabs: vec![player, merchant],
            target: Some(merchant),
            quest: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            skill_use: None,
        };
        let purchases = perks::entry_point_invocations(&mut world, &self.vm, player, 8, &context)?;
        let sales = perks::entry_point_invocations(&mut world, &self.vm, player, 60, &context)?;
        let buy_multiplier = perks::apply_value_modifiers(&mut world, &purchases, 1.0)?
            * (1.0 - equipment)
            * (1.0 - potion)
            * factor;
        let sale_multiplier = perks::apply_value_modifiers(&mut world, &sales, 1.0)?
            * (1.0 + equipment)
            * (1.0 + potion)
            / factor;
        if !buy_multiplier.is_finite() || !sale_multiplier.is_finite() || factor <= 0.0 {
            return Err(Error::ActorValues);
        }
        Ok(Prices {
            buy: buy_multiplier.max(buy_min),
            sell: sale_multiplier.clamp(0.0, sell_max),
        })
    }
}
