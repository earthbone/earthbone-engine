// SPDX-License-Identifier: GPL-3.0-or-later

//! The `SOEW` world save: every run-time table written in full and read
//! back against the same load order, plus the per-struct encoders.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use soe_esm::records::Transform;
use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Reader, Writer};
use soe_papyrus::{ObjectId, VmError};

use super::{
    ActorBaseState, ActorState, AnimationListener, CellState, FactionState, Handle, LeveledEntry,
    LeveledList, LocationState, LosListener, ObjectiveState, Pending, QuestState, RefState,
    UpdateTimer, World,
};
use crate::clock::GameClock;

/// Magic and version for [`World::save_state`]. Embedded as a length-prefixed
/// block inside the whole-game save `Game::save_state` writes, self-describing
/// on its own so a truncated or foreign blob is caught here too.
pub const SAVE_MAGIC: &[u8; 4] = b"SOEW";
/// Version 2 added the package driver state to every actor and the natives
/// round state (perks, flags, leveled lists, locations, factions, cells).
/// Version 3 added the DLC state: vampirism and lycanthropy, flying, the
/// deferred kill, eye texture, critical stage, bleedout, shouts, the
/// behavior graph floats and the player's perk points.
/// Version 4 added the resident cell, so a loaded game knows which cell's
/// temporary references it is holding. See `crate::world::transition`.
/// Versions 5 and 6 add magic and dialogue to legacy actor values.
/// Versions 7 and 8 add actor-value components and dialogue, respectively.
/// Version 9 combines components, dialogue and active magic effects.
/// Version 10 adds persistent per-reference leveled template selections.
/// Version 11 adds mapped quest runtime and journal instance state.
/// Version 12 retains distinct inventory stacks and their mapped extra data.
/// Version 13 persists runtime NPC record overrides and mapped links.
/// Version 14 adds temporary NPC identities, ownership and component provenance.
/// Version 15 persists deferred-death requests and their eventual event recipients.
/// Version 16 persists image-space applications and crossfade progress.
pub const SAVE_VERSION: u16 = 16;

impl World {
    /// Serialize the run-time state: the VM object handles, references,
    /// actors, the mutable half of every quest, globals, form lists, the
    /// dynamic id counter, the player, the RNG state, the clock, and
    /// whatever is queued (pending events and calls, update timers,
    /// notifications, message boxes, animation events, the INI overlay).
    ///
    /// A quest's definition (its stages, objectives and aliases as written
    /// in the record) is not stored, `load_state` re-reads it from the load
    /// order the same way [`soe_papyrus::Vm::load_state`] leaves scripts for
    /// the host to reload first. Everything else here is runtime state with
    /// no record to fall back on, dynamic references included, so it is
    /// written in full.
    pub fn save_state(&self) -> Vec<u8> {
        let mut pool = ArrayPool::new();
        let mut body = Writer::new();

        body.len(self.objects.len());
        let mut objects: Vec<_> = self.objects.iter().collect();
        objects.sort_by_key(|(_, object)| object.0);
        for (h, o) in objects {
            write_handle(&mut body, *h);
            body.u64(o.0);
        }

        body.len(self.refs.len());
        for (id, s) in self.refs.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            write_ref_state(&mut body, s);
        }

