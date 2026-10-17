// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared headless graph driver, moved from the probe comparison host.
//!
//! Records select actor rigs and furniture partners. NIF extra data selects
//! object projects. Existing HKX readers resolve projects, characters, graphs
//! and clips. Annotation conversion follows the local hkbClipGenerator flags.
//! See docs/game.md for the local sources and installed quest evidence.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use crate::Game;
use soe_bsa::{Archive, Vfs};
use soe_esm::GlobalFormId;
use soe_hkx::character::{CharacterData, ProjectData};
use soe_hkx::{
    behavior::{BehaviorGraph, VariableType},
    interp::{Clip, ClipLibrary, Interpreter},
    skeleton::AnimationContainer,
    Packfile, QsTransform,
};
use std::rc::Rc;

/// A graph asset or evaluation failure.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(String);

/// A graph, linked behaviors, decoded clips and its reference pose.
pub struct Assets {
    pub graph: BehaviorGraph,
    pub clips: ClipLibrary,
    pub pose: Vec<QsTransform>,
}

fn execution(error: impl std::fmt::Display) -> Error {
    Error(error.to_string())
}

fn graph(archive: &Vfs, path: &str) -> Result<BehaviorGraph, Error> {
    let bytes = archive.read(path).map_err(execution)?;
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).map_err(execution)?).map_err(execution)
}

fn asset_path(base: &str, name: &str) -> Result<String, Error> {
    let path = format!("{base}{}", name.replace('\\', "/"));
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(execution("asset path leaves the archive"));
                }
            }
            _ => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

impl Assets {
    /// Load an explicitly selected probe graph from the animation archive.
    pub fn load(path: &str, data: &Path) -> Result<Self, Error> {
        let mut archive = Vfs::new();
        archive
            .add_archive(Archive::open(&data.join("Skyrim - Animations.bsa")).map_err(execution)?);
        let normalized = path.replace('\\', "/").to_ascii_lowercase();
        let base = normalized
            .split("behaviors/")
            .next()
            .filter(|base| *base != normalized)
            .ok_or_else(|| execution("graph path must contain behaviors/"))?;
        Self::from_vfs(
            &archive,
            &normalized,
            base,
            &format!("{base}character assets/skeleton.hkx"),
        )
    }

