// SPDX-License-Identifier: GPL-3.0-or-later

//! References: the run-time state of a placed object built from its `REFR`
//! or `ACHR` record on first use, dynamic placement, the player, inventory,
//! movement, distance and activation.

use soe_esm::plugin::flags;
use soe_esm::records::refr::Refr;
use soe_esm::GlobalFormId;

use super::{
    le_f32, le_u32, Alloc, Handle, Pending, RefState, World, DYNAMIC_FILE, PLAYER_OBJECT, T,
};

/// A reference's runtime placement and visibility, in Skyrim space. This is
/// the state a host diffs each tick to reflect an enable or placement
/// change in its rendering and collision instances (`HST-04`). Read-only:
/// the fields already exist on `RefState`, this is only a stable, narrow
/// view of the ones a host needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefPlacement {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: f32,
    pub visible: bool,
}

impl World {
    /// Set whether host actor presentation should apply foot inverse kinematics.
    pub fn set_debug_foot_ik(&mut self, enabled: bool) {
        self.debug_foot_ik = enabled;
    }

    /// Return the foot inverse kinematics setting requested by Papyrus.
    #[must_use]
    pub fn debug_foot_ik(&self) -> bool {
        self.debug_foot_ik
    }

    /// Request a development position marker for a loaded reference.
    pub fn show_debug_ref_position(&mut self, id: GlobalFormId) -> bool {
        if self.reference(id).is_none() {
            return false;
        }
        self.debug_ref_positions.insert(id);
        true
    }

    /// Return development references whose positions were requested for display.
    #[must_use]
    pub fn debug_ref_positions(&self) -> Vec<GlobalFormId> {
        self.debug_ref_positions.iter().copied().collect()
    }

    /// Move the player to a placed reference and update the active cell.
    pub fn debug_player_move_to_reference(&mut self, target: GlobalFormId) -> bool {
        let player = self.player();
        let Some((position, rotation, cell)) = self
            .reference(target)
            .map(|reference| (reference.position, reference.rotation, reference.cell))
        else {
            return false;
        };
        let Some(player_state) = self.reference(player) else {
            return false;
        };
        player_state.position = position;
        player_state.rotation = rotation;
        player_state.cell = cell;
        if self.loaded_cell != cell {
            self.change_cell(cell);
        }
        true
    }

    /// Move the player to the origin of a cell selected by editor id.
    pub fn debug_player_move_to_cell(&mut self, editor_id: &str) -> bool {
        let Some(cell) = self.cell_by_editor_id(editor_id) else {
            return false;
        };
        if self.tag_of(cell).is_none_or(|tag| tag != T(b"CELL")) {
            return false;
        }
        let player = self.player();
        let Some(reference) = self.reference(player) else {
            return false;
        };
        reference.position = [0.0; 3];
        reference.cell = Some(cell);
        if self.loaded_cell != Some(cell) {
            self.change_cell(Some(cell));
        }
        true
    }

    /// Find a placed actor or reference with a matching base inside a radius.
    /// Record-backed references are read without constructing runtime state,
    /// while already instantiated references use their current position and
    /// enabled state.
    pub fn find_reference_of_base(
        &mut self,
        base: Option<GlobalFormId>,
        center: [f32; 3],
        radius: f32,
        actors_only: bool,
        random: bool,
    ) -> Option<GlobalFormId> {
        if !center.iter().all(|value| value.is_finite()) || !radius.is_finite() || radius < 0.0 {
            return None;
        }

        let radius_squared = radius * radius;
        let mut candidates: Vec<_> = self
            .order
            .winners_of(T(b"ACHR"))
            .map(|(id, _)| id)
            .chain(self.order.winners_of(T(b"REFR")).map(|(id, _)| id))
            .chain(self.refs.keys().copied())
            .collect();
        candidates.sort_unstable();
        candidates.dedup();

        let mut matches = Vec::new();
        for id in candidates {
            let Some(state) = self
                .refs
                .get(&id)
                .cloned()
                .or_else(|| self.load_ref_record(id))
            else {
                continue;
            };
            if !state.enabled || state.deleted || (actors_only && !state.is_actor) {
                continue;
            }
            if base.is_some_and(|expected| state.base != Some(expected)) {
                continue;
            }
            let distance_squared = state
                .position
                .iter()
                .zip(center)
                .map(|(position, origin)| (position - origin).powi(2))
                .sum::<f32>();
            if distance_squared.is_finite() && distance_squared <= radius_squared {
                matches.push((id, distance_squared));
            }
        }

        if matches.is_empty() {
            return None;
        }
        if random {
            return Some(matches[(self.random_u64() % matches.len() as u64) as usize].0);
        }
        matches
            .into_iter()
            .min_by(|(left_id, left_distance), (right_id, right_distance)| {
                left_distance
                    .total_cmp(right_distance)
                    .then_with(|| left_id.cmp(right_id))
            })
            .map(|(id, _)| id)
    }

