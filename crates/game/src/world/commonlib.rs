// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib-shaped views over state already owned by `World`.

use soe_commonlib_shim::form::{DataHandler, FileInfo, Form, FormId, FormType};
use soe_commonlib_shim::ni::NiPoint3;
use soe_commonlib_shim::refr::{
    Actor, ActorApiError, ActorHandle, ActorPtr, ActorState, ActorValue, ObjectRef,
    ObjectRefHandle, ObjectRefPtr, Unsupported, WeaponState,
};
use soe_commonlib_shim::world_runtime::{
    ReanimateEffect, RuntimeState, Setting, SettingValue as RuntimeSettingValue, SitSleepState,
    Sky, UnsupportedWorldHook,
};
use soe_commonlib_shim::world_types::{
    self, HazardState, HighProcessState, HitData, SettingValue, UnsupportedHook, WornState,
};
use soe_esm::{GlobalFormId, Slot};
use soe_papyrus::Value;

use super::{RefState, World};
use crate::combat::{HitResult, MeleeHit};

impl World {
    fn shim_id(&self, id: GlobalFormId) -> FormId {
        FormId(self.order.runtime_id(id))
    }

    /// Read the selected item's ExtraWorn markers from the actor inventory.
    #[must_use]
    pub fn commonlib_extra_worn(
        &self,
        actor: GlobalFormId,
        item: GlobalFormId,
    ) -> Option<WornState> {
        let inventory = &self.reference_ref(actor)?.inventory;
        let stacks = inventory.stacks(item);
        (!stacks.is_empty()).then(|| WornState {
            worn: stacks.iter().any(|stack| stack.extra(22).is_some()),
            worn_left: stacks.iter().any(|stack| stack.extra(23).is_some()),
        })
    }

    /// Read a setting through the same script override and winning GMST path
    /// used by the game runtime.
    #[must_use]
    pub fn commonlib_game_setting(&self, name: &str) -> Option<SettingValue> {
        match self.game_setting(name)? {
            Value::Bool(value) => Some(SettingValue::Bool(value)),
            Value::Int(value) => Some(SettingValue::Int(value)),
            Value::Float(value) => Some(SettingValue::Float(value)),
            Value::Str(value) => Some(SettingValue::String(value.to_string())),
            Value::None | Value::Object(_) | Value::Array(_) | Value::Struct(_) => None,
        }
    }

    /// Read the placed HAZD reference fields currently represented by the
    /// reference table. This does not simulate the hazard.
    pub fn commonlib_hazard(&mut self, reference: GlobalFormId) -> Option<HazardState> {
        let (base, position, enabled) = {
            let state = self.reference(reference)?;
            (state.base?, state.position, state.enabled)
        };
        if self.record(base)?.tag != soe_esm::Tag(*b"HAZD") {
            return None;
        }
        Some(HazardState {
            reference: self.shim_id(reference),
            base: self.shim_id(base),
            position: NiPoint3 {
                x: position[0],
                y: position[1],
                z: position[2],
            },
            enabled,
        })
    }

    /// Read the actor's currently represented high-process subset.
    #[must_use]
    pub fn commonlib_high_process(&self, actor: GlobalFormId) -> Option<HighProcessState> {
        let state = self.actor_ref(actor)?;
        Some(HighProcessState {
            actor: self.shim_id(actor),
            in_combat: state.in_combat,
            combat_target: state.combat_target.map(|id| self.shim_id(id)),
            staggered: self.is_staggered(actor),
        })
    }

    /// Resolve a host-confirmed melee contact and project the result as
    /// CommonLib HitData. The game combat path remains the only writer.
    pub fn commonlib_resolve_hit(
        &mut self,
        hit: MeleeHit,
        outgoing_perks: Vec<crate::perks::EntryPointInvocation>,
        incoming_perks: Vec<crate::perks::EntryPointInvocation>,
        alloc: crate::world::Alloc<'_>,
    ) -> Result<HitData, crate::combat::Error> {
        let result = self.melee_hit(hit, outgoing_perks, incoming_perks, alloc)?;
        Ok(self.commonlib_hit_data(hit, result))
    }