    fn from_vfs(archive: &Vfs, path: &str, base: &str, skeleton: &str) -> Result<Self, Error> {
        let mut graph = graph(archive, path)?;
        let mut failure = None;
        let linked = graph.link_all(16, |name| {
            match asset_path(base, name).and_then(|path| self::graph(archive, &path)) {
                Ok(graph) => Some(graph),
                Err(error) => {
                    failure = Some(error);
                    None
                }
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        if !linked.missing.is_empty() || !linked.failed.is_empty() {
            return Err(execution(format!("graph linking: {linked:?}")));
        }
        let mut clips = ClipLibrary::new();
        for name in graph.clip_animation_names() {
            let path = asset_path(base, &name)?;
            if !archive.exists(&path) {
                continue;
            }
            let bytes = archive.read(&path).map_err(execution)?;
            let container =
                AnimationContainer::from_packfile(&Packfile::parse(&bytes).map_err(execution)?)
                    .map_err(execution)?;
            clips.insert(Clip::from_container(&name, &container).map_err(execution)?);
        }
        let bytes = archive.read(skeleton).map_err(execution)?;
        let container =
            AnimationContainer::from_packfile(&Packfile::parse(&bytes).map_err(execution)?)
                .map_err(execution)?;
        let pose = container
            .skeletons
            .into_iter()
            .next()
            .ok_or_else(|| execution("missing actor skeleton"))?
            .reference_pose;
        annotation_triggers(&mut graph, &clips);
        Ok(Self { graph, clips, pose })
    }
}

/// One reference's graph, variables and ordered request cursors.
pub struct Host<'a> {
    graph: &'a BehaviorGraph,
    vm: Interpreter<'a>,
    actor: GlobalFormId,
    last_ints: BTreeMap<String, i32>,
    last_bools: BTreeMap<String, bool>,
    last_floats: BTreeMap<String, f32>,
    event_cursor: usize,
    reports: BTreeSet<usize>,
    request_cursor: usize,
    annotation_reports: Vec<String>,
}

impl<'a> Host<'a> {
    /// Bind one reference to borrowed graph assets.
    pub fn new(assets: &'a Assets, actor: GlobalFormId) -> Self {
        Self {
            graph: &assets.graph,
            vm: Interpreter::new(&assets.graph, &assets.clips, assets.pose.clone()),
            actor,
            last_ints: BTreeMap::new(),
            last_bools: BTreeMap::new(),
            last_floats: BTreeMap::new(),
            event_cursor: 0,
            reports: BTreeSet::new(),
            request_cursor: 0,
            annotation_reports: Vec::new(),
        }
    }

    /// Advance the graph and report emitted events through the game bridge.
    pub fn update(&mut self, game: &mut Game, dt: f32) -> Result<(), Error> {
        let limit = game.world().animation_requests.len();
        self.update_through(game, dt, limit)
    }

    fn update_through(
        &mut self,
        game: &mut Game,
        dt: f32,
        request_limit: usize,
    ) -> Result<(), Error> {
        self.annotation_reports.clear();
        let mut world = game.world_mut();
        let state = world
            .reference(self.actor)
            .ok_or_else(|| execution("missing animation actor"))?;
        for (name, value) in &state.anim_ints {
            if self.last_ints.get(name) != Some(value) {
                let index = self
                    .graph
                    .data
                    .variable_names
                    .iter()
                    .position(|variable| variable.eq_ignore_ascii_case(name))
                    .ok_or_else(|| execution(format!("unknown graph variable {name}")))?;
                if !self.vm.variables.set_word(index, *value) {
                    return Err(execution(format!("mistyped graph variable {name}")));
                }
            }
        }
        for (name, value) in &state.anim_bools {
            if self.last_bools.get(name) != Some(value) {
                let index = self
                    .graph
                    .data
                    .variable_names
                    .iter()
                    .position(|variable| variable.eq_ignore_ascii_case(name))
                    .ok_or_else(|| execution(format!("unknown graph variable {name}")))?;
                if !self.vm.variables.set_bool(index, *value) {
                    return Err(execution(format!("mistyped graph variable {name}")));
                }
            }
        }
        for (name, value) in &state.anim_floats {
            if self.last_floats.get(name) != Some(value) {
                let index = self
                    .graph
                    .data
                    .variable_names
                    .iter()
                    .position(|variable| variable.eq_ignore_ascii_case(name))
                    .ok_or_else(|| execution(format!("unknown graph variable {name}")))?;
                if !self.vm.variables.set_real(index, *value) {
                    return Err(execution(format!("mistyped graph variable {name}")));
                }
            }
        }
        for request in &world.animation_requests[self.request_cursor..request_limit] {
            if request.target != self.actor {
                continue;
            }
            let event = request_event(&world, &request.animation);
            if !self.vm.send_event_named(&event) {
                return Err(execution(format!("unknown graph event {event}")));
            }
        }
        self.request_cursor = request_limit;
        for (offset, (actor, event)) in world.animation_events[self.event_cursor..]
            .iter()
            .enumerate()
        {
            if self.reports.remove(&(self.event_cursor + offset)) {
                continue;
            }
            if *actor == self.actor {
                let event = request_event(&world, event);
                if !self.vm.send_event_named(&event) {
                    return Err(execution(format!("unknown graph event {event}")));
                }
            }
        }
        self.event_cursor = world.animation_events.len();
        self.vm.update(dt);
        if !self.vm.unsupported().missing_clips.is_empty() {
            return Err(execution(format!(
                "active clips missing: {:?}",
                self.vm.unsupported().missing_clips
            )));
        }
        let state = world
            .reference(self.actor)
            .ok_or_else(|| execution("missing animation actor"))?;
        for index in 0..self.vm.variables.len() {
            let Some(name) = self.vm.variables.name(index) else {
                continue;
            };
            match self.vm.variables.ty(index) {
                Some(VariableType::Bool) => {
                    state.anim_bools.insert(
                        name.to_ascii_lowercase(),
                        self.vm.variables.bool(index).unwrap_or_default(),
                    );
                }
                Some(VariableType::Int8 | VariableType::Int16 | VariableType::Int32) => {
                    state.anim_ints.insert(
                        name.to_ascii_lowercase(),
                        self.vm.variables.word(index).unwrap_or_default(),
                    );
                }
                Some(VariableType::Real) => {
                    state.anim_floats.insert(
                        name.to_ascii_lowercase(),
                        self.vm.variables.real(index).unwrap_or_default(),
                    );
                }
                _ => {}
            }
        }
        self.last_ints.clone_from(&state.anim_ints);
        self.last_bools.clone_from(&state.anim_bools);
        self.last_floats.clone_from(&state.anim_floats);
        let events = self.vm.take_events();
        let emitted: BTreeSet<_> = events.iter().map(|event| event.id).collect();
        self.annotation_reports = self
            .vm
            .take_annotation_events()
            .into_iter()
            .filter(|id| emitted.contains(id))
            .filter_map(|id| {
                usize::try_from(id)
                    .ok()
                    .and_then(|id| self.graph.data.event_names.get(id))
                    .cloned()
            })
            .collect();
        drop(world);
        for event in events {
            if let Ok(index) = usize::try_from(event.id) {
                if let Some(name) = self.graph.data.event_names.get(index) {
                    self.reports.insert(game.world().animation_events.len());
                    game.report_animation_event(self.actor, name);
                }
            }
        }
        Ok(())
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes.split(|byte| *byte == 0).next().unwrap_or_default()).into_owned()
}

fn behavior_project(nif: &soe_nif::NifFile) -> Result<Option<String>, Error> {
    for block in &nif.blocks {
        let soe_nif::blocks::Block::Unknown { type_name, data } = block else {
            continue;
        };
        if type_name != "BSBehaviorGraphExtraData" {
            continue;
        }
        let index = data
            .get(4..8)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| execution("truncated behavior graph extra data"))?;
        return nif
            .header
            .strings
            .get(index as usize)
            .cloned()
            .map(Some)
            .ok_or_else(|| execution("behavior graph string index is out of bounds"));
    }
    Ok(None)
}

fn idle_record<'a>(world: &'a crate::World, name: &str) -> Option<&'a soe_esm::Record> {
    world
        .form(name)
        .and_then(|id| world.record(id))
        .filter(|record| record.tag == soe_esm::Tag(*b"IDLE"))
}

