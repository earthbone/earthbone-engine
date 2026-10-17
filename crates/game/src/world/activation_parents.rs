// SPDX-License-Identifier: GPL-3.0-or-later

//! Authored activation-parent dispatch, using the existing XAPR view.

use std::collections::{BTreeMap, BTreeSet};

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::WorldState;
use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::VmError;

use super::save::{read_form_id, write_form_id, FormReader};
use super::{Alloc, World, T};

#[derive(Debug, Clone)]
pub(crate) struct Activation {
    pub target: GlobalFormId,
    pub actor: GlobalFormId,
    due: f64,
    path: Vec<GlobalFormId>,
}

#[derive(Default)]
pub(super) struct State {
    children: Option<BTreeMap<GlobalFormId, Vec<(GlobalFormId, f32)>>>,
    pub queued: Vec<Activation>,
    path: Vec<GlobalFormId>,
}

impl World {
    fn index_activation_children(&mut self) {
        if self.activation_parents.children.is_some() {
            return;
        }
        let mut ids: Vec<_> = self
            .order
            .winners()
            .filter_map(|(id, rref)| {
                let record = self.order.record(rref);
                (!record.is_deleted()
                    && (record.tag == T(b"REFR") || record.tag == T(b"ACHR"))
                    && record.find(T(b"XAPR")).is_some())
                .then_some(id)
            })
            .collect();
        ids.sort();
        let mut children: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for child in ids {
            let runtime = FormId(self.runtime_id(child));
            match crate::shim_world::CommonLibWorld::new(self).activate_parents(runtime) {
                Ok(parents) => {
                    for parent in parents {
                        if let Some(id) = self.from_runtime_id(parent.ref_id.0) {
                            children
                                .entry(id)
                                .or_default()
                                .push((child, parent.activate_delay));
                        }
                    }
                }
                Err(error) => self
                    .native_errors
                    .push(("activation parents".into(), format!("{child}: {error:?}"))),
            }
        }
        self.activation_parents.children = Some(children);
    }

    pub(super) fn queue_activation_children(&mut self, target: GlobalFormId) {
        self.index_activation_children();
        let children = self
            .activation_parents
            .children
            .as_ref()
            .and_then(|index| index.get(&target))
            .cloned()
            .unwrap_or_default();
        let mut path = self.activation_parents.path.clone();
        path.push(target);
        for (child, delay) in children {
            if path.contains(&child) {
                self.native_errors.push((
                    "activation parents".into(),
                    format!("activation cycle {target} -> {child}"),
                ));
                continue;
            }
            self.activation_parents.queued.push(Activation {
                target: child,
                actor: target,
                due: self.clock.real_seconds + f64::from(delay.max(0.0)),
                path: path.clone(),
            });
        }
        self.activation_parents.queued.sort_by(|a, b| {
            a.due
                .total_cmp(&b.due)
                .then(a.target.cmp(&b.target))
                .then(a.actor.cmp(&b.actor))
                .then(a.path.cmp(&b.path))
        });
    }

    pub(crate) fn has_due_activation(&self) -> bool {
        self.activation_parents
            .queued
            .first()
            .is_some_and(|entry| entry.due <= self.clock.real_seconds)
    }

    pub(crate) fn take_due_activation(&mut self) -> Option<Activation> {
        self.has_due_activation()
            .then(|| self.activation_parents.queued.remove(0))
    }

    pub(crate) fn activate_child(&mut self, entry: Activation, alloc: Alloc<'_>) {
        let previous = std::mem::replace(&mut self.activation_parents.path, entry.path);
        self.activate(entry.target, entry.actor, true, alloc);
        self.activation_parents.path = previous;
    }
}

pub(super) fn write(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    if world.activation_parents.queued.is_empty() {
        return;
    }
    writer.len(world.activation_parents.queued.len());
    for entry in &world.activation_parents.queued {
        write_form_id(writer, entry.target);
        write_form_id(writer, entry.actor);
        writer.f64(entry.due);
        writer.len(entry.path.len());
        for parent in &entry.path {
            write_form_id(writer, *parent);
        }
    }
}

