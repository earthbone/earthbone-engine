// SPDX-License-Identifier: GPL-3.0-or-later

//! The base game's native functions, ordered by how often vanilla scripts
//! call them (counts in `docs/papyrus.md`). Each native closes over the
//! shared [`World`] and reads or writes form state there. Anything that
//! must reach the VM (an event, a fragment call) is queued on the world.
//!
//! Semantics follow the Creation Kit wiki's function pages
//! (`engine-refs/ck-wiki`) and play behaviour, never the binary. A native
//! that returns a fixed value because the engine has no such system yet is
//! a stub and is listed as one in `docs/game.md`.

use std::cell::RefCell;
use std::rc::Rc;

use soe_esm::GlobalFormId;
use soe_papyrus::{NativeCall, NativeReturn, ObjectId, Value, Vm};

use crate::clock::GameClock;
use crate::world::{Handle, World};

pub type Shared = Rc<RefCell<World>>;

/// What [`register`] put into the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Registered {
    pub natives: usize,
    pub stubs: usize,
}

fn ret(v: Value) -> NativeReturn {
    NativeReturn::Value(v)
}
fn none() -> NativeReturn {
    ret(Value::None)
}
fn b(v: bool) -> NativeReturn {
    ret(Value::Bool(v))
}
fn i(v: i32) -> NativeReturn {
    ret(Value::Int(v))
}
fn f(v: f32) -> NativeReturn {
    ret(Value::Float(v))
}
fn s(v: &str) -> NativeReturn {
    ret(Value::string(v))
}

fn text(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        Value::None => String::new(),
        other => other.to_string(),
    }
}

/// The object allocator a native hands the world: `NativeCall::create_object`.
macro_rules! alloc {
    ($call:ident) => {
        &mut |name: &str, class: &str| -> ObjectId { $call.create_object(name, class) }
    };
}

fn form_value(
    world: &mut World,
    call: &mut NativeCall<'_>,
    id: Option<GlobalFormId>,
) -> NativeReturn {
    ret(world.form_value(id, alloc!(call)))
}

macro_rules! method {
    ($vm:expr, $w:expr, $class:expr, $name:expr, |$world:ident, $call:ident, $me:ident| $body:expr) => {{
        let w = $w.clone();
        $vm.natives.register_method(
            $class,
            $name,
            Box::new(move |$call| {
                let mut guard = w.borrow_mut();
                let $world: &mut World = &mut guard;
                let Some($me) = $call.self_object.and_then(|o| $world.form_of(o)) else {
                    return none();
                };
                $body
            }),
        );
    }};
}

macro_rules! alias_method {
    ($vm:expr, $w:expr, $name:expr, |$world:ident, $call:ident, $quest:ident, $alias:ident| $body:expr) => {{
        let w = $w.clone();
        $vm.natives.register_method(
            "Alias",
            $name,
            Box::new(move |$call| {
                let mut guard = w.borrow_mut();
                let $world: &mut World = &mut guard;
                let Some(Handle::Alias {
                    quest: $quest,
                    id: $alias,
                }) = $call.self_object.and_then(|o| $world.handle_of(o))
                else {
                    return none();
                };
                $body
            }),
        );
    }};
}

macro_rules! global {
    ($vm:expr, $w:expr, $class:expr, $name:expr, |$world:ident, $call:ident| $body:expr) => {{
        let w = $w.clone();
        $vm.natives.register_global(
            $class,
            $name,
            Box::new(move |$call| {
                let mut guard = w.borrow_mut();
                let $world: &mut World = &mut guard;
                $body
            }),
        );
    }};
}

/// Register every native and the class parents. Returns what went in. A
/// second provider registering the same name loses, and the VM's
/// `natives.duplicates` list says so.
pub fn register(vm: &mut Vm, world: &Shared) -> Registered {
    declare_parents(vm);
    let before = vm.natives.duplicates.len();
    form(vm, world);
    object_reference(vm, world);
    actor(vm, world);
    quest(vm, world);
    alias(vm, world);
    game(vm, world);
    debug(vm, world);
    utility(vm, world);
    globals_and_lists(vm, world);
    let stubs = stubs(vm);
    let natives = NATIVE_COUNT + stubs - (vm.natives.duplicates.len() - before);
    Registered { natives, stubs }
}

/// Hand-kept count of the non-stub natives registered below. The test in
/// this module checks it against the registry.
pub const NATIVE_COUNT: usize = 143;

fn declare_parents(vm: &mut Vm) {
    for (class, parent) in [
        ("ObjectReference", "Form"),
        ("Actor", "ObjectReference"),
        ("Quest", "Form"),
        ("Alias", "Form"),
        ("ReferenceAlias", "Alias"),
        ("LocationAlias", "Alias"),
        ("Cell", "Form"),
        ("GlobalVariable", "Form"),
        ("Keyword", "Form"),
        ("FormList", "Form"),
        ("ActorBase", "Form"),
        ("Activator", "Form"),
        ("Door", "Form"),
        ("Container", "Form"),
        ("Static", "Form"),
        ("MiscObject", "Form"),
        ("Weapon", "Form"),
        ("Armor", "Form"),
        ("Book", "Form"),
        ("Potion", "Form"),
        ("Ingredient", "Form"),
        ("Key", "MiscObject"),
        ("Faction", "Form"),
        ("Spell", "Form"),
        ("Location", "Form"),
        ("WorldSpace", "Form"),
        ("MagicEffect", "Form"),
        ("Furniture", "Form"),
        ("Light", "Form"),
        ("Message", "Form"),
        ("Race", "Form"),
        ("Package", "Form"),
        ("Scene", "Form"),
        ("Sound", "Form"),
        ("Idle", "Form"),
        ("Perk", "Form"),
        ("Shout", "Form"),
        ("WordOfPower", "Form"),
        ("Enchantment", "Form"),
        ("Ammo", "Form"),
        ("LeveledItem", "Form"),
        ("LeveledActor", "Form"),
        ("ImageSpaceModifier", "Form"),
        ("Weather", "Form"),
        ("TextureSet", "Form"),
        ("Outfit", "Form"),
    ] {
        vm.natives.declare_parent(class, parent);
    }
}