fn request_event(world: &crate::World, name: &str) -> String {
    idle_record(world, name)
        .and_then(|record| record.find(soe_esm::Tag(*b"ENAM")))
        .map(text)
        .unwrap_or_else(|| name.to_string())
}

fn annotation_triggers(graph: &mut BehaviorGraph, clips: &ClipLibrary) {
    use soe_hkx::behavior::{ClipTrigger, EventInfo, EventProperty, NodeKind};
    for node in &mut graph.nodes {
        let NodeKind::Clip(generator) = &mut node.kind else {
            continue;
        };
        if generator.flags & 16 != 0 {
            continue;
        }
        let Some(clip) = clips.get(&generator.animation_name) else {
            continue;
        };
        for annotation in clip
            .animation
            .annotation_tracks
            .iter()
            .flat_map(|track| &track.annotations)
        {
            let time = annotation.time - generator.crop_start_amount_local_time;
            if !time.is_finite()
                || time < 0.0
                || annotation.time > clip.duration() - generator.crop_end_amount_local_time
            {
                continue;
            }
            let index = graph
                .data
                .event_names
                .iter()
                .position(|name| name == &annotation.text)
                .unwrap_or_else(|| {
                    let index = graph.data.event_names.len();
                    graph.data.event_names.push(annotation.text.clone());
                    graph
                        .data
                        .event_infos
                        .resize(index + 1, EventInfo { flags: 0 });
                    index
                });
            let Ok(id) = i32::try_from(index) else {
                continue;
            };
            if generator.triggers.iter().any(|trigger| {
                trigger.event.id == id
                    && !trigger.relative_to_end_of_clip
                    && trigger.local_time == time
            }) {
                continue;
            }
            generator.triggers.push(ClipTrigger {
                local_time: time,
                event: EventProperty {
                    id,
                    has_payload: false,
                },
                relative_to_end_of_clip: false,
                acyclic: false,
                is_annotation: true,
            });
        }
    }
}

fn animation_furniture(world: &mut crate::World, actor: GlobalFormId) -> Option<GlobalFormId> {
    if let Some((furniture, _)) = world.furniture_seat(actor) {
        return Some(furniture);
    }
    let state = world.actor_ref(actor)?;
    let package = state.ai.scene_package.or(state.ai.package)?;
    let package_def = crate::package::Package::from_record(world.record(package)?).ok()?;
    let mut targets = BTreeSet::new();
    for input in package_def.inputs {
        let crate::package::DataValue::Target(target) = input.value else {
            continue;
        };
        let crate::package::Target::Reference(raw) = target.target else {
            continue;
        };
        let Some(target) = world.resolve_from(package, raw) else {
            continue;
        };
        if world.base_of(target).and_then(|base| world.tag_of(base)) == Some(soe_esm::Tag(*b"FURN"))
        {
            targets.insert(target);
        }
    }
    (targets.len() == 1)
        .then(|| targets.into_iter().next())
        .flatten()
}

struct Instance {
    assets: Rc<Assets>,
    state: Option<HostState>,
    started: bool,
}

struct HostState {
    vm: soe_hkx::interp::InterpreterState,
    last_ints: BTreeMap<String, i32>,
    last_bools: BTreeMap<String, bool>,
    last_floats: BTreeMap<String, f32>,
    event_cursor: usize,
    reports: BTreeSet<usize>,
    request_cursor: usize,
}

impl Host<'_> {
    fn into_state(self) -> HostState {
        HostState {
            vm: self.vm.into_state(),
            last_ints: self.last_ints,
            last_bools: self.last_bools,
            last_floats: self.last_floats,
            event_cursor: self.event_cursor,
            reports: self.reports,
            request_cursor: self.request_cursor,
        }
    }
}

impl<'a> Host<'a> {
    fn from_state(assets: &'a Assets, actor: GlobalFormId, state: HostState) -> Self {
        Self {
            graph: &assets.graph,
            vm: Interpreter::from_state(&assets.graph, &assets.clips, state.vm),
            actor,
            last_ints: state.last_ints,
            last_bools: state.last_bools,
            last_floats: state.last_floats,
            event_cursor: state.event_cursor,
            reports: state.reports,
            request_cursor: state.request_cursor,
            annotation_reports: Vec::new(),
        }
    }
}

/// Asset-backed headless graph instances, using the same evaluator as probes.
pub struct Driver {
    vfs: Vfs,
    assets: BTreeMap<String, Rc<Assets>>,
    instances: BTreeMap<GlobalFormId, Instance>,
    cursor: usize,
    partners: BTreeMap<GlobalFormId, Option<GlobalFormId>>,
}

impl Driver {
    pub(crate) fn reset(&mut self, request_cursor: usize) {
        self.instances.clear();
        self.partners.clear();
        self.cursor = request_cursor;
    }

