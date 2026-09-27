// SPDX-License-Identifier: GPL-3.0-or-later

//! The world side of the determinism hash (`docs/validation.md`,
//! "Determinism"). A child of the world module so it reads the private
//! tables directly. Maps go in by key order, floats by bit pattern, and
//! the load order and the editor id table stay out, they are inputs.

use std::collections::BTreeSet;

use soe_esm::records::Transform;
use soe_esm::GlobalFormId;
use soe_papyrus::vm::hash::StateHasher;

use super::{
    ActorBaseState, ActorState, CellState, FactionState, Handle, LeveledList, LocationState,
    Pending, QuestState, RefState, World,
};
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
    /// Leveled lists, locations, factions, actor bases and cells built from
    /// records on first use, with what scripts changed on them.
    pub forms: u64,
    /// `SetGameSetting*` overrides, selected difficulty and the INI overlay.
    pub settings: u64,
    /// Inventory filters, sleep, stats, animation and line-of-sight
    /// registrations.
    pub listeners: u64,
    /// Tracked stats, the player flags, words of power, achievements, the
    /// last ridden horse.
    pub flags: u64,
    /// Playing scenes, sound instances and recorded spell casts.
    pub scenes: u64,
    /// What the mod data files attached, distributed and swapped, including
    /// retained actor-specific deterministic distribution rules.
    pub mod_data: u64,
}