        body.len(self.actors.len());
        for (id, a) in self.actors.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            write_actor_state(&mut body, a);
        }

        body.len(self.quests.len());
        for (id, q) in self.quests.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            write_quest_mutable(&mut body, q);
        }

        body.len(self.globals.len());
        for (id, v) in self.globals.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            body.f32(*v);
        }

        body.len(self.form_lists.len());
        for (id, items) in self.form_lists.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            body.len(items.len());
            for i in items {
                write_form_id(&mut body, *i);
            }
        }

        body.u32(self.next_dynamic);
        write_opt_form_id(&mut body, self.player);
        write_opt_form_id(&mut body, self.loaded_cell);
        body.u64(self.rng);

        body.f64(self.clock.real_seconds);
        body.f64(self.clock.game_days);
        body.f64(self.clock.timescale);

        body.len(self.pending.len());
        for p in &self.pending {
            match p {
                Pending::DeferredDeath {
                    actor,
                    killer,
                    silent,
                } => {
                    body.u8(3);
                    write_form_id(&mut body, *actor);
                    write_opt_form_id(&mut body, *killer);
                    body.bool(*silent);
                }
                Pending::ActorDeath {
                    actor,
                    killer,
                    recipients,
                } => {
                    body.u8(4);
                    write_form_id(&mut body, *actor);
                    write_opt_form_id(&mut body, *killer);
                    body.len(recipients.len());
                    for recipient in recipients {
                        write_handle(&mut body, *recipient);
                    }
                }
                Pending::BindEffect { object, effect } => {
                    body.u8(2);
                    body.u64(object.0);
                    write_form_id(&mut body, *effect);
                }
                Pending::Event { object, name, args } => {
                    body.u8(0);
                    body.u64(object.0);
                    body.str(name);
                    body.len(args.len());
                    for v in args {
                        pool.write_value(&mut body, v);
                    }
                }
                Pending::Call {
                    object,
                    function,
                    args,
                } => {
                    body.u8(1);
                    body.u64(object.0);
                    body.str(function);
                    body.len(args.len());
                    for v in args {
                        pool.write_value(&mut body, v);
                    }
                }
            }
        }

        body.len(self.timers.len());
        for t in &self.timers {
            body.u64(t.object.0);
            body.f64(t.due);
            match t.interval {
                Some(v) => {
                    body.bool(true);
                    body.f64(v);
                }
                None => body.bool(false),
            }
            body.bool(t.game_time);
        }

        body.len(self.notifications.len());
        for s in &self.notifications {
            body.str(s);
        }
        body.len(self.message_boxes.len());
        for s in &self.message_boxes {
            body.str(s);
        }
        body.len(self.animation_events.len());
        for (id, s) in &self.animation_events {
            write_form_id(&mut body, *id);
            body.str(s);
        }
        body.bool(self.player_controls);
        body.len(self.ini.len());
        for (k, v) in self.ini.iter().collect::<BTreeMap<_, _>>() {
            body.str(k);
            body.str(v);
        }

        // Version 2: the state the natives round added.
        body.len(self.leveled_lists.len());
        for (id, l) in self.leveled_lists.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            body.u8(l.chance_none);
            body.u8(l.flags);
            body.u8(l.base_chance_none);
            body.len(l.base_entries);
            body.len(l.entries.len());
            for e in &l.entries {
                body.u16(e.level);
                write_form_id(&mut body, e.form);
                body.u16(e.count);
            }
        }
        body.len(self.locations.len());
        for (id, l) in self.locations.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            body.bool(l.cleared);
            body.len(l.keyword_data.len());
            for (k, v) in &l.keyword_data {
                write_form_id(&mut body, *k);
                body.f32(*v);
            }
        }
        body.len(self.factions.len());
        for (id, f) in self.factions.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            body.len(f.reactions.len());
            for (k, v) in &f.reactions {
                write_form_id(&mut body, *k);
                body.i32(*v);
            }
            body.i32(f.crime_gold_violent);
            body.i32(f.crime_gold_nonviolent);
            body.i32(f.infamy_violent);
            body.i32(f.infamy_nonviolent);
            body.bool(f.player_expelled);
            body.bool(f.player_enemy);
            body.bool(f.tracks_crime);
        }
        body.len(self.actor_bases.len());
        for (id, a) in self.actor_bases.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            for flag in [a.essential, a.protected, a.invulnerable, a.unique, a.female] {
                body.bool(flag);
            }
            for f in [a.race, a.class, a.outfit, a.sleep_outfit] {
                write_opt_form_id(&mut body, f);
            }
            body.i32(a.dead_count);
        }
        body.len(self.cells.len());
        for (id, c) in self.cells.iter().collect::<BTreeMap<_, _>>() {
            write_form_id(&mut body, *id);
            write_opt_form_id(&mut body, c.owner);
            body.bool(c.public);
        }
        body.len(self.settings.len());
        for (k, v) in self.settings.iter().collect::<BTreeMap<_, _>>() {
            body.str(k);
            pool.write_value(&mut body, v);
        }
        body.len(self.inventory_filters.len());
        for (o, list) in self.inventory_filters.iter().collect::<BTreeMap<_, _>>() {
            body.u64(o.0);
            body.len(list.len());
            for f in list {
                write_form_id(&mut body, *f);
            }
        }
        write_object_list(&mut body, &self.sleep_listeners);
        write_object_list(&mut body, &self.stats_listeners);
        body.len(self.animation_listeners.len());
        for l in &self.animation_listeners {
            body.u64(l.listener.0);
            write_form_id(&mut body, l.sender);
            body.str(&l.event);
        }
        body.len(self.los_listeners.len());
        for l in &self.los_listeners {
            body.u64(l.listener.0);
            write_form_id(&mut body, l.viewer);
            write_form_id(&mut body, l.target);
            body.bool(l.single);
            body.bool(l.gain);
        }
        body.len(self.stats.len());
        for (k, v) in &self.stats {
            body.str(k);
            body.i32(*v);
        }
        for flag in [
            self.fast_travel,
            self.player_ai_driven,
            self.in_chargen,
            self.beast_form,
        ] {
            body.bool(flag);
        }
        write_form_set(&mut body, &self.words_taught);
        write_form_set(&mut body, &self.words_unlocked);
        body.len(self.achievements.len());
        for a in &self.achievements {
            body.i32(*a);
        }
        write_opt_form_id(&mut body, self.last_ridden_horse);
        body.i32(self.perk_points);
        write_form_set(&mut body, &self.scenes);
        body.len(self.sound_instances.len());
        for (id, (sound, source, volume)) in &self.sound_instances {
            body.i32(*id);
            write_form_id(&mut body, *sound);
            write_form_id(&mut body, *source);
            body.f32(*volume);
        }
        body.i32(self.next_sound);
        body.len(self.spell_casts.len());
        for (spell, source, target) in &self.spell_casts {
            write_form_id(&mut body, *spell);
            write_form_id(&mut body, *source);
            write_opt_form_id(&mut body, *target);
        }

        self.write_dialogue(&mut body);
        self.write_magic(&mut body);
        self.write_actor_templates(&mut body);
        self.write_quest_aux(&mut body);
        self.write_item_stacks(&mut body);
        self.write_record_overrides(&mut body);
        self.write_image_space(&mut body);
        let mut out = Writer::new();
        out.out.extend_from_slice(SAVE_MAGIC);
        out.u16(SAVE_VERSION);
        pool.write_pool(&mut out);
        out.out.extend_from_slice(&body.out);
        out.out
    }

    /// Replace the live run-time state with a saved one, against a load
    /// order identical to the one the save was written against.
    ///
    /// # Errors
    ///
    /// A wrong magic or version, a truncated or inconsistent stream, or a
    /// quest the load order no longer defines.
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        let mut r = Reader::new(bytes);
        if r.take(4)? != SAVE_MAGIC {
            return Err(VmError::State("not a world state file".into()));
        }
        let version = r.u16()?;
        if !matches!(version, 4..=SAVE_VERSION) {
            return Err(VmError::State(format!(
                "world state version {version}, this build reads {SAVE_VERSION}"
            )));
        }
        let table = ArrayTable::read_pool(&mut r)?;

        let n = r.len()?;
        let mut objects = HashMap::with_capacity(n);
        let mut handles = HashMap::with_capacity(n);
        for _ in 0..n {
            let h = read_handle(&mut r)?;
            let o = ObjectId(r.u64()?);
            objects.insert(h, o);
            handles.insert(o, h);
        }

        let n = r.len()?;
        let mut refs = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            refs.insert(id, read_ref_state(&mut r)?);
        }

        let n = r.len()?;
        let mut actors = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            actors.insert(id, read_actor_state(&mut r, version)?);
        }

        let n = r.len()?;
        let mut quests = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            let running = r.bool()?;
            let completed = r.bool()?;
            let failed = r.bool()?;
            let active = r.bool()?;
            let current = r.u16()?;
            let highest = r.u16()?;
            let k = r.len()?;
            let mut done = BTreeSet::new();
            for _ in 0..k {
                done.insert(r.u16()?);
            }
            let k = r.len()?;
            let mut objectives = BTreeMap::new();
            for _ in 0..k {
                let idx = r.u16()?;
                objectives.insert(idx, read_objective(&mut r)?);
            }
            let k = r.len()?;
            let mut aliases = HashMap::with_capacity(k);
            for _ in 0..k {
                let alias = r.u32()?;
                aliases.insert(alias, read_opt_form_id(&mut r)?);
            }
            let k = r.len()?;
            let mut instance_globals = BTreeMap::new();
            for _ in 0..k {
                let id = read_form_id(&mut r)?;
                instance_globals.insert(id, r.f32()?);
            }
            let Some(initial) = self.initial_quest(id) else {
                return Err(VmError::State(format!(
                    "quest {id} is not in the load order"
                )));
            };
            quests.insert(
                id,
                QuestState {
                    def: initial.def,
                    running,
                    completed,
                    failed,
                    active,
                    current,
                    highest,
                    done,
                    objectives,
                    aliases,
                    journal: crate::quest::state::Journal::legacy(instance_globals),
                    runtime: initial.runtime,
                },
            );
        }

        let n = r.len()?;
        let mut globals = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            globals.insert(id, r.f32()?);
        }

        let n = r.len()?;
        let mut form_lists = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            let k = r.len()?;
            let mut items = Vec::with_capacity(k);
            for _ in 0..k {
                items.push(read_form_id(&mut r)?);
            }
            form_lists.insert(id, items);
        }

        let next_dynamic = r.u32()?;
        let player = read_opt_form_id(&mut r)?;
        let loaded_cell = read_opt_form_id(&mut r)?;
        let rng = r.u64()?;

        let real_seconds = r.f64()?;
        let game_days = r.f64()?;
        let timescale = r.f64()?;

        let n = r.len()?;
        let mut pending = Vec::with_capacity(n);
        for _ in 0..n {
            let tag = r.u8()?;
            match tag {
                3 if version >= 15 => pending.push(Pending::DeferredDeath {
                    actor: read_form_id(&mut r)?,
                    killer: read_opt_form_id(&mut r)?,
                    silent: r.bool()?,
                }),
                4 if version >= 15 => {
                    let actor = read_form_id(&mut r)?;
                    let killer = read_opt_form_id(&mut r)?;
                    let count = r.len()?;
                    if count > r.remaining() / 7 {
                        return Err(VmError::State("death recipient count exceeds data".into()));
                    }
                    let mut recipients = Vec::with_capacity(count);
                    for _ in 0..count {
                        recipients.push(read_handle(&mut r)?);
                    }
                    pending.push(Pending::ActorDeath {
                        actor,
                        killer,
                        recipients,
                    });
                }
                2 if version == 5 || version >= 9 => pending.push(Pending::BindEffect {
                    object: ObjectId(r.u64()?),
                    effect: read_form_id(&mut r)?,
                }),
                0 => {
                    let object = ObjectId(r.u64()?);
                    let name = r.str()?;
                    let k = r.len()?;
                    let mut args = Vec::with_capacity(k);
                    for _ in 0..k {
                        args.push(table.read_value(&mut r)?);
                    }
                    pending.push(Pending::Event { object, name, args });
                }
                1 => {
                    let object = ObjectId(r.u64()?);
                    let function = r.str()?;
                    let k = r.len()?;
                    let mut args = Vec::with_capacity(k);
                    for _ in 0..k {
                        args.push(table.read_value(&mut r)?);
                    }
                    pending.push(Pending::Call {
                        object,
                        function,
                        args,
                    });
                }
                other => return Err(VmError::State(format!("unknown pending kind {other}"))),
            }
        }

        let n = r.len()?;
        let mut timers = Vec::with_capacity(n);
        for _ in 0..n {
            let object = ObjectId(r.u64()?);
            let due = r.f64()?;
            let interval = if r.bool()? { Some(r.f64()?) } else { None };
            let game_time = r.bool()?;
            timers.push(UpdateTimer {
                object,
                due,
                interval,
                game_time,
            });
        }

        let n = r.len()?;
        let mut notifications = Vec::with_capacity(n);
        for _ in 0..n {
            notifications.push(r.str()?);
        }
        let n = r.len()?;
        let mut message_boxes = Vec::with_capacity(n);
        for _ in 0..n {
            message_boxes.push(r.str()?);
        }
        let n = r.len()?;
        let mut animation_events = Vec::with_capacity(n);
        for _ in 0..n {
            animation_events.push((read_form_id(&mut r)?, r.str()?));
        }
        let player_controls = r.bool()?;
        let n = r.len()?;
        let mut ini = HashMap::with_capacity(n);
        for _ in 0..n {
            let k = r.str()?;
            ini.insert(k, r.str()?);
        }

        let n = r.len()?;
        let mut leveled_lists = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            let chance_none = r.u8()?;
            let flags = r.u8()?;
            let base_chance_none = r.u8()?;
            let base_entries = r.len()?;
            let k = r.len()?;
            let mut entries = Vec::with_capacity(k);
            for _ in 0..k {
                entries.push(LeveledEntry {
                    level: r.u16()?,
                    form: read_form_id(&mut r)?,
                    count: r.u16()?,
                });
            }
            leveled_lists.insert(
                id,
                LeveledList {
                    chance_none,
                    flags,
                    entries,
                    base_entries,
                    base_chance_none,
                },
            );
        }
        let n = r.len()?;
        let mut locations = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            let cleared = r.bool()?;
            let k = r.len()?;
            let mut keyword_data = BTreeMap::new();
            for _ in 0..k {
                let kw = read_form_id(&mut r)?;
                keyword_data.insert(kw, r.f32()?);
            }
            locations.insert(
                id,
                LocationState {
                    cleared,
                    keyword_data,
                },
            );
        }
        let n = r.len()?;
        let mut factions = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            let k = r.len()?;
            let mut reactions = BTreeMap::new();
            for _ in 0..k {
                let other = read_form_id(&mut r)?;
                reactions.insert(other, r.i32()?);
            }
            factions.insert(
                id,
                FactionState {
                    reactions,
                    crime_gold_violent: r.i32()?,
                    crime_gold_nonviolent: r.i32()?,
                    infamy_violent: r.i32()?,
                    infamy_nonviolent: r.i32()?,
                    player_expelled: r.bool()?,
                    player_enemy: r.bool()?,
                    tracks_crime: r.bool()?,
                },
            );
        }
        let n = r.len()?;
        let mut actor_bases = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            actor_bases.insert(
                id,
                ActorBaseState {
                    essential: r.bool()?,
                    protected: r.bool()?,
                    invulnerable: r.bool()?,
                    unique: r.bool()?,
                    female: r.bool()?,
                    race: read_opt_form_id(&mut r)?,
                    class: read_opt_form_id(&mut r)?,
                    outfit: read_opt_form_id(&mut r)?,
                    sleep_outfit: read_opt_form_id(&mut r)?,
                    dead_count: r.i32()?,
                },
            );
        }
        let n = r.len()?;
        let mut cells = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = read_form_id(&mut r)?;
            cells.insert(
                id,
                CellState {
                    owner: read_opt_form_id(&mut r)?,
                    public: r.bool()?,
                },
            );
        }
        let n = r.len()?;
        let mut settings = HashMap::with_capacity(n);
        for _ in 0..n {
            let k = r.str()?;
            settings.insert(k, table.read_value(&mut r)?);
        }
        let n = r.len()?;
        let mut inventory_filters = HashMap::with_capacity(n);
        for _ in 0..n {
            let o = ObjectId(r.u64()?);
            let k = r.len()?;
            let mut list = Vec::with_capacity(k);
            for _ in 0..k {
                list.push(read_form_id(&mut r)?);
            }
            inventory_filters.insert(o, list);
        }
        let sleep_listeners = read_object_list(&mut r)?;
        let stats_listeners = read_object_list(&mut r)?;
        let n = r.len()?;
        let mut animation_listeners = Vec::with_capacity(n);
        for _ in 0..n {
            animation_listeners.push(AnimationListener {
                listener: ObjectId(r.u64()?),
                sender: read_form_id(&mut r)?,
                event: r.str()?,
            });
        }
        let n = r.len()?;
        let mut los_listeners = Vec::with_capacity(n);
        for _ in 0..n {
            los_listeners.push(LosListener {
                listener: ObjectId(r.u64()?),
                viewer: read_form_id(&mut r)?,
                target: read_form_id(&mut r)?,
                single: r.bool()?,
                gain: r.bool()?,
            });
        }
        let n = r.len()?;
        let mut stats = BTreeMap::new();
        for _ in 0..n {
            let k = r.str()?;
            stats.insert(k, r.i32()?);
        }
        let fast_travel = r.bool()?;
        let player_ai_driven = r.bool()?;
        let in_chargen = r.bool()?;
        let beast_form = r.bool()?;
        let words_taught = read_form_set(&mut r)?;
        let words_unlocked = read_form_set(&mut r)?;
        let n = r.len()?;
        let mut achievements = Vec::with_capacity(n);
        for _ in 0..n {
            achievements.push(r.i32()?);
        }
        let last_ridden_horse = read_opt_form_id(&mut r)?;
        let perk_points = r.i32()?;
        let scenes = read_form_set(&mut r)?;
        let n = r.len()?;
        let mut sound_instances = BTreeMap::new();
        for _ in 0..n {
            let id = r.i32()?;
            let sound = read_form_id(&mut r)?;
            let source = read_form_id(&mut r)?;
            sound_instances.insert(id, (sound, source, r.f32()?));
        }
        let next_sound = r.i32()?;
        let n = r.len()?;
        let mut spell_casts = Vec::with_capacity(n);
        for _ in 0..n {
            let spell = read_form_id(&mut r)?;
            let source = read_form_id(&mut r)?;
            spell_casts.push((spell, source, read_opt_form_id(&mut r)?));
        }
        let dialogue = if version == 6 || version >= 8 {
            crate::dialogue::read_dialogue(&mut r)?
        } else {
            crate::dialogue::State::default()
        };
        self.validate_dialogue(&dialogue)?;
        if let Some(session) = &dialogue.session {
            if !actors.contains_key(&session.speaker) || !actors.contains_key(&session.listener) {
                return Err(VmError::State("dialogue cursor without actors".into()));
            }
        }
        let (next_effect, active_effects) = if version == 5 || version >= 9 {
            super::magic::read_magic(&mut r)?
        } else {
            (1, BTreeMap::new())
        };
        for (id, effect) in &active_effects {
            if !objects.contains_key(&Handle::ActiveEffect(*id))
                || !actors.contains_key(&effect.target)
            {
                return Err(VmError::State(
                    "active effect without object or actor".into(),
                ));
            }
            let spell = self
                .spell(effect.spell)
                .map_err(|e| VmError::State(e.to_string()))?;
            let Some(entry) = spell.effects.get(effect.effect_index as usize) else {
                return Err(VmError::State("active effect without spell entry".into()));
            };
            if self.resolve_from(effect.spell, entry.base_effect) != Some(effect.effect) {
                return Err(VmError::State("active effect definition mismatch".into()));
            }
            self.magic_effect(effect.effect)
                .map_err(|e| VmError::State(e.to_string()))?;
        }
        for handle in objects.keys() {
            if let Handle::ActiveEffect(id) = handle {
                if !(version == 5 || version >= 9) || id.0 == 0 || id.0 >= next_effect {
                    return Err(VmError::State("invalid active effect handle".into()));
                }
            }
        }
        let mut actor_templates = if version >= 10 {
            self.read_actor_templates(&mut r, version >= 12)?
        } else {
            BTreeMap::new()
        };
        if version >= 11 {
            super::quest_save::read_quest_aux(&mut r, &mut quests)?;
        }
        if version >= 12 {
            super::item_save::read_item_stacks(&mut r, &mut refs)?;
        }
        let record_overrides = if version >= 13 {
            self.read_record_overrides(&mut r, version)?
        } else {
            BTreeMap::new()
        };
        let image_space = if version >= 16 {
            self.read_image_space(&mut r)?
        } else {
            super::image_space::ImageSpaceState::default()
        };
        r.expect_end()?;
        super::death::validate_pending(&actors, &pending)?;

        for (&id, value) in &record_overrides {
            let Some(actor) = value.temporary_for else {
                continue;
            };
            let choice = actor_templates
                .get_mut(&actor)
                .ok_or_else(|| VmError::State("temporary NPC has no actor choice".into()))?;
            if id.object > next_dynamic
                || refs.contains_key(&id)
                || refs.get(&actor).is_none_or(|r| !r.is_actor)
                || choice.temporary_base.is_some()
                || choice.chain.as_ref().and_then(|c| c.first()) != Some(&value.origin)
                || value.record.find(super::T(b"TPLT")).is_some()
            {
                return Err(VmError::State("inconsistent temporary NPC identity".into()));
            }
            choice.temporary_base = Some(id);
        }

        self.record_overrides = record_overrides;
        self.actor_templates = actor_templates;
        self.leveled_lists = leveled_lists;
        self.locations = locations;
        self.factions = factions;
        self.actor_bases = actor_bases;
        self.cells = cells;
        self.settings = settings;
        self.inventory_filters = inventory_filters;
        self.sleep_listeners = sleep_listeners;
        self.stats_listeners = stats_listeners;
        self.animation_listeners = animation_listeners;
        self.los_listeners = los_listeners;
        self.stats = stats;
        self.fast_travel = fast_travel;
        self.player_ai_driven = player_ai_driven;
        self.in_chargen = in_chargen;
        self.beast_form = beast_form;
        self.words_taught = words_taught;
        self.words_unlocked = words_unlocked;
        self.achievements = achievements;
        self.last_ridden_horse = last_ridden_horse;
        self.perk_points = perk_points;
        self.scenes = scenes;
        self.sound_instances = sound_instances;
        self.next_sound = next_sound;
        self.spell_casts = spell_casts;
        self.dialogue = dialogue;
        self.next_effect = next_effect;
        self.active_effects = active_effects;
        self.image_space = image_space;

        self.objects = objects;
        self.handles = handles;
        self.refs = refs;
        self.actors = actors;
        self.quests = quests;
        self.globals = globals;
        self.form_lists = form_lists;
        self.next_dynamic = next_dynamic;
        self.player = player;
        self.loaded_cell = loaded_cell;
        self.rng = rng;
        self.clock = GameClock {
            real_seconds,
            game_days,
            timescale,
        };
        self.pending = pending;
        self.timers = timers;
        self.notifications = notifications;
        self.message_boxes = message_boxes;
        self.animation_events = animation_events;
        self.player_controls = player_controls;
        self.ini = ini;
        Ok(())
    }
}