fn form(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "Form", "GetFormID", |world, call, me| i(
        world.runtime_id(me) as i32
    ));
    method!(vm, w, "Form", "GetName", |world, call, me| s(&world
        .name_of(me)
        .unwrap_or_default()));
    method!(vm, w, "Form", "GetType", |world, call, me| i(
        world.type_code(me)
    ));
    method!(vm, w, "Form", "HasKeyword", |world, call, me| {
        let kw = world.form_arg(&call.arg(0));
        b(kw.is_some_and(|k| world.has_keyword(me, k)))
    });
    method!(vm, w, "Form", "GetKeywords", |world, call, me| {
        let ids = world.keywords(me);
        let arr = soe_papyrus::ArrayRef::new(soe_papyrus::value::ElementKind::Object, 0);
        for id in ids {
            let v = world.form_value(Some(id), alloc!(call));
            arr.items.borrow_mut().push(v);
        }
        ret(Value::Array(arr))
    });
    // Update registration works on any object, aliases included, so these
    // take the raw self object.
    for (name, repeat, game_time) in [
        ("RegisterForSingleUpdate", false, false),
        ("RegisterForUpdate", true, false),
        ("RegisterForSingleUpdateGameTime", false, true),
        ("RegisterForUpdateGameTime", true, true),
    ] {
        let w = w.clone();
        vm.natives.register_method(
            "Form",
            name,
            Box::new(move |call| {
                if let Some(obj) = call.self_object {
                    w.borrow_mut().register_update(
                        obj,
                        f64::from(call.arg(0).as_float()),
                        repeat,
                        game_time,
                    );
                }
                none()
            }),
        );
    }
    for (name, game_time) in [
        ("UnregisterForUpdate", false),
        ("UnregisterForUpdateGameTime", true),
    ] {
        let w = w.clone();
        vm.natives.register_method(
            "Form",
            name,
            Box::new(move |call| {
                if let Some(obj) = call.self_object {
                    w.borrow_mut().unregister_update(obj, game_time);
                }
                none()
            }),
        );
    }
}

