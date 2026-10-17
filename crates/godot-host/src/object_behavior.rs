// SPDX-License-Identifier: GPL-3.0-or-later

//! Object HKX poses bound to NIF nodes, using nifly BSBehaviorGraphExtraData,
//! the shared game graph driver and the existing named-bone skin convention.

use std::sync::Arc;

use soe_esm::GlobalFormId;
use soe_game::anim_driver::{Assets, Host, HostState};
use soe_hkx::QsTransform;
use soe_nif::blocks::Block;
use soe_nif::object_animation::ObjectPose;
use soe_nif::{NifFile, Transform};

use crate::skinned::Affine;

/// Authored project or node binding failures, before a pose reaches the renderer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("object behavior: {0}")]
    Graph(#[from] soe_game::anim_driver::Error),
    #[error("object behavior: {0}")]
    Havok(#[from] soe_hkx::Error),
    #[error("object behavior binding: {0}")]
    Binding(String),
}

#[derive(Debug)]
struct NodeBinding {
    block: usize,
    parent: Option<usize>,
    local: Transform,
    bone: Option<usize>,
    controlled: bool,
}

/// Immutable graph, clips and NIF bindings shared by one model's surfaces.
#[derive(Debug)]
pub struct Asset {
    assets: Assets,
    nodes: Vec<NodeBinding>,
    rest: ObjectPose,
}

impl Asset {
    /// Event names authored by the loaded object graph, for activation diagnostics.
    pub fn events(&self) -> &[String] {
        &self.assets.graph.data.event_names
    }

    /// Load the NIF's project through the same asset path as headless gameplay.
    pub fn from_nif(
        vfs: &soe_bsa::Vfs,
        nif: &NifFile,
        rest: ObjectPose,
    ) -> Result<Option<Arc<Self>>, Error> {
        let Some(assets) = Assets::for_object(vfs, nif)? else {
            return Ok(None);
        };
        let skeleton = assets
            .skeleton
            .as_ref()
            .ok_or_else(|| Error::Binding("missing object skeleton".into()))?;
        let mut nodes = Vec::new();
        let mut stack: Vec<_> = nif
            .roots
            .iter()
            .rev()
            .map(|id| (*id, None, false))
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        while let Some((id, parent, controlled)) = stack.pop() {
            let Some(index) = id.get() else { continue };
            if !seen.insert(index) {
                return Err(Error::Binding("object hierarchy repeats a node".into()));
            }
            let (av, children) = match nif.get(id) {
                Some(Block::Node(node)) => (&node.av, node.children.as_slice()),
                Some(Block::BsTriShape(shape)) => (&shape.av, &[][..]),
                Some(Block::TriShape(shape)) => (&shape.av, &[][..]),
                _ => continue,
            };
            let bone = skeleton
                .bones
                .iter()
                .position(|bone| bone.name == av.name())
                .or_else(|| {
                    skeleton
                        .bones
                        .iter()
                        .position(|bone| bone.name.eq_ignore_ascii_case(av.name()))
                });
            let controlled = controlled || bone.is_some();
            nodes.push(NodeBinding {
                block: index,
                parent,
                local: av.transform,
                bone,
                controlled,
            });
            stack.extend(
                children
                    .iter()
                    .rev()
                    .map(|child| (*child, Some(index), controlled)),
            );
        }
        if !nodes.iter().any(|node| node.bone.is_some()) {
            return Err(Error::Binding("object skeleton matches no NIF node".into()));
        }
        Ok(Some(Arc::new(Self {
            assets,
            nodes,
            rest,
        })))
    }

    fn pose(&self, local: &[QsTransform]) -> Result<ObjectPose, Error> {
        let skeleton = self
            .assets
            .skeleton
            .as_ref()
            .ok_or_else(|| Error::Binding("missing object skeleton".into()))?;
        let model = skeleton.to_model_space(local)?;
        let mut pose = self.rest.clone();
        for node in &self.nodes {
            let transform = if let Some(bone) = node.bone {
                let value = model
                    .get(bone)
                    .ok_or_else(|| Error::Binding("missing animated bone".into()))?;
                if (value.scale[0] - value.scale[1]).abs() > 1e-5
                    || (value.scale[0] - value.scale[2]).abs() > 1e-5
                {
                    return Err(Error::Binding("nonuniform object bone scale".into()));
                }
                let mut rigid = *value;
                rigid.scale = [1.0; 4];
                let affine = Affine::from_qs(&rigid);
                Transform {
                    rotation: soe_nif::Mat33 { rows: affine.basis },
                    translation: soe_nif::Vec3::new(
                        affine.origin[0],
                        affine.origin[1],
                        affine.origin[2],
                    ),
                    scale: value.scale[0],
                }
            } else if let Some(parent) = node.parent {
                pose.transforms[parent].then(&node.local)
            } else {
                node.local
            };
            pose.transforms[node.block] = transform;
        }
        Ok(pose)
    }

    /// Override only graph-controlled subtrees, retaining independent NIF tracks.
    pub fn apply(&self, target: &mut ObjectPose, graph: &ObjectPose) {
        for node in self.nodes.iter().filter(|node| node.controlled) {
            if let (Some(target), Some(value)) = (
                target.transforms.get_mut(node.block),
                graph.transforms.get(node.block),
            ) {
                *target = *value;
            }
        }
    }
}

/// One placed reference's playhead, shared across all of its RID surfaces.
pub struct Runtime {
    asset: Arc<Asset>,
    state: Option<HostState>,
    target: GlobalFormId,
    started: bool,
    pub(crate) pose: Arc<ObjectPose>,
}

impl Runtime {
    /// Start an independent graph without changing reference placement or saved state.
    pub fn new(asset: Arc<Asset>, target: GlobalFormId) -> Self {
        let state = Some(Host::new(&asset.assets, target).into_state());
        let pose = Arc::new(asset.rest.clone());
        Self {
            asset,
            state,
            target,
            started: false,
            pose,
        }
    }

    /// Consume this reference's requests and deliver authored completion events.
    pub fn update(&mut self, game: &mut soe_game::Game, seconds: f32) -> Result<(), Error> {
        if !game.world().is_reference_3d_loaded(self.target) {
            if std::mem::take(&mut self.started) {
                game.report_animation_unloaded(self.target);
            }
            return Ok(());
        }
        if !std::mem::replace(&mut self.started, true) {
            game.report_animation_loaded(self.target);
        }
        let Some(state) = self.state.take() else {
            return Ok(());
        };
        let mut host = Host::from_state(&self.asset.assets, self.target, state);
        let result = host.update(game, seconds);
        let pose = self.asset.pose(host.pose());
        self.state = Some(host.into_state());
        result?;
        self.pose = Arc::new(pose?);
        Ok(())
    }

    /// Apply the last evaluated pose to a NIF's concurrent controller sample.
    pub fn apply(&self, target: &mut ObjectPose) {
        self.asset.apply(target, &self.pose);
    }
}