    fn commonlib_hit_data(&self, hit: MeleeHit, result: HitResult) -> HitData {
        HitData {
            attacker: self.shim_id(hit.attacker),
            target: self.shim_id(hit.target),
            weapon: self.shim_id(hit.weapon),
            weapon_damage: result.weapon_damage,
            armor_reduction: result.armor_reduction,
            health_damage: result.health_damage,
            blocked: result.blocked,
            killed: result.killed,
        }
    }
}

impl RuntimeState for World {
    fn setting(&self, name: &str) -> Option<Setting> {
        let value = match self.game_setting(name)? {
            Value::Bool(value) => RuntimeSettingValue::Bool(value),
            Value::Int(value) => RuntimeSettingValue::Integer(value),
            Value::Float(value) => RuntimeSettingValue::Float(value),
            Value::Str(value) => RuntimeSettingValue::String(value.to_string()),
            Value::None | Value::Object(_) | Value::Array(_) | Value::Struct(_) => return None,
        };
        Some(Setting {
            name: name.to_owned(),
            value,
        })
    }

    fn sky(&self) -> Sky {
        Sky {
            hour: self.calendar().hour as f32,
        }
    }

    fn reanimate_effect(&self, id: u64) -> Option<ReanimateEffect> {
        let state = self.active_effects.get(&super::magic::EffectId(id))?;
        if self.magic_effect(state.effect).ok()?.data.archetype != 22 {
            return None;
        }
        let packed = |form| FormId(self.order.runtime_id(form));
        Some(ReanimateEffect {
            id,
            spell: packed(state.spell),
            effect: packed(state.effect),
            target: packed(state.target),
            magnitude: state.magnitude,
            elapsed: state.elapsed,
            duration: state.duration,
        })
    }

    fn sit_sleep_state(&self, _actor: FormId) -> Result<SitSleepState, UnsupportedWorldHook> {
        Err(UnsupportedWorldHook {
            name: "SIT_SLEEP_STATE",
            reason: "actor sit and sleep animation state is not represented by the runtime",
        })
    }

    fn hook(&self, _name: &str) -> UnsupportedWorldHook {
        UnsupportedWorldHook {
            name: "binary hook",
            reason: "binary address and vtable hooks have no target in this engine",
        }
    }
}

/// Report the planned hooks with no corresponding engine behavior.
#[must_use]
pub fn unsupported_hook(type_name: &str, hook_name: &str) -> UnsupportedHook {
    world_types::unsupported_hook(type_name, hook_name)
}

#[derive(Clone, Debug)]
pub struct LiveForm {
    id: FormId,
    kind: FormType,
    editor_id: Option<String>,
    keywords: Vec<FormId>,
}

impl Form for LiveForm {
    fn form_id(&self) -> FormId {
        self.id
    }

    fn form_type(&self) -> FormType {
        self.kind
    }

    fn editor_id(&self) -> Option<&str> {
        self.editor_id.as_deref()
    }

    fn keywords(&self) -> &[FormId] {
        &self.keywords
    }
}

/// Read-only `TESDataHandler` view over winning records in a `World`.
pub struct LiveDataHandler {
    forms: Vec<LiveForm>,
    files: Vec<FileInfo>,
}

impl LiveDataHandler {
    pub fn new(world: &World) -> Self {
        let mut ids: Vec<_> = world.order().winners().map(|(id, _)| id).collect();
        ids.sort_unstable();
        ids.dedup();
        let forms = ids
            .into_iter()
            .filter_map(|id| {
                let record = world.record(id)?;
                let to_form_id = |value| FormId(world.runtime_id(value));
                Some(LiveForm {
                    id: to_form_id(id),
                    kind: FormType(*record.tag.as_bytes()),
                    editor_id: record.editor_id(),
                    keywords: world.keywords(id).into_iter().map(to_form_id).collect(),
                })
            })
            .collect();
        let files = world
            .order()
            .files()
            .iter()
            .map(|file| match file.slot {
                Slot::Regular(index) => FileInfo {
                    name: file.name.clone(),
                    regular_index: Some(index),
                    light_index: None,
                },
                Slot::Light(index) => FileInfo {
                    name: file.name.clone(),
                    regular_index: None,
                    light_index: Some(index),
                },
            })
            .collect();
        Self { forms, files }
    }
}

