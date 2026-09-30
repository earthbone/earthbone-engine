// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! The game runtime for the Skyrim open engine: a load order becomes a
//! [`World`] (the form table plus run-time state), scripts named by `VMAD`
//! subrecords are attached to VM objects with their property values, the
//! base game's natives are registered, and a clock drives the VM.
//!
//! Round 2 of `ENGINE-PLAN.md`. The design rule from section 3 of the
//! research map holds: every system reads forms by resolved FormID from the
//! load order, so plugins, xEdit output and patchers keep their meaning.
//!
//! ```no_run
//! use soe_esm::{LoadOrder, Plugin};
//! use soe_game::Game;
//!
//! let mut order = LoadOrder::new();
//! order.add("Skyrim.esm", Plugin::parse(&std::fs::read("Data/Skyrim.esm").unwrap()).unwrap()).unwrap();
//! let mut game = Game::new(order);
//! game.load_script(&std::fs::read("Data/Scripts/MQ101.pex").unwrap()).unwrap();
//! game.start();
//! game.tick(1.0 / 60.0);
//! ```

pub mod actor_values;
pub mod ai;
pub mod appearance;
pub mod class;
pub mod clock;
pub mod combat;
pub mod conditions;
pub mod coverage;
pub mod crafting;
pub mod crime;
pub mod debug;
pub mod detection;
pub mod determinism;
pub mod dialogue;
pub mod footsteps;
mod game_settings;
pub mod harness;
pub mod inventory;
pub mod lockpicking;
pub mod mcm;
pub mod moddata;
pub mod natives;
pub mod npc;
pub mod package;
pub mod perks;
pub mod progression;
pub mod quest;
pub mod race;
pub mod save;
pub mod save_generations;
pub mod services;
pub mod sound;
pub mod validation_runner;
pub mod vanilla_save;
pub mod vmad;
pub mod world;

use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use soe_esm::{GlobalFormId, LoadOrder, RecordRef, Tag};
use soe_papyrus::value::ElementKind;
use soe_papyrus::{ArrayRef, ObjectId, ThreadId, Value, Vm, VmError};

pub use clock::GameClock;
pub use natives::Registered;
pub use world::StoryError;
pub use world::{Handle, Pending, RefPlacement, World};

use vmad::{ObjectRef, PropertyValue, ScriptEntry, Vmad};

/// What [`Game::bind`] found in the load order's `VMAD` subrecords.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BindReport {
    pub forms_with_scripts: usize,
    pub scripts_attached: usize,
    /// `(script, form)` pairs whose PEX is not loaded.
    pub scripts_missing: Vec<(String, GlobalFormId)>,
    pub properties_set: usize,
    /// `(script, property)` the loaded PEX does not declare.
    pub properties_missing: Vec<(String, String)>,
    /// `(script, property)` for a full property with a getter and no setter,
    /// counted apart from `properties_missing` since the property is real,
    /// the form just has nothing to set.
    pub properties_read_only: Vec<(String, String)>,
    pub quests_started: usize,
}

/// What [`Game::load_scripts_from_data`] found building the load order's
/// [`soe_bsa::Vfs`] and loading every `.pex` it answered under `scripts/`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptLoadReport {
    /// Archives added to the view, one entry per plugin that shipped one.
    pub archives_loaded: Vec<PathBuf>,
    /// `.pex` paths the view answered under `scripts/`, loose files and
    /// archive records combined, loose beating archived the way `Vfs` always
    /// resolves it.
    pub scripts_found: usize,
    pub scripts_loaded: usize,
    /// `(path, error)` for a script the view had but the VM would not parse,
    /// or an archive the view could not open.
    pub scripts_failed: Vec<(String, String)>,
}