impl WorldHash {
    #[must_use]
    pub fn parts(&self) -> [(&'static str, u64); 18] {
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
            ("world.forms", self.forms),
            ("world.settings", self.settings),
            ("world.listeners", self.listeners),
            ("world.flags", self.flags),
            ("world.scenes", self.scenes),
            ("world.mod_data", self.mod_data),
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
        Handle::ActiveEffect(id) => {
            h.u8(2);
            h.u64(id.0);
        }
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
    let mut extras = soe_papyrus::vm::state::Writer::new();
    super::reference_extras::write_extra_state(&mut extras, s);
    h.bytes(&extras.out);
    h.opt_str(s.display_name.as_deref());
    h.len(s.inventory.len());
    for (item, count) in s.inventory.iter() {
        form(h, *item);
        h.i32(count);
    }
    let mut stacks = soe_papyrus::vm::state::Writer::new();
    super::item_save::write_stacks(&mut stacks, &s.inventory);
    h.bytes(&stacks.out);
    h.bool(s.item.is_some());
    if let Some(item) = &s.item {
        let mut data = soe_papyrus::vm::state::Writer::new();
        super::item_save::write_stack(&mut data, item);
        h.bytes(&data.out);
    }
    let mut stacks = soe_papyrus::vm::state::Writer::new();
    super::item_save::write_stacks(&mut stacks, &s.inventory);
    h.bytes(&stacks.out);
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
    opt_form(h, s.owner);
    opt_form(h, s.location);
    h.bool(s.on_map);
    h.bool(s.fast_travel);
    h.bool(s.destroyed);
    h.bool(s.ignore_friendly_hits);
    h.len(s.anim_floats.len());
    for (k, v) in &s.anim_floats {
        h.str(k);
        h.f32(*v);
    }
    h.len(s.anim_ints.len());
    for (k, v) in &s.anim_ints {
        h.str(k);
        h.i32(*v);
    }
    h.len(s.anim_bools.len());
    for (k, v) in &s.anim_bools {
        h.str(k);
        h.bool(*v);
    }
    h.f32(s.alpha);
}

fn form_set(h: &mut StateHasher, set: &BTreeSet<GlobalFormId>) {
    h.len(set.len());
    for id in set {
        form(h, *id);
    }
}

fn leveled_list(h: &mut StateHasher, l: &LeveledList) {
    h.u8(l.chance_none);
    h.u8(l.flags);
    h.u8(l.base_chance_none);
    h.len(l.base_entries);
    h.len(l.entries.len());
    for e in &l.entries {
        h.u16(e.level);
        form(h, e.form);
        h.u16(e.count);
    }
}

fn location_state(h: &mut StateHasher, l: &LocationState) {
    h.bool(l.cleared);
    h.len(l.keyword_data.len());
    for (k, v) in &l.keyword_data {
        form(h, *k);
        h.f32(*v);
    }
}

fn faction_state(h: &mut StateHasher, f: &FactionState) {
    h.len(f.reactions.len());
    for (k, v) in &f.reactions {
        form(h, *k);
        h.i32(*v);
    }
    h.i32(f.crime_gold_violent);
    h.i32(f.crime_gold_nonviolent);
    h.i32(f.infamy_violent);
    h.i32(f.infamy_nonviolent);
    h.bool(f.player_expelled);
    h.bool(f.player_enemy);
    h.bool(f.tracks_crime);
}

fn actor_base_state(h: &mut StateHasher, a: &ActorBaseState) {
    for flag in [a.essential, a.protected, a.invulnerable, a.unique, a.female] {
        h.bool(flag);
    }
    for f in [a.race, a.class, a.outfit, a.sleep_outfit] {
        opt_form(h, f);
    }
    h.i32(a.dead_count);
}

fn cell_state(h: &mut StateHasher, c: &CellState) {
    opt_form(h, c.owner);
    h.bool(c.public);
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
        Procedure::Patrol => {
            h.u8(6);
            h.bool(a.patrol.is_some());
            if let Some(state) = &a.patrol {
                form(h, state.start);
                form(h, state.current);
                h.bool(state.reverse);
                h.bool(state.dwell_until.is_some());
                if let Some(until) = state.dwell_until {
                    h.f64(until);
                }
            }
        }
        Procedure::Wait { stop_movement } => {
            h.u8(5);
            h.bool(*stop_movement);
            h.bool(a.wait_until.is_some());
            if let Some(until) = a.wait_until {
                h.f64(until);
            }
        }
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
    h.len(a.values.entries.len());
    for (k, v) in &a.values.entries {
        h.str(k);
        h.f32(v.base());
        h.f32(v.permanent());
        h.f32(v.temporary());
        h.f32(v.damage());
    }
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
    form_set(h, &a.perks);
    for flag in [
        a.alerted,
        a.sneaking,
        a.weapon_drawn,
        a.restrained,
        a.arrested,
        a.attack_on_sight,
        a.dont_move,
        a.allow_pc_dialogue,
    ] {
        h.bool(flag);
    }
    opt_form(h, a.exclusive_branch);
    h.len(a.relationships.len());
    for (id, rank) in &a.relationships {
        form(h, *id);
        h.i32(*rank);
    }
    for id in [
        a.combat_target,
        a.crime_faction,
        a.outfit,
        a.sleep_outfit,
        a.killer,
    ] {
        opt_form(h, id);
    }
    h.len(a.graph_floats.len());
    for (k, v) in &a.graph_floats {
        h.str(k);
        h.f32(*v);
    }
    for flag in [
        a.allow_flying,
        a.deferred_kill,
        a.vampire,
        a.werewolf,
        a.no_bleedout_recovery,
        a.ai_disabled,
    ] {
        h.bool(flag);
    }
    h.i32(a.critical_stage);
    opt_form(h, a.eye_texture);
    opt_form(h, a.ash_pile);
    form_set(h, &a.shouts);
    opt_form(h, a.equipped_shout);
    opt_form(h, a.left_weapon);
    opt_form(h, a.right_weapon);
    opt_form(h, a.left_spell);
    opt_form(h, a.right_spell);
    form_set(h, &a.witnessed_crimes);
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
    let mut saved = soe_papyrus::vm::state::Writer::new();
    super::quest_save::write_aux(&mut saved, q);
    h.bytes(&saved.out);
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
        // Which cell is resident belongs with the reference set rather than
        // in a part of its own, because it describes that same set: a
        // transition changes both together.
        opt_form(&mut h, self.loaded_cell);
        let refs = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.actors, |h, id, a| {
            form(h, *id);
            actor_state(h, a);
        });
        let mut templates = soe_papyrus::vm::state::Writer::new();
        self.write_actor_templates(&mut templates);
        h.bytes(&templates.out);
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
                Pending::DeferredDeath {
                    actor,
                    killer,
                    silent,
                } => {
                    h.u8(3);
                    form(&mut h, *actor);
                    opt_form(&mut h, *killer);
                    h.bool(*silent);
                }
                Pending::ActorDeath {
                    actor,
                    killer,
                    recipients,
                } => {
                    h.u8(4);
                    form(&mut h, *actor);
                    opt_form(&mut h, *killer);
                    h.len(recipients.len());
                    for recipient in recipients {
                        handle(&mut h, *recipient);
                    }
                }
                Pending::BindEffect { object, effect } => {
                    h.u8(2);
                    h.u64(object.0);
                    form(&mut h, *effect);
                }
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

        let mut h = StateHasher::new();
        h.map(&self.leveled_lists, |h, id, l| {
            form(h, *id);
            leveled_list(h, l);
        });
        h.map(&self.locations, |h, id, l| {
            form(h, *id);
            location_state(h, l);
        });
        h.map(&self.factions, |h, id, f| {
            form(h, *id);
            faction_state(h, f);
        });
        h.map(&self.actor_bases, |h, id, a| {
            form(h, *id);
            actor_base_state(h, a);
        });
        h.map(&self.cells, |h, id, c| {
            form(h, *id);
            cell_state(h, c);
        });
        h.len(self.encounter_levels.len());
        for (id, level) in &self.encounter_levels {
            form(&mut h, *id);
            h.u16(*level);
        }
        h.len(self.detection.pools.len());
        for (target, pool) in &self.detection.pools {
            form(&mut h, *target);
            h.f32(pool.points);
            h.f32(pool.alert_wait);
            h.len(pool.observers.len());
            for (observer, a) in &pool.observers {
                form(&mut h, *observer);
                h.u8(a.level as u8);
                h.f32(a.detection);
                h.f32(a.lost_timer);
            }
        }
        let forms = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.settings, |h, k, v| {
            h.str(k);
            h.value(v);
        });
        h.map(&self.ini, |h, k, v| {
            h.str(k);
            h.str(v);
        });
        h.u8(self.difficulty as u8);
        let settings = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.inventory_filters, |h, o, list| {
            h.u64(o.0);
            h.len(list.len());
            for f in list {
                form(h, *f);
            }
        });
        h.len(self.sleep_listeners.len());
        for o in &self.sleep_listeners {
            h.u64(o.0);
        }
        h.len(self.stats_listeners.len());
        for o in &self.stats_listeners {
            h.u64(o.0);
        }
        h.len(self.animation_listeners.len());
        for l in &self.animation_listeners {
            h.u64(l.listener.0);
            form(&mut h, l.sender);
            h.str(&l.event);
        }
        h.len(self.los_listeners.len());
        for l in &self.los_listeners {
            h.u64(l.listener.0);
            form(&mut h, l.viewer);
            form(&mut h, l.target);
            h.bool(l.single);
            h.bool(l.gain);
        }
        let listeners = h.finish();

        let mut h = StateHasher::new();
        h.len(self.stats.len());
        for (k, v) in &self.stats {
            h.str(k);
            h.i32(*v);
        }
        for flag in [
            self.fast_travel,
            self.player_ai_driven,
            self.in_chargen,
            self.beast_form,
        ] {
            h.bool(flag);
        }
        form_set(&mut h, &self.words_taught);
        form_set(&mut h, &self.words_unlocked);
        h.len(self.achievements.len());
        for a in &self.achievements {
            h.i32(*a);
        }
        opt_form(&mut h, self.last_ridden_horse);
        h.i32(self.perk_points);
        let flags = h.finish();

        let mut h = StateHasher::new();
        form_set(&mut h, &self.scenes);
        h.len(self.sound_instances.len());
        for (id, (sound, source, volume)) in &self.sound_instances {
            h.i32(*id);
            form(&mut h, *sound);
            form(&mut h, *source);
            h.f32(*volume);
        }
        h.i32(self.next_sound);
        h.len(self.spell_casts.len());
        for (spell, source, target) in &self.spell_casts {
            form(&mut h, *spell);
            form(&mut h, *source);
            opt_form(&mut h, *target);
        }
        let mut dialogue = soe_papyrus::vm::state::Writer::new();
        self.write_dialogue(&mut dialogue);
        h.bytes(&dialogue.out);
        let mut magic = soe_papyrus::vm::state::Writer::new();
        self.write_magic(&mut magic);
        self.write_image_space(&mut magic);
        h.bytes(&magic.out);
        let mut records = soe_papyrus::vm::state::Writer::new();
        self.write_record_overrides(&mut records);
        h.bytes(&records.out);
        let scenes = h.finish();

        // These three are `BTreeMap`, already in key order, so they are walked
        // directly rather than through the sorting helper the `HashMap` fields
        // need.
        let mut h = StateHasher::new();
        h.len(self.mod_data.added_keywords.len());
        for (id, kws) in &self.mod_data.added_keywords {
            form(&mut h, *id);
            h.len(kws.len());
            for k in kws {
                form(&mut h, *k);
            }
        }
        h.len(self.mod_data.distributed.len());
        for (id, list) in &self.mod_data.distributed {
            form(&mut h, *id);
            h.len(list.len());
            for d in list {
                h.str(d.record_type_name());
                form(&mut h, d.what);
                h.i32(d.count);
                h.bool(d.on_death);
            }
        }
        h.len(self.mod_data.actor_rules.len());
        for rule in &self.mod_data.actor_rules {
            form(&mut h, rule.base);
            form(&mut h, rule.what);
            h.u64(rule.distribution.line_seed);
            h.f32(rule.distribution.chance);
            h.i32(rule.count);
            h.str(&format!("{:?}", rule.distribution));
        }
        h.len(self.mod_data.base_swaps.len());
        for (from, to) in &self.mod_data.base_swaps {
            form(&mut h, *from);
            form(&mut h, *to);
        }
        h.len(self.mod_data.reference_swaps.len());
        for (reference, to) in &self.mod_data.reference_swaps {
            form(&mut h, *reference);
            form(&mut h, *to);
        }
        let mod_data = h.finish();

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
            forms,
            settings,
            listeners,
            flags,
            scenes,
            mod_data,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use soe_esm::{GlobalFormId, LoadOrder};
    use soe_papyrus::{ObjectId, Value};

    use super::super::{
        ActorBaseState, ActorState, AnimationListener, CellState, FactionState, LeveledEntry,
        LeveledList, LocationState, LosListener, QuestState, RefState, World,
    };
    use super::WorldHash;
    use crate::quest::{build_record, Quest};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    /// A world with one built reference, actor and quest, so a mutation on
    /// any of their fields has something to land on.
    fn seeded() -> World {
        let mut w = World::new(LoadOrder::new());
        let mut r = RefState::new(None);
        r.is_actor = true;
        w.refs.insert(id(1), r);
        w.actors.insert(id(1), ActorState::default());
        let def = Quest::from_record(&build_record(2, "SeedQuest", 0), false).expect("seed quest");
        w.quests.insert(
            id(2),
            QuestState {
                def,
                running: false,
                completed: false,
                failed: false,
                active: false,
                current: 0,
                highest: 0,
                done: BTreeSet::new(),
                objectives: BTreeMap::new(),
                aliases: HashMap::new(),
                journal: crate::quest::state::Journal::default(),
                runtime: crate::quest::state::Runtime::default(),
            },
        );
        w
    }

    fn refs(h: &WorldHash) -> u64 {
        h.refs
    }

    fn actors(h: &WorldHash) -> u64 {
        h.actors
    }

    fn quests(h: &WorldHash) -> u64 {
        h.quests
    }

    fn forms(h: &WorldHash) -> u64 {
        h.forms
    }

    fn settings(h: &WorldHash) -> u64 {
        h.settings
    }

    fn listeners(h: &WorldHash) -> u64 {
        h.listeners
    }

    fn flags(h: &WorldHash) -> u64 {
        h.flags
    }

    fn scenes(h: &WorldHash) -> u64 {
        h.scenes
    }

    /// Applies `mutate` to a seeded world and asserts that exactly the
    /// named part moved, so a field a future change forgets to hash shows
    /// up here as a failing case rather than as silent noise later.
    fn moves(name: &str, part: fn(&WorldHash) -> u64, mutate: impl FnOnce(&mut World)) {
        let mut w = seeded();
        let before = w.state_hash();
        mutate(&mut w);
        let after = w.state_hash();
        assert_ne!(part(&before), part(&after), "{name} did not move its part");
        let moved: Vec<&str> = before
            .parts()
            .iter()
            .zip(after.parts())
            .filter(|(b, a)| b.1 != a.1)
            .map(|(b, _)| b.0)
            .collect();
        assert_eq!(moved.len(), 1, "{name} moved more than one part: {moved:?}");
    }

    fn actor(w: &mut World) -> &mut ActorState {
        w.actors.get_mut(&id(1)).expect("seeded actor")
    }

    fn reference(w: &mut World) -> &mut RefState {
        w.refs.get_mut(&id(1)).expect("seeded reference")
    }

    #[test]
    fn every_reference_field_moves_the_refs_part() {
        moves("owner", refs, |w| reference(w).owner = Some(id(9)));
        moves("location", refs, |w| reference(w).location = Some(id(9)));
        moves("on_map", refs, |w| reference(w).on_map = true);
        moves("fast_travel", refs, |w| reference(w).fast_travel = true);
        moves("destroyed", refs, |w| reference(w).destroyed = true);
        moves("ignore_friendly_hits", refs, |w| {
            reference(w).ignore_friendly_hits = true;
        });
        moves("anim_floats", refs, |w| {
            reference(w).anim_floats.insert("speed".into(), 1.0);
        });
        moves("anim_ints", refs, |w| {
            reference(w).anim_ints.insert("stage".into(), 1);
        });
        moves("anim_bools", refs, |w| {
            reference(w).anim_bools.insert("open".into(), true);
        });
        moves("alpha", refs, |w| reference(w).alpha = 0.5);
    }

    #[test]
    fn every_actor_field_moves_the_actors_part() {
        moves("perks", actors, |w| {
            actor(w).perks.insert(id(9));
        });
        moves("alerted", actors, |w| actor(w).alerted = true);
        moves("sneaking", actors, |w| actor(w).sneaking = true);
        moves("weapon_drawn", actors, |w| actor(w).weapon_drawn = true);
        moves("restrained", actors, |w| actor(w).restrained = true);
        moves("arrested", actors, |w| actor(w).arrested = true);
        moves("attack_on_sight", actors, |w| {
            actor(w).attack_on_sight = true;
        });
        moves("dont_move", actors, |w| actor(w).dont_move = true);
        moves("allow_pc_dialogue", actors, |w| {
            actor(w).allow_pc_dialogue = true;
        });
        moves("relationships", actors, |w| {
            actor(w).relationships.insert(id(9), 2);
        });
        moves("combat_target", actors, |w| {
            actor(w).combat_target = Some(id(9));
        });
        moves("crime_faction", actors, |w| {
            actor(w).crime_faction = Some(id(9));
        });
        moves("outfit", actors, |w| actor(w).outfit = Some(id(9)));
        moves("sleep_outfit", actors, |w| {
            actor(w).sleep_outfit = Some(id(9));
        });
        moves("killer", actors, |w| actor(w).killer = Some(id(9)));
        moves("graph_floats", actors, |w| {
            actor(w).graph_floats.insert("speed".into(), 1.0);
        });
        moves("allow_flying", actors, |w| actor(w).allow_flying = true);
        moves("deferred_kill", actors, |w| actor(w).deferred_kill = true);
        moves("vampire", actors, |w| actor(w).vampire = true);
        moves("werewolf", actors, |w| actor(w).werewolf = true);
        moves("no_bleedout_recovery", actors, |w| {
            actor(w).no_bleedout_recovery = true;
        });
        moves("ai_disabled", actors, |w| actor(w).ai_disabled = true);
        moves("critical_stage", actors, |w| actor(w).critical_stage = 3);
        moves("eye_texture", actors, |w| {
            actor(w).eye_texture = Some(id(9));
        });
        moves("ash_pile", actors, |w| actor(w).ash_pile = Some(id(9)));
        moves("shouts", actors, |w| {
            actor(w).shouts.insert(id(9));
        });
        moves("equipped_shout", actors, |w| {
            actor(w).equipped_shout = Some(id(9));
        });
    }

    #[test]
    fn quest_instance_globals_move_the_quests_part() {
        moves("instance_globals", quests, |w| {
            w.quests
                .get_mut(&id(2))
                .expect("seeded quest")
                .journal
                .update_global(id(9), 1.5);
        });
    }

    #[test]
    fn every_form_table_moves_the_forms_part() {
        moves("leveled_lists", forms, |w| {
            w.leveled_lists.insert(
                id(3),
                LeveledList {
                    chance_none: 10,
                    flags: 0,
                    entries: vec![LeveledEntry {
                        level: 1,
                        form: id(9),
                        count: 1,
                    }],
                    base_entries: 1,
                    base_chance_none: 10,
                },
            );
        });
        moves("locations", forms, |w| {
            w.locations.insert(id(4), LocationState::default());
        });
        moves("factions", forms, |w| {
            w.factions.insert(id(5), FactionState::default());
        });
        moves("actor_bases", forms, |w| {
            w.actor_bases.insert(id(6), ActorBaseState::default());
        });
        moves("cells", forms, |w| {
            w.cells.insert(id(7), CellState::default());
        });
    }

    #[test]
    fn settings_and_ini_move_the_settings_part() {
        moves("settings", settings, |w| {
            w.settings.insert("fjumpheight".into(), Value::Float(76.0));
        });
        moves("ini", settings, |w| {
            w.ini.insert("bAlwaysActive".into(), "1".into());
        });
    }

    #[test]
    fn every_listener_list_moves_the_listeners_part() {
        moves("inventory_filters", listeners, |w| {
            w.inventory_filters.insert(ObjectId(1), vec![id(9)]);
        });
        moves("sleep_listeners", listeners, |w| {
            w.sleep_listeners.push(ObjectId(1));
        });
        moves("stats_listeners", listeners, |w| {
            w.stats_listeners.push(ObjectId(1));
        });
        moves("animation_listeners", listeners, |w| {
            w.animation_listeners.push(AnimationListener {
                listener: ObjectId(1),
                sender: id(1),
                event: "weaponSwing".into(),
            });
        });
        moves("los_listeners", listeners, |w| {
            w.los_listeners.push(LosListener {
                listener: ObjectId(1),
                viewer: id(1),
                target: id(9),
                single: true,
                gain: true,
            });
        });
    }

    #[test]
    fn every_player_flag_moves_the_flags_part() {
        moves("stats", flags, |w| {
            w.stats.insert("Locks Picked".into(), 1);
        });
        moves("fast_travel", flags, |w| w.fast_travel = false);
        moves("player_ai_driven", flags, |w| w.player_ai_driven = true);
        moves("in_chargen", flags, |w| w.in_chargen = true);
        moves("beast_form", flags, |w| w.beast_form = true);
        moves("words_taught", flags, |w| {
            w.words_taught.insert(id(9));
        });
        moves("words_unlocked", flags, |w| {
            w.words_unlocked.insert(id(9));
        });
        moves("achievements", flags, |w| w.achievements.push(3));
        moves("last_ridden_horse", flags, |w| {
            w.last_ridden_horse = Some(id(9));
        });
        moves("perk_points", flags, |w| w.perk_points = 2);
    }

    #[test]
    fn scenes_sounds_and_casts_move_the_scenes_part() {
        moves("scenes", scenes, |w| {
            w.scenes.insert(id(9));
        });
        moves("sound_instances", scenes, |w| {
            w.sound_instances.insert(1, (id(8), id(1), 1.0));
        });
        moves("next_sound", scenes, |w| w.next_sound = 1);
        moves("spell_casts", scenes, |w| {
            w.spell_casts.push((id(8), id(1), None));
        });
    }
}