pub(super) fn read(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let mut queued = Vec::new();
    for _ in 0..reader.len()? {
        let target = read_form_id(reader)?;
        let actor = read_form_id(reader)?;
        let due = reader.f64()?;
        let mut path = Vec::new();
        for _ in 0..reader.len()? {
            path.push(read_form_id(reader)?);
        }
        if !due.is_finite()
            || due < 0.0
            || path.is_empty()
            || path.contains(&target)
            || path.iter().copied().collect::<BTreeSet<_>>().len() != path.len()
        {
            return Err(VmError::State("invalid SCR-76 activation task".into()));
        }
        queued.push(Activation {
            target,
            actor,
            due,
            path,
        });
    }
    queued.sort_by(|a, b| {
        a.due
            .total_cmp(&b.due)
            .then(a.target.cmp(&b.target))
            .then(a.actor.cmp(&b.actor))
            .then(a.path.cmp(&b.path))
    });
    world.activation_parents.queued = queued;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::vmad::{ScriptEntry, Vmad};
    use crate::Game;
    use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
    use soe_papyrus::asm::{ident, none, FunctionBuilder, ScriptBuilder};
    use soe_papyrus::Opcode::{Assign, Return};
    use soe_papyrus::Value;

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn game(delay: f32) -> Game {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"ACTI"), 42));
        plugin.push_top(Record::new(Tag(*b"CONT"), 45));
        plugin.push_top(
            Record::new(Tag(*b"REFR"), 43)
                .sub(b"NAME", 42_u32.to_le_bytes())
                .sub(b"DATA", [0; 24]),
        );
        let mut parent = 43_u32.to_le_bytes().to_vec();
        parent.extend(delay.to_le_bytes());
        let vmad = Vmad {
            scripts: vec![ScriptEntry::new("ChildObserver")],
            ..Default::default()
        };
        plugin.push_top(
            Record::new(Tag(*b"REFR"), 44)
                .sub(b"VMAD", vmad.write(Tag(*b"REFR")))
                .sub(b"NAME", 45_u32.to_le_bytes())
                .sub(b"DATA", [0; 24])
                .sub(b"XAPR", parent),
        );
        let mut order = LoadOrder::new();
        order.add("fixture.esm", plugin).unwrap();
        let mut game = Game::new(order);
        let script = ScriptBuilder::new("ChildObserver", "ObjectReference")
            .variable("who", "ObjectReference", none())
            .function(
                FunctionBuilder::new("OnActivate", "None")
                    .param("actor", "ObjectReference")
                    .ins(Assign, vec![ident("who"), ident("actor")])
                    .ins(Return, vec![none()])
                    .build(),
            )
            .bytes();
        game.load_script(&script).unwrap();
        let root = game.object_of_form(id(43));
        game.vm.attach(root, "ChildObserver").unwrap();
        for target in [43, 44] {
            game.world_mut().reference(id(target)).unwrap();
        }
        game
    }

    #[test]
    fn scr_76_activation_parent_opens_child_and_retains_original_actor() {
        let mut game = game(0.0);
        let player = game.player();
        let cold = game.object_of_form(id(44));
        assert!(game.vm.scripts_on(cold).is_empty());
        assert!(game.activate(id(43), player));
        assert!(
            game.world_mut().reference(id(44)).unwrap().opened,
            "default activation must reach the child declared by XAPR"
        );
        let child = game.object_of_form(id(44));
        let actor = game.object_of_form(player);
        let parent = game.object_of_form(id(43));
        assert_eq!(
            game.vm.get_var(parent, "ChildObserver", "who").unwrap(),
            Value::Object(actor)
        );
        assert_eq!(
            game.vm.get_var(child, "ChildObserver", "who").unwrap(),
            Value::Object(parent)
        );
    }
    #[test]
    fn scr_76_delayed_activation_round_trips_and_old_save_clears_queue() {
        let mut game = game(2.0);
        let player = game.player();
        let empty = game.world().save_state();
        assert!(game.activate(id(43), player));
        assert!(!game.world_mut().reference(id(44)).unwrap().opened);
        assert!(!game.world().has_pending_calls());
        let saved = game.world().save_state();
        assert_eq!(super::super::SAVE_VERSION, 37);
        let hash = game.world().state_hash();
        game.world_mut().clock.advance(1.0);
        game.run_until_idle(64);
        assert!(!game.world_mut().reference(id(44)).unwrap().opened);
        game.world_mut().load_state(&saved).unwrap();
        assert_eq!(game.world().state_hash(), hash);
        assert_eq!(game.world().save_state(), saved);
        game.world_mut().clock.advance(2.0);
        game.run_until_idle(64);
        assert!(game.world_mut().reference(id(44)).unwrap().opened);
        game.world_mut().load_state(&saved).unwrap();
        game.world_mut().load_state(&empty).unwrap();
        game.world_mut().clock.advance(3.0);
        game.run_until_idle(64);
        assert!(!game.world_mut().reference(id(44)).unwrap().opened);
    }

    #[test]
    fn scr_76_blocked_parent_does_not_dispatch_but_default_only_does() {
        let mut game = game(0.0);
        let player = game.player();
        game.world_mut()
            .reference(id(43))
            .unwrap()
            .activation_blocked = true;
        assert!(!game.activate(id(43), player));
        assert!(!game.world_mut().reference(id(44)).unwrap().opened);
        {
            let vm = &mut game.vm;
            assert!(game
                .world
                .borrow_mut()
                .activate(id(43), player, false, &mut |name, class| vm
                    .create_object_of(name, class)));
        }
        game.run_until_idle(64);
        assert!(game.world_mut().reference(id(44)).unwrap().opened);
    }

    #[test]
    fn scr_76_cyclic_children_stop_with_a_diagnostic() {
        let mut game = game(0.0);
        let player = game.player();
        let mut world = game.world_mut();
        world.activation_parents.children = Some(std::collections::BTreeMap::from([
            (id(43), vec![(id(44), 0.0)]),
            (id(44), vec![(id(43), 0.0)]),
        ]));
        drop(world);
        assert!(game.activate(id(43), player));
        assert!(game.world_mut().reference(id(44)).unwrap().opened);
        assert!(game.world().activation_parents.queued.is_empty());
        assert!(game
            .world()
            .native_errors
            .iter()
            .any(|(_, error)| error.contains("activation cycle")));
    }

    #[test]
    fn scr_76_nonfinite_parent_delay_is_reported_without_dispatch() {
        let mut game = game(f32::NAN);
        let player = game.player();
        assert!(game.activate(id(43), player));
        assert!(!game.world_mut().reference(id(44)).unwrap().opened);
        assert!(game
            .world()
            .native_errors
            .iter()
            .any(|(_, error)| error.contains("non-finite XAPR")));
    }
    #[test]
    fn scr_76_blocked_child_gets_event_without_default_processing() {
        let mut game = game(0.0);
        let player = game.player();
        game.world_mut()
            .reference(id(44))
            .unwrap()
            .activation_blocked = true;
        assert!(game.activate(id(43), player));
        assert!(!game.world_mut().reference(id(44)).unwrap().opened);
        let child = game.object_of_form(id(44));
        let parent = game.object_of_form(id(43));
        assert_eq!(
            game.vm.get_var(child, "ChildObserver", "who").unwrap(),
            Value::Object(parent)
        );
    }

    #[test]
    fn scr_76_invalid_saved_deadline_does_not_change_live_queue() {
        let mut game = game(2.0);
        let player = game.player();
        assert!(game.activate(id(43), player));
        let saved = game.world().save_state();
        game.world_mut().activation_parents.queued[0].due = f64::NAN;
        let invalid = game.world().save_state();
        game.world_mut().load_state(&saved).unwrap();
        assert!(game.world_mut().load_state(&invalid).is_err());
        assert_eq!(game.world().save_state(), saved);
    }
}