impl DataHandler for LiveDataHandler {
    type Form = LiveForm;

    fn lookup(&self, id: FormId) -> Option<&Self::Form> {
        self.forms.iter().find(|form| form.id == id)
    }

    fn lookup_by_editor_id(&self, editor_id: &str) -> Option<&Self::Form> {
        self.forms.iter().find(|form| {
            form.editor_id
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(editor_id))
        })
    }

    fn forms_of_type(&self, kind: FormType) -> Vec<FormId> {
        self.forms
            .iter()
            .filter(|form| form.kind == kind)
            .map(|form| form.id)
            .collect()
    }

    fn files(&self) -> Vec<String> {
        self.files.iter().map(|file| file.name.clone()).collect()
    }

    fn file(&self, index: usize) -> Option<FileInfo> {
        self.files.get(index).cloned()
    }
}

/// Mutable live view over one instantiated reference. A cold reference is
/// built through `World::reference` before this adapter is returned.
pub struct LiveObjectRef<'a> {
    runtime_id: FormId,
    base_runtime_id: Option<FormId>,
    cell_runtime_id: Option<FormId>,
    state: &'a mut RefState,
}

/// Live actor view over the shared reference and actor tables.
pub struct LiveActor<'a> {
    id: GlobalFormId,
    world: &'a mut World,
}

impl ObjectRef for LiveObjectRef<'_> {
    fn handle(&self) -> ObjectRefHandle {
        ObjectRefHandle(self.runtime_id.0)
    }

    fn form_id(&self) -> FormId {
        self.runtime_id
    }

    fn base(&self) -> FormId {
        self.base_runtime_id.unwrap_or(FormId(0))
    }

    fn position(&self) -> NiPoint3 {
        NiPoint3::new(
            self.state.position[0],
            self.state.position[1],
            self.state.position[2],
        )
    }

    fn rotation(&self) -> NiPoint3 {
        NiPoint3::new(
            self.state.rotation[0],
            self.state.rotation[1],
            self.state.rotation[2],
        )
    }

    fn parent_cell(&self) -> Option<FormId> {
        self.cell_runtime_id
    }

    fn is_disabled(&self) -> bool {
        !self.state.enabled
    }

    fn is_deleted(&self) -> bool {
        self.state.deleted
    }

    fn set_base(&mut self, _base: FormId) -> Result<(), Unsupported> {
        Err(Unsupported {
            operation: "TESObjectREFR::SetBaseObject",
        })
    }
}

impl LiveObjectRef<'_> {
    pub fn pointer(&self) -> ObjectRefPtr {
        ObjectRefPtr(Some(self.handle()))
    }
}

