// SPDX-License-Identifier: GPL-3.0-or-later

//! Trainer menu rows and purchases over the shared Game.

use soe_esm::GlobalFormId;
use soe_game::Game;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub trainer: GlobalFormId,
    pub skill: String,
    pub current: f32,
    pub cap: f32,
    pub cost: i32,
    pub sessions_remaining: u32,
}

/// Text displayed above the trainer's purchase action.
#[must_use]
pub fn heading(entry: &Entry) -> String {
    format!(
        "{}: {:.0} / {:.0} ({} sessions left)",
        entry.skill, entry.current, entry.cap, entry.sessions_remaining
    )
}

/// Button text for the trainer's current offer.
#[must_use]
pub fn purchase_label(entry: &Entry) -> String {
    format!("Train for {} gold", entry.cost)
}

/// Message shown when the actor has no available training offer.
pub const UNAVAILABLE_LABEL: &str = "Training is unavailable.";

/// Build the single skill row supplied by a trainer's winning class record.
pub fn entry(game: &mut Game, trainer: GlobalFormId) -> Option<Entry> {
    let offer = game.world_mut().training_offer(trainer).ok()?;
    Some(Entry {
        trainer,
        skill: offer.skill.to_owned(),
        current: offer.current,
        cap: offer.cap,
        cost: offer.cost,
        sessions_remaining: offer.sessions_remaining,
    })
}

/// Buy the offered session through World and allocate its event objects in VM.
pub fn purchase(
    game: &mut Game,
    trainer: GlobalFormId,
) -> Result<i32, soe_game::world::training::Error> {
    let world = game.shared_world();
    let vm = &mut game.vm;
    let result = world
        .borrow_mut()
        .purchase_training(trainer, &mut |name, _class| vm.create_object(name));
    result
}