    /// Open the load order's archives with loose files taking precedence.
    pub fn open(data: &Path, order: &soe_esm::LoadOrder) -> Result<Self, Error> {
        let mut vfs = Vfs::new();
        let mut archives = std::fs::read_dir(data)
            .map_err(execution)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(execution)?;
        archives.sort();
        for file in order.files() {
            let stem = Path::new(&file.name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let plain = format!("{stem}.bsa");
            let prefix = format!("{stem} - ");
            for path in &archives {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if name == plain
                    || (name.starts_with(&prefix)
                        && path
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("bsa")))
                {
                    vfs.add_archive_path(path).map_err(execution)?;
                }
            }
        }
        vfs.add_loose_root(data).map_err(execution)?;
        Ok(Self {
            vfs,
            assets: BTreeMap::new(),
            instances: BTreeMap::new(),
            cursor: 0,
            partners: BTreeMap::new(),
        })
    }

    fn load_project(&self, path: &str) -> Result<Assets, Error> {
        let bytes = self.vfs.read(path).map_err(execution)?;
        let project = ProjectData::from_packfile(&Packfile::parse(&bytes).map_err(execution)?)
            .map_err(execution)?;
        let base = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        let base = format!("{base}/");
        let join = |prefix: &str, name: &str| {
            if name
                .replace('\\', "/")
                .to_ascii_lowercase()
                .starts_with("meshes/")
            {
                asset_path("", name)
            } else if name
                .replace('\\', "/")
                .to_ascii_lowercase()
                .starts_with("actors/")
            {
                asset_path("meshes/", name)
            } else {
                asset_path(
                    &base,
                    &format!("{}/{}", prefix.trim_matches(['/', '\\']), name),
                )
            }
        };
        let name = project
            .character_filenames
            .first()
            .ok_or_else(|| execution("project has no character"))?;
        let character_path = join(&project.character_path, name)?;
        let bytes = self.vfs.read(&character_path).map_err(execution)?;
        let character = CharacterData::from_packfile(&Packfile::parse(&bytes).map_err(execution)?)
            .map_err(execution)?;
        let behavior = join(&project.behavior_path, &character.behavior_filename)?;
        let skeleton = join(&project.character_path, &character.rig_name)?;
        Assets::from_vfs(&self.vfs, &behavior, &base, &skeleton)
    }

    fn graph_for(
        &self,
        world: &mut crate::World,
        target: GlobalFormId,
        _animation: &str,
    ) -> Result<Option<String>, Error> {
        let Some(base) = world.base_of(target) else {
            return Ok(None);
        };
        let Some(record) = world.record(base) else {
            return Ok(None);
        };
        if record.tag == soe_esm::Tag(*b"NPC_") {
            let rig = world.actor_animation_rig(target).map_err(execution)?;
            return rig
                .and_then(|rig| rig.behavior)
                .map(|path| asset_path("meshes/", &path).map(|path| path.to_ascii_lowercase()))
                .transpose();
        }
        let Some(model) = record.find(soe_esm::Tag(*b"MODL")).map(text) else {
            return Ok(None);
        };
        let bytes = self
            .vfs
            .read(&asset_path("meshes/", &model)?)
            .map_err(execution)?;
        let nif = soe_nif::parse(&bytes).map_err(execution)?;
        behavior_project(&nif)?
            .map(|name| asset_path("meshes/", &name).map(|path| path.to_ascii_lowercase()))
            .transpose()
    }

    fn install(&mut self, game: &Game, target: GlobalFormId, animation: &str) -> Result<(), Error> {
        if self.instances.contains_key(&target) {
            return Ok(());
        }
        let Some(path) = self.graph_for(&mut game.world_mut(), target, animation)? else {
            return Ok(());
        };
        let assets = if let Some(assets) = self.assets.get(&path) {
            Rc::clone(assets)
        } else {
            let assets = if path.contains("/behaviors/") {
                let base = path.split("behaviors/").next().unwrap_or_default();
                let skeleton = format!("{base}character assets/skeleton.hkx");
                Assets::from_vfs(&self.vfs, &path, base, &skeleton)?
            } else {
                self.load_project(&path)?
            };
            let assets = Rc::new(assets);
            self.assets.insert(path, Rc::clone(&assets));
            assets
        };
        let mut host = Host::new(&assets, target);
        host.request_cursor = self.cursor;
        host.event_cursor = game.world().animation_events.len();
        let state = Some(host.into_state());
        let instance = Instance {
            assets,
            state,
            started: false,
        };
        self.instances.insert(target, instance);
        Ok(())
    }

    fn forward_annotations(&mut self, game: &mut Game, source: GlobalFormId, events: &[String]) {
        let partner = self
            .partners
            .get(&source)
            .copied()
            .flatten()
            .or_else(|| animation_furniture(&mut game.world_mut(), source));
        let Some(partner) = partner else { return };
        for name in events {
            if let Err(error) = self.install(game, partner, name) {
                game.vm.log.push(
                    game.vm.time(),
                    format!("Headless animation {partner}: {error}"),
                );
                continue;
            }
            if self.instances.get(&partner).is_some_and(|instance| {
                instance
                    .assets
                    .graph
                    .data
                    .event_names
                    .iter()
                    .any(|event| event == name)
            }) {
                game.world_mut().request_animation(partner, name);
            }
        }
    }