impl ObjectRef for LiveActor<'_> {
    fn handle(&self) -> ObjectRefHandle {
        ObjectRefHandle(self.world.runtime_id(self.id))
    }

    fn form_id(&self) -> FormId {
        FormId(self.world.runtime_id(self.id))
    }

    fn base(&self) -> FormId {
        self.world
            .reference_ref(self.id)
            .and_then(|state| state.base)
            .map_or(FormId(0), |id| FormId(self.world.runtime_id(id)))
    }

    fn position(&self) -> NiPoint3 {
        self.world
            .reference_ref(self.id)
            .map_or_else(NiPoint3::default, |state| {
                NiPoint3::new(state.position[0], state.position[1], state.position[2])
            })
    }

    fn rotation(&self) -> NiPoint3 {
        self.world
            .reference_ref(self.id)
            .map_or_else(NiPoint3::default, |state| {
                NiPoint3::new(state.rotation[0], state.rotation[1], state.rotation[2])
            })
    }

    fn parent_cell(&self) -> Option<FormId> {
        self.world
            .reference_ref(self.id)
            .and_then(|state| state.cell)
            .map(|id| FormId(self.world.runtime_id(id)))
    }

    fn is_disabled(&self) -> bool {
        self.world
            .reference_ref(self.id)
            .is_none_or(|state| !state.enabled)
    }

    fn is_deleted(&self) -> bool {
        self.world
            .reference_ref(self.id)
            .is_none_or(|state| state.deleted)
    }

    fn set_base(&mut self, _base: FormId) -> Result<(), Unsupported> {
        Err(Unsupported {
            operation: "TESObjectREFR::SetBaseObject",
        })
    }
}

impl Actor for LiveActor<'_> {
    fn actor_handle(&self) -> ActorHandle {
        ActorHandle(self.world.runtime_id(self.id))
    }

    fn actor_ptr(&self) -> ActorPtr {
        ActorPtr(Some(self.actor_handle()))
    }

    fn state(&self) -> ActorState {
        self.world
            .actor_ref(self.id)
            .map_or_else(ActorState::default, |actor| ActorState {
                dead: actor.dead,
                unconscious: actor.unconscious,
                sneaking: actor.sneaking,
                weapon_drawn: actor.weapon_drawn,
                blocking: actor.blocking,
            })
    }

    fn actor_value(&self, value: ActorValue) -> Result<f32, ActorApiError> {
        let name = crate::actor_values::actor_value_name(value.0 as i32)
            .ok_or(ActorApiError::UnsupportedOperation)?;
        self.world
            .actor_ref(self.id)
            .map(|actor| actor.values.get(name).current())
            .ok_or(ActorApiError::InvalidActor)
    }

    fn set_actor_value(&mut self, value: ActorValue, amount: f32) -> Result<(), ActorApiError> {
        let name = crate::actor_values::actor_value_name(value.0 as i32)
            .ok_or(ActorApiError::UnsupportedOperation)?;
        self.world
            .set_actor_value(self.id, name, amount)
            .map_err(|_| ActorApiError::UnsupportedOperation)
    }

    fn race(&self) -> FormId {
        self.world
            .actor_ref(self.id)
            .and_then(|actor| actor.race)
            .map_or(FormId(0), |id| FormId(self.world.runtime_id(id)))
    }

    fn is_dead(&self) -> bool {
        self.world
            .actor_ref(self.id)
            .is_some_and(|actor| actor.dead)
    }

    fn is_player(&self) -> bool {
        self.world.player == Some(self.id)
    }

    fn weapon_state(&self) -> WeaponState {
        if self
            .world
            .actor_ref(self.id)
            .is_some_and(|actor| actor.weapon_drawn)
        {
            WeaponState::Drawn
        } else {
            WeaponState::Sheathed
        }
    }

    fn add_spell(&mut self, spell: FormId) -> bool {
        self.world
            .from_runtime_id(spell.0)
            .and_then(|spell| {
                self.world
                    .actor(self.id)
                    .map(|actor| actor.spells.insert(spell))
            })
            .unwrap_or(false)
    }

    fn add_perk(&mut self, perk: FormId) -> bool {
        self.world
            .from_runtime_id(perk.0)
            .and_then(|perk| {
                self.world
                    .actor(self.id)
                    .map(|actor| actor.perks.insert(perk))
            })
            .unwrap_or(false)
    }

    fn add_item(&mut self, item: FormId, count: u32) -> bool {
        let Some(item) = self.world.from_runtime_id(item.0) else {
            return false;
        };
        let Ok(count) = i32::try_from(count) else {
            return false;
        };
        let before = self.world.item_count(self.id, item);
        self.world.add_item(self.id, item, count);
        self.world.item_count(self.id, item) > before
    }
}

