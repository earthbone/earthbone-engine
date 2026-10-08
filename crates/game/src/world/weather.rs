// SPDX-License-Identifier: GPL-3.0-or-later

//! Runtime weather requests from Papyrus, following `Weather.psc` and the
//! `Sky::SetWeather` parameters documented by CommonLibSSE-NG.

use soe_esm::GlobalFormId;
use soe_papyrus::VmError;

use super::{World, T};

/// The most recent weather selected by `Weather.SetActive`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeatherRequest {
    /// The selected `WTHR` form.
    pub weather: GlobalFormId,
    /// Whether the selected weather overrides the region's weather choice.
    pub override_weather: bool,
    /// Whether the sky should accelerate the transition.
    pub accelerate: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct WeatherState {
    pub(super) request: Option<WeatherRequest>,
    pub(super) sky: super::sky::SkyState,
}

impl World {
    /// Select a weather form and retain the sky transition flags.
    ///
    /// # Errors
    /// Returns an error if `weather` does not resolve to a `WTHR` record.
    pub fn set_active_weather(
        &mut self,
        weather: GlobalFormId,
        override_weather: bool,
        accelerate: bool,
    ) -> Result<(), VmError> {
        if self
            .record(weather)
            .is_none_or(|record| record.is_deleted() || record.tag != T(b"WTHR"))
        {
            return Err(VmError::State(format!(
                "Weather.SetActive requires a WTHR form, got {weather}"
            )));
        }
        self.weather.request = Some(WeatherRequest {
            weather,
            override_weather,
            accelerate,
        });
        Ok(())
    }

    /// `Weather.ReleaseOverride`: the sky stops holding an overriding weather.
    /// A request made without the override flag is not an override and stays.
    pub fn release_weather_override(&mut self) {
        if self
            .weather
            .request
            .is_some_and(|request| request.override_weather)
        {
            self.weather.request = None;
        }
    }

    /// Return the latest weather request for the host sky implementation.
    #[must_use]
    pub fn weather_request(&self) -> Option<WeatherRequest> {
        self.weather.request
    }
}