    /// Drain new requests and advance active graphs using gameplay seconds.
    pub(crate) fn step(&mut self, game: &mut Game, dt: f32) {
        for round in 0..8 {
            let requests = game.world().animation_requests[self.cursor..].to_vec();
            for request in requests {
                self.partners.insert(request.target, request.partner);
                if let Err(error) = self.install(game, request.target, &request.animation) {
                    game.vm.log.push(
                        game.vm.time(),
                        format!("Headless animation {}: {error}", request.target),
                    );
                }
            }
            self.cursor = game.world().animation_requests.len();
            let targets: Vec<_> = self.instances.keys().copied().collect();
            for target in targets {
                let starting = self
                    .instances
                    .get_mut(&target)
                    .is_some_and(|instance| !std::mem::replace(&mut instance.started, true));
                if starting {
                    let loaded = game.world_mut().animation_graph_loaded(target);
                    if loaded {
                        game.run_until_idle(64);
                    }
                }
                if !game.world().is_reference_3d_loaded(target) {
                    self.instances.remove(&target);
                    self.partners.remove(&target);
                    game.report_animation_unloaded(target);
                    continue;
                }
                let mut forwarded = Vec::new();
                if let Some(instance) = self.instances.get_mut(&target) {
                    let Some(state) = instance.state.take() else {
                        continue;
                    };
                    let mut host = Host::from_state(&instance.assets, target, state);
                    let result =
                        host.update_through(game, if round == 0 { dt } else { 0.0 }, self.cursor);
                    forwarded.clone_from(&host.annotation_reports);
                    instance.state = Some(host.into_state());
                    if let Err(error) = result {
                        game.vm.log.push(
                            game.vm.time(),
                            format!("Headless animation {target}: {error}"),
                        );
                        self.instances.remove(&target);
                        game.report_animation_unloaded(target);
                        continue;
                    }
                }
                self.forward_annotations(game, target, &forwarded);
            }
            if game.world().animation_requests.len() == self.cursor {
                break;
            }
        }
    }
}

#[cfg(test)]
#[path = "../../hkx/tests/common/mod.rs"]
mod test_graph;

#[cfg(test)]
mod tests {
    use super::*;
    use soe_hkx::behavior::NodeKind;

    use super::test_graph as common;

    fn assets() -> Assets {
        let bytes = common::graph(&common::Root::Machine, "a", "b");
        let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
        let root = graph.root.unwrap();
        let NodeKind::StateMachine(machine) = &mut graph.nodes[root.0].kind else {
            panic!("machine");
        };
        machine.wildcard_transitions.clear();
        machine.event_to_send_when_state_or_transition_changes.id = -1;
        let mut transition = machine.states[0].transitions[0].clone();
        for state in &mut machine.states {
            state.generator = None;
            state.enter_notify_events.clear();
            state.exit_notify_events.clear();
            state.transitions.clear();
        }
        transition.to_state_id = 1;
        transition.transition = None;
        transition.condition = None;
        transition.flags = 0;
        transition.event_id = 1;
        machine.states[0].transitions.push(transition.clone());
        transition.to_state_id = 0;
        machine.states[1].transitions.push(transition);
        Assets {
            graph,
            clips: ClipLibrary::new(),
            pose: vec![],
        }
    }

    #[test]
    fn anm_54_asset_paths_resolve_shared_clips_and_reject_escape() {
        assert_eq!(
            asset_path("meshes/actors/character/", "..\\SharedKillMoves\\clip.hkx").unwrap(),
            "meshes/actors/SharedKillMoves/clip.hkx"
        );
        assert!(asset_path("meshes/actors/character/", "../../../../clip.hkx").is_err());
    }

    #[test]
    fn anm_54_host_does_not_reinject_graph_reports() {
        let assets = assets();
        let root = assets.graph.root.unwrap();
        let mut game = Game::new(soe_esm::LoadOrder::new());
        let actor = game.world_mut().player();
        let mut host = Host::new(&assets, actor);
        host.update(&mut game, 0.0).unwrap();
        game.world_mut()
            .animation_events
            .push((actor, "walkStart".into()));
        host.update(&mut game, 0.0).unwrap();
        assert_eq!(host.vm.current_state(root), Some(1));
        host.update(&mut game, 0.0).unwrap();
        assert_eq!(host.vm.current_state(root), Some(1));
    }