impl World {
    pub fn commonlib_data_handler(&self) -> LiveDataHandler {
        LiveDataHandler::new(self)
    }

    pub fn commonlib_reference(&mut self, id: GlobalFormId) -> Option<LiveObjectRef<'_>> {
        let runtime_id = self.runtime_id(id);
        let (base, cell) = {
            let state = self.reference(id)?;
            (state.base, state.cell)
        };
        let base_runtime_id = base.map(|base| FormId(self.runtime_id(base)));
        let cell_runtime_id = cell.map(|cell| FormId(self.runtime_id(cell)));
        let state = self.refs.get_mut(&id)?;
        Some(LiveObjectRef {
            runtime_id: FormId(runtime_id),
            base_runtime_id,
            cell_runtime_id,
            state,
        })
    }

    pub fn commonlib_reference_ptr(&mut self, pointer: ObjectRefPtr) -> Option<LiveObjectRef<'_>> {
        let handle = pointer.0?;
        let id = self.from_runtime_id(handle.0)?;
        self.commonlib_reference(id)
    }

    pub fn commonlib_actor(&mut self, id: GlobalFormId) -> Option<LiveActor<'_>> {
        if !self.refs.contains_key(&id)
            || !self.actors.contains_key(&id)
            || !self
                .refs
                .get(&id)
                .is_some_and(|reference| reference.is_actor)
        {
            return None;
        }
        Some(LiveActor { id, world: self })
    }

    /// The world tracks weapon draw state as a boolean, so intermediate
    /// animation states are explicitly unavailable from this runtime.
    pub fn commonlib_weapon_state(&self, id: GlobalFormId) -> Option<WeaponState> {
        let drawn = self.actor_ref(id)?.weapon_drawn;
        Some(if drawn {
            WeaponState::Drawn
        } else {
            WeaponState::Sheathed
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::magic::{ActiveEffect, EffectId};
    use crate::world::{ActorState, RefState};
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId {
            file: soe_esm::CREATED_FILE,
            object,
        }
    }

    #[test]
    fn shm_11_game_settings_read_the_live_world_override() {
        let mut world = World::new(LoadOrder::new());
        world
            .settings
            .insert("fcombatdistance".to_owned(), Value::Float(96.0));
        assert_eq!(
            world.commonlib_game_setting("fCombatDistance"),
            Some(SettingValue::Float(96.0))
        );
        assert_eq!(world.commonlib_game_setting("missing"), None);
    }

    #[test]
    fn shm_11_high_process_reads_combat_target_and_stagger_state() {
        let actor = id(1);
        let target = id(2);
        let mut world = World::new(LoadOrder::new());
        world.actors.insert(
            actor,
            ActorState {
                in_combat: true,
                combat_target: Some(target),
                stagger_end_time: Some(world.clock.real_seconds + 1.0),
                ..ActorState::default()
            },
        );
        let process = world.commonlib_high_process(actor).unwrap();
        assert!(process.in_combat && process.staggered);
        assert_eq!(process.actor, FormId(0xff00_0001));
        assert_eq!(process.combat_target, Some(FormId(0xff00_0002)));
    }

    #[test]
    fn shm_11_extra_worn_hazard_and_hit_data_have_explicit_absence_paths() {
        let mut world = World::new(LoadOrder::new());
        let actor = id(1);
        let item = id(2);
        let mut reference = RefState::new(None);
        reference
            .inventory
            .add_selected_stack(
                item,
                crate::inventory::Stack::new(
                    1,
                    vec![crate::inventory::Extra {
                        kind: 22,
                        fields: Vec::new(),
                    }],
                )
                .unwrap(),
            )
            .unwrap();
        world.refs.insert(actor, reference);
        assert_eq!(
            world.commonlib_extra_worn(actor, item),
            Some(WornState {
                worn: true,
                worn_left: false
            })
        );
        assert_eq!(world.commonlib_hazard(id(3)), None);
        assert_eq!(
            unsupported_hook("HitData", "dispatch"),
            UnsupportedHook {
                type_name: "HitData",
                hook_name: "dispatch",
                reason: "hit event dispatch is owned by game combat execution",
            }
        );
    }
    #[test]
    fn shm_15_reanimation_view_reads_the_live_effect_and_rejects_other_archetypes() {
        let mut data = vec![0; 152];
        data[64..68].copy_from_slice(&22u32.to_le_bytes());
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"MGEF"), 0x200).sub(Tag(*b"DATA"), data));
        plugin.push_top(Record::new(Tag(*b"MGEF"), 0x201).sub(Tag(*b"DATA"), vec![0; 152]));
        let mut order = LoadOrder::new();
        order.add("fixture.esp", plugin).unwrap();
        let mut world = World::new(order);
        let spell = crate::world::GlobalFormId {
            file: 0,
            object: 0x300,
        };
        let effect = crate::world::GlobalFormId {
            file: 0,
            object: 0x200,
        };
        let target = crate::world::GlobalFormId {
            file: 0,
            object: 0x400,
        };
        world.active_effects.insert(
            EffectId(1),
            ActiveEffect {
                id: EffectId(1),
                spell,
                effect,
                effect_index: 0,
                source: spell,
                caster: None,
                target,
                magnitude: 10.0,
                duration: Some(8.0),
                elapsed: 2.0,
                summoned: None,
                reanimated_previous_owner: None,
                restore_owner: None,
                recover: false,
                changes: Vec::new(),
            },
        );
        world.active_effects.insert(
            EffectId(2),
            ActiveEffect {
                id: EffectId(2),
                spell,
                effect: crate::world::GlobalFormId {
                    file: 0,
                    object: 0x201,
                },
                effect_index: 0,
                source: spell,
                caster: None,
                target,
                magnitude: 1.0,
                duration: None,
                elapsed: 0.0,
                summoned: None,
                reanimated_previous_owner: None,
                restore_owner: None,
                recover: false,
                changes: Vec::new(),
            },
        );

        let live = world.reanimate_effect(1).unwrap();
        assert_eq!(live.target, FormId(world.order.runtime_id(target)));
        assert_eq!(live.magnitude, 10.0);
        assert_eq!(live.elapsed, 2.0);
        assert_eq!(live.duration, Some(8.0));
        assert_eq!(world.reanimate_effect(2), None);
        assert_eq!(world.reanimate_effect(3), None);
    }

    fn world() -> World {
        let base = Record::new(Tag(*b"MISC"), 0x800).sub(Tag(*b"EDID"), b"Shm16Base\0".to_vec());
        let reference = Record::new(Tag(*b"REFR"), 0x801)
            .sub(Tag(*b"NAME"), 0x800u32.to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                [12.5f32, -4.0, 9.0, 0.1, 0.2, 0.3]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(base);
        plugin.push_top(reference);
        let mut light_header = FileHeader::default();
        light_header.flags |= soe_esm::plugin::flags::LIGHT_MASTER;
        let mut light = Plugin::new(light_header);
        light
            .push_top(Record::new(Tag(*b"MISC"), 0x10).sub(Tag(*b"EDID"), b"LightBase\0".to_vec()));
        let mut order = LoadOrder::new();
        order.add("Shm16.esm", plugin).unwrap();
        order.add("Shm16Light.esl", light).unwrap();
        World::new(order)
    }

    #[test]
    fn shm_16_live_data_handler_and_file_view_follow_the_load_order() {
        let world = world();
        let handler = world.commonlib_data_handler();
        let form = handler.lookup_by_editor_id("shm16base").unwrap();
        assert_eq!(form.form_id().0, 0x0000_0800);
        assert_eq!(form.form_type().as_str(), "MISC");
        assert_eq!(
            handler.lookup(form.form_id()).unwrap().editor_id(),
            Some("Shm16Base")
        );
        assert_eq!(handler.forms_of_type(FormType(*b"REFR")), [FormId(0x801)]);
        assert_eq!(handler.files(), ["Shm16.esm", "Shm16Light.esl"]);
        assert_eq!(handler.file(0).unwrap().regular_index, Some(0));
        let light_form = handler.lookup_by_editor_id("LightBase").unwrap();
        assert_eq!(light_form.form_id(), FormId(0xFE00_0010));
        assert_eq!(handler.file(1).unwrap().light_index, Some(0));
        assert_eq!(handler.file(2), None);
    }

    #[test]
    fn shm_16_live_reference_pointer_and_unsupported_set_base_are_explicit() {
        let mut world = world();
        let id = GlobalFormId {
            file: 0,
            object: 0x801,
        };
        let pointer = {
            let reference = world.commonlib_reference(id).unwrap();
            assert_eq!(reference.position(), NiPoint3::new(12.5, -4.0, 9.0));
            assert_eq!(reference.rotation(), NiPoint3::new(0.1, 0.2, 0.3));
            assert_eq!(reference.base(), FormId(0x800));
            assert_eq!(reference.pointer(), ObjectRefPtr(Some(reference.handle())));
            reference.pointer()
        };
        assert!(ObjectRefPtr::default().is_null());
        {
            let resolved = world.commonlib_reference_ptr(pointer).unwrap();
            assert_eq!(resolved.form_id(), FormId(0x801));
        }
        let mut reference = world.commonlib_reference(id).unwrap();
        assert_eq!(
            reference.set_base(FormId(0x900)).unwrap_err().operation,
            "TESObjectREFR::SetBaseObject"
        );
        assert_eq!(reference.base(), FormId(0x800));
    }

    #[test]
    fn shm_16_weapon_state_exposes_live_drawn_state_and_stable_values() {
        let mut world = world();
        let id = GlobalFormId {
            file: 0,
            object: 0x801,
        };
        world.actors.insert(id, super::super::ActorState::default());
        assert_eq!(
            world.commonlib_weapon_state(id),
            Some(WeaponState::Sheathed)
        );
        world.actors.get_mut(&id).unwrap().weapon_drawn = true;
        assert_eq!(world.commonlib_weapon_state(id), Some(WeaponState::Drawn));
        assert_eq!(WeaponState::Sheathed as u32, 0);
        assert_eq!(WeaponState::WantToDraw as u32, 1);
        assert_eq!(WeaponState::Drawing as u32, 2);
        assert_eq!(WeaponState::Drawn as u32, 3);
        assert_eq!(WeaponState::WantToSheathe as u32, 4);
        assert_eq!(WeaponState::Sheathing as u32, 5);
    }

    #[test]
    fn shm_16_live_actor_trait_reads_and_changes_shared_actor_values() {
        let mut world = world();
        let id = GlobalFormId {
            file: 0,
            object: 0x801,
        };
        let _ = world.reference(id).unwrap();
        world.refs.get_mut(&id).unwrap().is_actor = true;
        let mut actor_state = super::super::ActorState::default();
        actor_state.values.set_base("health", 90.0).unwrap();
        actor_state.weapon_drawn = true;
        actor_state.race = Some(GlobalFormId {
            file: 0,
            object: 0x800,
        });
        world.actors.insert(id, actor_state);
        let mut actor = world.commonlib_actor(id).unwrap();
        assert_eq!(actor.form_id(), FormId(0x801));
        assert_eq!(actor.actor_handle(), ActorHandle(0x801));
        assert_eq!(actor.actor_value(ActorValue::HEALTH), Ok(90.0));
        assert_eq!(actor.weapon_state(), WeaponState::Drawn);
        assert_eq!(actor.race(), FormId(0x800));
        actor.set_actor_value(ActorValue::HEALTH, 75.0).unwrap();
        assert_eq!(actor.actor_value(ActorValue::HEALTH), Ok(75.0));
        assert!(actor
            .set_actor_value(ActorValue(u32::MAX), 10.0)
            .is_err_and(|error| error == ActorApiError::UnsupportedOperation));
    }
}