pub(crate) fn write_form_id(w: &mut Writer, id: GlobalFormId) {
    w.u16(id.file);
    w.u32(id.object);
}

pub(crate) fn read_form_id(r: &mut Reader<'_>) -> Result<GlobalFormId, VmError> {
    Ok(GlobalFormId {
        file: r.u16()?,
        object: r.u32()?,
    })
}

pub(crate) fn write_opt_form_id(w: &mut Writer, id: Option<GlobalFormId>) {
    match id {
        Some(id) => {
            w.bool(true);
            write_form_id(w, id);
        }
        None => w.bool(false),
    }
}

pub(crate) fn read_opt_form_id(r: &mut Reader<'_>) -> Result<Option<GlobalFormId>, VmError> {
    Ok(if r.bool()? {
        Some(read_form_id(r)?)
    } else {
        None
    })
}

fn write_transform(w: &mut Writer, t: Transform) {
    for v in t.position {
        w.f32(v);
    }
    for v in t.rotation {
        w.f32(v);
    }
}

fn read_transform(r: &mut Reader<'_>) -> Result<Transform, VmError> {
    let mut position = [0f32; 3];
    let mut rotation = [0f32; 3];
    for v in &mut position {
        *v = r.f32()?;
    }
    for v in &mut rotation {
        *v = r.f32()?;
    }
    Ok(Transform { position, rotation })
}