    pub(super) fn load_ref(&mut self, id: GlobalFormId) -> Option<RefState> {
        let mut st = self.load_ref_record(id)?;
        if let Some(base) = st.base {
            let mut present = true;
            if st.is_actor {
                let level = self.encounter_level(id).ok()?;
                present = self.prepare_actor_template(id, base, level).ok()?;
                if !present {
                    st.enabled = false;
                }
            }
            st.inventory = self.reference_inventory(Some(id), base);
            if st
                .inventory
                .iter()
                .any(|(item, _)| self.tag_of(*item) == Some(T(b"LVLI")))
            {
                let level = self.encounter_level(id).ok()?;
                st.inventory = self.resolve_inventory_lists(st.inventory, level).ok()?;
            }
            if st.is_actor && present {
                st.inventory = self
                    .default_outfit_inventory(id, base, &st.inventory)
                    .ok()?;
            }
        }
        Some(st)
    }

    /// Read authored placement without choosing templates or touching RNG.
    pub(super) fn load_ref_record(&self, id: GlobalFormId) -> Option<RefState> {
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        let file = rref.file;
        let mut st = match rec.tag.as_bytes() {
            b"REFR" => {
                let r = Refr::from_record(rec).ok()?;
                let mut st = RefState::new(Some(self.order.resolve(file, r.base)));
                st.opened = rec.find(T(b"ONAM")).is_some();
                if let Some(t) = r.transform {
                    st.position = t.position;
                    st.rotation = t.rotation;
                }
                st.scale = r.scale;
                if let Some(lock) = r.lock {
                    st.locked = true;
                    st.lock_level = i32::from(lock.level);
                }
                st.linked = r
                    .linked_refs
                    .iter()
                    .map(|l| {
                        let kw = (l.keyword != 0).then(|| self.order.resolve(file, l.keyword));
                        (kw, self.order.resolve(file, l.reference))
                    })
                    .collect();
                st.teleport = r
                    .teleport
                    .map(|t| (self.order.resolve(file, t.door), t.destination));
                st.owner = r
                    .owner
                    .filter(|o| *o != 0)
                    .map(|o| self.order.resolve(file, o));
                st.location = r
                    .location
                    .filter(|l| *l != 0)
                    .map(|l| self.order.resolve(file, l));
                st
            }
            b"ACHR" => {
                let base = self.order.resolve(file, le_u32(rec.find(T(b"NAME"))?)?);
                let mut st = RefState::new(Some(base));
                if let Some(d) = rec.find(T(b"DATA")).filter(|d| d.len() >= 24) {
                    for (i, v) in st.position.iter_mut().enumerate() {
                        *v = le_f32(&d[i * 4..]);
                    }
                    for (i, v) in st.rotation.iter_mut().enumerate() {
                        *v = le_f32(&d[12 + i * 4..]);
                    }
                }
                if let Some(s) = rec.find(T(b"XSCL")).filter(|d| d.len() >= 4) {
                    st.scale = le_f32(s);
                }
                st.owner = rec
                    .find(T(b"XOWN"))
                    .and_then(le_u32)
                    .filter(|o| *o != 0)
                    .map(|o| self.order.resolve(file, o));
                st.location = rec
                    .find(T(b"XLCN"))
                    .and_then(le_u32)
                    .filter(|l| *l != 0)
                    .map(|l| self.order.resolve(file, l));
                // An actor reference carries XLKR the same way a REFR does,
                // which is how the DLC scripts find a coffin or a cart.
                st.linked = rec
                    .all(T(b"XLKR"))
                    .filter_map(|d| {
                        let reference = match d.len() {
                            4 => le_u32(d)?,
                            8.. => le_u32(&d[4..])?,
                            _ => return None,
                        };
                        let keyword = (d.len() >= 8)
                            .then(|| le_u32(d))
                            .flatten()
                            .filter(|k| *k != 0)
                            .map(|k| self.order.resolve(file, k));
                        Some((keyword, self.order.resolve(file, reference)))
                    })
                    .collect();
                st.is_actor = true;
                st
            }
            b"PMIS" | b"PGRE" | b"PBEA" | b"PCON" | b"PARW" => {
                let base = le_u32(rec.find(T(b"NAME"))?)?;
                let mut st = RefState::new(Some(self.order.resolve(file, base)));
                if let Some(data) = rec.find(T(b"DATA")).filter(|data| data.len() >= 24) {
                    for (i, value) in st.position.iter_mut().enumerate() {
                        *value = le_f32(&data[i * 4..]);
                    }
                    for (i, value) in st.rotation.iter_mut().enumerate() {
                        *value = le_f32(&data[12 + i * 4..]);
                    }
                }
                if let Some(scale) = rec.find(T(b"XSCL")).filter(|data| data.len() >= 4) {
                    st.scale = le_f32(scale);
                }
                st
            }
            _ => return None,
        };
        // A deleted winner starts disabled, the way xEdit's undelete and
        // disable cleaning would have left it, so whatever still names it
        // resolves instead of crashing the way the vanilla engine does.
        st.enabled = rec.flags & flags::INITIALLY_DISABLED == 0 && !rec.is_deleted();
        st.record_flags = rec.flags;
        st.persistent = rec.flags & flags::PERSISTENT != 0;
        st.cell = rec.placement.cell.map(|c| self.order.resolve(file, c));
        let original_base = st.base;
        // Base Object Swapper replaces the base of a placed reference, and a
        // reference is built here rather than at start, which is why the swap
        // is held until now. See `crate::moddata`.
        st.base = st
            .base
            .map(|base| self.swapped_reference_base(id, base, st.location.or(st.cell)));
        let properties = self
            .mod_data
            .properties
            .get(&id)
            .or_else(|| original_base.and_then(|base| self.mod_data.properties.get(&base)));
        if let Some(properties) = properties {
            let location = st.location.or(st.cell);
            Self::apply_bos_properties(&mut st, properties, id, original_base, location);
        }
        st.item = self.loose_item(st.base, st.owner, Some(rec));
        Some(st)
    }

