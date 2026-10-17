// SPDX-License-Identifier: GPL-3.0-or-later

//! Probe configuration for the shared game graph driver.

use crate::{Error, Probe};
use soe_esm::GlobalFormId;
use soe_game::{anim_driver, Game};
use std::path::Path;

pub(crate) struct Assets(anim_driver::Assets);

fn execution(error: impl std::fmt::Display) -> Error {
    Error::Execution(error.to_string())
}

impl Assets {
    pub fn load(probe: &Probe, data: &Path) -> Result<Option<Self>, Error> {
        probe
            .animation_graph
            .as_ref()
            .map(|config| {
                anim_driver::Assets::load(&config.path, data)
                    .map(Self)
                    .map_err(execution)
            })
            .transpose()
    }
}

pub(crate) struct Host<'a>(anim_driver::Host<'a>);

impl<'a> Host<'a> {
    pub fn new(assets: &'a Assets, actor: GlobalFormId) -> Self {
        Self(anim_driver::Host::new(&assets.0, actor))
    }

    pub fn update(&mut self, game: &mut Game, dt: f32) -> Result<(), Error> {
        self.0.update(game, dt).map_err(execution)
    }
}