fn write_handle(w: &mut Writer, h: Handle) {
    match h {
        Handle::ActiveEffect(id) => {
            w.u8(2);
            w.u64(id.0);
        }
        Handle::Form(id) => {
            w.u8(0);
            write_form_id(w, id);
        }
        Handle::Alias { quest, id } => {
            w.u8(1);
            write_form_id(w, quest);
            w.u32(id);
        }
    }
}

fn read_handle(r: &mut Reader<'_>) -> Result<Handle, VmError> {
    Ok(match r.u8()? {
        2 => Handle::ActiveEffect(super::magic::EffectId(r.u64()?)),
        0 => Handle::Form(read_form_id(r)?),
        1 => Handle::Alias {
            quest: read_form_id(r)?,
            id: r.u32()?,
        },
        other => return Err(VmError::State(format!("unknown handle tag {other}"))),
    })
}

fn write_ref_state(w: &mut Writer, s: &RefState) {
    write_opt_form_id(w, s.base);
    write_opt_form_id(w, s.cell);
    for v in s.position {
        w.f32(v);
    }
    for v in s.rotation {
        w.f32(v);
    }
    w.f32(s.scale);
    w.bool(s.enabled);
    w.bool(s.deleted);
    w.bool(s.activation_blocked);
    w.bool(s.opened);
    w.bool(s.locked);
    w.i32(s.lock_level);
    w.opt_str(s.display_name.as_deref());
    w.len(s.inventory.len());
    for (item, count) in s.inventory.iter() {
        write_form_id(w, *item);
        w.i32(count);
    }
    w.len(s.linked.len());
    for (kw, r) in &s.linked {
        write_opt_form_id(w, *kw);
        write_form_id(w, *r);
    }
    match s.teleport {
        Some((door, t)) => {
            w.bool(true);
            write_form_id(w, door);
            write_transform(w, t);
        }
        None => w.bool(false),
    }
    w.bool(s.is_actor);
    write_opt_form_id(w, s.owner);
    write_opt_form_id(w, s.location);
    w.bool(s.on_map);
    w.bool(s.fast_travel);
    w.bool(s.destroyed);
    w.bool(s.ignore_friendly_hits);
    w.len(s.anim_floats.len());
    for (k, v) in &s.anim_floats {
        w.str(k);
        w.f32(*v);
    }
    w.len(s.anim_ints.len());
    for (k, v) in &s.anim_ints {
        w.str(k);
        w.i32(*v);
    }
    w.len(s.anim_bools.len());
    for (k, v) in &s.anim_bools {
        w.str(k);
        w.bool(*v);
    }
    w.f32(s.alpha);
}

