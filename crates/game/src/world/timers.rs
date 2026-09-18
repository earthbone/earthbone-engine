// SPDX-License-Identifier: GPL-3.0-or-later

//! Update timers: `RegisterForUpdate` and `RegisterForUpdateGameTime`
//! registrations on the real and the game clock, fired into `pending`.

use soe_papyrus::ObjectId;

use super::{Pending, UpdateTimer, World};

impl World {
    pub fn register_update(
        &mut self,
        object: ObjectId,
        seconds: f64,
        repeat: bool,
        game_time: bool,
    ) {
        self.unregister_update(object, game_time);
        let now = if game_time {
            self.clock.game_hours()
        } else {
            self.clock.real_seconds
        };
        self.timers.push(UpdateTimer {
            object,
            due: now + seconds,
            interval: repeat.then_some(seconds),
            game_time,
        });
    }

    pub fn unregister_update(&mut self, object: ObjectId, game_time: bool) {
        self.timers
            .retain(|t| !(t.object == object && t.game_time == game_time));
    }

    /// Move every due timer into `pending` as `OnUpdate` or
    /// `OnUpdateGameTime` events.
    pub fn fire_timers(&mut self) {
        let real = self.clock.real_seconds;
        let hours = self.clock.game_hours();
        let mut i = 0;
        while i < self.timers.len() {
            let t = &mut self.timers[i];
            let now = if t.game_time { hours } else { real };
            if now < t.due {
                i += 1;
                continue;
            }
            let name = if t.game_time {
                "OnUpdateGameTime"
            } else {
                "OnUpdate"
            };
            let object = t.object;
            match t.interval {
                Some(iv) => {
                    t.due += iv;
                    i += 1;
                }
                None => {
                    self.timers.remove(i);
                }
            }
            self.pending.push(Pending::Event {
                object,
                name: name.into(),
                args: Vec::new(),
            });
        }
    }
}