/// Script and string-table archives: the split Misc and Interface archives,
/// and a plugin's combined archive. Matched
/// case-insensitively since installs spell plugin names either way.
fn data_archives_for(plugin_name: &str, data_dir: &Path) -> Vec<PathBuf> {
    let stem = Path::new(plugin_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if stem.is_empty() {
        return Vec::new();
    }
    let wanted = [
        format!("{stem} - interface.bsa"),
        format!("{stem} - misc.bsa"),
        format!("{stem}.bsa"),
    ];
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(data_dir) else {
        return found;
    };
    for entry in entries.flatten() {
        if !soe_bsa::is_bsa_or_ba2(&entry.path()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if wanted.contains(&name) {
            found.push(entry.path());
        }
    }
    found.sort();
    found
}

pub struct Game {
    pub vm: Vm,
    world: Rc<RefCell<World>>,
    pub registered: Registered,
    bound: Option<BindReport>,
    /// Instructions per pump round before the loop yields.
    pub budget: usize,
    /// One hash per tick once [`Game::record_hashes`] turned it on.
    hash_log: Option<Vec<determinism::TickHash>>,
    mcm_registered: Vec<GlobalFormId>,
    mcm_active: Option<GlobalFormId>,
    /// Actors that were in greeting range on the previous gameplay tick.
    greeting_range_actors: BTreeSet<soe_esm::GlobalFormId>,
    /// Hands currently held by direct player or scripted cast input.
    active_hand_casts: BTreeSet<(soe_esm::GlobalFormId, combat::Hand)>,
}

impl Game {
    /// Evaluate one objective target's QSTA conditions against the current world and VM.
    ///
    /// # Errors
    ///
    /// Returns an error when a target condition is unsupported or lacks runtime context.
    pub fn objective_target_conditions_met(
        &mut self,
        quest: GlobalFormId,
        objective: u16,
        target_index: usize,
    ) -> Result<bool, conditions::Error> {
        let mut world = self.world.borrow_mut();
        let Some(state) = world.quest_state(quest) else {
            return Ok(false);
        };
        let Some(target) = state
            .def
            .objective(objective)
            .and_then(|definition| definition.targets.get(target_index))
        else {
            return Ok(false);
        };
        let Some(alias) = u32::try_from(target.alias).ok() else {
            return Ok(false);
        };
        let target_ref = state.aliases.get(&alias).copied().flatten();
        let conditions = target.conditions.clone();
        let context = conditions::Context {
            owner: quest,
            subject: world.player(),
            target: target_ref,
            quest: Some(quest),
            tempering_enchanted: None,
            skill_use: None,
            light_level: None,
        };
        conditions::evaluate(&mut world, &self.vm, context, &conditions)
    }

    pub fn new(order: LoadOrder) -> Game {
        Self::from_shared_order(std::sync::Arc::new(order))
    }

    /// Use the immutable form table already opened by the host.
    pub fn from_shared_order(order: std::sync::Arc<LoadOrder>) -> Game {
        let world = Rc::new(RefCell::new(World::from_shared_order(order)));
        let mut vm = Vm::new();
        let registered = natives::register(&mut vm, &world);
        Game {
            vm,
            world,
            registered,
            bound: None,
            budget: 100_000,
            hash_log: None,
            mcm_registered: Vec::new(),
            mcm_active: None,
            greeting_range_actors: BTreeSet::new(),
            active_hand_casts: BTreeSet::new(),
        }
    }

    /// Register natives from another provider, an extender in the PAPER
    /// mould. First registration wins, and the returned list names every
    /// function this provider lost.
    pub fn add_natives(
        &mut self,
        provider: impl FnOnce(&mut Vm, &Rc<RefCell<World>>),
    ) -> Vec<(String, String, bool)> {
        let before = self.vm.natives.duplicates.len();
        provider(&mut self.vm, &self.world);
        self.vm.natives.duplicates[before..].to_vec()
    }

    pub fn load_script(&mut self, pex: &[u8]) -> Result<(), VmError> {
        self.vm.load_bytes(pex)
    }

    /// Build the load order's data view over `data_dir` (a `soe_bsa::Vfs`)
    /// and load every `.pex` it answers under `scripts/` into the VM.
    ///
    /// An install ships its compiled scripts either loose under
    /// `Data/Scripts` or packed inside an archive named after the plugin
    /// that owns them (`Skyrim - Misc.bsa`, or a DLC's own `<stem>.bsa`).
    /// The view resolves both the same way the game does: loose files beat
    /// archived ones, and among archives the one added last wins, so this
    /// mirrors `Game::bind` reading VMAD script names against whatever
    /// actually got loaded here rather than assuming a loose folder exists.
    pub fn load_scripts_from_data(&mut self, data_dir: &Path) -> ScriptLoadReport {
        self.load_scripts_from_data_language(data_dir, "english")
    }

    /// Load scripts and localized strings for a selected Bethesda language.
    pub fn load_scripts_from_data_language(
        &mut self,
        data_dir: &Path,
        language: &str,
    ) -> ScriptLoadReport {
        let mut report = ScriptLoadReport::default();
        let mut vfs = soe_bsa::Vfs::new();
        let plugin_names: Vec<String> = self
            .world
            .borrow()
            .order()
            .files()
            .iter()
            .map(|f| f.name.clone())
            .collect();
        for name in &plugin_names {
            for path in data_archives_for(name, data_dir) {
                match vfs.add_archive_path(&path) {
                    Ok(_) => report.archives_loaded.push(path),
                    Err(e) => report
                        .scripts_failed
                        .push((path.display().to_string(), e.to_string())),
                }
            }
        }
        if data_dir.is_dir() {
            let _ = vfs.add_loose_root(data_dir);
        }
        if let Err(error) = self
            .world
            .borrow_mut()
            .load_localized_strings(&vfs, language)
        {
            report
                .scripts_failed
                .push(("localized strings".into(), error.to_string()));
        }
        let loaded = self.load_scripts_from_vfs(&vfs);
        report.scripts_found = loaded.scripts_found;
        report.scripts_loaded = loaded.scripts_loaded;
        report.scripts_failed.extend(loaded.scripts_failed);
        report
    }

    /// Load scripts through the host's existing archive and loose-file overlay.
    pub fn load_scripts_from_vfs(&mut self, vfs: &soe_bsa::Vfs) -> ScriptLoadReport {
        let mut report = ScriptLoadReport::default();
        let mut pex: Vec<String> = vfs
            .list("scripts")
            .into_iter()
            .filter(|p| Path::new(p).extension().is_some_and(|e| e == "pex"))
            .map(str::to_string)
            .collect();
        pex.sort();
        report.scripts_found = pex.len();
        for path in pex {
            match vfs.read(&path) {
                Ok(bytes) => match self.load_script(&bytes) {
                    Ok(()) => report.scripts_loaded += 1,
                    Err(e) => report.scripts_failed.push((path, e.to_string())),
                },
                Err(e) => report.scripts_failed.push((path, e.to_string())),
            }
        }
        report
    }

    pub fn world(&self) -> Ref<'_, World> {
        self.world.borrow()
    }

    pub fn world_mut(&self) -> RefMut<'_, World> {
        self.world.borrow_mut()
    }

    /// Route an authored Story Manager event through the shared runtime.
    ///
    /// # Errors
    /// Returns malformed record, condition, or reset data errors.
    pub fn send_story_event(
        &mut self,
        event: &crate::quest::state::StoryEvent,
    ) -> Result<Vec<GlobalFormId>, StoryError> {
        let vm = &mut self.vm;
        let mut world = self.world.borrow_mut();
        world.send_story_event(vm, event)
    }

    /// Resolve a vendor faction's current service for an actor.
    ///
    /// # Errors
    /// Returns malformed vendor data, unsupported faction conditions or
    /// invalid merchandise errors from [`services::resolve_vendor_service`].
    pub fn resolve_vendor_service(
        &mut self,
        faction: GlobalFormId,
        merchant: GlobalFormId,
    ) -> Result<Option<services::VendorService>, services::VendorError> {
        let vm = &self.vm;
        let mut world = self.world.borrow_mut();
        services::resolve_vendor_service(&mut world, vm, faction, merchant)
    }

    /// Buy an exact quantity from an available vendor, exchanging gold and
    /// merchandise as one inventory transaction.
    ///
    /// `unit_price` is the already-adjusted menu price per item. A caller
    /// applies the barter multiplier and rounding before calling this method.
    ///
    /// # Errors
    /// Returns an error when vendor service data is invalid or unavailable,
    /// the item is not listed, the price is invalid, or either holder cannot
    /// complete the full exchange.
    pub fn buy_vendor_item(
        &mut self,
        faction: GlobalFormId,
        merchant: GlobalFormId,
        item: GlobalFormId,
        count: i32,
        unit_price: i32,
    ) -> Result<(), services::TradeError> {
        if count <= 0 || unit_price <= 0 {
            return Err(services::TradeError::Price);
        }
        let service = self
            .resolve_vendor_service(faction, merchant)?
            .filter(|service| service.available)
            .ok_or(services::TradeError::Unavailable)?;
        if service.merchandise.count(item) < count {
            return Err(services::TradeError::Stock);
        }
        let total = count
            .checked_mul(unit_price)
            .ok_or(services::TradeError::Price)?;
        let currency = self.form("Gold001").unwrap_or(GlobalFormId {
            file: 0,
            object: 0xF,
        });
        let player = self.player();
        let vm = &mut self.vm;
        self.world.borrow_mut().trade_items_notified(
            service.merchant_container,
            item,
            count,
            player,
            currency,
            total,
            &mut |name, class| vm.create_object_of(name, class),
        )?;
        self.run_until_idle(64);
        Ok(())
    }

    /// Return the selected damage difficulty tier.
    #[must_use]
    pub fn difficulty(&self) -> combat::Difficulty {
        self.world.borrow().difficulty()
    }

    /// Select the damage difficulty tier.
    pub fn set_difficulty(&mut self, difficulty: combat::Difficulty) {
        self.world.borrow_mut().set_difficulty(difficulty);
    }

    pub fn shared_world(&self) -> Rc<RefCell<World>> {
        self.world.clone()
    }

    pub fn form(&self, editor_id: &str) -> Option<GlobalFormId> {
        self.world.borrow().form(editor_id)
    }

    pub fn object_of(&mut self, handle: Handle) -> ObjectId {
        let vm = &mut self.vm;
        self.world
            .borrow_mut()
            .object_of(handle, &mut |name, class| vm.create_object_of(name, class))
    }

    pub fn object_of_form(&mut self, id: GlobalFormId) -> ObjectId {
        self.object_of(Handle::Form(id))
    }

    /// Attach every script the load order names and fill its properties.
    /// Runs once, later calls return the first report.
    pub fn bind(&mut self) -> BindReport {
        if let Some(r) = &self.bound {
            return r.clone();
        }
        let mut report = BindReport::default();
        let carriers: Vec<(GlobalFormId, RecordRef, Tag, Vmad)> = {
            let world = self.world.borrow();
            let order = world.order();
            let mut out = Vec::new();
            for (file_index, file) in order.files().iter().enumerate() {
                for (i, rec) in file.plugin.records.iter().enumerate() {
                    if rec.tag == Tag::new(b"MGEF") {
                        continue;
                    }
                    let Some(raw) = rec.find(Tag::new(b"VMAD")) else {
                        continue;
                    };
                    let id = order.resolve(file_index, rec.form_id);
                    let rref = RecordRef {
                        file: file_index,
                        record: i,
                    };
                    if order.winner_ref(id) != Some(rref) {
                        continue;
                    }
                    if let Ok(vmad) = Vmad::parse(raw, rec.tag) {
                        out.push((id, rref, rec.tag, vmad));
                    }
                }
            }
            out
        };
        let _bind_span = soe_trace::Span::with_payload("game.bind", "game", carriers.len() as u64);
        for (id, rref, _tag, vmad) in carriers {
            let _span = soe_trace::Span::new("game.bind_form", "game");
            report.forms_with_scripts += 1;
            let object = self.object_of(Handle::Form(id));
            self.attach_all(object, id, rref.file, &vmad.scripts, &mut report);
            for alias in &vmad.aliases {
                let alias_id = u32::try_from(alias.object.alias).unwrap_or(u32::MAX);
                let handle = Handle::Alias {
                    quest: id,
                    id: alias_id,
                };
                let object = self.object_of(handle);
                self.attach_all(object, id, rref.file, &alias.scripts, &mut report);
            }
        }
        self.bound = Some(report.clone());
        report
    }

    fn attach_all(
        &mut self,
        object: ObjectId,
        owner: GlobalFormId,
        file: usize,
        scripts: &[ScriptEntry],
        report: &mut BindReport,
    ) {
        for entry in scripts {
            if entry.status == 3 {
                continue;
            }
            if self.vm.attach(object, &entry.name).is_err() {
                report.scripts_missing.push((entry.name.clone(), owner));
                continue;
            }
            report.scripts_attached += 1;
            for p in &entry.properties {
                if p.status == 3 {
                    continue;
                }
                let value = self.property_value(owner, file, &p.value);
                match self.vm.set_property(object, &entry.name, &p.name, value) {
                    Ok(()) => report.properties_set += 1,
                    Err(VmError::ReadOnlyProperty { .. }) => report
                        .properties_read_only
                        .push((entry.name.clone(), p.name.clone())),
                    Err(_) => report
                        .properties_missing
                        .push((entry.name.clone(), p.name.clone())),
                }
            }
        }
    }

    fn object_ref_value(&mut self, owner: GlobalFormId, file: usize, o: ObjectRef) -> Value {
        if o.form_id == 0 {
            return Value::None;
        }
        let target = self.world.borrow().order().resolve(file, o.form_id);
        let handle = match u32::try_from(o.alias) {
            Ok(alias) => Handle::Alias {
                quest: target,
                id: alias,
            },
            Err(_) => Handle::Form(target),
        };
        let exists = self.world.borrow().exists(target);
        if !exists {
            return Value::None;
        }
        let _ = owner;
        Value::Object(self.object_of(handle))
    }

    fn property_value(&mut self, owner: GlobalFormId, file: usize, v: &PropertyValue) -> Value {
        fn array(kind: ElementKind, items: Vec<Value>) -> Value {
            let arr = ArrayRef::new(kind, 0);
            *arr.items.borrow_mut() = items;
            Value::Array(arr)
        }
        match v {
            PropertyValue::Object(o) => self.object_ref_value(owner, file, *o),
            PropertyValue::Str(s) => Value::string(s),
            PropertyValue::Int(i) => Value::Int(*i),
            PropertyValue::Float(f) => Value::Float(*f),
            PropertyValue::Bool(b) => Value::Bool(*b),
            PropertyValue::Objects(v) => {
                let items = v
                    .iter()
                    .map(|o| self.object_ref_value(owner, file, *o))
                    .collect();
                array(ElementKind::Object, items)
            }
            PropertyValue::Strs(v) => array(
                ElementKind::Str,
                v.iter().map(|s| Value::string(s)).collect(),
            ),
            PropertyValue::Ints(v) => {
                array(ElementKind::Int, v.iter().map(|i| Value::Int(*i)).collect())
            }
            PropertyValue::Floats(v) => array(
                ElementKind::Float,
                v.iter().map(|f| Value::Float(*f)).collect(),
            ),
            PropertyValue::Bools(v) => array(
                ElementKind::Bool,
                v.iter().map(|b| Value::Bool(*b)).collect(),
            ),
        }
    }

    /// Bind scripts, start every quest flagged Start Game Enabled, and run
    /// the VM until it settles.
    /// Reads every mod data file under `roots` and applies it, lowest root
    /// first the way a profile layers its directories. Call this before
    /// [`Game::start`]: both distributors run once at load, before any script,
    /// which is what the plugins do. See [`crate::moddata`].
    pub fn apply_mod_data(&mut self, roots: &[std::path::PathBuf]) -> moddata::ModDataReport {
        crate::moddata::apply(&mut self.world.borrow_mut(), roots)
    }

    /// The same, from a resolved profile, which already knows its data roots
    /// in layering order.
    pub fn apply_mod_data_from(
        &mut self,
        loadout: &soe_profile::Loadout,
    ) -> moddata::ModDataReport {
        self.apply_mod_data(&loadout.data_roots)
    }

    pub fn start(&mut self) -> BindReport {
        let mut report = self.bind();
        let quests: Vec<GlobalFormId> = self
            .world
            .borrow()
            .order()
            .winners_of(Tag::new(b"QUST"))
            .map(|(id, _)| id)
            .collect();
        for id in quests {
            let starts = self
                .world
                .borrow_mut()
                .quest_state(id)
                .is_some_and(|q| q.def.starts_enabled());
            if starts && self.start_quest(id) {
                report.quests_started += 1;
            }
        }
        self.run_until_idle(64);
        self.mcm_configs();
        self.bound = Some(report.clone());
        report
    }

    pub fn start_quest(&mut self, id: GlobalFormId) -> bool {
        let vm = &mut self.vm;
        self.world
            .borrow_mut()
            .start_quest(id, &mut |n, c| vm.create_object_of(n, c))
    }

    pub fn set_stage(&mut self, id: GlobalFormId, stage: u16) -> bool {
        let vm = &mut self.vm;
        self.world
            .borrow_mut()
            .set_stage(id, stage, &mut |n, c| vm.create_object_of(n, c))
    }

    /// Queue the activation without running the VM. `activate` is this
    /// followed by [`Game::run_until_idle`], split out so a caller can save
    /// state with the `OnActivate` event still queued and unresolved.
    pub fn activate_no_settle(&mut self, target: GlobalFormId, activator: GlobalFormId) -> bool {
        let vm = &mut self.vm;
        self.world
            .borrow_mut()
            .activate(target, activator, true, &mut |n, c| {
                vm.create_object_of(n, c)
            })
    }

    /// Apply a host-confirmed ordinary melee hit, then run its script events.
    ///
    /// # Errors
    /// See World::melee_hit for the current supported contact rules.
    pub fn melee_hit(&mut self, hit: combat::MeleeHit) -> Result<combat::HitResult, combat::Error> {
        let (outgoing, incoming) = if hit.is_bash {
            (Vec::new(), Vec::new())
        } else {
            let vm = &self.vm;
            let mut world = self.world.borrow_mut();
            let outgoing_context = perks::EntryPointContext {
                tabs: vec![hit.attacker, hit.weapon, hit.target],
                target: Some(hit.target),
                quest: None,
                skill_use: None,
            };
            let incoming_context = perks::EntryPointContext {
                tabs: vec![hit.target, hit.attacker, hit.weapon],
                target: Some(hit.attacker),
                quest: None,
                skill_use: None,
            };
            (
                perks::entry_point_invocations(
                    &mut world,
                    vm,
                    hit.attacker,
                    35,
                    &outgoing_context,
                )?,
                perks::entry_point_invocations(&mut world, vm, hit.target, 36, &incoming_context)?,
            )
        };
        let vm = &mut self.vm;
        let result =
            self.world
                .borrow_mut()
                .melee_hit(hit, outgoing, incoming, &mut |name, class| {
                    vm.create_object_of(name, class)
                })?;
        self.run_until_idle(64);
        Ok(result)
    }

    /// Consume one equipped arrow or bolt and return the host's flight request.
    ///
    /// # Errors
    /// Invalid equipment, records, launch vectors or inventory state.
    pub fn release_projectile(
        &mut self,
        attacker: GlobalFormId,
        weapon: GlobalFormId,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<combat::FlightRequest, combat::Error> {
        let vm = &mut self.vm;
        self.world.borrow_mut().release_projectile(
            attacker,
            weapon,
            origin,
            direction,
            &mut |name, class| vm.create_object_of(name, class),
        )
    }

    /// Begin a costed cast from an actor's equipped hand spell.
    ///
    /// Repeated begins while that hand remains held do not request another
    /// cast. Fire-and-forget spells execute on the initial press, while
    /// concentration spells remain active until the hand is released.
    ///
    /// # Errors
    /// Returns the magic validation, delivery, or magicka error for the spell.
    pub fn begin_hand_cast(
        &mut self,
        actor: soe_esm::GlobalFormId,
        hand: combat::Hand,
        target: Option<soe_esm::GlobalFormId>,
    ) -> Result<bool, world::magic::MagicError> {
        let key = (actor, hand);
        if self.active_hand_casts.contains(&key) {
            return Ok(false);
        }
        let spell = self.world.borrow_mut().hand_spell(actor, hand);
        let Some(spell) = spell else {
            return Ok(false);
        };
        let vm = &mut self.vm;
        let result = {
            let mut world = self.world.borrow_mut();
            if world.spell(spell)?.data.cast_type == 2 {
                world
                    .begin_concentration_cast(spell, actor, actor, target, &mut |name, class| {
                        vm.create_object_of(name, class)
                    })
                    .map(|()| Vec::new())
            } else {
                world.cast_charged_spell(spell, actor, actor, target, &mut |name, class| {
                    vm.create_object_of(name, class)
                })
            }
        };
        match result {
            Ok(_) => {
                self.active_hand_casts.insert(key);
                self.run_until_idle(64);
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    /// Release a casting hand after its input is lifted.
    #[must_use]
    pub fn end_hand_cast(&mut self, actor: soe_esm::GlobalFormId, hand: combat::Hand) -> bool {
        let was_active = self.active_hand_casts.remove(&(actor, hand));
        let stopped_concentration = {
            let mut world = self.world.borrow_mut();
            world.is_concentration_casting(actor) && world.release_concentration_cast()
        };
        was_active || stopped_concentration
    }

    /// Activate a reference the way a player click would, then settle.
    pub fn activate(&mut self, target: GlobalFormId, activator: GlobalFormId) -> bool {
        let ok = self.activate_no_settle(target, activator);
        self.run_until_idle(64);
        ok
    }

    /// Activate through the usual default processing and event path, retaining
    /// the source cell until the host replaces its resources and settles unload.
    pub fn activate_deferred(&mut self, target: GlobalFormId, activator: GlobalFormId) -> bool {
        let vm = &mut self.vm;
        let ok = self.world.borrow_mut().activate_with_unload(
            target,
            activator,
            true,
            true,
            &mut |n, c| vm.create_object_of(n, c),
        );
        self.run_until_idle(64);
        ok
    }

    /// Apply a lockpick result supplied by the host interaction UI.
    ///
    /// A broken pick is removed from inventory. Successful picks unlock the
    /// reference through the shared lock state and run its lock event.
    pub fn resolve_lockpick(
        &mut self,
        target: GlobalFormId,
        activator: GlobalFormId,
        success: bool,
    ) -> bool {
        let vm = &mut self.vm;
        let unlocked = self.world.borrow_mut().resolve_lockpick(
            target,
            activator,
            success,
            &mut |name, class| vm.create_object_of(name, class),
        );
        self.run_until_idle(64);
        unlocked
    }

    /// Complete player sleep at a furniture reference and deliver its sleep events.
    pub fn sleep_player_at_furniture(
        &mut self,
        furniture: GlobalFormId,
        elapsed_hours: f64,
        interrupted: bool,
    ) -> bool {
        let slept =
            self.world
                .borrow_mut()
                .sleep_at_furniture(furniture, elapsed_hours, interrupted);
        if slept {
            self.run_until_idle(64);
        }
        slept
    }

    /// Move an inventory item between two references through the same notified
    /// transaction that Papyrus `RemoveItem` uses. A rejected transfer leaves
    /// both holders unchanged.
    pub fn transfer_item(
        &mut self,
        source: GlobalFormId,
        item: GlobalFormId,
        count: i32,
        target: GlobalFormId,
    ) -> i32 {
        let vm = &mut self.vm;
        let moved = self.world.borrow_mut().remove_item_notified(
            source,
            item,
            count,
            Some(target),
            &mut |name, class| vm.create_object_of(name, class),
        );
        self.run_until_idle(64);
        moved
    }

    /// Deliver an event to a form's scripts and settle.
    pub fn send_event(&mut self, id: GlobalFormId, name: &str, args: &[Value]) -> Vec<ThreadId> {
        let object = self.object_of_form(id);
        let threads = self.vm.send_event(object, name, args);
        self.run_until_idle(64);
        threads
    }

    /// Resume mapped quest story events whose handlers are loaded.
    ///
    /// Saved `SCPT` contexts resume through `OnStoryScript`. Their mapped
    /// arguments remain on the quest if the script has not been loaded yet or
    /// a mapped reference is unresolved.
    ///
    /// # Errors
    /// This operation currently has no error path.
    pub fn resume_vanilla_quest_story_events(&mut self) -> usize {
        let mut resumed = 0;
        let events = self.world.borrow().saved_story_events();
        for (id, event) in events {
            if event.code != *b"SCPT" || event.parameters.len() != 6 {
                continue;
            }
            let (object, args) = {
                let vm = &mut self.vm;
                let mut world = self.world.borrow_mut();
                if event.parameters.iter().any(|parameter| match parameter {
                    crate::quest::state::StoryParameter::Reference {
                        reference: Some(reference),
                        ..
                    } => !world.exists(*reference),
                    _ => false,
                }) {
                    continue;
                }
                let mut args = Vec::with_capacity(event.parameters.len());
                for parameter in event.parameters {
                    args.push(match parameter {
                        crate::quest::state::StoryParameter::Reference { reference, .. } => world
                            .form_value(reference, &mut |name, class| {
                                vm.create_object_of(name, class)
                            }),
                        crate::quest::state::StoryParameter::Integer(value) => {
                            Value::Int(i32::from_le_bytes(value.to_le_bytes()))
                        }
                    });
                }
                let object = world.object_of(Handle::Form(id), &mut |name, class| {
                    vm.create_object_of(name, class)
                });
                (object, args)
            };
            let threads = self.vm.send_event(object, "OnStoryScript", &args);
            if !threads.is_empty() {
                self.world.borrow_mut().clear_saved_story_event(id);
                resumed += threads.len();
            }
            self.run_until_idle(64);
        }
        resumed
    }

    pub fn player(&mut self) -> GlobalFormId {
        self.world.borrow_mut().player()
    }

    /// Advance real time: the clock, due update timers, the package driver,
    /// the VM scheduler, then run until the VM settles.
    pub fn tick(&mut self, real_seconds: f64) {
        self.tick_menu_mode(real_seconds, false);
    }

    /// Tick with the host's menu pause state. Real-time menu waits continue
    /// while world simulation, ordinary waits and game-time waits pause.
    pub fn tick_menu_mode(&mut self, real_seconds: f64, menu_mode: bool) {
        let mut game_hours = 0.0;
        if !menu_mode {
            self.resume_quest_lifecycle_tasks();
            let vm = &mut self.vm;
            let mut w = self.world.borrow_mut();
            let before = w.clock.game_days;
            w.advance_clock(real_seconds);
            w.tick_image_space(real_seconds);
            let errors = w.tick_magic(real_seconds, &mut |n, c| vm.create_object_of(n, c));
            for error in errors {
                vm.log.push(vm.time(), error.to_string());
            }
            let errors = w.tick_concentration(real_seconds, &mut |n, c| vm.create_object_of(n, c));
            for error in errors {
                vm.log.push(vm.time(), error.to_string());
            }
            game_hours = (w.clock.game_days - before) * 24.0;
            w.fire_timers();
            w.ai_tick(real_seconds, &mut |n, c| vm.create_object_of(n, c));
        }
        self.vm.advance_clocks(real_seconds, game_hours, menu_mode);
        self.run_until_idle(64);
        if !menu_mode {
            if let Err(error) = self.update_dialogue_range() {
                self.vm.log.push(self.vm.time(), error.to_string());
            }
            if let Err(error) = self.update_automatic_greetings() {
                self.vm.log.push(self.vm.time(), error.to_string());
            }
        }
        self.push_tick_hash();
    }

    fn resume_quest_lifecycle_tasks(&mut self) {
        let tasks = self.world.borrow().lifecycle_tasks();
        for (id, task) in tasks {
            match task {
                quest::state::LifecycleTask::Starting => {
                    self.start_quest(id);
                }
                quest::state::LifecycleTask::Stopping => {
                    self.world.borrow_mut().stop_quest(id);
                }
            }
        }
    }

    /// Drain queued events and calls into the VM. Returns how many.
    pub fn pump(&mut self) -> usize {
        let pending: Vec<Pending> = std::mem::take(&mut self.world.borrow_mut().pending);
        let n = pending
            .iter()
            .filter(|event| !matches!(event, Pending::DeferredDeath { .. }))
            .count();
        let _span = soe_trace::Span::with_payload("game.pump", "game", n as u64);
        for p in pending {
            match p {
                event @ Pending::DeferredDeath { .. } => {
                    self.world.borrow_mut().pending.push(event);
                }
                Pending::ActorDeath {
                    killer, recipients, ..
                } => {
                    let (objects, argument) = {
                        let vm = &mut self.vm;
                        let mut world = self.world.borrow_mut();
                        let mut alloc = |name: &str, class: &str| vm.create_object_of(name, class);
                        let argument = world.form_value(killer, &mut alloc);
                        let objects: Vec<_> = recipients
                            .into_iter()
                            .map(|handle| world.object_of(handle, &mut alloc))
                            .collect();
                        (objects, argument)
                    };
                    for name in ["OnDying", "OnDeath"] {
                        for object in &objects {
                            self.vm
                                .send_event(*object, name, std::slice::from_ref(&argument));
                        }
                    }
                }
                Pending::BindEffect { object, effect } => {
                    let definition = {
                        let world = self.world.borrow();
                        world.order().winner_ref(effect).and_then(|rref| {
                            let record = world.order().record(rref);
                            let raw = record.find(Tag::new(b"VMAD"))?;
                            Some((rref.file, Vmad::parse(raw, record.tag)))
                        })
                    };
                    if let Some((file, definition)) = definition {
                        match definition {
                            Ok(vmad) => {
                                let mut report = BindReport::default();
                                self.attach_all(object, effect, file, &vmad.scripts, &mut report);
                                for (script, _) in report.scripts_missing {
                                    self.vm.log.push(
                                        self.vm.time(),
                                        format!("effect script missing: {script}"),
                                    );
                                }
                            }
                            Err(error) => self
                                .vm
                                .log
                                .push(self.vm.time(), format!("effect VMAD: {error}")),
                        }
                    }
                }
                Pending::Event { object, name, args } => {
                    self.vm.send_event(object, &name, &args);
                }
                Pending::Call {
                    object,
                    function,
                    args,
                } => {
                    if let Err(e) = self.vm.call(object, &function, &args) {
                        self.vm
                            .log
                            .push(self.vm.time(), format!("fragment {function}: {e}"));
                    }
                }
                Pending::CallScript {
                    object,
                    script,
                    function,
                    args,
                } => {
                    if let Err(e) = self.vm.call_script(object, &script, &function, &args) {
                        self.vm
                            .log
                            .push(self.vm.time(), format!("fragment {script}.{function}: {e}"));
                    }
                }
            }
        }
        n
    }

    /// Alternate VM runs and pumps until nothing is runnable and nothing is
    /// queued, or `max_rounds` passes. Returns true when settled.
    pub fn run_until_idle(&mut self, max_rounds: usize) -> bool {
        for _ in 0..max_rounds {
            self.pump();
            self.vm.run(self.budget);
            self.world.borrow_mut().collect_effect_objects(&mut self.vm);
            let queued = self.world.borrow().has_pending_calls();
            if self.vm.idle() && !queued {
                return true;
            }
        }
        false
    }

    pub fn traces(&self) -> Vec<String> {
        self.vm
            .log
            .entries
            .iter()
            .map(|e| e.message.clone())
            .collect()
    }
}