fn read_ref_state(r: &mut Reader<'_>) -> Result<RefState, VmError> {
    let base = read_opt_form_id(r)?;
    let cell = read_opt_form_id(r)?;
    let mut position = [0f32; 3];
    let mut rotation = [0f32; 3];
    for v in &mut position {
        *v = r.f32()?;
    }
    for v in &mut rotation {
        *v = r.f32()?;
    }
    let scale = r.f32()?;
    let enabled = r.bool()?;
    let deleted = r.bool()?;
    let activation_blocked = r.bool()?;
    let opened = r.bool()?;
    let locked = r.bool()?;
    let lock_level = r.i32()?;
    let display_name = r.opt_str()?;
    let n = r.len()?;
    let mut inventory = crate::inventory::Inventory::default();
    for _ in 0..n {
        let item = read_form_id(r)?;
        if inventory.contains(item) {
            return Err(VmError::State("duplicate inventory item".into()));
        }
        inventory
            .restore(item, r.i32()?, vec![])
            .map_err(|error| VmError::State(error.to_string()))?;
    }
    let n = r.len()?;
    let mut linked = Vec::with_capacity(n);
    for _ in 0..n {
        let kw = read_opt_form_id(r)?;
        linked.push((kw, read_form_id(r)?));
    }
    let teleport = if r.bool()? {
        Some((read_form_id(r)?, read_transform(r)?))
    } else {
        None
    };
    let is_actor = r.bool()?;
    let owner = read_opt_form_id(r)?;
    let location = read_opt_form_id(r)?;
    let on_map = r.bool()?;
    let fast_travel = r.bool()?;
    let destroyed = r.bool()?;
    let ignore_friendly_hits = r.bool()?;
    let mut anim_floats = BTreeMap::new();
    let n = r.len()?;
    for _ in 0..n {
        let k = r.str()?;
        anim_floats.insert(k, r.f32()?);
    }
    let mut anim_ints = BTreeMap::new();
    let n = r.len()?;
    for _ in 0..n {
        let k = r.str()?;
        anim_ints.insert(k, r.i32()?);
    }
    let mut anim_bools = BTreeMap::new();
    let n = r.len()?;
    for _ in 0..n {
        let k = r.str()?;
        anim_bools.insert(k, r.bool()?);
    }
    let alpha = r.f32()?;
    Ok(RefState {
        base,
        cell,
        position,
        rotation,
        scale,
        enabled,
        deleted,
        activation_blocked,
        opened,
        locked,
        lock_level,
        display_name,
        inventory,
        linked,
        teleport,
        is_actor,
        owner,
        location,
        on_map,
        fast_travel,
        destroyed,
        ignore_friendly_hits,
        anim_floats,
        anim_ints,
        anim_bools,
        alpha,
    })
}

