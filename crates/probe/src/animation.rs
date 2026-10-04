// SPDX-License-Identifier: GPL-3.0-or-later

//! Archive-backed comparison host, using the HKX interpreter and CK graph variables.
//! BSA normalization does not resolve parent components in shared animation paths.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use soe_bsa::Archive;
use soe_esm::GlobalFormId;
use soe_game::Game;
use soe_hkx::{
    behavior::{BehaviorGraph, VariableType},
    interp::{Clip, ClipLibrary, Interpreter},
    skeleton::AnimationContainer,
    Packfile, QsTransform,
};

use crate::{Error, Probe};

pub(crate) struct Assets {
    pub graph: BehaviorGraph,
    pub clips: ClipLibrary,
    pub pose: Vec<QsTransform>,
}

fn execution(error: impl std::fmt::Display) -> Error {
    Error::Execution(error.to_string())
}

fn graph(archive: &Archive, path: &str) -> Result<BehaviorGraph, Error> {
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
    pub fn load(probe: &Probe, data: &Path) -> Result<Option<Self>, Error> {
        let Some(config) = &probe.animation_graph else {
            return Ok(None);
        };
        let archive = Archive::open(&data.join("Skyrim - Animations.bsa")).map_err(execution)?;
        let base = config
            .path
            .split("behaviors/")
            .next()
            .ok_or_else(|| execution("graph path has no actor base"))?;
        if base == config.path {
            return Err(execution("graph path must contain behaviors/"));
        }
        let mut graph = graph(&archive, &config.path)?;
        let mut failure = None;
        let linked = graph.link_all(16, |name| {
            match asset_path(base, name).and_then(|path| self::graph(&archive, &path)) {
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
            if !archive.contains(&path) {
                continue;
            }
            let bytes = archive.read(&path).map_err(execution)?;
            let container =
                AnimationContainer::from_packfile(&Packfile::parse(&bytes).map_err(execution)?)
                    .map_err(execution)?;
            clips.insert(Clip::from_container(&name, &container).map_err(execution)?);
        }
        let bytes = archive
            .read(&format!("{base}character assets/skeleton.hkx"))
            .map_err(execution)?;
        let container =
            AnimationContainer::from_packfile(&Packfile::parse(&bytes).map_err(execution)?)
                .map_err(execution)?;
        let pose = container
            .skeletons
            .into_iter()
            .next()
            .ok_or_else(|| execution("missing actor skeleton"))?
            .reference_pose;
        Ok(Some(Self { graph, clips, pose }))
    }
}

pub(crate) struct Host<'a> {
    graph: &'a BehaviorGraph,
    vm: Interpreter<'a>,
    actor: GlobalFormId,
    last_ints: BTreeMap<String, i32>,
    last_bools: BTreeMap<String, bool>,
    last_floats: BTreeMap<String, f32>,
    event_cursor: usize,
    reports: BTreeSet<usize>,
}

impl<'a> Host<'a> {
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
        }
    }

    pub fn update(&mut self, game: &mut Game, dt: f32) -> Result<(), Error> {
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
        for (offset, (actor, event)) in world.animation_events[self.event_cursor..]
            .iter()
            .enumerate()
        {
            if self.reports.remove(&(self.event_cursor + offset)) {
                continue;
            }
            if *actor == self.actor && !self.vm.send_event_named(event) {
                return Err(execution(format!("unknown graph event {event}")));
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

#[cfg(test)]
#[path = "../../hkx/tests/common/mod.rs"]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod test_graph;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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
}