    fn swapped_reference_base(
        &self,
        id: GlobalFormId,
        base: GlobalFormId,
        location: Option<GlobalFormId>,
    ) -> GlobalFormId {
        if let Some(to) = self.mod_data.reference_swaps.get(&id).copied() {
            let chance = self.mod_data.reference_swap_chances.get(&id);
            if chance.is_none_or(|chance| Self::bos_chance_passes(chance, id, Some(base), location))
            {
                return to;
            }
        }
        let chance = self.mod_data.base_swap_chances.get(&base);
        if chance.is_none_or(|chance| Self::bos_chance_passes(chance, id, Some(base), location)) {
            self.mod_data.swap(base)
        } else {
            base
        }
    }

    fn apply_bos_properties(
        state: &mut RefState,
        properties: &soe_moddata::bos::Properties,
        id: GlobalFormId,
        base: Option<GlobalFormId>,
        location: Option<GlobalFormId>,
    ) {
        let seed = (u64::from(id.file) << 32) | u64::from(id.object);
        if let Some(chance) = properties.chance {
            if !Self::bos_chance_passes(&chance, id, base, location) {
                return;
            }
        }
        for (operation, target, property) in [
            (0_u64, &mut state.position, properties.position),
            (3_u64, &mut state.rotation, properties.rotation),
        ] {
            if let Some(property) = property {
                for (index, (value, range)) in target.iter_mut().zip(property.values).enumerate() {
                    let value_to_apply =
                        Self::sample_bos_range(range, seed ^ operation ^ index as u64);
                    if property.relative {
                        *value += value_to_apply;
                    } else {
                        *value = value_to_apply;
                    }
                }
            }
        }
        if let Some(scale) = properties.scale {
            let value = Self::sample_bos_range(scale.values, seed ^ 6);
            state.scale = if scale.absolute {
                value
            } else {
                state.scale * value
            };
            state.scale = state.scale.clamp(0.0, 1000.0);
        }
        state.record_flags &= !properties.flags_clear;
        state.record_flags |= properties.flags_set;
    }