fn write_actor_state(w: &mut Writer, a: &ActorState) {
    a.values.write(w);
    w.bool(a.dead);
    w.bool(a.in_combat);
    w.bool(a.teammate);
    w.bool(a.ghost);
    w.bool(a.essential);
    w.bool(a.unconscious);
    w.bool(a.doing_favor);
    w.len(a.factions.len());
    for (id, rank) in a.factions.iter().collect::<BTreeMap<_, _>>() {
        write_form_id(w, *id);
        w.i32(*rank);
    }
    w.len(a.spells.len());
    for id in &a.spells {
        write_form_id(w, *id);
    }
    w.len(a.equipped.len());
    for id in &a.equipped {
        write_form_id(w, *id);
    }
    w.bool(a.protected);
    w.u32(a.level);
    w.bool(a.race.is_some());
    if let Some(race) = a.race {
        write_form_id(w, race);
    }
    crate::ai::write_state(w, &a.ai);
    write_form_set(w, &a.perks);
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
        w.bool(flag);
    }
    w.len(a.relationships.len());
    for (id, rank) in &a.relationships {
        write_form_id(w, *id);
        w.i32(*rank);
    }
    for id in [
        a.combat_target,
        a.crime_faction,
        a.outfit,
        a.sleep_outfit,
        a.killer,
    ] {
        write_opt_form_id(w, id);
    }
    w.len(a.graph_floats.len());
    for (k, v) in &a.graph_floats {
        w.str(k);
        w.f32(*v);
    }
    for flag in [
        a.allow_flying,
        a.deferred_kill,
        a.vampire,
        a.werewolf,
        a.no_bleedout_recovery,
        a.ai_disabled,
    ] {
        w.bool(flag);
    }
    w.i32(a.critical_stage);
    write_opt_form_id(w, a.eye_texture);
    write_opt_form_id(w, a.ash_pile);
    write_form_set(w, &a.shouts);
    write_opt_form_id(w, a.equipped_shout);
}