    #[test]
    fn anm_54_host_maps_lowercase_variables_and_rejects_unknown_names() {
        let assets = assets();
        let mut game = Game::new(soe_esm::LoadOrder::new());
        let actor = game.world_mut().player();
        let mut host = Host::new(&assets, actor);
        game.world_mut()
            .reference(actor)
            .unwrap()
            .anim_ints
            .insert("istate".into(), 7);
        host.update(&mut game, 0.0).unwrap();
        assert_eq!(host.vm.variables.word(1), Some(7));
        game.world_mut()
            .reference(actor)
            .unwrap()
            .anim_ints
            .insert("missing".into(), 1);
        assert!(host
            .update(&mut game, 0.0)
            .unwrap_err()
            .to_string()
            .contains("unknown graph variable missing"));
    }
    fn opening_assets() -> Assets {
        use soe_hkx::animation::SplineCompressedAnimation;
        use soe_hkx::behavior::{ClipTrigger, EventProperty};
        let mut assets = assets();
        assets.graph.data.event_names[0] = "Begin".into();
        assets.graph.data.event_names[1] = "Open".into();
        let clip_id = assets
            .graph
            .nodes
            .iter()
            .find(|node| node.name == "WalkClip")
            .unwrap()
            .id;
        let NodeKind::Clip(clip) = &mut assets.graph.nodes[clip_id.0].kind else {
            panic!("clip")
        };
        clip.triggers = vec![ClipTrigger {
            local_time: 0.0,
            event: EventProperty {
                id: 0,
                has_payload: false,
            },
            relative_to_end_of_clip: false,
            acyclic: false,
            is_annotation: false,
        }];
        let NodeKind::StateMachine(machine) =
            &mut assets.graph.nodes[assets.graph.root.unwrap().0].kind
        else {
            panic!("machine")
        };
        machine.states[1].generator = Some(clip_id);
        let mut data = vec![0, 1, 0, 0];
        data.extend_from_slice(&0.0_f32.to_le_bytes());
        assets.clips.insert(
            Clip::from_parts(
                "b",
                SplineCompressedAnimation {
                    duration: 1.0,
                    num_transform_tracks: 1,
                    num_float_tracks: 0,
                    annotation_tracks: vec![],
                    num_frames: 2,
                    num_blocks: 1,
                    max_frames_per_block: 2,
                    mask_and_quantization_size: 4,
                    block_duration: 1.0,
                    block_inverse_duration: 1.0,
                    frame_duration: 1.0,
                    block_offsets: vec![0],
                    float_block_offsets: vec![8],
                    transform_offsets: vec![],
                    float_offsets: vec![],
                    data,
                    endian: 0,
                    extracted_motion: None,
                },
                &[0],
            )
            .unwrap(),
        );
        assets.pose = vec![QsTransform::IDENTITY];
        assets
    }

