// SPDX-License-Identifier: GPL-3.0-or-later

//! The world side of the determinism hash (`docs/validation.md`,
//! "Determinism"). A child of the world module so it reads the private
//! tables directly. Maps go in by key order, floats by bit pattern, and
//! the load order and the editor id table stay out, they are inputs.

use soe_esm::records::Transform;
use soe_esm::GlobalFormId;
use soe_papyrus::vm::hash::StateHasher;

use super::{ActorState, Handle, Pending, QuestState, RefState, World};
use crate::ai::{AiState, Procedure};

/// The world's state hash, one part per subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldHash {
    pub clock: u64,
    pub rng: u64,
    /// VM object ids handed to forms and aliases, and the dynamic id counter.
    pub objects: u64,
    pub refs: u64,
    pub actors: u64,
    pub quests: u64,
    pub globals: u64,
    pub form_lists: u64,
    /// Queued events and calls the next pump drains.
    pub pending: u64,
    pub timers: u64,
    /// Notifications, message boxes, animation events, player controls.
    pub output: u64,
    /// The package driver's counters.
    pub ai: u64,
}

impl WorldHash {
    #[must_use]
    pub fn parts(&self) -> [(&'static str, u64); 12] {
        [
            ("world.clock", self.clock),
            ("world.rng", self.rng),
            ("world.objects", self.objects),
            ("world.refs", self.refs),
            ("world.actors", self.actors),
            ("world.quests", self.quests),
            ("world.globals", self.globals),
            ("world.form_lists", self.form_lists),
            ("world.pending", self.pending),
            ("world.timers", self.timers),
            ("world.output", self.output),
            ("world.ai", self.ai),
        ]
    }
}

fn form(h: &mut StateHasher, id: GlobalFormId) {
    h.u16(id.file);
    h.u32(id.object);
}

fn opt_form(h: &mut StateHasher, id: Option<GlobalFormId>) {
    match id {
        None => h.u8(0),
        Some(id) => {
            h.u8(1);
            form(h, id);
        }
    }
}

fn point(h: &mut StateHasher, p: [f32; 3]) {
    for v in p {
        h.f32(v);
    }
}

fn opt_point(h: &mut StateHasher, p: Option<[f32; 3]>) {
    match p {
        None => h.u8(0),
        Some(p) => {
            h.u8(1);
            point(h, p);
        }
    }
}

fn transform(h: &mut StateHasher, t: Transform) {
    point(h, t.position);
    point(h, t.rotation);
}

fn handle(h: &mut StateHasher, handle: Handle) {
    match handle {
        Handle::Form(id) => {
            h.u8(0);
            form(h, id);
        }
        Handle::Alias { quest, id } => {
            h.u8(1);
            form(h, quest);
            h.u32(id);
        }
    }
}

fn ref_state(h: &mut StateHasher, s: &RefState) {
    opt_form(h, s.base);
    opt_form(h, s.cell);
    point(h, s.position);
    point(h, s.rotation);
    h.f32(s.scale);
    h.bool(s.enabled);
    h.bool(s.deleted);
    h.bool(s.activation_blocked);
    h.bool(s.opened);
    h.bool(s.locked);
    h.i32(s.lock_level);
    h.opt_str(s.display_name.as_deref());
    h.len(s.inventory.len());
    for (item, count) in &s.inventory {
        form(h, *item);
        h.i32(*count);
    }
    h.len(s.linked.len());
    for (keyword, target) in &s.linked {
        opt_form(h, *keyword);
        form(h, *target);
    }
    match s.teleport {
        None => h.u8(0),
        Some((door, t)) => {
            h.u8(1);
            form(h, door);
            transform(h, t);
        }
    }
    h.bool(s.is_actor);
}

fn ai_state(h: &mut StateHasher, a: &AiState) {
    opt_form(h, a.package);
    match a.alias {
        None => h.u8(0),
        Some((quest, id)) => {
            h.u8(1);
            form(h, quest);
            h.u32(id);
        }
    }
    match &a.procedure {
        Procedure::None => h.u8(0),
        Procedure::Travel => h.u8(1),
        Procedure::Sandbox => h.u8(2),
        Procedure::Sleep => h.u8(3),
        Procedure::Unsupported(name) => {
            h.u8(4);
            h.str(name);
        }
    }
    opt_point(h, a.destination);
    h.len(a.path.len());
    for p in &a.path {
        point(h, *p);
    }
    h.len(a.path_cells.len());
    for c in &a.path_cells {
        opt_form(h, *c);
    }
    h.bool(a.arrived);
    h.bool(a.sleeping);
    opt_form(h, a.bed);
    opt_point(h, a.center);
    opt_form(h, a.center_cell);
    h.f32(a.radius);
    h.f64(a.dwell_until);
    h.f32(a.speed);
    point(h, a.start_position);
    h.bool(a.reevaluate);
    h.f64(a.last_evaluation);
    h.len(a.completed.len());
    for (package, day) in &a.completed {
        form(h, *package);
        h.u32(*day);
    }
}

fn actor_state(h: &mut StateHasher, a: &ActorState) {
    h.map(&a.values, |h, k, v| {
        h.str(k);
        h.f32(*v);
    });
    h.map(&a.base_values, |h, k, v| {
        h.str(k);
        h.f32(*v);
    });
    h.bool(a.dead);
    h.bool(a.in_combat);
    h.bool(a.teammate);
    h.bool(a.ghost);
    h.bool(a.essential);
    h.bool(a.protected);
    h.u32(a.level);
    opt_form(h, a.race);
    h.bool(a.unconscious);
    h.bool(a.doing_favor);
    h.map(&a.factions, |h, k, v| {
        form(h, *k);
        h.i32(*v);
    });
    h.len(a.spells.len());
    for s in &a.spells {
        form(h, *s);
    }
    h.len(a.equipped.len());
    for e in &a.equipped {
        form(h, *e);
    }
    ai_state(h, &a.ai);
}

fn quest_state(h: &mut StateHasher, q: &QuestState) {
    h.bool(q.running);
    h.bool(q.completed);
    h.bool(q.failed);
    h.bool(q.active);
    h.u16(q.current);
    h.u16(q.highest);
    h.len(q.done.len());
    for s in &q.done {
        h.u16(*s);
    }
    h.len(q.objectives.len());
    for (index, o) in &q.objectives {
        h.u16(*index);
        h.bool(o.displayed);
        h.bool(o.completed);
        h.bool(o.failed);
    }
    h.map(&q.aliases, |h, k, v| {
        h.u32(*k);
        opt_form(h, *v);
    });
}

impl World {
    /// Hash every piece of run-time state the world owns.
    #[must_use]
    pub fn state_hash(&self) -> WorldHash {
        let mut h = StateHasher::new();
        h.f64(self.clock.real_seconds);
        h.f64(self.clock.game_days);
        h.f64(self.clock.timescale);
        let clock = h.finish();

        let mut h = StateHasher::new();
        h.u64(self.rng);
        let rng = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.handles, |h, id, hd| {
            h.u64(id.0);
            handle(h, *hd);
        });
        h.u32(self.next_dynamic);
        opt_form(&mut h, self.player);
        h.u32(self.player_level);
        let objects = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.refs, |h, id, s| {
            form(h, *id);
            ref_state(h, s);
        });
        let refs = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.actors, |h, id, a| {
            form(h, *id);
            actor_state(h, a);
        });
        let actors = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.quests, |h, id, q| {
            form(h, *id);
            quest_state(h, q);
        });
        let quests = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.globals, |h, id, v| {
            form(h, *id);
            h.f32(*v);
        });
        let globals = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.form_lists, |h, id, list| {
            form(h, *id);
            h.len(list.len());
            for f in list {
                form(h, *f);
            }
        });
        let form_lists = h.finish();

        let mut h = StateHasher::new();
        h.len(self.pending.len());
        for p in &self.pending {
            match p {
                Pending::Event { object, name, args } => {
                    h.u8(0);
                    h.u64(object.0);
                    h.str(name);
                    h.len(args.len());
                    for a in args {
                        h.value(a);
                    }
                }
                Pending::Call {
                    object,
                    function,
                    args,
                } => {
                    h.u8(1);
                    h.u64(object.0);
                    h.str(function);
                    h.len(args.len());
                    for a in args {
                        h.value(a);
                    }
                }
            }
        }
        let pending = h.finish();

        let mut h = StateHasher::new();
        h.len(self.timers.len());
        for t in &self.timers {
            h.u64(t.object.0);
            h.f64(t.due);
            match t.interval {
                None => h.u8(0),
                Some(i) => {
                    h.u8(1);
                    h.f64(i);
                }
            }
            h.bool(t.game_time);
        }
        let timers = h.finish();

        let mut h = StateHasher::new();
        h.len(self.notifications.len());
        for n in &self.notifications {
            h.str(n);
        }
        h.len(self.message_boxes.len());
        for m in &self.message_boxes {
            h.str(m);
        }
        h.len(self.animation_events.len());
        for (id, name) in &self.animation_events {
            form(&mut h, *id);
            h.str(name);
        }
        h.bool(self.player_controls);
        let output = h.finish();

        let mut h = StateHasher::new();
        h.len(self.ai.evaluations);
        h.len(self.ai.unsupported_functions.len());
        for (f, n) in &self.ai.unsupported_functions {
            h.u16(*f);
            h.len(*n);
        }
        let ai = h.finish();

        WorldHash {
            clock,
            rng,
            objects,
            refs,
            actors,
            quests,
            globals,
            form_lists,
            pending,
            timers,
            output,
            ai,
        }
    }
}