fn write_form_set(w: &mut Writer, set: &BTreeSet<GlobalFormId>) {
    w.len(set.len());
    for id in set {
        write_form_id(w, *id);
    }
}

fn read_form_set(r: &mut Reader<'_>) -> Result<BTreeSet<GlobalFormId>, VmError> {
    let n = r.len()?;
    let mut out = BTreeSet::new();
    for _ in 0..n {
        out.insert(read_form_id(r)?);
    }
    Ok(out)
}

fn write_object_list(w: &mut Writer, list: &[ObjectId]) {
    w.len(list.len());
    for o in list {
        w.u64(o.0);
    }
}

fn read_object_list(r: &mut Reader<'_>) -> Result<Vec<ObjectId>, VmError> {
    let n = r.len()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(ObjectId(r.u64()?));
    }
    Ok(out)
}

fn read_actor_state(r: &mut Reader<'_>, version: u16) -> Result<ActorState, VmError> {
    let mut a = ActorState {
        values: crate::actor_values::Values::read(r, version)?,
        ..ActorState::default()
    };
    a.dead = r.bool()?;
    a.in_combat = r.bool()?;
    a.teammate = r.bool()?;
    a.ghost = r.bool()?;
    a.essential = r.bool()?;
    a.unconscious = r.bool()?;
    a.doing_favor = r.bool()?;
    let n = r.len()?;
    for _ in 0..n {
        let id = read_form_id(r)?;
        a.factions.insert(id, r.i32()?);
    }
    let n = r.len()?;
    for _ in 0..n {
        a.spells.insert(read_form_id(r)?);
    }
    let n = r.len()?;
    for _ in 0..n {
        a.equipped.insert(read_form_id(r)?);
    }
    a.protected = r.bool()?;
    a.level = r.u32()?;
    if r.bool()? {
        a.race = Some(read_form_id(r)?);
    }
    a.ai = crate::ai::read_state(r)?;
    a.perks = read_form_set(r)?;
    a.alerted = r.bool()?;
    a.sneaking = r.bool()?;
    a.weapon_drawn = r.bool()?;
    a.restrained = r.bool()?;
    a.arrested = r.bool()?;
    a.attack_on_sight = r.bool()?;
    a.dont_move = r.bool()?;
    a.allow_pc_dialogue = r.bool()?;
    let n = r.len()?;
    for _ in 0..n {
        let id = read_form_id(r)?;
        a.relationships.insert(id, r.i32()?);
    }
    a.combat_target = read_opt_form_id(r)?;
    a.crime_faction = read_opt_form_id(r)?;
    a.outfit = read_opt_form_id(r)?;
    a.sleep_outfit = read_opt_form_id(r)?;
    a.killer = read_opt_form_id(r)?;
    let n = r.len()?;
    for _ in 0..n {
        let k = r.str()?;
        a.graph_floats.insert(k, r.f32()?);
    }
    a.allow_flying = r.bool()?;
    a.deferred_kill = r.bool()?;
    a.vampire = r.bool()?;
    a.werewolf = r.bool()?;
    a.no_bleedout_recovery = r.bool()?;
    a.ai_disabled = r.bool()?;
    a.critical_stage = r.i32()?;
    a.eye_texture = read_opt_form_id(r)?;
    a.ash_pile = read_opt_form_id(r)?;
    a.shouts = read_form_set(r)?;
    a.equipped_shout = read_opt_form_id(r)?;
    Ok(a)
}