fn object_reference(vm: &mut Vm, w: &Shared) {
    for name in ["Enable", "EnableNoWait"] {
        method!(vm, w, "ObjectReference", name, |world, call, me| {
            if let Some(r) = world.reference(me) {
                r.enabled = true;
            }
            none()
        });
    }
    for name in ["Disable", "DisableNoWait"] {
        method!(vm, w, "ObjectReference", name, |world, call, me| {
            if let Some(r) = world.reference(me) {
                r.enabled = false;
            }
            none()
        });
    }
    method!(vm, w, "ObjectReference", "IsEnabled", |world, call, me| b(
        world.reference(me).is_some_and(|r| r.enabled)
    ));
    method!(vm, w, "ObjectReference", "IsDisabled", |world, call, me| b(
        world.reference(me).is_some_and(|r| !r.enabled)
    ));
    method!(vm, w, "ObjectReference", "Delete", |world, call, me| {
        if let Some(r) = world.reference(me) {
            r.deleted = true;
            r.enabled = false;
        }
        none()
    });
    method!(vm, w, "ObjectReference", "IsDeleted", |world, call, me| b(
        world.reference(me).is_some_and(|r| r.deleted)
    ));
    method!(
        vm,
        w,
        "ObjectReference",
        "GetBaseObject",
        |world, call, me| {
            let base = world.base_of(me);
            form_value(world, call, base)
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetParentCell",
        |world, call, me| {
            let cell = world.reference(me).and_then(|r| r.cell);
            form_value(world, call, cell)
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetWorldSpace",
        |world, call, me| {
            let cell = world.reference(me).and_then(|r| r.cell);
            let ws = cell.and_then(|c| {
                world
                    .record(c)
                    .and_then(|r| r.placement.world)
                    .and_then(|raw| world.resolve_from(c, raw))
            });
            form_value(world, call, ws)
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "IsInInterior",
        |world, call, me| {
            let cell = world.reference(me).and_then(|r| r.cell);
            b(cell
                .and_then(|c| world.record(c))
                .is_some_and(|r| r.placement.world.is_none()))
        }
    );
    for (name, axis) in [
        ("GetPositionX", 0),
        ("GetPositionY", 1),
        ("GetPositionZ", 2),
    ] {
        method!(vm, w, "ObjectReference", name, |world, call, me| f(world
            .reference(me)
            .map_or(0.0, |r| r.position[axis])));
    }
    for (name, axis) in [("GetAngleX", 0), ("GetAngleY", 1), ("GetAngleZ", 2)] {
        method!(vm, w, "ObjectReference", name, |world, call, me| f(world
            .reference(me)
            .map_or(0.0, |r| r.rotation[axis].to_degrees())));
    }
    method!(
        vm,
        w,
        "ObjectReference",
        "SetPosition",
        |world, call, me| {
            if let Some(r) = world.reference(me) {
                r.position = [
                    call.arg(0).as_float(),
                    call.arg(1).as_float(),
                    call.arg(2).as_float(),
                ];
            }
            none()
        }
    );
    method!(vm, w, "ObjectReference", "SetAngle", |world, call, me| {
        if let Some(r) = world.reference(me) {
            r.rotation = [
                call.arg(0).as_float().to_radians(),
                call.arg(1).as_float().to_radians(),
                call.arg(2).as_float().to_radians(),
            ];
        }
        none()
    });
    method!(vm, w, "ObjectReference", "MoveTo", |world, call, me| {
        let offset = [
            call.arg(1).as_float(),
            call.arg(2).as_float(),
            call.arg(3).as_float(),
        ];
        if let Some(target) = world.form_arg(&call.arg(0)) {
            world.move_to(me, target, offset);
        }
        none()
    });
    method!(
        vm,
        w,
        "ObjectReference",
        "GetDistance",
        |world, call, me| {
            f(world
                .form_arg(&call.arg(0))
                .map_or(0.0, |t| world.distance(me, t)))
        }
    );
    method!(vm, w, "ObjectReference", "Activate", |world, call, me| {
        let Some(activator) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        let default_only = call.arg(1).as_bool();
        b(world.activate(me, activator, !default_only, alloc!(call)))
    });
    method!(
        vm,
        w,
        "ObjectReference",
        "BlockActivation",
        |world, call, me| {
            let block = call.args.first().is_none_or(Value::as_bool);
            if let Some(r) = world.reference(me) {
                r.activation_blocked = block;
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "IsActivationBlocked",
        |world, call, me| b(world.reference(me).is_some_and(|r| r.activation_blocked))
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetLinkedRef",
        |world, call, me| {
            let keyword = world.form_arg(&call.arg(0));
            let linked = world.reference(me).and_then(|r| {
                r.linked
                    .iter()
                    .find(|(k, _)| keyword.is_none() || *k == keyword)
                    .map(|(_, l)| *l)
            });
            form_value(world, call, linked)
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetItemCount",
        |world, call, me| {
            i(world
                .form_arg(&call.arg(0))
                .map_or(0, |item| world.item_count(me, item)))
        }
    );
    method!(vm, w, "ObjectReference", "AddItem", |world, call, me| {
        let count = call.args.get(1).map_or(1, Value::as_int);
        if let Some(item) = world.form_arg(&call.arg(0)) {
            world.add_item(me, item, count.max(1));
        }
        none()
    });
    method!(vm, w, "ObjectReference", "RemoveItem", |world, call, me| {
        let count = call.args.get(1).map_or(1, Value::as_int);
        let target = world.form_arg(&call.arg(3));
        if let Some(item) = world.form_arg(&call.arg(0)) {
            world.remove_item(me, item, count.max(1), target);
        }
        none()
    });
    method!(
        vm,
        w,
        "ObjectReference",
        "RemoveAllItems",
        |world, call, me| {
            let target = world.form_arg(&call.arg(0));
            let items: Vec<(GlobalFormId, i32)> = world
                .reference(me)
                .map(|r| r.inventory.iter().map(|(k, v)| (*k, *v)).collect())
                .unwrap_or_default();
            for (item, count) in items {
                world.remove_item(me, item, count, target);
            }
            none()
        }
    );
    method!(vm, w, "ObjectReference", "GetScale", |world, call, me| f(
        world.reference(me).map_or(1.0, |r| r.scale)
    ));
    method!(vm, w, "ObjectReference", "SetScale", |world, call, me| {
        if let Some(r) = world.reference(me) {
            r.scale = call.arg(0).as_float();
        }
        none()
    });
    method!(
        vm,
        w,
        "ObjectReference",
        "GetDisplayName",
        |world, call, me| s(&world.name_of(me).unwrap_or_default())
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "SetDisplayName",
        |world, call, me| {
            let name = text(&call.arg(0));
            match world.reference(me) {
                Some(r) => {
                    r.display_name = Some(name);
                    b(true)
                }
                None => b(false),
            }
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "PlayAnimation",
        |world, call, me| {
            let event = text(&call.arg(0));
            world.animation_events.push((me, event));
            b(true)
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetOpenState",
        |world, call, me| {
            i(if world.reference(me).is_some_and(|r| r.opened) {
                1
            } else {
                3
            })
        }
    );
    method!(vm, w, "ObjectReference", "SetOpen", |world, call, me| {
        let open = call.args.first().is_none_or(Value::as_bool);
        if let Some(r) = world.reference(me) {
            r.opened = open;
        }
        none()
    });
    method!(vm, w, "ObjectReference", "Lock", |world, call, me| {
        let lock = call.args.first().is_none_or(Value::as_bool);
        if let Some(r) = world.reference(me) {
            r.locked = lock;
        }
        none()
    });
    method!(vm, w, "ObjectReference", "IsLocked", |world, call, me| b(
        world.reference(me).is_some_and(|r| r.locked)
    ));
    method!(
        vm,
        w,
        "ObjectReference",
        "GetLockLevel",
        |world, call, me| i(world.reference(me).map_or(0, |r| r.lock_level))
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "SetLockLevel",
        |world, call, me| {
            if let Some(r) = world.reference(me) {
                r.lock_level = call.arg(0).as_int();
            }
            none()
        }
    );
    method!(vm, w, "ObjectReference", "PlaceAtMe", |world, call, me| {
        let Some(base) = world.form_arg(&call.arg(0)) else {
            return none();
        };
        let count = call.args.get(1).map_or(1, Value::as_int).max(1);
        let disabled = call.arg(3).as_bool();
        let mut last = None;
        for _ in 0..count {
            last = Some(world.place_at(base, me, disabled));
        }
        form_value(world, call, last)
    });
}

fn actor(vm: &mut Vm, w: &Shared) {
    for name in ["GetActorValue", "GetAV"] {
        method!(vm, w, "Actor", name, |world, call, me| f(
            world.actor_value(me, &text(&call.arg(0)))
        ));
    }
    for name in ["GetBaseActorValue", "GetBaseAV"] {
        method!(vm, w, "Actor", name, |world, call, me| f(
            world.base_actor_value(me, &text(&call.arg(0)))
        ));
    }
    for name in ["SetActorValue", "SetAV"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            world.set_actor_value(me, &text(&call.arg(0)), call.arg(1).as_float());
            none()
        });
    }
    for name in ["ModActorValue", "ModAV", "RestoreActorValue", "RestoreAV"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            world.mod_actor_value(me, &text(&call.arg(0)), call.arg(1).as_float());
            none()
        });
    }
    for name in ["DamageActorValue", "DamageAV"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            world.mod_actor_value(me, &text(&call.arg(0)), -call.arg(1).as_float());
            none()
        });
    }
    for name in ["GetActorValuePercentage", "GetAVPercentage"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            let name = text(&call.arg(0));
            let base = world.base_actor_value(me, &name);
            f(if base > 0.0 {
                world.actor_value(me, &name) / base
            } else {
                0.0
            })
        });
    }
    method!(vm, w, "Actor", "IsDead", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.dead)));
    method!(vm, w, "Actor", "Kill", |world, call, me| {
        if let Some(a) = world.actor(me) {
            if !a.essential {
                a.dead = true;
                a.values.insert("health".into(), 0.0);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "Resurrect", |world, call, me| {
        if let Some(a) = world.actor(me) {
            a.dead = false;
            let base = a.base_values.get("health").copied().unwrap_or(100.0);
            a.values.insert("health".into(), base);
        }
        none()
    });
    method!(vm, w, "Actor", "IsInCombat", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.in_combat)));
    method!(vm, w, "Actor", "EvaluatePackage", |world, call, me| {
        world.evaluate_package(me, alloc!(call));
        none()
    });
    method!(vm, w, "Actor", "GetCurrentPackage", |world, call, me| {
        let package = world.current_package(me);
        form_value(world, call, package)
    });
    method!(vm, w, "Actor", "StartCombat", |world, call, me| {
        if let Some(a) = world.actor(me) {
            a.in_combat = true;
        }
        none()
    });
    method!(vm, w, "Actor", "StopCombat", |world, call, me| {
        if let Some(a) = world.actor(me) {
            a.in_combat = false;
        }
        none()
    });
    for name in ["GetActorBase", "GetLeveledActorBase"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            let base = world.base_of(me);
            form_value(world, call, base)
        });
    }
    method!(vm, w, "Actor", "IsEssential", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.essential)));
    method!(vm, w, "Actor", "IsProtected", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.protected)));
    method!(vm, w, "Actor", "GetLevel", |world, call, me| i(world
        .actor(me)
        .map_or(1, |a| a.level as i32)));
    method!(vm, w, "Actor", "GetRace", |world, call, me| {
        let race = world.actor(me).and_then(|a| a.race);
        form_value(world, call, race)
    });
    method!(vm, w, "Actor", "IsPlayerTeammate", |world, call, me| b(
        world.actor(me).is_some_and(|a| a.teammate)
    ));
    method!(vm, w, "Actor", "SetPlayerTeammate", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(a) = world.actor(me) {
            a.teammate = on;
        }
        none()
    });
    method!(vm, w, "Actor", "IsGhost", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.ghost)));
    method!(vm, w, "Actor", "SetGhost", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(a) = world.actor(me) {
            a.ghost = on;
        }
        none()
    });
    method!(vm, w, "Actor", "IsUnconscious", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.unconscious)));
    method!(vm, w, "Actor", "SetUnconscious", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(a) = world.actor(me) {
            a.unconscious = on;
        }
        none()
    });
    method!(vm, w, "Actor", "IsDoingFavor", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.doing_favor)));
    method!(vm, w, "Actor", "SetDoingFavor", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(a) = world.actor(me) {
            a.doing_favor = on;
        }
        none()
    });
    method!(vm, w, "Actor", "AddSpell", |world, call, me| {
        let Some(spell) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        b(world.actor(me).is_some_and(|a| a.spells.insert(spell)))
    });
    method!(vm, w, "Actor", "RemoveSpell", |world, call, me| {
        let Some(spell) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        b(world.actor(me).is_some_and(|a| a.spells.remove(&spell)))
    });
    method!(vm, w, "Actor", "HasSpell", |world, call, me| {
        let Some(spell) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        b(world.actor(me).is_some_and(|a| a.spells.contains(&spell)))
    });
    method!(vm, w, "Actor", "EquipItem", |world, call, me| {
        if let Some(item) = world.form_arg(&call.arg(0)) {
            if world.item_count(me, item) == 0 {
                world.add_item(me, item, 1);
            }
            if let Some(a) = world.actor(me) {
                a.equipped.insert(item);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "UnequipItem", |world, call, me| {
        if let Some(item) = world.form_arg(&call.arg(0)) {
            if let Some(a) = world.actor(me) {
                a.equipped.remove(&item);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "IsEquipped", |world, call, me| {
        let Some(item) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        b(world.actor(me).is_some_and(|a| a.equipped.contains(&item)))
    });
    method!(vm, w, "Actor", "IsInFaction", |world, call, me| {
        let Some(faction) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        b(world
            .actor(me)
            .is_some_and(|a| a.factions.contains_key(&faction)))
    });
    method!(vm, w, "Actor", "GetFactionRank", |world, call, me| {
        let Some(faction) = world.form_arg(&call.arg(0)) else {
            return i(-2);
        };
        i(world
            .actor(me)
            .and_then(|a| a.factions.get(&faction).copied())
            .unwrap_or(-2))
    });
    method!(vm, w, "Actor", "AddToFaction", |world, call, me| {
        if let Some(faction) = world.form_arg(&call.arg(0)) {
            if let Some(a) = world.actor(me) {
                a.factions.entry(faction).or_insert(0);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "RemoveFromFaction", |world, call, me| {
        if let Some(faction) = world.form_arg(&call.arg(0)) {
            if let Some(a) = world.actor(me) {
                a.factions.remove(&faction);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "SetFactionRank", |world, call, me| {
        let rank = call.arg(1).as_int();
        if let Some(faction) = world.form_arg(&call.arg(0)) {
            if let Some(a) = world.actor(me) {
                a.factions.insert(faction, rank);
            }
        }
        none()
    });
}

fn quest(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "Quest", "Start", |world, call, me| b(
        world.start_quest(me, alloc!(call))
    ));
    method!(vm, w, "Quest", "Stop", |world, call, me| {
        world.stop_quest(me);
        none()
    });
    method!(vm, w, "Quest", "Reset", |world, call, me| {
        if let Some(q) = world.quest_state(me) {
            q.running = false;
            q.completed = false;
            q.failed = false;
            q.current = 0;
            q.highest = 0;
            q.done.clear();
            q.objectives.clear();
        }
        none()
    });
    method!(vm, w, "Quest", "IsRunning", |world, call, me| b(world
        .quest_state(me)
        .is_some_and(|q| q.running)));
    method!(vm, w, "Quest", "IsCompleted", |world, call, me| b(world
        .quest_state(me)
        .is_some_and(|q| q.completed)));
    method!(vm, w, "Quest", "IsActive", |world, call, me| b(world
        .quest_state(me)
        .is_some_and(|q| q.active)));
    method!(vm, w, "Quest", "SetActive", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(q) = world.quest_state(me) {
            q.active = on;
        }
        none()
    });
    method!(vm, w, "Quest", "CompleteQuest", |world, call, me| {
        if let Some(q) = world.quest_state(me) {
            q.completed = true;
            q.running = false;
        }
        none()
    });
    method!(
        vm,
        w,
        "Quest",
        "CompleteAllObjectives",
        |world, call, me| {
            let indices: Vec<u16> = world
                .quest_state(me)
                .map(|q| q.def.objectives.iter().map(|o| o.index).collect())
                .unwrap_or_default();
            for idx in indices {
                if let Some(o) = world.objective(me, idx) {
                    o.completed = true;
                }
            }
            none()
        }
    );
    method!(vm, w, "Quest", "FailAllObjectives", |world, call, me| {
        let indices: Vec<u16> = world
            .quest_state(me)
            .map(|q| q.def.objectives.iter().map(|o| o.index).collect())
            .unwrap_or_default();
        for idx in indices {
            if let Some(o) = world.objective(me, idx) {
                o.failed = true;
            }
        }
        none()
    });
    for name in ["SetStage", "SetCurrentStageID"] {
        method!(vm, w, "Quest", name, |world, call, me| {
            let stage = call.arg(0).as_int();
            b(u16::try_from(stage).is_ok_and(|st| world.set_stage(me, st, alloc!(call))))
        });
    }
    method!(vm, w, "Quest", "GetStage", |world, call, me| i(world
        .quest_state(me)
        .map_or(0, |q| i32::from(q.highest))));
    method!(vm, w, "Quest", "GetCurrentStageID", |world, call, me| i(
        world.quest_state(me).map_or(0, |q| i32::from(q.current))
    ));
    for name in ["GetStageDone", "IsStageDone"] {
        method!(vm, w, "Quest", name, |world, call, me| {
            let stage = call.arg(0).as_int();
            b(u16::try_from(stage)
                .is_ok_and(|st| world.quest_state(me).is_some_and(|q| q.done.contains(&st))))
        });
    }
    method!(
        vm,
        w,
        "Quest",
        "SetObjectiveDisplayed",
        |world, call, me| {
            let on = call.args.get(1).is_none_or(Value::as_bool);
            if let Ok(idx) = u16::try_from(call.arg(0).as_int()) {
                if let Some(o) = world.objective(me, idx) {
                    o.displayed = on;
                }
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "Quest",
        "SetObjectiveCompleted",
        |world, call, me| {
            let on = call.args.get(1).is_none_or(Value::as_bool);
            if let Ok(idx) = u16::try_from(call.arg(0).as_int()) {
                if let Some(o) = world.objective(me, idx) {
                    o.completed = on;
                }
            }
            none()
        }
    );
    method!(vm, w, "Quest", "SetObjectiveFailed", |world, call, me| {
        let on = call.args.get(1).is_none_or(Value::as_bool);
        if let Ok(idx) = u16::try_from(call.arg(0).as_int()) {
            if let Some(o) = world.objective(me, idx) {
                o.failed = on;
            }
        }
        none()
    });
    method!(vm, w, "Quest", "IsObjectiveDisplayed", |world, call, me| {
        b(u16::try_from(call.arg(0).as_int())
            .ok()
            .and_then(|idx| world.objective(me, idx))
            .is_some_and(|o| o.displayed))
    });
    method!(vm, w, "Quest", "IsObjectiveCompleted", |world, call, me| {
        b(u16::try_from(call.arg(0).as_int())
            .ok()
            .and_then(|idx| world.objective(me, idx))
            .is_some_and(|o| o.completed))
    });
    method!(vm, w, "Quest", "IsObjectiveFailed", |world, call, me| {
        b(u16::try_from(call.arg(0).as_int())
            .ok()
            .and_then(|idx| world.objective(me, idx))
            .is_some_and(|o| o.failed))
    });
    for name in ["GetAlias", "GetAliasById"] {
        method!(vm, w, "Quest", name, |world, call, me| {
            let Ok(id) = u32::try_from(call.arg(0).as_int()) else {
                return none();
            };
            if world
                .quest_state(me)
                .is_some_and(|q| q.def.alias(id).is_some())
            {
                ret(Value::Object(
                    world.object_of(Handle::Alias { quest: me, id }, alloc!(call)),
                ))
            } else {
                none()
            }
        });
    }
    method!(vm, w, "Quest", "GetAliasByName", |world, call, me| {
        let name = text(&call.arg(0));
        match world
            .quest_state(me)
            .and_then(|q| q.def.alias_named(&name).map(|a| a.id))
        {
            Some(id) => ret(Value::Object(
                world.object_of(Handle::Alias { quest: me, id }, alloc!(call)),
            )),
            None => none(),
        }
    });
    method!(vm, w, "Quest", "GetID", |world, call, me| s(&world
        .editor_id(me)
        .unwrap_or_default()));
    method!(vm, w, "Quest", "GetPriority", |world, call, me| i(world
        .quest_state(me)
        .map_or(0, |q| i32::from(q.def.priority))));
    global!(vm, w, "Quest", "GetQuest", |world, call| {
        let id = world
            .form(&text(&call.arg(0)))
            .filter(|id| world.tag_of(*id).is_some_and(|t| t.as_bytes() == b"QUST"));
        form_value(world, call, id)
    });
}

fn alias(vm: &mut Vm, w: &Shared) {
    alias_method!(vm, w, "GetOwningQuest", |world, call, quest, _alias| {
        form_value(world, call, Some(quest))
    });
    alias_method!(vm, w, "GetID", |world, call, _quest, alias| i(alias as i32));
    alias_method!(vm, w, "GetName", |world, call, quest, alias| {
        s(&world
            .quest_state(quest)
            .and_then(|q| q.def.alias(alias).map(|a| a.name.clone()))
            .unwrap_or_default())
    });
    for name in ["GetReference", "GetRef", "GetActorReference", "GetActorRef"] {
        alias_method!(vm, w, name, |world, call, quest, alias| {
            let r = world.alias_ref(quest, alias);
            form_value(world, call, r)
        });
    }
    alias_method!(vm, w, "ForceRefTo", |world, call, quest, alias| {
        let target = world.form_arg(&call.arg(0));
        world.force_alias(quest, alias, target);
        none()
    });
    alias_method!(vm, w, "ForceRefIfEmpty", |world, call, quest, alias| {
        if world.alias_ref(quest, alias).is_some() {
            return b(false);
        }
        let target = world.form_arg(&call.arg(0));
        b(world.force_alias(quest, alias, target))
    });
    alias_method!(vm, w, "Clear", |world, call, quest, alias| {
        world.force_alias(quest, alias, None);
        none()
    });
}

fn game(vm: &mut Vm, w: &Shared) {
    global!(vm, w, "Game", "GetPlayer", |world, call| {
        let p = world.player();
        form_value(world, call, Some(p))
    });
    for name in ["GetForm", "GetFormEx"] {
        global!(vm, w, "Game", name, |world, call| {
            let id = world.from_runtime_id(call.arg(0).as_int() as u32);
            form_value(world, call, id)
        });
    }
    global!(vm, w, "Game", "GetFormFromFile", |world, call| {
        let id = world.form_from_file(call.arg(0).as_int() as u32, &text(&call.arg(1)));
        form_value(world, call, id)
    });
    global!(vm, w, "Game", "EnablePlayerControls", |world, _call| {
        world.player_controls = true;
        none()
    });
    global!(vm, w, "Game", "DisablePlayerControls", |world, _call| {
        world.player_controls = false;
        none()
    });
    global!(vm, w, "Game", "GetGameSettingFloat", |world, call| {
        ret(world
            .game_setting(&text(&call.arg(0)))
            .unwrap_or(Value::Float(0.0)))
    });
    global!(vm, w, "Game", "GetGameSettingInt", |world, call| {
        ret(world
            .game_setting(&text(&call.arg(0)))
            .unwrap_or(Value::Int(0)))
    });
    global!(vm, w, "Game", "GetGameSettingString", |world, call| {
        ret(world
            .game_setting(&text(&call.arg(0)))
            .unwrap_or_else(|| Value::string("")))
    });
    global!(vm, w, "Game", "GetRealHoursPassed", |world, _call| f(
        (world.clock.real_seconds / 3600.0) as f32
    ));
}

fn debug(vm: &mut Vm, w: &Shared) {
    for name in ["Trace", "TraceUser", "TraceConditional", "TraceStack"] {
        global!(vm, w, "Debug", name, |_world, call| {
            let msg = match name {
                "TraceUser" => text(&call.arg(1)),
                "TraceConditional" if !call.arg(1).as_bool() => return none(),
                _ => text(&call.arg(0)),
            };
            call.log.push(call.time, msg);
            none()
        });
    }
    global!(vm, w, "Debug", "Notification", |world, call| {
        world.notifications.push(text(&call.arg(0)));
        none()
    });
    global!(vm, w, "Debug", "MessageBox", |world, call| {
        world.message_boxes.push(text(&call.arg(0)));
        none()
    });
    global!(vm, w, "Debug", "SendAnimationEvent", |world, call| {
        match world.form_arg(&call.arg(0)) {
            Some(target) => {
                world.animation_events.push((target, text(&call.arg(1))));
                b(true)
            }
            None => b(false),
        }
    });
}

fn utility(vm: &mut Vm, w: &Shared) {
    global!(vm, w, "Utility", "RandomInt", |world, call| {
        let (min, max) = (
            call.args.first().map_or(0, Value::as_int),
            call.args.get(1).map_or(100, Value::as_int),
        );
        i(world.random_int(min, max))
    });
    global!(vm, w, "Utility", "RandomFloat", |world, call| {
        let (min, max) = (
            call.args.first().map_or(0.0, Value::as_float),
            call.args.get(1).map_or(1.0, Value::as_float),
        );
        f(world.random_float(min, max))
    });
    global!(vm, w, "Utility", "GetCurrentGameTime", |world, _call| f(
        world.clock.game_days as f32
    ));
    global!(vm, w, "Utility", "GetCurrentRealTime", |world, _call| f(
        world.clock.real_seconds as f32
    ));
    global!(vm, w, "Utility", "GameTimeToString", |_world, call| s(
        &GameClock::format_game_time(call.arg(0).as_float())
    ));
    for (name, kind) in [
        ("SetINIBool", 0),
        ("SetINIFloat", 1),
        ("SetINIInt", 2),
        ("SetINIString", 3),
    ] {
        global!(vm, w, "Utility", name, |world, call| {
            let v = match kind {
                0 => call.arg(1).as_bool().to_string(),
                1 => call.arg(1).as_float().to_string(),
                2 => call.arg(1).as_int().to_string(),
                _ => text(&call.arg(1)),
            };
            world.ini.insert(text(&call.arg(0)).to_ascii_lowercase(), v);
            none()
        });
    }
    global!(vm, w, "Utility", "GetINIBool", |world, call| {
        b(world
            .ini
            .get(&text(&call.arg(0)).to_ascii_lowercase())
            .is_some_and(|v| v == "true" || v == "1"))
    });
    global!(vm, w, "Utility", "GetINIFloat", |world, call| {
        f(world
            .ini
            .get(&text(&call.arg(0)).to_ascii_lowercase())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0))
    });
    global!(vm, w, "Utility", "GetINIInt", |world, call| {
        i(world
            .ini
            .get(&text(&call.arg(0)).to_ascii_lowercase())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    });
    global!(vm, w, "Utility", "GetINIString", |world, call| {
        s(world
            .ini
            .get(&text(&call.arg(0)).to_ascii_lowercase())
            .map_or("", String::as_str))
    });
}

fn globals_and_lists(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "GlobalVariable", "GetValue", |world, call, me| f(
        world.global_value(me)
    ));
    method!(vm, w, "GlobalVariable", "GetValueInt", |world, call, me| i(
        world.global_value(me) as i32
    ));
    method!(vm, w, "GlobalVariable", "SetValue", |world, call, me| {
        world.set_global(me, call.arg(0).as_float());
        none()
    });
    method!(vm, w, "GlobalVariable", "SetValueInt", |world, call, me| {
        world.set_global(me, call.arg(0).as_int() as f32);
        none()
    });
    method!(vm, w, "GlobalVariable", "Mod", |world, call, me| {
        let v = world.global_value(me) + call.arg(0).as_float();
        world.set_global(me, v);
        f(v)
    });
    method!(vm, w, "Keyword", "GetString", |world, call, me| s(&world
        .editor_id(me)
        .unwrap_or_default()));
    method!(vm, w, "Cell", "IsInterior", |world, call, me| b(world
        .record(me)
        .is_some_and(|r| r.placement.world.is_none())));
    method!(vm, w, "FormList", "GetSize", |world, call, me| i(world
        .form_list(me)
        .map_or(0, |l| l.len() as i32)));
    method!(vm, w, "FormList", "GetAt", |world, call, me| {
        let idx = usize::try_from(call.arg(0).as_int()).ok();
        let item = idx.and_then(|k| world.form_list(me).and_then(|l| l.get(k).copied()));
        form_value(world, call, item)
    });
    method!(vm, w, "FormList", "HasForm", |world, call, me| {
        let Some(item) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        b(world.form_list(me).is_some_and(|l| l.contains(&item)))
    });
    method!(vm, w, "FormList", "Find", |world, call, me| {
        let Some(item) = world.form_arg(&call.arg(0)) else {
            return i(-1);
        };
        i(world
            .form_list(me)
            .and_then(|l| l.iter().position(|x| *x == item))
            .map_or(-1, |p| p as i32))
    });
    method!(vm, w, "FormList", "AddForm", |world, call, me| {
        if let Some(item) = world.form_arg(&call.arg(0)) {
            if let Some(l) = world.form_list(me) {
                l.push(item);
            }
        }
        none()
    });
}

/// Natives whose systems the engine does not have yet. They return the
/// value the game gives in the quiet case so vanilla scripts run past them.
fn stubs(vm: &mut Vm) -> usize {
    let mut n = 0;
    let bools: &[(&str, &str, bool)] = &[
        ("ObjectReference", "Is3DLoaded", true),
        ("ObjectReference", "IsFurnitureInUse", false),
        ("ObjectReference", "IsInLocation", false),
        ("ObjectReference", "IsIgnoringFriendlyHits", false),
        ("Actor", "IsAlerted", false),
        ("Actor", "IsAlarmed", false),
        ("Actor", "IsArrested", false),
        ("Actor", "IsBleedingOut", false),
        ("Actor", "IsSneaking", false),
        ("Actor", "IsRunning", false),
        ("Actor", "IsSprinting", false),
        ("Actor", "IsWeaponDrawn", false),
        ("Actor", "IsHostileToActor", false),
        ("Actor", "IsChild", false),
        ("Actor", "IsCommandedActor", false),
        ("Actor", "IsOnMount", false),
        ("Actor", "IsInDialogueWithPlayer", false),
        ("Actor", "IsTrespassing", false),
        ("Actor", "IsDetectedBy", true),
        ("Actor", "HasLOS", true),
        ("Actor", "PathToReference", true),
        ("Quest", "IsStarting", false),
        ("Quest", "IsStopping", false),
        ("Quest", "UpdateCurrentInstanceGlobal", true),
        ("Quest", "ModObjectiveGlobal", true),
        ("Cell", "IsAttached", true),
        ("Game", "IsFastTravelEnabled", true),
        ("Game", "IsWordUnlocked", false),
        ("Game", "IsPlayerSungazing", false),
        ("Utility", "IsInMenuMode", false),
        ("Debug", "OpenUserLog", true),
    ];
    for (class, name, value) in bools {
        let global = matches!(*class, "Game" | "Utility" | "Debug");
        let v = *value;
        let f: soe_papyrus::NativeFn = Box::new(move |_| b(v));
        if global {
            vm.natives.register_global(class, name, f);
        } else {
            vm.natives.register_method(class, name, f);
        }
        n += 1;
    }
    let ints: &[(&str, &str, i32)] = &[
        ("ObjectReference", "GetTriggerObjectCount", 0),
        ("ObjectReference", "GetCurrentDestructionStage", 0),
        ("Actor", "GetCombatState", 0),
        ("Actor", "GetSleepState", 0),
        ("Actor", "GetSitState", 0),
        ("Actor", "GetFlyingState", 0),
        ("Actor", "GetRelationshipRank", 0),
        ("Game", "QueryStat", 0),
        ("Game", "GetPlayersLastRiddenHorse", 0),
    ];
    for (class, name, value) in ints {
        let global = *class == "Game";
        let v = *value;
        let f: soe_papyrus::NativeFn = Box::new(move |_| i(v));
        if global {
            vm.natives.register_global(class, name, f);
        } else {
            vm.natives.register_method(class, name, f);
        }
        n += 1;
    }
    let methods_none: &[(&str, &str)] = &[
        ("ObjectReference", "GetCurrentLocation"),
        ("ObjectReference", "GetEditorLocation"),
        ("ObjectReference", "GetActorOwner"),
        ("ObjectReference", "GetFactionOwner"),
        ("ObjectReference", "GetCurrentScene"),
        ("ObjectReference", "PlayAnimationAndWait"),
        ("ObjectReference", "PlayGamebryoAnimation"),
        ("ObjectReference", "SetMotionType"),
        ("ObjectReference", "ApplyHavokImpulse"),
        ("ObjectReference", "SetActorOwner"),
        ("ObjectReference", "SetFactionOwner"),
        ("ObjectReference", "SetNoFavorAllowed"),
        ("ObjectReference", "EnableFastTravel"),
        ("ObjectReference", "ClearDestruction"),
        ("ObjectReference", "DamageObject"),
        ("ObjectReference", "KnockAreaEffect"),
        ("ObjectReference", "PushActorAway"),
        ("Actor", "SetAlert"),
        ("Actor", "SetEssential"),
        ("Actor", "SetOutfit"),
        ("Actor", "SetRestrained"),
        ("Actor", "SetVehicle"),
        ("Actor", "SetLookAt"),
        ("Actor", "ClearLookAt"),
        ("Actor", "SetHeadTracking"),
        ("Actor", "SetExpressionOverride"),
        ("Actor", "ClearExpressionOverride"),
        ("Actor", "SetDontMove"),
        ("Actor", "SetAllowPCDialogue"),
        ("Actor", "SetAttackActorOnSight"),
        ("Actor", "SetCrimeFaction"),
        ("Actor", "SetRelationshipRank"),
        ("Actor", "ShowGiftMenu"),
        ("Actor", "ShowBarterMenu"),
        ("Actor", "OpenInventory"),
        ("Actor", "SendTrespassAlarm"),
        ("Actor", "StopCombatAlarm"),
        ("Actor", "ClearArrested"),
        ("Actor", "ClearForcedMovement"),
        ("Actor", "ClearKeepOffsetFromActor"),
        ("Actor", "KeepOffsetFromActor"),
        ("Actor", "SetForcedLandingMarker"),
        ("Actor", "GetCombatTarget"),
        ("Actor", "GetDialogueTarget"),
        ("Actor", "GetKiller"),
        ("Actor", "GetEquippedWeapon"),
        ("Actor", "GetEquippedShield"),
        ("Actor", "GetEquippedSpell"),
        ("Actor", "GetCrimeFaction"),
        ("Actor", "GetVoiceRecoveryTime"),
        ("Actor", "Dismount"),
        ("Actor", "DrawWeapon"),
        ("Actor", "SheatheWeapon"),
        ("Actor", "StartSneaking"),
        ("Actor", "Trespass"),
        ("Actor", "UnequipAll"),
        ("Actor", "RemoveAllSpells"),
        ("Actor", "StartCannibal"),
        ("Actor", "StartVampireFeed"),
        ("Actor", "WillIntimidateSucceed"),
        ("Quest", "SetObjectiveDisplayedNoWait"),
        ("Cell", "SetPublic"),
        ("Cell", "SetActorOwner"),
        ("Cell", "SetFactionOwner"),
        ("Cell", "Reset"),
    ];
    for (class, name) in methods_none {
        vm.natives
            .register_method(class, name, Box::new(|_| none()));
        n += 1;
    }
    let globals_none: &[(&str, &str)] = &[
        ("Game", "ShakeCamera"),
        ("Game", "ShakeController"),
        ("Game", "AddAchievement"),
        ("Game", "FadeOutGame"),
        ("Game", "RequestSave"),
        ("Game", "RequestAutoSave"),
        ("Game", "ForceFirstPerson"),
        ("Game", "ForceThirdPerson"),
        ("Game", "SetPlayerAIDriven"),
        ("Game", "SetInChargen"),
        ("Game", "IncrementStat"),
        ("Game", "EnableFastTravel"),
        ("Game", "SetHudCartMode"),
        ("Game", "ShowRaceMenu"),
        ("Game", "TriggerScreenBlood"),
        ("Game", "PlayBink"),
        ("Game", "TeachWord"),
        ("Game", "UnlockWord"),
        ("Game", "AdvanceSkill"),
        ("Game", "PrecacheCharGen"),
        ("Game", "PrecacheCharGenClear"),
        ("Game", "ShowTrainingMenu"),
        ("Game", "ShowFirstPersonGeometry"),
        ("Game", "SetCameraTarget"),
        ("Game", "SetBeastForm"),
        ("Game", "QuitToMainMenu"),
        ("Game", "SetSittingRotation"),
        ("Game", "StartTitleSequence"),
        ("Game", "SetGameSettingFloat"),
        ("Game", "SetGameSettingInt"),
        ("Game", "SetGameSettingBool"),
        ("Game", "SetGameSettingString"),
        ("Game", "GetCurrentCrosshairRef"),
        ("Game", "GetPlayerGrabbedRef"),
        ("Game", "FindClosestActor"),
        ("Game", "FindRandomActor"),
        ("Game", "FindClosestReferenceOfType"),
        ("Game", "FindRandomReferenceOfType"),
        ("Game", "GetDialogueTarget"),
        ("Game", "GetFormEx2"),
        ("Debug", "QuitGame"),
        ("Debug", "CenterOnCell"),
        ("Debug", "CenterOnCellAndWait"),
        ("Debug", "CloseUserLog"),
        ("Debug", "SetGodMode"),
        ("Debug", "ToggleAI"),
        ("Debug", "ToggleCollisions"),
        ("Debug", "DumpAliasData"),
        ("Debug", "DebugChannelNotify"),
        ("Debug", "StartScriptProfiling"),
        ("Debug", "StopScriptProfiling"),
        ("Debug", "StartStackProfiling"),
        ("Debug", "StopStackProfiling"),
        ("Debug", "TakeScreenshot"),
        ("Debug", "SetFootIK"),
        ("Debug", "ShowRefPosition"),
        ("Debug", "PlayerMoveToAndWait"),
        ("Debug", "MoveToCell"),
        ("Utility", "CaptureFrameRate"),
        ("Utility", "StartFrameRateCapture"),
        ("Utility", "EndFrameRateCapture"),
        ("Utility", "GetAverageFrameRate"),
        ("Utility", "GetMinFrameRate"),
        ("Utility", "GetMaxFrameRate"),
        ("Utility", "GetCurrentMemory"),
        ("Utility", "GetBudgetCount"),
        ("Utility", "GetBudgetName"),
        ("Utility", "GetCurrentBudget"),
        ("Utility", "OverBudget"),
    ];
    for (class, name) in globals_none {
        vm.natives
            .register_global(class, name, Box::new(|_| none()));
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::LoadOrder;

    #[test]
    fn the_hand_kept_count_matches_the_registry() {
        let mut vm = Vm::new();
        let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
        let r = register(&mut vm, &world);
        assert!(
            vm.natives.duplicates.is_empty(),
            "{:?}",
            vm.natives.duplicates
        );
        assert_eq!(r.natives - r.stubs, NATIVE_COUNT);
        assert!(vm.natives.has_method("quest", "setstage"));
        assert!(vm.natives.has_global("game", "getplayer"));
        assert!(vm.natives.has_method("alias", "getowningquest"));
    }
}