    fn bos_chance_passes(
        chance: &soe_moddata::bos::PropertyChance,
        id: GlobalFormId,
        base: Option<GlobalFormId>,
        location: Option<GlobalFormId>,
    ) -> bool {
        if chance.percent >= 100.0 {
            return true;
        }
        if chance.percent <= 0.0 {
            return false;
        }
        let seed = Self::form_seed(id);
        let chance_seed = if chance.location_based {
            location.map(Self::form_seed).unwrap_or(seed)
                ^ base.map(Self::form_seed).unwrap_or_default()
        } else {
            seed
        } ^ chance.seed;
        Self::sample_bos_range([0.0, 100.0], chance_seed) <= chance.percent
    }

    fn sample_bos_range(range: [f32; 2], seed: u64) -> f32 {
        let mut value = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^= value >> 31;
        let unit = (value >> 11) as f64 / ((1_u64 << 53) as f64);
        range[0] + (range[1] - range[0]) * unit as f32
    }

    fn form_seed(id: GlobalFormId) -> u64 {
        let raw = (u64::from(id.file) << 32) | u64::from(id.object);
        raw.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    /// A base's own `CNTO` list plus anything a distribution handed it as an
    /// item, so a distributed item counts the same as a shipped one through
    /// `GetItemCount`. See `crate::moddata`.
    fn base_inventory(&self, base: GlobalFormId) -> crate::inventory::Inventory {
        self.reference_inventory(None, base)
    }

    pub(super) fn reference_inventory(
        &self,
        reference: Option<GlobalFormId>,
        base: GlobalFormId,
    ) -> crate::inventory::Inventory {
        let mut out = crate::inventory::Inventory::default();
        let distributions = reference.map_or_else(
            || self.mod_data.for_base(base).to_vec(),
            |reference| crate::moddata::for_actor(self, reference, base),
        );
        for d in &distributions {
            if d.record_type == soe_moddata::spid::RecordType::Item {
                let _ = out.add(d.what, d.count);
            }
        }
        let source = if self.tag_of(base) == Some(T(b"NPC_")) {
            match reference.map_or_else(
                || self.npc_component_source(base, crate::npc::template::INVENTORY),
                |reference| {
                    self.reference_component_source(
                        reference,
                        base,
                        crate::npc::template::INVENTORY,
                    )
                },
            ) {
                Ok(source) => source,
                Err(_) => return out,
            }
        } else {
            base
        };
        let Some(record) = self.record(source) else {
            return out;
        };
        for cnto in record.all(T(b"CNTO")) {
            if cnto.len() >= 8 {
                let Some(item) = self.resolve_from(source, le_u32(cnto).unwrap_or(0)) else {
                    continue;
                };
                let count = i32::from_le_bytes([cnto[4], cnto[5], cnto[6], cnto[7]]);
                let _ = out.add(item, count);
            }
        }
        out
    }

    pub fn reference(&mut self, id: GlobalFormId) -> Option<&mut RefState> {
        if !self.refs.contains_key(&id) {
            let st = self.load_ref(id)?;
            self.refs.insert(id, st);
            self.track_light_reference(id);
        }
        self.refs.get_mut(&id)
    }

    pub fn reference_ref(&self, id: GlobalFormId) -> Option<&RefState> {
        self.refs.get(&id)
    }

    /// Begin an opening animation for a placed door and record its open destination.
    ///
    /// Returns false when the form is missing, deleted, or is not a door.
    pub fn begin_door_opening(&mut self, id: GlobalFormId) -> bool {
        self.begin_door_transition(id, true)
    }

    /// Begin a closing animation for a placed door and record its closed destination.
    ///
    /// Returns false when the form is missing, deleted, or is not a door.
    pub fn begin_door_closing(&mut self, id: GlobalFormId) -> bool {
        self.begin_door_transition(id, false)
    }

    fn begin_door_transition(&mut self, id: GlobalFormId, opening: bool) -> bool {
        let Some((base, deleted)) = self
            .reference(id)
            .map(|reference| (reference.base, reference.deleted))
        else {
            return false;
        };
        if deleted || base.is_none_or(|base| self.tag_of(base) != Some(T(b"DOOR"))) {
            return false;
        }
        let Some(reference) = self.reference(id) else {
            return false;
        };
        reference.opened = opening;
        reference.open_transition = Some(if opening {
            super::DoorOpenTransition::Opening
        } else {
            super::DoorOpenTransition::Closing
        });
        true
    }

    /// Finish a placed door's current opening or closing animation.
    ///
    /// Returns false when the form is missing or has no transition in progress.
    pub fn finish_door_transition(&mut self, id: GlobalFormId) -> bool {
        let Some(reference) = self.reference(id) else {
            return false;
        };
        if reference.open_transition.is_none() {
            return false;
        }
        reference.open_transition = None;
        true
    }

    pub(crate) fn door_open_state(&mut self, id: GlobalFormId) -> Option<i32> {
        let (base, deleted) = self
            .reference(id)
            .map(|reference| (reference.base, reference.deleted))?;
        if deleted {
            return None;
        }
        let is_door = base.is_some_and(|base| self.tag_of(base) == Some(T(b"DOOR")));
        self.reference(id)
            .map(|reference| reference.open_state(is_door))
    }

    /// The current runtime placement and visibility of a loaded reference,
    /// for a host to compare against what it last drew or collided. `None`
    /// only when the reference has never been instantiated, so its
    /// record-authored placement is still current and there is nothing to
    /// sync. A deleted reference still yields `Some`, with `visible: false`
    /// at its last known placement, the same hide signal `Disable()` alone
    /// produces: `Delete()` sets both `deleted` and `enabled`
    /// (`crate::natives::object_reference`), and the host has no other way
    /// to learn it must stop drawing and colliding with it.
    #[must_use]
    pub fn ref_placement(&self, id: GlobalFormId) -> Option<RefPlacement> {
        let state = self.reference_ref(id)?;
        Some(RefPlacement {
            position: state.position,
            rotation: state.rotation,
            scale: state.scale,
            visible: state.enabled && !state.deleted,
        })
    }

    /// Records a simulated rigid body's pose in the shared reference state.
    /// A disabled or deleted reference cannot be revived by a late physics tick.
    /// The existing position and rotation fields are already part of world saves.
    pub fn set_host_ref_pose(
        &mut self,
        id: GlobalFormId,
        position: [f32; 3],
        rotation: [f32; 3],
    ) -> bool {
        if !position
            .iter()
            .chain(rotation.iter())
            .all(|v| v.is_finite())
        {
            return false;
        }
        let Some(state) = self.reference(id) else {
            return false;
        };
        if !state.enabled || state.deleted || state.is_actor {
            return false;
        }
        state.position = position;
        state.rotation = rotation;
        true
    }

    /// Records a host-controlled actor's held or thrown pose in shared
    /// reference state. Actor movement remains transient host physics, while
    /// the latest placement is part of the existing reference save data.
    pub fn set_host_actor_pose(
        &mut self,
        id: GlobalFormId,
        position: [f32; 3],
        rotation: [f32; 3],
    ) -> bool {
        if !position
            .iter()
            .chain(rotation.iter())
            .all(|value| value.is_finite())
        {
            return false;
        }
        let Some(state) = self.reference(id) else {
            return false;
        };
        if !state.is_actor || !state.enabled || state.deleted {
            return false;
        }
        state.position = position;
        state.rotation = rotation;
        true
    }

    /// Read a live or authored base without initializing inventory or actor templates.
    pub(crate) fn reference_base_view(&self, id: GlobalFormId) -> Option<GlobalFormId> {
        if let Some(reference) = self.reference_ref(id) {
            return reference.base;
        }
        let rref = self.order.winner_ref(id)?;
        let record = self.order.record(rref);
        let location = match record.tag.as_bytes() {
            b"REFR" | b"ACHR" => record
                .find(T(b"XLCN"))
                .and_then(le_u32)
                .filter(|raw| *raw != 0),
            b"PMIS" | b"PGRE" | b"PBEA" | b"PCON" | b"PARW" => None,
            _ => return None,
        }
        .or(record.placement.cell)
        .map(|raw| self.order.resolve(rref.file, raw));
        let base = self
            .order
            .resolve(rref.file, le_u32(record.find(T(b"NAME"))?)?);
        Some(self.swapped_reference_base(id, base, location))
    }

    pub fn base_of(&mut self, id: GlobalFormId) -> Option<GlobalFormId> {
        self.reference(id).and_then(|r| r.base)
    }

    pub fn is_reference(&mut self, id: GlobalFormId) -> bool {
        self.reference(id).is_some()
    }

    /// A new reference of `base` at the position of `at`, the `PlaceAtMe`
    /// path. Returns the dynamic id.
    pub fn place_at(
        &mut self,
        base: GlobalFormId,
        at: GlobalFormId,
        disabled: bool,
    ) -> GlobalFormId {
        let (position, rotation, cell) = match self.reference(at) {
            Some(r) => (r.position, r.rotation, r.cell),
            None => ([0.0; 3], [0.0; 3], None),
        };
        self.next_dynamic += 1;
        let id = GlobalFormId {
            file: DYNAMIC_FILE,
            object: self.next_dynamic,
        };
        let mut st = RefState::new(Some(base));
        st.position = position;
        st.rotation = rotation;
        st.cell = cell;
        st.enabled = !disabled;
        st.is_actor = self.tag_of(base) == Some(T(b"NPC_"));
        let mut present = true;
        if st.is_actor {
            present = self
                .encounter_level_in_cell(cell)
                .and_then(|level| self.prepare_actor_template(id, base, level))
                .unwrap_or(false);
            if !present {
                st.enabled = false;
            }
        }
        st.inventory = self.reference_inventory(Some(id), base);
        if st
            .inventory
            .iter()
            .any(|(item, _)| self.tag_of(*item) == Some(T(b"LVLI")))
        {
            match self.encounter_level_in_cell(cell).and_then(|level| {
                self.resolve_inventory_lists(std::mem::take(&mut st.inventory), level)
            }) {
                Ok(inventory) => st.inventory = inventory,
                Err(error) => {
                    soe_trace::error!(target: "game", system = "game", form_id = %id, error = %error, "initial inventory failed");
                    st.inventory = crate::inventory::Inventory::default();
                    st.enabled = false;
                    present = false;
                }
            }
        }
        if st.is_actor && present {
            match self.default_outfit_inventory(id, base, &st.inventory) {
                Ok(inventory) => st.inventory = inventory,
                Err(_) => st.enabled = false,
            }
        }
        st.item = self.loose_item(Some(base), None, None);
        self.refs.insert(id, st);
        self.track_light_reference(id);
        id
    }

    pub(crate) fn is_player_reference(&self, id: GlobalFormId) -> bool {
        self.player == Some(id)
    }

    pub fn player(&mut self) -> GlobalFormId {
        if let Some(player) = self.player {
            return player;
        }
        let id = GlobalFormId {
            file: 0,
            object: PLAYER_OBJECT,
        };
        if self.reference(id).is_none() {
            let base = self
                .form("Player")
                .filter(|base| self.tag_of(*base) == Some(T(b"NPC_")));
            let mut state = RefState::new(base);
            state.is_actor = true;
            state.display_name = Some("Player".into());
            if let Some(base) = base {
                state.inventory = self.base_inventory(base);
                if state
                    .inventory
                    .iter()
                    .any(|(item, _)| self.tag_of(*item) == Some(T(b"LVLI")))
                {
                    state.inventory = match self
                        .resolve_inventory_lists(state.inventory, self.player_level.max(1))
                    {
                        Ok(inventory) => inventory,
                        Err(error) => {
                            soe_trace::error!(target: "game", system = "game", form_id = %id, error = %error, "initial player inventory failed");
                            crate::inventory::Inventory::default()
                        }
                    };
                }
            }
            self.refs.insert(id, state);
        }
        self.player = Some(id);
        id
    }

    pub fn item_count(&mut self, holder: GlobalFormId, item: GlobalFormId) -> i32 {
        self.reference(holder)
            .map_or(0, |r| r.inventory.count(item))
    }

    pub fn add_item(&mut self, holder: GlobalFormId, item: GlobalFormId, count: i32) {
        if let Some(r) = self.reference(holder) {
            if count >= 0 {
                let _ = r.inventory.add(item, count);
            } else {
                r.inventory.take(item, count.saturating_abs());
            }
        }
    }

    /// Remove up to `count`, preserving distinct stacks during a transfer.
    /// Missing recipients, self-transfers and overflow leave quantities unchanged.
    pub fn remove_item(
        &mut self,
        holder: GlobalFormId,
        item: GlobalFormId,
        count: i32,
        target: Option<GlobalFormId>,
    ) -> i32 {
        self.remove_item_bundle(holder, item, count, target).count()
    }

    pub(super) fn remove_item_bundle(
        &mut self,
        holder: GlobalFormId,
        item: GlobalFormId,
        count: i32,
        target: Option<GlobalFormId>,
    ) -> crate::inventory::Bundle {
        self.remove_item_bundle_with_ownership(holder, item, count, target, true)
    }

    pub(super) fn remove_item_bundle_with_ownership(
        &mut self,
        holder: GlobalFormId,
        item: GlobalFormId,
        count: i32,
        target: Option<GlobalFormId>,
        keep_ownership: bool,
    ) -> crate::inventory::Bundle {
        let moved = count
            .min(
                self.reference(holder)
                    .map_or(0, |state| state.inventory.count(item)),
            )
            .max(0);
        if moved == 0 || target == Some(holder) {
            return crate::inventory::Bundle::default();
        }
        if let Some(target) = target {
            let Some(recipient) = self.reference(target) else {
                return crate::inventory::Bundle::default();
            };
            if recipient.inventory.count(item).checked_add(moved).is_none() {
                return crate::inventory::Bundle::default();
            }
        }
        let Some(source) = self.refs.get_mut(&holder) else {
            return crate::inventory::Bundle::default();
        };
        let mut bundle = source.inventory.take(item, moved);
        if let Some(target) = target {
            bundle.unequip();
            if !keep_ownership {
                bundle.clear_ownership();
            }
            // CONTEXT: Both references and recipient capacity were checked above.
            if let Some(recipient) = self.refs.get_mut(&target) {
                let result = recipient.inventory.receive(item, bundle.clone());
                debug_assert!(result.is_ok());
            }
        }
        bundle
    }

    pub fn move_to(&mut self, id: GlobalFormId, target: GlobalFormId, offset: [f32; 3]) -> bool {
        self.move_to_with_rotation(id, target, offset, true)
    }

    pub fn move_to_with_rotation(
        &mut self,
        id: GlobalFormId,
        target: GlobalFormId,
        offset: [f32; 3],
        match_rotation: bool,
    ) -> bool {
        let Some((pos, rotation, cell)) = self
            .reference(target)
            .map(|t| (t.position, t.rotation, t.cell))
        else {
            return false;
        };
        let match_rotation = match_rotation && cell.is_some_and(|cell| self.cell_is_resident(cell));
        match self.reference(id) {
            Some(r) => {
                r.position = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
                if match_rotation {
                    r.rotation = rotation;
                }
                r.cell = cell;
                true
            }
            None => false,
        }
    }

    /// `MoveToMyEditorLocation`: back to the record's cell and transform.
    /// A dynamic reference has no record and stays put.
    pub fn move_to_editor_location(&mut self, id: GlobalFormId) -> bool {
        let Some(fresh) = self.load_ref(id) else {
            return false;
        };
        match self.reference(id) {
            Some(r) => {
                r.position = fresh.position;
                r.rotation = fresh.rotation;
                r.cell = fresh.cell;
                true
            }
            None => false,
        }
    }

    /// Reset a reference to its winning record state and base inventory.
    /// An optional loaded target supplies the destination position and cell.
    /// Actor faction and relationship state lives outside `RefState` and is
    /// retained, while actors are resurrected.
    pub fn reset_reference(&mut self, id: GlobalFormId, target: Option<GlobalFormId>) -> bool {
        let Some(mut fresh) = self.load_ref(id) else {
            return false;
        };
        let destination = if fresh.cell.is_some_and(|cell| self.cell_is_resident(cell)) {
            target
                .and_then(|target| self.reference(target))
                .map(|target| (target.position, target.cell))
        } else {
            None
        };
        if let Some((position, cell)) = destination {
            fresh.position = position;
            fresh.cell = cell;
        }
        let is_actor = fresh.is_actor;
        self.refs.insert(id, fresh);
        self.track_light_reference(id);
        if is_actor {
            let _ = self.resurrect_actor(id);
        }
        true
    }

    pub fn distance(&mut self, a: GlobalFormId, b: GlobalFormId) -> f32 {
        let (Some(pa), Some(pb)) = (
            self.reference(a).map(|r| r.position),
            self.reference(b).map(|r| r.position),
        ) else {
            return 0.0;
        };
        ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt()
    }

    /// Activate a reference and unload the source cell immediately.
    /// A default-only call suppresses the event and ignores BlockActivation.
    pub fn activate(
        &mut self,
        target: GlobalFormId,
        activator: GlobalFormId,
        send_event: bool,
        alloc: Alloc<'_>,
    ) -> bool {
        self.activate_with_unload(target, activator, send_event, false, alloc)
    }

    /// Activate with optional deferred source-cell unload. Default-only calls
    /// bypass BlockActivation, as specified by the CK Activate reference.
    /// Default processing for doors and
    /// containers unless the reference blocks activation, then `OnActivate`
    /// to the target's scripts when `send_event` is set. Returns false when
    /// the target is not a reference.
    pub fn activate_with_unload(
        &mut self,
        target: GlobalFormId,
        activator: GlobalFormId,
        send_event: bool,
        defer_unload: bool,
        alloc: Alloc<'_>,
    ) -> bool {
        if self.is_ethereal(activator) {
            return false;
        }
        let Some((blocked, base, teleport)) = self
            .reference(target)
            .map(|r| (r.activation_blocked, r.base, r.teleport))
        else {
            return false;
        };
        self.break_invisibility(activator, alloc);
        let kind = base.and_then(|b| self.tag_of(b));
        let lockable = kind == Some(T(b"DOOR")) || kind == Some(T(b"CONT"));
        let mut process = (!blocked || !send_event)
            && (!lockable || self.unlock_with_key(target, activator, alloc));
        if process {
            match kind.map(|t| *t.as_bytes()) {
                // A door with teleport data is a load door, the kind that
                // fires a load screen in vanilla. A door without it simply
                // swings, and both exist in the shipped files.
                Some([b'D', b'O', b'O', b'R']) => {
                    if teleport.is_some() && activator == self.player() && defer_unload {
                        if let Some(way) = self.arm_doorway(target) {
                            self.commit_doorway(&way);
                        }
                    } else if let Some((far_door, landing)) = teleport {
                        let far_cell = self.reference(far_door).and_then(|d| d.cell);
                        if let Some(r) = self.reference(activator) {
                            r.position = landing.position;
                            r.rotation = landing.rotation;
                            r.cell = far_cell;
                        }
                        // Only the player's walk swaps what is resident. An
                        // actor going through a door moves without dragging
                        // the world after it.
                        if activator == self.player() {
                            self.change_cell(far_cell);
                        }
                    } else if let Some(r) = self.reference(target) {
                        r.opened = !r.opened;
                    }
                }
                Some([b'C', b'O', b'N', b'T']) => {
                    if let Some(r) = self.reference(target) {
                        r.opened = true;
                    }
                }
                Some([b'F', b'U', b'R', b'N']) => {
                    self.activate_furniture(target, activator);
                }
                _ if base.is_some_and(|base| self.is_inventory_base(base)) => {
                    process = self.add_item_notified(activator, target, 1, alloc);
                }
                _ => {}
            }
        }
        if send_event {
            let who = self.form_value(Some(activator), alloc);
            let object = self.object_of(Handle::Form(target), alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnActivate".into(),
                args: vec![who],
            });
        }
        process
    }
}