fn write_objective(w: &mut Writer, o: ObjectiveState) {
    w.bool(o.displayed);
    w.bool(o.completed);
    w.bool(o.failed);
}

fn read_objective(r: &mut Reader<'_>) -> Result<ObjectiveState, VmError> {
    Ok(ObjectiveState {
        displayed: r.bool()?,
        completed: r.bool()?,
        failed: r.bool()?,
    })
}

fn write_quest_mutable(w: &mut Writer, q: &QuestState) {
    w.bool(q.running);
    w.bool(q.completed);
    w.bool(q.failed);
    w.bool(q.active);
    w.u16(q.current);
    w.u16(q.highest);
    w.len(q.done.len());
    for s in &q.done {
        w.u16(*s);
    }
    w.len(q.objectives.len());
    for (idx, o) in &q.objectives {
        w.u16(*idx);
        write_objective(w, *o);
    }
    w.len(q.aliases.len());
    for (id, target) in q.aliases.iter().collect::<BTreeMap<_, _>>() {
        w.u32(*id);
        write_opt_form_id(w, *target);
    }
    w.len(q.journal.globals().map_or(0, BTreeMap::len));
    for (id, v) in q.journal.globals().into_iter().flatten() {
        write_form_id(w, *id);
        w.f32(*v);
    }
}