    #[test]
    fn ply_03_tick_delivers_opening_clip_begin_to_a_registered_script() {
        use soe_papyrus::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};
        use soe_papyrus::Opcode::{CallMethod, CallStatic, Return};
        let script = ScriptBuilder::new("OpeningListener", "ObjectReference")
            .function(
                FunctionBuilder::new("OnInit", "None")
                    .local("ok", "Bool")
                    .ins(
                        CallMethod,
                        vec![
                            s("RegisterForAnimationEvent"),
                            ident("self"),
                            ident("ok"),
                            ident("self"),
                            s("Begin"),
                        ],
                    )
                    .ins(Return, vec![none()])
                    .build(),
            )
            .function(
                FunctionBuilder::new("OnAnimationEvent", "None")
                    .param("source", "ObjectReference")
                    .param("event", "String")
                    .local("unused", "None")
                    .ins(
                        CallStatic,
                        vec![s("Debug"), s("Trace"), ident("unused"), ident("event")],
                    )
                    .ins(Return, vec![none()])
                    .build(),
            )
            .bytes();
        let mut game = Game::new(soe_esm::LoadOrder::new());
        let actor = game.player();
        game.load_script(&script).unwrap();
        let object = game.object_of_form(actor);
        game.vm.attach(object, "OpeningListener").unwrap();
        game.send_event(actor, "OnInit", &[]);
        game.run_until_idle(64);
        assert_eq!(game.world().animation_listeners.len(), 1);
        let assets = Rc::new(opening_assets());
        let state = Some(Host::new(&assets, actor).into_state());
        let driver = Driver {
            vfs: Vfs::new(),
            assets: BTreeMap::new(),
            instances: BTreeMap::from([(
                actor,
                Instance {
                    assets,
                    state,
                    started: false,
                },
            )]),
            cursor: 0,
            partners: BTreeMap::new(),
        };
        game.set_headless_animations(driver);
        game.world_mut().request_animation(actor, "Open");
        game.tick(0.1);
        assert!(
            game.vm
                .log
                .entries
                .iter()
                .any(|entry| entry.message == "Begin"),
            "{:?}",
            game.vm.log.entries
        );
        game.tick(0.1);
        assert_eq!(
            game.world()
                .animation_events
                .iter()
                .filter(|(_, event)| event == "Begin")
                .count(),
            1
        );
    }

    #[test]
    fn ply_03_rebinding_preserves_clip_playhead_and_pending_events() {
        let assets = opening_assets();
        let actor = GlobalFormId {
            file: 0,
            object: 0x14,
        };
        let mut host = Host::new(&assets, actor);
        host.vm.send_event_named("Open");
        host.vm.update(0.2);
        let before = host.vm.active_clips();
        let mut host = Host::from_state(&assets, actor, host.into_state());
        assert_eq!(host.vm.active_clips(), before);
        assert_eq!(
            host.vm
                .take_events()
                .iter()
                .filter(|event| event.id == 0)
                .count(),
            1
        );
        host.vm.update(0.3);
        assert!(host.vm.active_clips()[0].1 > before[0].1);
    }

    #[test]
    fn ply_03_graph_load_binds_base_script_before_the_first_clip_event() {
        use crate::vmad::{ScriptEntry, Vmad};
        use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
        use soe_papyrus::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};
        use soe_papyrus::Opcode::{CallMethod, CallStatic, Return};

        let script = ScriptBuilder::new("LoadListener", "ObjectReference")
            .function(
                FunctionBuilder::new("OnLoad", "None")
                    .local("ok", "Bool")
                    .local("unused", "None")
                    .ins(
                        CallMethod,
                        vec![
                            s("RegisterForAnimationEvent"),
                            ident("self"),
                            ident("ok"),
                            ident("self"),
                            s("Begin"),
                        ],
                    )
                    .ins(
                        CallStatic,
                        vec![s("Debug"), s("Trace"), ident("unused"), s("loaded")],
                    )
                    .ins(Return, vec![none()])
                    .build(),
            )
            .function(
                FunctionBuilder::new("OnAnimationEvent", "None")
                    .param("source", "ObjectReference")
                    .param("event", "String")
                    .local("unused", "None")
                    .ins(
                        CallStatic,
                        vec![s("Debug"), s("Trace"), ident("unused"), ident("event")],
                    )
                    .ins(Return, vec![none()])
                    .build(),
            )
            .bytes();
        let attachment = Vmad {
            scripts: vec![ScriptEntry::new("LoadListener")],
            ..Vmad::default()
        };
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(Tag(*b"FURN"), 0x800).sub(b"VMAD", attachment.write(Tag(*b"FURN"))),
        );
        plugin.push_top(Record::new(Tag(*b"REFR"), 0x801).sub(b"NAME", 0x800u32.to_le_bytes()));
        let target = GlobalFormId {
            file: 0,
            object: 0x801,
        };
        let mut order = LoadOrder::new();
        order.add("Opening.esm", plugin).unwrap();
        let mut game = Game::new(order);
        game.load_script(&script).unwrap();
        let assets = Rc::new(opening_assets());
        let state = Some(Host::new(&assets, target).into_state());
        game.set_headless_animations(Driver {
            vfs: Vfs::new(),
            assets: BTreeMap::new(),
            instances: BTreeMap::from([(
                target,
                Instance {
                    assets,
                    state,
                    started: false,
                },
            )]),
            cursor: 0,
            partners: BTreeMap::new(),
        });
        assert!(!game.world().is_reference_3d_loaded(target));
        game.world_mut().request_animation(target, "Open");
        game.tick(0.1);
        game.tick(0.1);
        assert!(game.world().is_reference_3d_loaded(target));
        let messages: Vec<_> = game
            .vm
            .log
            .entries
            .iter()
            .map(|entry| entry.message.as_str())
            .collect();
        assert_eq!(messages, ["loaded", "Begin"]);
    }

    #[test]
    fn ply_03_idle_request_resolves_the_record_event() {
        let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
        let mut idle = soe_esm::Record::new(soe_esm::Tag(*b"IDLE"), 0x800);
        idle.subrecords.push(soe_esm::Subrecord::new(
            soe_esm::Tag(*b"EDID"),
            b"OpeningIdle\0".to_vec(),
        ));
        idle.subrecords.push(soe_esm::Subrecord::new(
            soe_esm::Tag(*b"ENAM"),
            b"Open\0".to_vec(),
        ));
        plugin.push_top(idle);
        let mut order = soe_esm::LoadOrder::new();
        order.add("Opening.esm", plugin).unwrap();
        let mut game = Game::new(order);
        let actor = game.player();
        let assets = opening_assets();
        let mut host = Host::new(&assets, actor);
        game.world_mut().request_animation(actor, "OpeningIdle");
        host.update(&mut game, 0.1).unwrap();
        assert!(game
            .world()
            .animation_events
            .iter()
            .any(|(source, event)| *source == actor && event == "Begin"));
    }

    #[test]
    fn ply_03_world_restore_resets_graphs_without_replaying_old_requests() {
        let mut game = Game::new(soe_esm::LoadOrder::new());
        let actor = game.player();
        let saved = game.save_state().unwrap();
        let assets = Rc::new(opening_assets());
        let state = Some(Host::new(&assets, actor).into_state());
        game.set_headless_animations(Driver {
            vfs: Vfs::new(),
            assets: BTreeMap::new(),
            instances: BTreeMap::from([(
                actor,
                Instance {
                    assets,
                    state,
                    started: false,
                },
            )]),
            cursor: 0,
            partners: BTreeMap::new(),
        });
        game.world_mut().request_animation(actor, "Open");
        game.tick(0.1);
        assert!(!game.world().animation_events.is_empty());
        assert!(game.load_state(b"invalid save").is_err());
        assert_eq!(
            game.headless_animations.as_ref().unwrap().instances.len(),
            1
        );
        game.load_state(&saved).unwrap();
        assert!(game
            .headless_animations
            .as_ref()
            .unwrap()
            .instances
            .is_empty());
        game.tick(0.1);
        assert!(game.world().animation_events.is_empty());
        assert_eq!(game.headless_animations.as_ref().unwrap().cursor, 1);
    }

    #[test]
    fn ply_03_cell_unload_stops_its_graph_without_reloading_it_on_the_next_tick() {
        use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
        let target = GlobalFormId {
            file: 0,
            object: 0x801,
        };
        let cell = GlobalFormId {
            file: 0,
            object: 0x802,
        };
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"FURN"), 0x800));
        plugin.push_top(Record::new(Tag(*b"CELL"), cell.object).sub(b"DATA", [1, 0]));
        let mut reference =
            Record::new(Tag(*b"REFR"), target.object).sub(b"NAME", 0x800u32.to_le_bytes());
        reference.placement.cell = Some(cell.object);
        plugin.push_top(reference);
        let mut order = LoadOrder::new();
        order.add("Opening.esm", plugin).unwrap();
        let mut game = Game::new(order);
        game.load_cell(cell);
        let assets = Rc::new(opening_assets());
        let state = Some(Host::new(&assets, target).into_state());
        game.set_headless_animations(Driver {
            vfs: Vfs::new(),
            assets: BTreeMap::new(),
            instances: BTreeMap::from([(
                target,
                Instance {
                    assets,
                    state,
                    started: false,
                },
            )]),
            cursor: 0,
            partners: BTreeMap::new(),
        });
        game.world_mut().request_animation(target, "Open");
        game.tick(0.1);
        assert_eq!(
            game.headless_animations.as_ref().unwrap().instances.len(),
            1
        );
        game.change_cell(None);
        game.tick(0.1);
        assert!(!game.world().is_reference_3d_loaded(target));
        assert!(game
            .headless_animations
            .as_ref()
            .unwrap()
            .instances
            .is_empty());
    }

    #[test]
    fn ply_03_annotation_reaches_only_the_recorded_furniture_at_clip_time() {
        let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
        let base = GlobalFormId {
            file: 0,
            object: 0x800,
        };
        plugin.push_top(soe_esm::Record::new(soe_esm::Tag(*b"FURN"), base.object));
        let mut order = soe_esm::LoadOrder::new();
        order.add("Opening.esm", plugin).unwrap();
        let mut game = Game::new(order);
        let actor = game.player();
        let furniture = game.world_mut().place_at(base, actor, false);
        let unrelated = game.world_mut().place_at(base, actor, false);
        game.world_mut().actor(actor).unwrap().furniture_seat = Some((furniture, 0));
        let mut source_assets = opening_assets();
        source_assets.graph.data.event_names[0] = "Open".into();
        source_assets.graph.data.event_names[1] = "Smash".into();
        for node in &mut source_assets.graph.nodes {
            if let NodeKind::Clip(clip) = &mut node.kind {
                clip.triggers.clear();
            }
        }
        let mut clip = source_assets.clips.get("b").unwrap().clone();
        clip.animation.annotation_tracks = vec![soe_hkx::animation::AnnotationTrack {
            track_name: "events".into(),
            annotations: vec![soe_hkx::animation::Annotation {
                time: 0.4,
                text: "Open".into(),
            }],
        }];
        source_assets.clips.insert(clip);
        annotation_triggers(&mut source_assets.graph, &source_assets.clips);
        let mut instances = BTreeMap::new();
        for (target, assets) in [
            (actor, source_assets),
            (furniture, opening_assets()),
            (unrelated, opening_assets()),
        ] {
            let assets = Rc::new(assets);
            let state = Some(Host::new(&assets, target).into_state());
            instances.insert(
                target,
                Instance {
                    assets,
                    state,
                    started: false,
                },
            );
        }
        game.set_headless_animations(Driver {
            vfs: Vfs::new(),
            assets: BTreeMap::new(),
            instances,
            cursor: 0,
            partners: BTreeMap::new(),
        });
        game.world_mut().request_animation(actor, "Smash");
        game.tick(0.2);
        assert!(!game
            .world()
            .animation_events
            .iter()
            .any(|(_, event)| event == "Begin"));
        game.tick(0.21);
        let world = game.world();
        let receivers: Vec<_> = world
            .animation_events
            .iter()
            .filter(|(_, event)| event == "Begin")
            .map(|(source, _)| *source)
            .collect();
        assert_eq!(receivers, vec![furniture]);
        drop(world);
        let mut driver = game.headless_animations.take().unwrap();
        let instance = driver.instances.get_mut(&furniture).unwrap();
        let host = Host::from_state(&instance.assets, furniture, instance.state.take().unwrap());
        assert_eq!(host.vm.active_clips()[0].1, 0.0);
    }

    #[test]
    fn ply_03_annotations_preserve_authored_triggers_and_the_disable_flag() {
        use soe_hkx::animation::{Annotation, AnnotationTrack};
        let mut assets = opening_assets();
        let mut clip = assets.clips.get("b").unwrap().clone();
        clip.animation.annotation_tracks = vec![AnnotationTrack {
            track_name: "events".into(),
            annotations: vec![Annotation {
                time: 0.0,
                text: "Begin".into(),
            }],
        }];
        assets.clips.insert(clip);
        annotation_triggers(&mut assets.graph, &assets.clips);
        let generator = assets
            .graph
            .nodes
            .iter_mut()
            .find_map(|node| match &mut node.kind {
                NodeKind::Clip(clip) if clip.animation_name == "b" => Some(clip),
                _ => None,
            })
            .unwrap();
        assert_eq!(generator.triggers.len(), 1);
        assert!(!generator.triggers[0].is_annotation);
        generator.triggers.clear();
        generator.flags |= 16;
        annotation_triggers(&mut assets.graph, &assets.clips);
        let generator = assets
            .graph
            .nodes
            .iter()
            .find_map(|node| match &node.kind {
                NodeKind::Clip(clip) if clip.animation_name == "b" => Some(clip),
                _ => None,
            })
            .unwrap();
        assert!(generator.triggers.is_empty());
    }
}
