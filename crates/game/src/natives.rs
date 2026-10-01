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

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::{NativeCall, NativeReturn, ObjectId, Value, Vm};

use crate::clock::GameClock;
use crate::combat::Hand;
use crate::conditions;
use crate::world::{
    reaction, AnimationListener, BinkOptions, DebugHostRequest, Handle, HostRequest, LeveledEntry,
    LosListener, MenuRequest, World,
};

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

/// Like [`text`], but an object argument carries its form id the way the
/// game's own log does, `[Object <handle> <formid>]`, the real 8-hex-digit
/// runtime FormID from [`World::runtime_id`], not the engine's internal
/// `{file}:{object}` load order notation. Aliases and active effects have no
/// form id, so they fall back to the bare handle.
fn text_world(world: &World, v: &Value) -> String {
    match v {
        Value::Object(o) => match world.form_of(*o) {
            Some(id) => format!("[Object <{}> {:08X}]", o.0, world.runtime_id(id)),
            None => v.to_string(),
        },
        other => text(other),
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

/// A native on the raw self object, for registrations that aliases and
/// forms share (`RegisterForUpdate` and its relatives).
macro_rules! self_method {
    ($vm:expr, $w:expr, $class:expr, $name:expr, |$world:ident, $call:ident, $me:ident| $body:expr) => {{
        let w = $w.clone();
        $vm.natives.register_method(
            $class,
            $name,
            Box::new(move |$call| {
                let mut guard = w.borrow_mut();
                let $world: &mut World = &mut guard;
                let Some($me) = $call.self_object else {
                    return none();
                };
                $body
            }),
        );
    }};
}

macro_rules! alias_method {
    ($vm:expr, $w:expr, $name:expr, |$world:ident, $call:ident, $quest:ident, $alias:ident| $body:expr) => {
        alias_method_on!($vm, $w, "Alias", $name, |$world, $call, $quest, $alias| {
            $body
        })
    };
}

macro_rules! alias_method_on {
    ($vm:expr, $w:expr, $class:expr, $name:expr, |$world:ident, $call:ident, $quest:ident, $alias:ident| $body:expr) => {{
        let w = $w.clone();
        $vm.natives.register_method(
            $class,
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
    leveled(vm, world);
    location(vm, world);
    faction(vm, world);
    actor_base(vm, world);
    cell(vm, world);
    message_scene_sound_spell(vm, world);
    visual_effect(vm, world);
    dlc(vm, world);
    fragments_and_effects(vm, world);
    active_magic(vm, world);
    image_space(vm, world);
    effect_shader(vm, world);
    weapon(vm, world);
    weather(vm, world);
    let stubs = stubs(vm);
    let natives = NATIVE_COUNT + stubs - (vm.natives.duplicates.len() - before);
    Registered { natives, stubs }
}

/// Hand-kept count of the non-stub natives registered below. The test in
/// this module checks it against the registry.
pub const NATIVE_COUNT: usize = 383;

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
        ("LeveledSpell", "Form"),
        ("ImageSpaceModifier", "Form"),
        ("Weather", "Form"),
        ("TextureSet", "Form"),
        ("Outfit", "Form"),
        ("Class", "Form"),
        ("CombatStyle", "Form"),
        ("VoiceType", "Form"),
        ("MusicType", "Form"),
        ("SoulGem", "MiscObject"),
        ("Scroll", "Form"),
        ("Apparatus", "Form"),
        ("Flora", "Activator"),
        ("TalkingActivator", "Activator"),
        ("Explosion", "Form"),
        ("Projectile", "Form"),
        ("Hazard", "Form"),
        ("EffectShader", "Form"),
        ("VisualEffect", "Form"),
        ("ShaderParticleGeometry", "Form"),
        ("EncounterZone", "Form"),
        ("Topic", "Form"),
        ("TopicInfo", "Form"),
        ("Art", "Form"),
        ("ImpactDataSet", "Form"),
        ("HeadPart", "Form"),
        ("ColorForm", "Form"),
        ("AssociationType", "Form"),
        ("LocationRefType", "Keyword"),
        ("ConstructibleObject", "Form"),
    ] {
        vm.natives.declare_parent(class, parent);
    }
}

fn form(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "Form", "GetFormID", |world, call, me| i(
        world.runtime_id(me) as i32
    ));
    method!(vm, w, "Form", "GetName", |world, call, me| {
        if world
            .tag_of(me)
            .is_some_and(|tag| matches!(tag.as_bytes(), b"ACHR" | b"REFR"))
        {
            return s("");
        }
        s(&world.name_of(me).unwrap_or_default())
    });
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
    self_method!(vm, w, "Form", "RegisterForSleep", |world, call, me| {
        if !world.sleep_listeners.contains(&me) {
            world.sleep_listeners.push(me);
        }
        none()
    });
    self_method!(vm, w, "Form", "UnregisterForSleep", |world, call, me| {
        world.sleep_listeners.retain(|o| *o != me);
        none()
    });
    self_method!(
        vm,
        w,
        "Form",
        "RegisterForTrackedStatsEvent",
        |world, call, me| {
            if !world.stats_listeners.contains(&me) {
                world.stats_listeners.push(me);
            }
            none()
        }
    );
    self_method!(
        vm,
        w,
        "Form",
        "UnregisterForTrackedStatsEvent",
        |world, call, me| {
            world.stats_listeners.retain(|o| *o != me);
            none()
        }
    );
    self_method!(
        vm,
        w,
        "Form",
        "RegisterForAnimationEvent",
        |world, call, me| {
            let Some(sender) = world.form_arg(&call.arg(0)) else {
                return b(false);
            };
            let event = text(&call.arg(1));
            let l = AnimationListener {
                listener: me,
                sender,
                event,
            };
            if !world.animation_listeners.contains(&l) {
                world.animation_listeners.push(l);
            }
            b(true)
        }
    );
    self_method!(
        vm,
        w,
        "Form",
        "UnregisterForAnimationEvent",
        |world, call, me| {
            let sender = world.form_arg(&call.arg(0));
            let event = text(&call.arg(1));
            world.animation_listeners.retain(|l| {
                !(l.listener == me
                    && Some(l.sender) == sender
                    && l.event.eq_ignore_ascii_case(&event))
            });
            none()
        }
    );
    for (name, single, gain) in [
        ("RegisterForLOS", false, false),
        ("RegisterForSingleLOSGain", true, true),
        ("RegisterForSingleLOSLost", true, false),
    ] {
        self_method!(vm, w, "Form", name, |world, call, me| {
            let (Some(viewer), Some(target)) =
                (world.form_arg(&call.arg(0)), world.form_arg(&call.arg(1)))
            else {
                return none();
            };
            let l = LosListener {
                listener: me,
                viewer,
                target,
                single,
                gain,
            };
            if !world.los_listeners.contains(&l) {
                world.los_listeners.push(l);
            }
            none()
        });
    }
    self_method!(vm, w, "Form", "UnregisterForLOS", |world, call, me| {
        let viewer = world.form_arg(&call.arg(0));
        let target = world.form_arg(&call.arg(1));
        world.los_listeners.retain(|l| {
            !(l.listener == me && Some(l.viewer) == viewer && Some(l.target) == target)
        });
        none()
    });
}

fn object_reference(vm: &mut Vm, w: &Shared) {
    method!(
        vm,
        w,
        "ObjectReference",
        "GetCurrentScene",
        |world, call, me| {
            let scene = world.current_scene(me);
            form_value(world, call, scene)
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "CreateDetectionEvent",
        |world, call, me| {
            if let Some(owner) = world.form_arg(&call.arg(0)) {
                world.create_detection_event(me, owner, call.arg(1).as_int());
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "DamageObject",
        |world, call, me| {
            world.damage_object(me, call.arg(0).as_float(), alloc!(call));
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetCurrentDestructionStage",
        |world, call, me| i(world.destruction_stage(me))
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "ClearDestruction",
        |world, call, me| {
            world.clear_destruction(me, alloc!(call));
            none()
        }
    );
    let waits = w.clone();
    vm.natives.register_method(
        "ObjectReference",
        "WaitForAnimationEvent",
        Box::new(move |call| {
            let Some(source) = call
                .self_object
                .and_then(|object| waits.borrow().form_of(object))
            else {
                return b(false);
            };
            let event = text(&call.arg(0));
            let latent = call.defer();
            waits
                .borrow_mut()
                .wait_for_animation_event(source, &event, latent);
            NativeReturn::Latent(latent)
        }),
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "InterruptCast",
        |world, _call, me| {
            world.interrupt_spell_cast(me);
            none()
        }
    );
    for name in ["Enable", "EnableNoWait"] {
        method!(vm, w, "ObjectReference", name, |world, call, me| {
            world.set_reference_enabled(me, true);
            none()
        });
    }
    for name in ["Disable", "DisableNoWait"] {
        method!(vm, w, "ObjectReference", name, |world, call, me| {
            world.set_reference_enabled(me, false);
            none()
        });
    }
    method!(vm, w, "ObjectReference", "IsEnabled", |world, call, me| b(
        world.reference(me).is_some_and(|r| r.enabled)
    ));
    method!(vm, w, "ObjectReference", "IsDisabled", |world, call, me| b(
        world.reference(me).is_some_and(|r| !r.enabled)
    ));
    method!(vm, w, "ObjectReference", "Is3DLoaded", |world, call, me| b(
        world.is_reference_3d_loaded(me)
    ));
    method!(vm, w, "ObjectReference", "Delete", |world, call, me| {
        if let Some(r) = world.reference(me) {
            r.deleted = true;
        }
        world.set_reference_enabled(me, false);
        none()
    });
    method!(vm, w, "ObjectReference", "IsDeleted", |world, call, me| b(
        world.reference(me).is_some_and(|r| r.deleted)
    ));
    method!(
        vm,
        w,
        "ObjectReference",
        "GetTriggerObjectCount",
        |world, call, me| match world.trigger_object_count(me) {
            Some(count) => i(count as i32),
            None => {
                call.log.push(
                    call.time,
                    "Error: GetTriggerObjectCount called on a non-trigger",
                );
                i(0)
            }
        }
    );
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
    method!(
        vm,
        w,
        "ObjectReference",
        "TranslateTo",
        |world, call, me| {
            world.start_translation(
                me,
                [
                    call.arg(0).as_float(),
                    call.arg(1).as_float(),
                    call.arg(2).as_float(),
                ],
                [
                    call.arg(3).as_float().to_radians(),
                    call.arg(4).as_float().to_radians(),
                    call.arg(5).as_float().to_radians(),
                ],
                call.arg(6).as_float(),
                call.args.get(7).map_or(0.0, Value::as_float),
                crate::world::TranslationPath::Linear,
                alloc!(call),
            );
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "SplineTranslateTo",
        |world, call, me| {
            world.start_translation(
                me,
                [
                    call.arg(0).as_float(),
                    call.arg(1).as_float(),
                    call.arg(2).as_float(),
                ],
                [
                    call.arg(3).as_float().to_radians(),
                    call.arg(4).as_float().to_radians(),
                    call.arg(5).as_float().to_radians(),
                ],
                call.arg(7).as_float(),
                call.args.get(8).map_or(0.0, Value::as_float),
                crate::world::TranslationPath::Spline {
                    tangent_magnitude: call.arg(6).as_float(),
                },
                alloc!(call),
            );
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "StopTranslation",
        |world, call, me| {
            world.stop_translation(me, alloc!(call));
            none()
        }
    );
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
                .and_then(|item| conditions::inventory_count(world, me, item, &mut Vec::new()).ok())
                .unwrap_or(0))
        }
    );
    method!(vm, w, "ObjectReference", "AddItem", |world, call, me| {
        let count = call.args.get(1).map_or(1, Value::as_int);
        if let Some(item) = world.form_arg(&call.arg(0)) {
            if world.tag_of(item) == Some(Tag::new(b"FLST")) {
                world.add_form_list_notified(me, item, count, &mut |name, class| {
                    call.create_object(name, class)
                });
            } else {
                world.add_item_notified(me, item, count, &mut |name, class| {
                    call.create_object(name, class)
                });
            }
        }
        none()
    });
    method!(vm, w, "ObjectReference", "RemoveItem", |world, call, me| {
        let count = call.args.get(1).map_or(1, Value::as_int);
        let target = world.form_arg(&call.arg(3));
        if let Some(item) = world.form_arg(&call.arg(0)) {
            world.remove_item_notified(me, item, count.max(1), target, &mut |name, class| {
                call.create_object(name, class)
            });
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
                .map(|r| r.inventory.iter().map(|(k, v)| (*k, v)).collect())
                .unwrap_or_default();
            for (item, count) in items {
                world.remove_item_notified(me, item, count, target, &mut |name, class| {
                    call.create_object(name, class)
                });
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
                    if let Some(item) = &mut r.item {
                        item.set_display_name(&name);
                    }
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
            world.animation_event(me, &event, alloc!(call));
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
        world.set_locked(me, lock, alloc!(call));
        none()
    });
    method!(vm, w, "ObjectReference", "GetKey", |world, call, me| {
        let key = world.key(me);
        form_value(world, call, key)
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
            world.set_lock_level(me, call.arg(0).as_int(), alloc!(call));
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
    method!(vm, w, "ObjectReference", "AddToMap", |world, call, me| {
        let fast_travel = call.arg(0).as_bool();
        if let Some(r) = world.reference(me) {
            r.on_map = true;
            r.fast_travel = fast_travel;
        }
        none()
    });
    method!(
        vm,
        w,
        "ObjectReference",
        "EnableFastTravel",
        |world, call, me| {
            let on = call.args.first().is_none_or(Value::as_bool);
            if let Some(r) = world.reference(me) {
                r.fast_travel = on;
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "SetDestroyed",
        |world, call, me| {
            let on = call.args.first().is_none_or(Value::as_bool);
            if let Some(r) = world.reference(me) {
                r.destroyed = on;
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "IgnoreFriendlyHits",
        |world, call, me| {
            let on = call.args.first().is_none_or(Value::as_bool);
            if let Some(r) = world.reference(me) {
                r.ignore_friendly_hits = on;
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "IsIgnoringFriendlyHits",
        |world, call, me| b(world.reference(me).is_some_and(|r| r.ignore_friendly_hits))
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetCurrentLocation",
        |world, call, me| {
            let loc = world.current_location(me);
            form_value(world, call, loc)
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetEditorLocation",
        |world, call, me| {
            let loc = world.editor_location(me);
            form_value(world, call, loc)
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "IsInLocation",
        |world, call, me| {
            let Some(wanted) = world.form_arg(&call.arg(0)) else {
                return b(false);
            };
            let Some(current) = world.current_location(me) else {
                return b(false);
            };
            b(current == wanted || world.location_is_child(wanted, current))
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "MoveToMyEditorLocation",
        |world, call, me| b(world.move_to_editor_location(me))
    );
    method!(vm, w, "ObjectReference", "Reset", |world, call, me| {
        let target = world.form_arg(&call.arg(0));
        world.reset_reference(me, target);
        none()
    });
    method!(
        vm,
        w,
        "ObjectReference",
        "TetherToHorse",
        |world, call, me| {
            let Some(horse) = world.form_arg(&call.arg(0)) else {
                return none();
            };
            let Some((position, rotation, cell)) = world
                .reference(me)
                .map(|cart| (cart.position, cart.rotation, cart.cell))
            else {
                return none();
            };
            if let Some(horse) = world.reference(horse) {
                horse.position = position;
                horse.rotation = rotation;
                horse.cell = cell;
            }
            none()
        }
    );
    for (name, tag) in [("GetActorOwner", *b"NPC_"), ("GetFactionOwner", *b"FACT")] {
        method!(vm, w, "ObjectReference", name, |world, call, me| {
            let owner = world
                .reference(me)
                .and_then(|r| r.owner)
                .filter(|o| world.tag_of(*o).is_some_and(|t| *t.as_bytes() == tag));
            form_value(world, call, owner)
        });
    }
    for name in ["SetActorOwner", "SetFactionOwner"] {
        method!(vm, w, "ObjectReference", name, |world, call, me| {
            let owner = world.form_arg(&call.arg(0));
            if let Some(r) = world.reference(me) {
                r.owner = owner;
                if let Some(item) = &mut r.item {
                    item.set_owner(owner);
                }
            }
            none()
        });
    }
    for class in ["ObjectReference", "ReferenceAlias"] {
        self_method!(
            vm,
            w,
            class,
            "AddInventoryEventFilter",
            |world, call, me| {
                if let Some(filter) = world.form_arg(&call.arg(0)) {
                    world.inventory_filters.entry(me).or_default().push(filter);
                }
                none()
            }
        );
        self_method!(
            vm,
            w,
            class,
            "RemoveInventoryEventFilter",
            |world, call, me| {
                let filter = world.form_arg(&call.arg(0));
                if let Some(list) = world.inventory_filters.get_mut(&me) {
                    list.retain(|f| Some(*f) != filter);
                }
                none()
            }
        );
        self_method!(
            vm,
            w,
            class,
            "RemoveAllInventoryEventFilters",
            |world, call, me| {
                world.inventory_filters.remove(&me);
                none()
            }
        );
    }
    method!(
        vm,
        w,
        "ObjectReference",
        "PushActorAway",
        |world, call, me| {
            if let Some(target) = world.form_arg(&call.arg(0)) {
                let _ = world.push_actor_away(me, target, call.arg(1).as_float());
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "KnockAreaEffect",
        |world, call, me| {
            let _ = world.knock_area_effect(me, call.arg(0).as_float(), call.arg(1).as_float());
            none()
        }
    );
    for (name, enabled) in [
        ("ForceAddRagdollToWorld", true),
        ("ForceRemoveRagdollFromWorld", false),
    ] {
        method!(vm, w, "ObjectReference", name, |world, _call, me| {
            let _ = world.force_ragdoll(me, enabled);
            none()
        });
    }
}

fn actor(vm: &mut Vm, w: &Shared) {
    {
        let w = w.clone();
        vm.natives.register_method(
            "Actor",
            "ShowGiftMenu",
            Box::new(move |call| {
                let mut world = w.borrow_mut();
                let Some(giver) = call.self_object.and_then(|object| world.form_of(object)) else {
                    return i(0);
                };
                if world.gift_menu.is_some() {
                    return i(0);
                }
                let player = world.player();
                let giving = matches!(call.arg(0), Value::Bool(true));
                let request = crate::world::GiftMenuRequest {
                    latent: call.defer(),
                    giver,
                    source: if giving { player } else { giver },
                    target: if giving { giver } else { player },
                    filter: world.form_arg(&call.arg(1)),
                    show_stolen: matches!(call.arg(2), Value::Bool(true)),
                    use_favor_points: !matches!(call.arg(3), Value::Bool(false)),
                    favor_points: 0,
                };
                world.gift_menu = Some(request.clone());
                NativeReturn::Latent(request.latent)
            }),
        );
    }
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
            if let Err(error) =
                world.set_actor_value(me, &text(&call.arg(0)), call.arg(1).as_float())
            {
                call.log.push(call.time, error.to_string());
            }
            none()
        });
    }
    for name in ["ModActorValue", "ModAV"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            if let Err(error) =
                world.mod_actor_value(me, &text(&call.arg(0)), call.arg(1).as_float())
            {
                call.log.push(call.time, error.to_string());
            }
            none()
        });
    }
    for name in ["DamageActorValue", "DamageAV"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            if let Err(error) =
                world.damage_actor_value(me, &text(&call.arg(0)), call.arg(1).as_float())
            {
                call.log.push(call.time, error.to_string());
            }
            none()
        });
    }
    for name in ["GetActorValuePercentage", "GetAVPercentage"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            let name = text(&call.arg(0));
            f(world.actor_value_percentage(me, &name))
        });
    }
    for name in ["GetActorValueMax", "GetAVMax"] {
        method!(vm, w, "Actor", name, |world, call, me| f(
            world.actor_value_maximum(me, &text(&call.arg(0)))
        ));
    }
    for name in ["RestoreActorValue", "RestoreAV"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            if let Err(error) =
                world.restore_actor_value(me, &text(&call.arg(0)), call.arg(1).as_float())
            {
                call.log.push(call.time, error.to_string());
            }
            none()
        });
    }
    for name in ["ForceActorValue", "ForceAV"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            if let Err(error) =
                world.force_actor_value(me, &text(&call.arg(0)), call.arg(1).as_float())
            {
                call.log.push(call.time, error.to_string());
            }
            none()
        });
    }
    method!(vm, w, "Actor", "IsDead", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.dead)));
    method!(
        vm,
        w,
        "Actor",
        "SetExpressionOverride",
        |world, call, me| {
            let mood = call.args.first().map_or(-1, Value::as_int);
            let strength = call.args.get(1).map_or(100, Value::as_int);
            if let Err(error) = world.set_expression_override(me, mood, strength) {
                call.log.push(call.time, error);
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "Actor",
        "ClearExpressionOverride",
        |world, call, me| {
            if let Err(error) = world.clear_expression_override(me) {
                call.log.push(call.time, error);
            }
            none()
        }
    );
    method!(vm, w, "Actor", "Kill", |world, call, me| {
        let killer = world.form_arg(&call.arg(0));
        world.kill_actor(me, killer);
        none()
    });
    method!(vm, w, "Actor", "GetKiller", |world, call, me| {
        let killer = world.actor(me).filter(|a| a.dead).and_then(|a| a.killer);
        form_value(world, call, killer)
    });
    method!(vm, w, "Actor", "Resurrect", |world, call, me| {
        if let Err(error) = world.resurrect_actor(me) {
            call.log.push(call.time, error.to_string());
        }
        none()
    });
    method!(vm, w, "Actor", "IsInCombat", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.in_combat)));
    method!(vm, w, "Actor", "GetCombatState", |world, _call, me| i(
        i32::from(world.actor(me).is_some_and(|a| a.in_combat))
    ));
    method!(vm, w, "Actor", "GetSleepState", |world, _call, me| i(
        if world.actor(me).is_some_and(|a| a.ai.sleeping) {
            3
        } else {
            0
        }
    ));
    method!(vm, w, "Actor", "HasLOS", |world, call, me| {
        let Some(target) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        let from = world.reference(me).map(|reference| reference.position);
        let to = world.reference(target).and_then(|reference| {
            (reference.enabled && !reference.deleted).then_some(reference.position)
        });
        b(from.zip(to).is_some_and(|(from, to)| from == to))
    });
    method!(vm, w, "Actor", "PathToReference", |world, call, me| {
        let speed = call.arg(1).as_float();
        let target = world.form_arg(&call.arg(0));
        let from = world.reference(me).map(|reference| reference.position);
        let to = target.and_then(|target| {
            world.reference(target).and_then(|reference| {
                (reference.enabled && !reference.deleted).then_some(reference.position)
            })
        });
        b(speed.is_finite()
            && (0.0..=1.0).contains(&speed)
            && from.zip(to).is_some_and(|(from, to)| from == to))
    });
    method!(vm, w, "Actor", "EvaluatePackage", |world, call, me| {
        world.evaluate_package(me, alloc!(call));
        none()
    });
    method!(
        vm,
        w,
        "Actor",
        "MoveToPackageLocation",
        |world, call, me| {
            if !call.args.is_empty() {
                call.log.push(
                    call.time,
                    "Actor.MoveToPackageLocation expects no arguments",
                );
            } else if !world.move_to_package_location(me) {
                call.log.push(
                    call.time,
                    "Actor.MoveToPackageLocation has no package location",
                );
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "Actor",
        "WillIntimidateSucceed",
        |world, call, me| {
            if !call.args.is_empty() {
                call.log.push(
                    call.time,
                    "Actor.WillIntimidateSucceed expects no arguments",
                );
                return b(false);
            }
            match crate::conditions::will_intimidate_succeed(world, me) {
                Ok(succeeds) => b(succeeds),
                Err(error) => {
                    call.log
                        .push(call.time, format!("Actor.WillIntimidateSucceed: {error}"));
                    b(false)
                }
            }
        }
    );
    method!(vm, w, "Actor", "DoCombatSpellApply", |world, call, me| {
        let Some(spell) = call.args.first().and_then(|value| world.form_arg(value)) else {
            call.log
                .push(call.time, "Actor.DoCombatSpellApply requires a spell form");
            return none();
        };
        let Some(target) = call.args.get(1).and_then(|value| world.form_arg(value)) else {
            call.log
                .push(call.time, "Actor.DoCombatSpellApply requires a target form");
            return none();
        };
        if call.args.len() != 2 {
            call.log
                .push(call.time, "Actor.DoCombatSpellApply expects two arguments");
            return none();
        }
        if let Err(error) = world.cast_spell(spell, me, Some(me), Some(target), alloc!(call)) {
            call.log
                .push(call.time, format!("Actor.DoCombatSpellApply: {error}"));
        }
        none()
    });
    method!(vm, w, "Actor", "GetCurrentPackage", |world, call, me| {
        let package = world.current_package(me);
        form_value(world, call, package)
    });
    method!(vm, w, "Actor", "StartCombat", |world, call, me| {
        let target = world.form_arg(&call.arg(0));
        if let Some(a) = world.actor(me) {
            a.in_combat = true;
            a.combat_target = target;
        }
        none()
    });
    method!(vm, w, "Actor", "StopCombat", |world, call, me| {
        if let Some(a) = world.actor(me) {
            a.in_combat = false;
            a.combat_target = None;
        }
        none()
    });
    method!(vm, w, "Actor", "GetCombatTarget", |world, call, me| {
        let target = world
            .actor(me)
            .filter(|a| a.in_combat)
            .and_then(|a| a.combat_target);
        form_value(world, call, target)
    });
    method!(vm, w, "Actor", "GetActorBase", |world, call, me| {
        let base = world.base_of(me);
        form_value(world, call, base)
    });
    method!(vm, w, "Actor", "GetLeveledActorBase", |world, call, me| {
        match world.leveled_actor_base(me) {
            Ok(base) => form_value(world, call, base),
            Err(error) => {
                call.log
                    .push(call.time, format!("Actor.GetLeveledActorBase: {error}"));
                none()
            }
        }
    });
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
        let can_do_favors = call.args.get(1).is_none_or(Value::as_bool);
        if let Some(a) = world.actor(me) {
            a.teammate = on;
            a.can_do_favors = can_do_favors;
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
            if on {
                a.restrained = false;
            }
        }
        none()
    });
    // Restrained and unconscious exclude each other (the SetRestrained page).
    method!(vm, w, "Actor", "SetRestrained", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(a) = world.actor(me) {
            a.restrained = on;
            if on {
                a.unconscious = false;
            }
        }
        none()
    });
    for (name, field) in [
        ("SetAlert", 0),
        ("SetAttackActorOnSight", 1),
        ("SetDontMove", 2),
        ("SetAllowPCDialogue", 3),
    ] {
        method!(vm, w, "Actor", name, |world, call, me| {
            let on = call.args.first().is_none_or(Value::as_bool);
            if let Some(a) = world.actor(me) {
                match field {
                    0 => a.alerted = on,
                    1 => a.attack_on_sight = on,
                    2 => a.dont_move = on,
                    _ => a.allow_pc_dialogue = on,
                }
            }
            none()
        });
    }
    method!(vm, w, "Actor", "IsAlerted", |world, call, me| b(
        world.actor_is_alarmed(me)
    ));
    method!(vm, w, "Actor", "IsAlarmed", |world, call, me| b(
        world.actor_is_alarmed(me)
    ));
    method!(vm, w, "Actor", "IsBleedingOut", |world, call, me| b(
        world.actor_is_bleeding_out(me)
    ));
    // StartSneaking toggles (the page's note), IsSneaking reads it.
    method!(vm, w, "Actor", "StartSneaking", |world, call, me| {
        if let Some(a) = world.actor(me) {
            a.sneaking = !a.sneaking;
        }
        none()
    });
    method!(vm, w, "Actor", "IsSneaking", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.sneaking)));
    for (name, drawn) in [("DrawWeapon", true), ("SheatheWeapon", false)] {
        method!(vm, w, "Actor", name, |world, call, me| {
            if let Some(a) = world.actor(me) {
                a.weapon_drawn = drawn;
            }
            none()
        });
    }
    method!(vm, w, "Actor", "IsWeaponDrawn", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.weapon_drawn)));
    method!(vm, w, "Actor", "IsArrested", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.arrested)));
    method!(vm, w, "Actor", "ClearArrested", |world, call, me| {
        if let Some(a) = world.actor(me) {
            a.arrested = false;
        }
        none()
    });
    method!(vm, w, "Actor", "AddPerk", |world, call, me| {
        if let Some(perk) = world.form_arg(&call.arg(0)) {
            if let Some(a) = world.actor(me) {
                a.perks.insert(perk);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "RemovePerk", |world, call, me| {
        if let Some(perk) = world.form_arg(&call.arg(0)) {
            if let Some(a) = world.actor(me) {
                a.perks.remove(&perk);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "HasPerk", |world, call, me| {
        let Some(perk) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        b(world.actor(me).is_some_and(|a| a.perks.contains(&perk)))
    });
    method!(vm, w, "Actor", "SetRelationshipRank", |world, call, me| {
        let rank = call.arg(1).as_int().clamp(-4, 4);
        if let Some(other) = world.form_arg(&call.arg(0)) {
            if let Some(a) = world.actor(me) {
                a.relationships.insert(other, rank);
            }
            if let Some(o) = world.actor(other) {
                o.relationships.insert(me, rank);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "GetRelationshipRank", |world, call, me| {
        let Some(other) = world.form_arg(&call.arg(0)) else {
            return i(0);
        };
        i(world
            .actor(me)
            .and_then(|a| a.relationships.get(&other).copied())
            .or_else(|| world.relationship_record_rank(me, other))
            .unwrap_or(0))
    });
    method!(
        vm,
        w,
        "Actor",
        "GetHighestRelationshipRank",
        |world, call, me| i(world
            .actor(me)
            .and_then(|a| a.relationships.values().max().copied())
            .unwrap_or(0))
    );
    method!(
        vm,
        w,
        "Actor",
        "GetLowestRelationshipRank",
        |world, call, me| i(world
            .actor(me)
            .and_then(|a| a.relationships.values().min().copied())
            .unwrap_or(0))
    );
    method!(vm, w, "Actor", "GetCrimeFaction", |world, call, me| {
        let faction = world.actor(me).and_then(|a| a.crime_faction);
        form_value(world, call, faction)
    });
    // Setting a crime faction also joins it (the SetCrimeFaction page).
    method!(vm, w, "Actor", "SetCrimeFaction", |world, call, me| {
        let faction = world.form_arg(&call.arg(0));
        if let Some(a) = world.actor(me) {
            a.crime_faction = faction;
            if let Some(f) = faction {
                a.factions.entry(f).or_insert(0);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "SetOutfit", |world, call, me| {
        let outfit = world.form_arg(&call.arg(0));
        let sleep = call.arg(1).as_bool();
        if let Some(a) = world.actor(me) {
            if sleep {
                a.sleep_outfit = outfit;
            } else {
                a.outfit = outfit;
            }
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
        b(world.add_actor_spell(me, spell))
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
        b(world.has_spell(me, spell))
    });
    method!(vm, w, "Actor", "EquipItem", |world, call, me| {
        if let Some(item) = world.form_arg(&call.arg(0)) {
            if world.item_count(me, item) == 0 {
                world.add_item(me, item, 1);
            }
            if world
                .tag_of(item)
                .is_some_and(|tag| tag.as_bytes() == b"ALCH")
            {
                if let Err(error) = world.drink_ingestible(item, me, alloc!(call)) {
                    call.log
                        .push(call.time, format!("Actor.EquipItem: {error}"));
                }
            } else if let Err(error) = world.equip_item_with_effects(me, item, alloc!(call)) {
                call.log
                    .push(call.time, format!("Actor.EquipItem: {error}"));
            }
        }
        none()
    });
    method!(vm, w, "Actor", "UnequipItem", |world, call, me| {
        if let Some(item) = world.form_arg(&call.arg(0)) {
            if let Err(error) = world.unequip_item_with_effects(me, item, alloc!(call)) {
                call.log
                    .push(call.time, format!("Actor.UnequipItem: {error}"));
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
    // "Gets the spell the actor currently has equipped in the specified
    // source", 0 left hand, 1 right hand (ck-wiki GetEquippedSpell - Actor).
    // Sources 2 (other) and 3 (instant) have no per-hand state here, the
    // same gap EquipSpell/UnequipSpell leave for those two sources below.
    method!(vm, w, "Actor", "GetEquippedSpell", |world, call, me| {
        let hand = match call.arg(0).as_int() {
            0 => Hand::Left,
            1 => Hand::Right,
            _ => return form_value(world, call, None),
        };
        let spell = world.hand_spell(me, hand);
        form_value(world, call, spell)
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
        .is_some_and(|q| {
            q.running
                && !matches!(
                    q.lifecycle_task,
                    Some(
                        crate::quest::state::LifecycleTask::Starting
                            | crate::quest::state::LifecycleTask::Stopping
                    )
                )
        })));
    method!(vm, w, "Quest", "IsStarting", |world, _call, me| b(
        world.quest_is_starting(me)
    ));
    method!(vm, w, "Quest", "IsStopping", |world, _call, me| b(
        world.quest_is_stopping(me)
    ));
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
        world.quest_state(me).map_or(0, |q| i32::from(q.highest))
    ));
    for name in ["GetStageDone", "IsStageDone"] {
        method!(vm, w, "Quest", name, |world, call, me| {
            let stage = call.arg(0).as_int();
            b(u16::try_from(stage)
                .is_ok_and(|st| world.quest_state(me).is_some_and(|q| q.done.contains(&st))))
        });
    }
    for name in ["SetObjectiveDisplayed", "SetObjectiveDisplayedNoWait"] {
        method!(vm, w, "Quest", name, |world, call, me| {
            let on = call.args.get(1).is_none_or(Value::as_bool);
            if let Ok(idx) = u16::try_from(call.arg(0).as_int()) {
                if let Some(o) = world.objective(me, idx) {
                    o.displayed = on;
                }
            }
            none()
        });
    }
    // ModObjectiveGlobal(afModValue, aModGlobal, aiObjectiveID = -1,
    // afTargetValue = -1.0, abCountingUp = true, abCompleteObjective = true,
    // abRedisplayObjective = true): mod the global, and when an objective
    // is named, redisplay it and complete (or fail) it once the target is
    // reached. Returns whether it was reached.
    method!(vm, w, "Quest", "ModObjectiveGlobal", |world, call, me| {
        let Some(global) = world.form_arg(&call.arg(1)) else {
            return b(false);
        };
        let value = world.global_value(global) + call.arg(0).as_float();
        world.set_global(global, value);
        let objective = call.args.get(2).map_or(-1, Value::as_int);
        let target = call.args.get(3).map_or(-1.0, Value::as_float);
        let counting_up = call.args.get(4).is_none_or(Value::as_bool);
        let complete = call.args.get(5).is_none_or(Value::as_bool);
        let redisplay = call.args.get(6).is_none_or(Value::as_bool);
        let reached = target >= 0.0
            && if counting_up {
                value >= target
            } else {
                value <= target
            };
        if let Ok(idx) = u16::try_from(objective) {
            if let Some(o) = world.objective(me, idx) {
                if redisplay {
                    o.displayed = true;
                }
                if reached {
                    if complete {
                        o.completed = true;
                    } else {
                        o.failed = true;
                    }
                }
            }
        }
        b(reached)
    });
    method!(
        vm,
        w,
        "Quest",
        "UpdateCurrentInstanceGlobal",
        |world, call, me| {
            let Some(global) = world.form_arg(&call.arg(0)) else {
                return b(false);
            };
            let value = world.global_value(global);
            match world.quest_state(me) {
                Some(q) => {
                    q.journal.update_global(global, value);
                    b(true)
                }
                None => b(false),
            }
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
    for name in ["GetReference", "GetRef"] {
        alias_method!(vm, w, name, |world, call, quest, alias| {
            let r = world.alias_ref(quest, alias).filter(|target| {
                matches!(
                    world.class_of(Handle::Form(*target)),
                    "ObjectReference" | "Actor"
                )
            });
            form_value(world, call, r)
        });
    }
    for name in ["GetActorReference", "GetActorRef"] {
        alias_method!(vm, w, name, |world, call, quest, alias| {
            let r = world
                .alias_ref(quest, alias)
                .filter(|target| world.class_of(Handle::Form(*target)) == "Actor");
            form_value(world, call, r)
        });
    }
    alias_method!(vm, w, "ForceRefTo", |world, call, quest, alias| {
        if let Some(target) = world.form_arg(&call.arg(0)).filter(|target| {
            world
                .tag_of(*target)
                .is_some_and(|tag| matches!(tag.as_bytes(), b"REFR" | b"ACHR"))
        }) {
            world.force_alias(quest, alias, Some(target));
        }
        none()
    });
    alias_method_on!(
        vm,
        w,
        "ReferenceAlias",
        "ForceRefIfEmpty",
        |world, call, quest, alias| {
            if world.alias_ref(quest, alias).is_some() {
                return b(false);
            }
            let target = world.form_arg(&call.arg(0));
            b(world.force_alias(quest, alias, target))
        }
    );
    alias_method!(vm, w, "Clear", |world, call, quest, alias| {
        world.force_alias(quest, alias, None);
        none()
    });
    // A location alias holds a form the same way a reference alias does.
    alias_method_on!(
        vm,
        w,
        "LocationAlias",
        "GetLocation",
        |world, call, quest, alias| {
            let l = world.alias_ref(quest, alias);
            form_value(world, call, l)
        }
    );
    alias_method_on!(
        vm,
        w,
        "LocationAlias",
        "ForceLocationTo",
        |world, call, quest, alias| {
            let target = world.form_arg(&call.arg(0));
            world.force_alias(quest, alias, target);
            none()
        }
    );
}

fn game(vm: &mut Vm, w: &Shared) {
    global!(vm, w, "Game", "GetCurrentCrosshairRef", |world, call| {
        form_value(world, call, world.crosshair_reference())
    });
    global!(vm, w, "Game", "GetPlayerGrabbedRef", |world, call| {
        form_value(world, call, world.grabbed_reference())
    });
    global!(vm, w, "Game", "QuitToMainMenu", |world, _call| {
        world.host_requests.push(HostRequest::QuitToMainMenu);
        none()
    });
    global!(vm, w, "Game", "StartTitleSequence", |world, call| {
        world
            .host_requests
            .push(HostRequest::StartTitleSequence(text(&call.arg(0))));
        none()
    });
    global!(vm, w, "Game", "GetPlayer", |world, call| {
        let p = world.player();
        form_value(world, call, Some(p))
    });
    for (name, request) in [
        ("PrecacheCharGen", MenuRequest::PrecacheCharGen),
        ("PrecacheCharGenClear", MenuRequest::PrecacheCharGenClear),
    ] {
        let shared = w.clone();
        vm.natives.register_global(
            "Game",
            name,
            Box::new(move |_| {
                shared.borrow_mut().menu_requests.push(request);
                none()
            }),
        );
    }
    global!(vm, w, "Game", "ShowTrainingMenu", |world, call| {
        let Some(trainer) = world.form_arg(&call.arg(0)).filter(|id| {
            world
                .reference(*id)
                .is_some_and(|reference| reference.is_actor)
        }) else {
            return none();
        };
        world.menu_requests.push(MenuRequest::Training(trainer));
        world.request_training_menu(trainer);
        none()
    });
    global!(vm, w, "Game", "SetCameraTarget", |world, call| {
        let target = world.form_arg(&call.arg(0));
        if let Some(target) = target.filter(|id| {
            world
                .reference(*id)
                .is_some_and(|reference| reference.is_actor)
        }) {
            world.camera_controls.target = Some(target);
        }
        none()
    });
    global!(vm, w, "Game", "ShowFirstPersonGeometry", |world, call| {
        world.camera_controls.first_person_geometry = call.arg(0).as_bool();
        none()
    });
    global!(vm, w, "Game", "SetSittingRotation", |world, call| {
        world.camera_controls.sitting_rotation_degrees = call.arg(0).as_float();
        none()
    });
    global!(vm, w, "Game", "AdvanceSkill", |world, call| {
        let skill = text(&call.arg(0));
        let magnitude = call.arg(1).as_float();
        let player = world.player();
        if let Err(error) = world.advance_skill(player, &skill, magnitude) {
            world.report_native_error("Game.AdvanceSkill", error.to_string());
        }
        none()
    });
    global!(vm, w, "Game", "SetHudCartMode", |world, call| {
        world.set_hud_cart_mode(call.arg(0).as_bool());
        none()
    });
    global!(vm, w, "Game", "ShowRaceMenu", |world, _call| {
        world.show_race_menu();
        none()
    });
    global!(vm, w, "Game", "TriggerScreenBlood", |world, call| {
        world.trigger_screen_blood(call.arg(0).as_int());
        none()
    });
    global!(vm, w, "Game", "PlayBink", |world, call| {
        let file_name = text(&call.arg(0));
        if let Err(error) = world.play_bink(
            &file_name,
            BinkOptions {
                interruptible: call.arg(1).as_bool(),
                mute_audio: call.arg(2).as_bool(),
                mute_music: call.arg(3).as_bool(),
                letterbox: call.arg(4).as_bool(),
            },
        ) {
            world.report_native_error("Game.PlayBink", error.to_string());
        }
        none()
    });
    global!(vm, w, "Game", "ServeTime", |world, _call| {
        let player = world.player();
        if let Some(faction) = world.actor(player).and_then(|actor| actor.crime_faction) {
            let _ = world.serve_jail_time(faction);
        }
        none()
    });
    global!(vm, w, "Game", "ClearPrison", |world, _call| {
        world.clear_prison();
        let player = world.player();
        if let Some(actor) = world.actor(player) {
            actor.arrested = false;
        }
        none()
    });
    global!(vm, w, "Game", "FadeOutGame", |world, call| {
        let seconds_before_fade = call.arg(2).as_float();
        let fade_duration = call.arg(3).as_float();
        if !seconds_before_fade.is_finite()
            || !fade_duration.is_finite()
            || seconds_before_fade < 0.0
            || fade_duration < 0.0
        {
            call.log.push(
                call.time,
                "Game.FadeOutGame: fade timings must be finite and non-negative".to_string(),
            );
            return none();
        }
        world.host_requests.push(HostRequest::FadeOutGame {
            fading_out: call.arg(0).as_bool(),
            black: call.arg(1).as_bool(),
            seconds_before_fade,
            fade_duration,
        });
        none()
    });
    global!(vm, w, "Game", "RequestSave", |world, _call| {
        world
            .host_requests
            .push(HostRequest::Save { automatic: false });
        none()
    });
    global!(vm, w, "Game", "RequestAutoSave", |world, _call| {
        world
            .host_requests
            .push(HostRequest::Save { automatic: true });
        none()
    });
    global!(vm, w, "Game", "ForceFirstPerson", |world, _call| {
        world.host_requests.push(HostRequest::CameraFirstPerson);
        none()
    });
    global!(vm, w, "Game", "ForceThirdPerson", |world, _call| {
        world.host_requests.push(HostRequest::CameraThirdPerson);
        none()
    });
    for name in ["GetForm", "GetFormEx", "GetFormEx2"] {
        global!(vm, w, "Game", name, |world, call| {
            let id = world.from_runtime_id(call.arg(0).as_int() as u32);
            form_value(world, call, id)
        });
    }
    for (name, actors_only, random) in [
        ("FindClosestActor", true, false),
        ("FindRandomActor", true, true),
        ("FindClosestReferenceOfType", false, false),
        ("FindRandomReferenceOfType", false, true),
    ] {
        global!(vm, w, "Game", name, |world, call| {
            let base = if actors_only {
                None
            } else {
                let Some(base) = world.form_arg(&call.arg(0)) else {
                    return none();
                };
                Some(base)
            };
            let offset = usize::from(!actors_only);
            let center = [
                call.arg(offset).as_float(),
                call.arg(offset + 1).as_float(),
                call.arg(offset + 2).as_float(),
            ];
            let radius = call.arg(offset + 3).as_float();
            let id = world.find_reference_of_base(base, center, radius, actors_only, random);
            form_value(world, call, id)
        });
    }
    global!(vm, w, "Game", "GetFormFromFile", |world, call| {
        let id = world.form_from_file(call.arg(0).as_int() as u32, &text(&call.arg(1)));
        form_value(world, call, id)
    });
    global!(vm, w, "Game", "EnablePlayerControls", |world, call| {
        let defaults = [true; 8];
        for (index, enabled) in world.player_control_flags.iter_mut().enumerate() {
            if call.args.get(index).map_or(defaults[index], Value::as_bool) {
                *enabled = true;
            }
        }
        if call.args.get(2).is_none_or(Value::as_bool) {
            world.player_control_pov_type = call.args.get(8).map_or(0, Value::as_int);
        }
        world.player_controls = world.player_control_flags.iter().all(|enabled| *enabled);
        none()
    });
    global!(vm, w, "Game", "DisablePlayerControls", |world, call| {
        let defaults = [true, true, false, false, false, true, true, false];
        for (index, enabled) in world.player_control_flags.iter_mut().enumerate() {
            if call.args.get(index).map_or(defaults[index], Value::as_bool) {
                *enabled = false;
            }
        }
        if call.args.get(2).is_some_and(Value::as_bool) {
            world.player_control_pov_type = call.args.get(8).map_or(0, Value::as_int);
        }
        world.player_controls = world.player_control_flags.iter().all(|enabled| *enabled);
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
    for (name, axis) in [
        ("GetSunPositionX", 0),
        ("GetSunPositionY", 1),
        ("GetSunPositionZ", 2),
    ] {
        global!(vm, w, "Game", name, |world, _call| f(
            world.sun_position()[axis]
        ));
    }
    global!(vm, w, "Game", "IsPlayerSungazing", |world, _call| {
        b(world.player_is_sungazing())
    });
    global!(vm, w, "Game", "ShakeCamera", |world, call| {
        let player = world.player();
        let source = match call.args.first() {
            None | Some(Value::None) => Some(player),
            Some(Value::Object(object)) => world.form_of(*object),
            _ => None,
        };
        if let Some(source) = source {
            let strength = call.args.get(1).map_or(0.5, Value::as_float);
            let duration = call.args.get(2).map_or(0.0, Value::as_float);
            if duration == 0.0 {
                let setting = world
                    .game_setting("fCameraShakeDuration")
                    .map_or(0.25, |v| v.as_float());
                world.shake_camera(source, strength, setting);
            } else {
                world.shake_camera(source, strength, duration);
            }
        }
        none()
    });
    global!(vm, w, "Game", "IncrementStat", |world, call| {
        let amount = call.args.get(1).map_or(1, Value::as_int);
        world.increment_stat(&text(&call.arg(0)), amount);
        none()
    });
    global!(vm, w, "Game", "QueryStat", |world, call| i(world
        .stats
        .get(&text(&call.arg(0)))
        .copied()
        .unwrap_or(0)));
    global!(vm, w, "Game", "IsFastTravelEnabled", |world, _call| b(
        world.fast_travel
    ));
    global!(vm, w, "Game", "EnableFastTravel", |world, call| {
        world.fast_travel = call.args.first().is_none_or(Value::as_bool);
        none()
    });
    global!(vm, w, "Game", "SetPlayerAIDriven", |world, call| {
        world.player_ai_driven = call.args.first().is_none_or(Value::as_bool);
        none()
    });
    global!(vm, w, "Game", "SetInChargen", |world, call| {
        world.chargen_disable_saving = call.args.first().is_some_and(Value::as_bool);
        world.chargen_disable_waiting = call.args.get(1).is_some_and(Value::as_bool);
        world.chargen_show_controls_disabled_message = call.args.get(2).is_some_and(Value::as_bool);
        none()
    });
    global!(vm, w, "Game", "SetBeastForm", |world, call| {
        world.beast_form = call.args.first().is_none_or(Value::as_bool);
        none()
    });
    global!(vm, w, "Game", "TeachWord", |world, call| {
        if let Some(word) = world.form_arg(&call.arg(0)) {
            world.words_taught.insert(word);
        }
        none()
    });
    global!(vm, w, "Game", "UnlockWord", |world, call| {
        if let Some(word) = world.form_arg(&call.arg(0)) {
            world.words_taught.insert(word);
            world.words_unlocked.insert(word);
        }
        none()
    });
    global!(vm, w, "Game", "IsWordUnlocked", |world, call| b(world
        .form_arg(&call.arg(0))
        .is_some_and(|w| world.words_unlocked.contains(&w))));
    global!(vm, w, "Game", "AddAchievement", |world, call| {
        world.achievements.push(call.arg(0).as_int());
        none()
    });
    global!(vm, w, "Game", "GetPlayersLastRiddenHorse", |world, call| {
        let horse = world.last_ridden_horse;
        form_value(world, call, horse)
    });
    for (name, kind) in [
        ("SetGameSettingFloat", 0),
        ("SetGameSettingInt", 1),
        ("SetGameSettingBool", 2),
        ("SetGameSettingString", 3),
    ] {
        global!(vm, w, "Game", name, |world, call| {
            let v = match kind {
                0 => Value::Float(call.arg(1).as_float()),
                1 => Value::Int(call.arg(1).as_int()),
                2 => Value::Bool(call.arg(1).as_bool()),
                _ => Value::string(&text(&call.arg(1))),
            };
            world
                .settings
                .insert(text(&call.arg(0)).to_ascii_lowercase(), v);
            none()
        });
    }
}

fn debug(vm: &mut Vm, w: &Shared) {
    for name in ["Trace", "TraceConditional", "TraceStack"] {
        global!(vm, w, "Debug", name, |world, call| {
            let msg = match name {
                "TraceConditional" if !call.arg(1).as_bool() => return none(),
                _ => text_world(world, &call.arg(0)),
            };
            call.log.push(call.time, msg);
            none()
        });
    }
    global!(vm, w, "Debug", "TraceUser", |world, call| {
        let log_name = text(&call.arg(0));
        let message = text_world(world, &call.arg(1));
        if let Some(lines) = world.user_logs.get_mut(&log_name) {
            lines.push(message.clone());
            call.log.push(call.time, message);
        } else {
            call.log.push(
                call.time,
                format!("error: Debug.TraceUser log is not open: {log_name}"),
            );
        }
        b(world.user_logs.contains_key(&log_name))
    });
    global!(vm, w, "Debug", "OpenUserLog", |world, call| {
        let name = text(&call.arg(0));
        if name.is_empty() || world.user_logs.contains_key(&name) {
            return b(false);
        }
        world.user_logs.insert(name, Vec::new());
        b(true)
    });
    global!(vm, w, "Debug", "CloseUserLog", |world, call| {
        world.user_logs.remove(&text(&call.arg(0)));
        none()
    });
    global!(vm, w, "Debug", "QuitGame", |world, _call| {
        world.debug_host_requests.push(DebugHostRequest::Quit);
        none()
    });
    for (name, wait) in [("CenterOnCell", false), ("CenterOnCellAndWait", true)] {
        global!(vm, w, "Debug", name, |world, call| {
            let cell_name = text(&call.arg(0));
            let Some(cell) = world.cell_by_editor_id(&cell_name) else {
                call.log.push(
                    call.time,
                    format!("error: Debug.{name} cell not found: {cell_name}"),
                );
                return if wait { f(-1.0) } else { none() };
            };
            let latent = wait.then(|| call.defer());
            world
                .debug_host_requests
                .push(DebugHostRequest::CenterOnCell { cell, latent });
            if let Some(latent) = latent {
                NativeReturn::Latent(latent)
            } else {
                none()
            }
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
    global!(vm, w, "Debug", "TakeScreenshot", |world, call| {
        world.request_screenshot(text(&call.arg(0)));
        none()
    });
    global!(vm, w, "Debug", "SetGodMode", |world, call| {
        world.debug_god_mode = call.arg(0).as_bool();
        none()
    });
    global!(vm, w, "Debug", "ToggleAI", |world, _call| {
        world.debug_ai_enabled = !world.debug_ai_enabled;
        none()
    });
    global!(vm, w, "Debug", "ToggleCollisions", |world, _call| {
        world.debug_collisions_enabled = !world.debug_collisions_enabled;
        none()
    });
    global!(vm, w, "Debug", "DumpAliasData", |world, call| {
        let Some(quest) = world.form_arg(&call.arg(0)) else {
            call.log
                .push(call.time, "Debug.DumpAliasData: argument is not a quest");
            return none();
        };
        let Some(state) = world.quest_state(quest) else {
            call.log
                .push(call.time, "Debug.DumpAliasData: quest has no runtime state");
            return none();
        };
        let mut aliases: Vec<_> = state.aliases.iter().collect();
        aliases.sort_by_key(|(id, _)| **id);
        for (id, reference) in aliases {
            let fill = (*reference).map_or_else(|| "empty".to_string(), |form| form.to_string());
            call.log
                .push(call.time, format!("Debug.DumpAliasData: {id} {fill}"));
        }
        none()
    });
    global!(vm, w, "Debug", "DebugChannelNotify", |_world, call| {
        call.log.push(
            call.time,
            format!("[{}] {}", text(&call.arg(0)), text(&call.arg(1))),
        );
        none()
    });
    global!(vm, w, "Debug", "SendAnimationEvent", |world, call| {
        if let Some(target) = world.form_arg(&call.arg(0)) {
            world.animation_event(target, &text(&call.arg(1)), alloc!(call));
        }
        none()
    });
    global!(vm, w, "Debug", "StartScriptProfiling", |_world, call| {
        call.start_script_profiling(&text(&call.arg(0)));
        none()
    });
    global!(vm, w, "Debug", "StopScriptProfiling", |_world, call| {
        if let Err(error) = call.stop_script_profiling(&text(&call.arg(0))) {
            call.log
                .push(call.time, format!("Papyrus profile write failed: {error}"));
        }
        none()
    });
    global!(vm, w, "Debug", "StartStackProfiling", |_world, call| {
        call.start_stack_profiling();
        none()
    });
    global!(vm, w, "Debug", "StopStackProfiling", |_world, call| {
        if let Err(error) = call.stop_stack_profiling() {
            call.log
                .push(call.time, format!("Papyrus profile write failed: {error}"));
        }
        none()
    });
    global!(vm, w, "Debug", "SetFootIK", |world, call| {
        world.set_debug_foot_ik(call.arg(0).as_bool());
        none()
    });
    global!(vm, w, "Debug", "ShowRefPosition", |world, call| {
        match world.form_arg(&call.arg(0)) {
            Some(reference) if world.show_debug_ref_position(reference) => none(),
            _ => {
                call.log
                    .push(call.time, "Debug.ShowRefPosition: invalid reference");
                none()
            }
        }
    });
    global!(vm, w, "Debug", "PlayerMoveToAndWait", |world, call| {
        let target = world.form(&text(&call.arg(0)));
        let success = target.is_some_and(|target| world.debug_player_move_to_reference(target));
        if !success {
            call.log.push(
                call.time,
                "Debug.PlayerMoveToAndWait: destination reference not found",
            );
        }
        f(if success { 1.0 } else { 0.0 })
    });
    global!(vm, w, "Debug", "MoveToCell", |world, call| {
        if !world.debug_player_move_to_cell(&text(&call.arg(0))) {
            call.log
                .push(call.time, "Debug.MoveToCell: destination cell not found");
        }
        none()
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
    vm.natives.register_global(
        "Utility",
        "GetBudgetCount",
        Box::new(|_| i(ENGINE_BUDGETS.len() as i32)),
    );
    vm.natives.register_global(
        "Utility",
        "GetBudgetName",
        Box::new(|call| {
            let index = call.arg(0).as_int();
            ENGINE_BUDGETS
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .map_or_else(|| s(""), |budget| s(budget.0))
        }),
    );
    vm.natives.register_global(
        "Utility",
        "GetCurrentBudget",
        Box::new(|call| {
            let index = call.arg(0).as_int();
            ENGINE_BUDGETS
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .map_or_else(|| i(0), |budget| i(budget.1))
        }),
    );
    vm.natives.register_global(
        "Utility",
        "OverBudget",
        Box::new(|call| {
            let index = call.arg(0).as_int();
            b(ENGINE_BUDGETS
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .is_some_and(|budget| budget.2))
        }),
    );
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

const ENGINE_BUDGETS: [(&str, i32, bool); 2] = [
    (
        "Papyrus instructions per slice",
        crate::PAPYRUS_INSTRUCTIONS_PER_SLICE as i32,
        false,
    ),
    (
        "Exterior cell loads per update",
        crate::EXTERIOR_CELL_LOADS_PER_UPDATE as i32,
        false,
    ),
];

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
        world.add_form_list_entry(me, world.form_arg(&call.arg(0)));
        none()
    });
}

/// `LeveledItem`, `LeveledActor` and `LeveledSpell` share one shape. The
/// SKSE getters are included since scripts in the wild call them and the
/// state is there anyway.
fn leveled(vm: &mut Vm, w: &Shared) {
    for class in ["LeveledItem", "LeveledActor", "LeveledSpell"] {
        // AddForm(apForm, aiLevel, aiCount): a spell list has no count.
        method!(vm, w, class, "AddForm", |world, call, me| {
            let Some(form) = world.form_arg(&call.arg(0)) else {
                return none();
            };
            let level = u16::try_from(call.arg(1).as_int()).unwrap_or(0);
            let count = if class == "LeveledSpell" {
                1
            } else {
                u16::try_from(call.args.get(2).map_or(1, Value::as_int)).unwrap_or(1)
            };
            if let Some(l) = world.leveled_list(me) {
                l.entries.push(LeveledEntry { level, form, count });
            }
            none()
        });
        method!(vm, w, class, "Revert", |world, call, me| {
            if let Some(l) = world.leveled_list(me) {
                l.entries.truncate(l.base_entries);
                l.chance_none = l.base_chance_none;
            }
            none()
        });
        method!(vm, w, class, "GetNumForms", |world, call, me| i(world
            .leveled_list(me)
            .map_or(0, |l| l.entries.len() as i32)));
        method!(vm, w, class, "GetNthForm", |world, call, me| {
            let idx = usize::try_from(call.arg(0).as_int()).ok();
            let form = idx.and_then(|k| {
                world
                    .leveled_list(me)
                    .and_then(|l| l.entries.get(k).map(|e| e.form))
            });
            form_value(world, call, form)
        });
        method!(vm, w, class, "GetNthLevel", |world, call, me| {
            let idx = usize::try_from(call.arg(0).as_int()).ok();
            i(idx
                .and_then(|k| {
                    world
                        .leveled_list(me)
                        .and_then(|l| l.entries.get(k).map(|e| e.level))
                })
                .map_or(0, i32::from))
        });
        if class != "LeveledSpell" {
            method!(vm, w, class, "GetNthCount", |world, call, me| {
                let idx = usize::try_from(call.arg(0).as_int()).ok();
                i(idx
                    .and_then(|k| {
                        world
                            .leveled_list(me)
                            .and_then(|l| l.entries.get(k).map(|e| e.count))
                    })
                    .map_or(0, i32::from))
            });
        }
        method!(vm, w, class, "SetNthLevel", |world, call, me| {
            let idx = usize::try_from(call.arg(0).as_int()).ok();
            let raw_level = u32::try_from(call.arg(1).as_int()).unwrap_or(u32::MAX);
            let level = raw_level.min(u32::from(u16::MAX)) as u16;
            if let Some(entry) =
                idx.and_then(|index| world.leveled_list(me)?.entries.get_mut(index))
            {
                entry.level = level;
            }
            none()
        });
        method!(vm, w, class, "GetChanceNone", |world, call, me| i(world
            .leveled_list(me)
            .map_or(0, |l| i32::from(l.chance_none))));
        method!(vm, w, class, "SetChanceNone", |world, call, me| {
            let chance = u8::try_from(call.arg(0).as_int().rem_euclid(256)).unwrap_or(0);
            if let Some(l) = world.leveled_list(me) {
                l.chance_none = chance;
            }
            none()
        });
        if class != "LeveledSpell" {
            method!(vm, w, class, "SetNthCount", |world, call, me| {
                let idx = usize::try_from(call.arg(0).as_int()).ok();
                let raw_count = u32::try_from(call.arg(1).as_int()).unwrap_or(u32::MAX);
                let count = raw_count.min(u32::from(u16::MAX)) as u16;
                if let Some(entry) =
                    idx.and_then(|index| world.leveled_list(me)?.entries.get_mut(index))
                {
                    entry.count = count;
                }
                none()
            });
        }
    }
}

fn location(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "Location", "GetKeywordData", |world, call, me| {
        let kw = world.form_arg(&call.arg(0));
        f(kw.and_then(|k| {
            world
                .location(me)
                .and_then(|l| l.keyword_data.get(&k).copied())
        })
        .unwrap_or(0.0))
    });
    method!(vm, w, "Location", "SetKeywordData", |world, call, me| {
        let value = call.arg(1).as_float();
        if let Some(kw) = world.form_arg(&call.arg(0)) {
            if let Some(l) = world.location(me) {
                l.keyword_data.insert(kw, value);
            }
        }
        none()
    });
    method!(vm, w, "Location", "IsCleared", |world, call, me| b(world
        .location(me)
        .is_some_and(|l| l.cleared)));
    method!(vm, w, "Location", "SetCleared", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(l) = world.location(me) {
            l.cleared = on;
        }
        none()
    });
    method!(vm, w, "Location", "IsChild", |world, call, me| b(world
        .form_arg(&call.arg(0))
        .is_some_and(|other| world.location_is_child(me, other))));
    method!(vm, w, "Location", "HasCommonParent", |world, call, me| {
        let Some(other) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        let filter = world.form_arg(&call.arg(1));
        b(world.locations_share_parent(me, other, filter))
    });
    // IsSameLocation also accepts a keyworded parent/child pair, as the
    // shipped Location.psc wrapper specifies.
    method!(vm, w, "Location", "IsSameLocation", |world, call, me| {
        let Some(other) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        if other == me {
            return b(true);
        }
        let Some(filter) = world.form_arg(&call.arg(1)) else {
            return b(false);
        };
        let common = world.locations_share_parent(me, other, Some(filter));
        let other_matches_parent =
            world.keywords(other).contains(&filter) && world.location_is_child(other, me);
        let self_matches_parent =
            world.keywords(me).contains(&filter) && world.location_is_child(me, other);
        b(common || other_matches_parent || self_matches_parent)
    });
}

fn faction(vm: &mut Vm, w: &Shared) {
    // SetAlly(akOther, abSelfIsFriend = false, abOtherIsFriend = false)
    // and SetEnemy(akOther, abSelfIsNeutral = false, abOtherIsNeutral =
    // false) write both directions of the relation.
    for (name, on, off) in [
        ("SetAlly", reaction::FRIEND, reaction::ALLY),
        ("SetEnemy", reaction::NEUTRAL, reaction::ENEMY),
    ] {
        method!(vm, w, "Faction", name, |world, call, me| {
            let Some(other) = world.form_arg(&call.arg(0)) else {
                return none();
            };
            let mine = if call.arg(1).as_bool() { on } else { off };
            let theirs = if call.arg(2).as_bool() { on } else { off };
            if let Some(f) = world.faction(me) {
                f.reactions.insert(other, mine);
            }
            if let Some(f) = world.faction(other) {
                f.reactions.insert(me, theirs);
            }
            none()
        });
    }
    method!(vm, w, "Faction", "GetReaction", |world, call, me| {
        let Some(other) = world.form_arg(&call.arg(0)) else {
            return i(reaction::NEUTRAL);
        };
        i(world
            .faction(me)
            .and_then(|f| f.reactions.get(&other).copied())
            .unwrap_or(reaction::NEUTRAL))
    });
    method!(vm, w, "Faction", "SetReaction", |world, call, me| {
        let value = call.arg(1).as_int();
        if let Some(other) = world.form_arg(&call.arg(0)) {
            if let Some(f) = world.faction(me) {
                f.reactions.insert(other, value);
            }
        }
        none()
    });
    method!(vm, w, "Faction", "ModReaction", |world, call, me| {
        let amount = call.arg(1).as_int();
        if let Some(other) = world.form_arg(&call.arg(0)) {
            if let Some(f) = world.faction(me) {
                let reaction = f
                    .reactions
                    .get(&other)
                    .copied()
                    .unwrap_or(reaction::NEUTRAL);
                f.reactions.insert(other, reaction.saturating_add(amount));
            }
        }
        none()
    });
    method!(vm, w, "Faction", "GetCrimeGold", |world, call, me| i(world
        .faction(me)
        .map_or(0, |f| f.crime_gold_violent + f.crime_gold_nonviolent)));
    method!(
        vm,
        w,
        "Faction",
        "GetCrimeGoldViolent",
        |world, call, me| i(world.faction(me).map_or(0, |f| f.crime_gold_violent))
    );
    method!(
        vm,
        w,
        "Faction",
        "GetCrimeGoldNonViolent",
        |world, call, me| i(world.faction(me).map_or(0, |f| f.crime_gold_nonviolent))
    );
    method!(vm, w, "Faction", "SetCrimeGold", |world, call, me| {
        let v = call.arg(0).as_int().max(0);
        if let Some(f) = world.faction(me) {
            f.crime_gold_nonviolent = v;
        }
        none()
    });
    method!(
        vm,
        w,
        "Faction",
        "SetCrimeGoldViolent",
        |world, call, me| {
            let v = call.arg(0).as_int().max(0);
            if let Some(f) = world.faction(me) {
                f.crime_gold_violent = v;
            }
            none()
        }
    );
    // ModCrimeGold(aiAmount, abViolent = false) also grows the infamy of
    // the same kind, the bounty that was ever earned.
    method!(vm, w, "Faction", "ModCrimeGold", |world, call, me| {
        let amount = call.arg(0).as_int();
        let violent = call.arg(1).as_bool();
        if let Some(f) = world.faction(me) {
            if violent {
                f.crime_gold_violent = (f.crime_gold_violent + amount).max(0);
                f.infamy_violent += amount.max(0);
            } else {
                f.crime_gold_nonviolent = (f.crime_gold_nonviolent + amount).max(0);
                f.infamy_nonviolent += amount.max(0);
            }
        }
        none()
    });
    method!(vm, w, "Faction", "GetInfamy", |world, call, me| i(world
        .faction(me)
        .map_or(0, |f| f.infamy_violent + f.infamy_nonviolent)));
    method!(vm, w, "Faction", "GetInfamyViolent", |world, call, me| i(
        world.faction(me).map_or(0, |f| f.infamy_violent)
    ));
    method!(
        vm,
        w,
        "Faction",
        "GetInfamyNonViolent",
        |world, call, me| i(world.faction(me).map_or(0, |f| f.infamy_nonviolent))
    );
    method!(vm, w, "Faction", "CanPayCrimeGold", |world, call, me| {
        let owed = world
            .faction(me)
            .map_or(0, |f| f.crime_gold_violent + f.crime_gold_nonviolent);
        let player = world.player();
        let gold = world
            .form("Gold001")
            .map_or(0, |g| world.item_count(player, g));
        b(gold >= owed)
    });
    // PlayerPayCrimeGold(abRemoveStolenItems = true, abGoToJail = true):
    // the bounty leaves the player's purse and the faction's ledger.
    method!(vm, w, "Faction", "PlayerPayCrimeGold", |world, call, me| {
        let owed = world
            .faction(me)
            .map_or(0, |f| f.crime_gold_violent + f.crime_gold_nonviolent);
        let player = world.player();
        if let Some(g) = world.form("Gold001") {
            if owed > 0 {
                world.remove_item(player, g, owed, None);
            }
        }
        if let Some(f) = world.faction(me) {
            f.crime_gold_violent = 0;
            f.crime_gold_nonviolent = 0;
        }
        none()
    });
    method!(vm, w, "Faction", "IsPlayerExpelled", |world, call, me| b(
        world.faction(me).is_some_and(|f| f.player_expelled)
    ));
    method!(vm, w, "Faction", "SetPlayerExpelled", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(f) = world.faction(me) {
            f.player_expelled = on;
        }
        none()
    });
    method!(vm, w, "Faction", "SetPlayerEnemy", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(f) = world.faction(me) {
            f.player_enemy = on;
        }
        none()
    });
    method!(
        vm,
        w,
        "Faction",
        "IsFactionInCrimeGroup",
        |world, call, me| {
            let Some(other) = world.form_arg(&call.arg(0)) else {
                return b(false);
            };
            b(world.faction_in_crime_group(me, other))
        }
    );
}

fn actor_base(vm: &mut Vm, w: &Shared) {
    for class in ["ActorBase", "Race"] {
        method!(vm, w, class, "GetSkin", |world, call, me| {
            match world.skin(me) {
                Ok(skin) => form_value(world, call, skin),
                Err(error) => {
                    call.log
                        .push(call.time, format!("{class}.GetSkin: {error}"));
                    none()
                }
            }
        });
        method!(vm, w, class, "SetSkin", |world, call, me| {
            let skin = call.args.first().and_then(|value| world.form_arg(value));
            let result = if class == "ActorBase" {
                world.set_actor_base_skin(me, skin)
            } else {
                world.set_race_skin(me, skin)
            };
            if let Err(error) = result {
                call.log
                    .push(call.time, format!("{class}.SetSkin: {error}"));
            }
            none()
        });
    }
    method!(vm, w, "ActorBase", "GetRace", |world, call, me| {
        let race = world.actor_base(me).and_then(|a| a.race);
        form_value(world, call, race)
    });
    method!(vm, w, "ActorBase", "GetClass", |world, call, me| {
        let class = world.actor_base(me).and_then(|a| a.class);
        form_value(world, call, class)
    });
    method!(vm, w, "ActorBase", "GetGiftFilter", |world, call, me| {
        match world.actor_base_gift_filter(me) {
            Ok(filter) => form_value(world, call, filter),
            Err(error) => {
                call.log
                    .push(call.time, format!("ActorBase.GetGiftFilter: {error}"));
                none()
            }
        }
    });
    method!(vm, w, "ActorBase", "GetSex", |world, call, me| i(world
        .actor_base(me)
        .map_or(-1, |a| i32::from(a.female))));
    method!(vm, w, "ActorBase", "GetDeadCount", |world, call, me| i(
        world.actor_base(me).map_or(0, |a| a.dead_count)
    ));
    for (name, field) in [
        ("IsEssential", 0),
        ("IsProtected", 1),
        ("IsInvulnerable", 2),
        ("IsUnique", 3),
    ] {
        method!(vm, w, "ActorBase", name, |world, call, me| b(world
            .actor_base(me)
            .is_some_and(|a| match field {
                0 => a.essential,
                1 => a.protected,
                2 => a.invulnerable,
                _ => a.unique,
            })));
    }
    for (name, field) in [
        ("SetEssential", 0),
        ("SetProtected", 1),
        ("SetInvulnerable", 2),
    ] {
        method!(vm, w, "ActorBase", name, |world, call, me| {
            let on = call.args.first().is_none_or(Value::as_bool);
            match field {
                0 => world.set_actor_base_essential(me, on),
                1 => world.set_actor_base_protected(me, on),
                _ => {
                    if let Some(a) = world.actor_base(me) {
                        a.invulnerable = on;
                    }
                }
            }
            none()
        });
    }
    method!(vm, w, "ActorBase", "SetOutfit", |world, call, me| {
        let outfit = world.form_arg(&call.arg(0));
        let sleep = call.arg(1).as_bool();
        if let Some(a) = world.actor_base(me) {
            if sleep {
                a.sleep_outfit = outfit;
            } else {
                a.outfit = outfit;
            }
        }
        none()
    });
}

fn cell(vm: &mut Vm, w: &Shared) {
    for (name, tag) in [("GetActorOwner", *b"NPC_"), ("GetFactionOwner", *b"FACT")] {
        method!(vm, w, "Cell", name, |world, call, me| {
            let owner = world
                .cell_state(me)
                .and_then(|c| c.owner)
                .filter(|o| world.tag_of(*o).is_some_and(|t| *t.as_bytes() == tag));
            form_value(world, call, owner)
        });
    }
    for (name, expected) in [
        ("SetActorOwner", Tag::new(b"NPC_")),
        ("SetFactionOwner", Tag::new(b"FACT")),
    ] {
        method!(vm, w, "Cell", name, |world, call, me| {
            let arg = call.arg(0);
            let owner = if matches!(arg, Value::None) {
                None
            } else if let Some(owner) = world
                .form_arg(&arg)
                .filter(|owner| cell_owner_tag_matches(world.tag_of(*owner), expected))
            {
                Some(owner)
            } else {
                return none();
            };
            if let Some(c) = world.cell_state(me) {
                c.owner = owner;
            }
            none()
        });
    }
    method!(vm, w, "Cell", "SetPublic", |world, call, me| {
        let on = call.args.first().is_none_or(Value::as_bool);
        if let Some(c) = world.cell_state(me) {
            c.public = on;
        }
        none()
    });
    method!(vm, w, "Cell", "IsAttached", |world, _call, me| {
        b(world.cell_is_attached(me))
    });
    method!(vm, w, "Cell", "Reset", |world, _call, me| {
        let _ = world.reset_cell(me);
        none()
    });
}

fn cell_owner_tag_matches(actual: Option<soe_esm::Tag>, expected: soe_esm::Tag) -> bool {
    actual == Some(expected)
}

fn message_scene_sound_spell(vm: &mut Vm, w: &Shared) {
    // Show queues a message box or notification. Without a selected button,
    // notifications report -1 while queued message boxes report button 0.
    method!(vm, w, "Message", "Show", |world, call, me| {
        let body = world
            .description_of(me)
            .or_else(|| world.name_of(me))
            .unwrap_or_default();
        let boxed = world
            .record(me)
            .and_then(|r| r.find(soe_esm::Tag::new(b"DNAM")))
            .and_then(|d| d.first().copied())
            .is_some_and(|f| f & 1 != 0);
        if boxed {
            world.message_boxes.push(body);
            i(0)
        } else {
            world.notifications.push(body);
            i(-1)
        }
    });
    method!(vm, w, "Message", "ShowAsHelpMessage", |world, call, me| {
        let body = world
            .description_of(me)
            .or_else(|| world.name_of(me))
            .unwrap_or_default();
        world.notifications.push(body);
        none()
    });
    for name in ["Start", "ForceStart"] {
        method!(vm, w, "Scene", name, |world, call, me| {
            world.start_scene(me, alloc!(call));
            none()
        });
    }
    method!(vm, w, "Scene", "Stop", |world, call, me| {
        world.stop_scene(me, alloc!(call));
        none()
    });
    method!(vm, w, "Scene", "IsPlaying", |world, call, me| b(world
        .scenes
        .contains(&me)));
    method!(vm, w, "Sound", "Play", |world, call, me| {
        let Some(source) = world.form_arg(&call.arg(0)) else {
            return i(0);
        };
        i(world.play_sound(me, source))
    });
    global!(vm, w, "Sound", "StopInstance", |world, call| {
        world.sound_instances.remove(&call.arg(0).as_int());
        none()
    });
    global!(vm, w, "Sound", "SetInstanceVolume", |world, call| {
        let volume = call.arg(1).as_float().clamp(0.0, 1.0);
        if let Some(inst) = world.sound_instances.get_mut(&call.arg(0).as_int()) {
            inst.2 = volume;
        }
        none()
    });
    method!(vm, w, "Spell", "Cast", |world, call, me| {
        let Some(source) = world.form_arg(&call.arg(0)) else {
            return none();
        };
        let target = world.form_arg(&call.arg(1));
        world.spell_casts.push((me, source, target));
        if let Err(error) = world.cast_spell(me, source, Some(source), target, alloc!(call)) {
            call.log.push(call.time, format!("Spell.Cast: {error}"));
        }
        none()
    });
    method!(vm, w, "Spell", "RemoteCast", |world, call, me| {
        let Some(source) = world.form_arg(&call.arg(0)) else {
            return none();
        };
        let target = world.form_arg(&call.arg(2));
        world.spell_casts.push((me, source, target));
        let caster = world.form_arg(&call.arg(1));
        if let Err(error) = world.cast_spell(me, source, caster, target, alloc!(call)) {
            call.log
                .push(call.time, format!("Spell.RemoteCast: {error}"));
        }
        none()
    });
}

/// Shipped ImageSpaceModifier methods keep ordinary and crossfade lifetimes apart.
fn image_space(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "ImageSpaceModifier", "Apply", |world, call, me| {
        let strength = call.args.first().map_or(1.0, Value::as_float);
        if let Err(error) = world.apply_image_space(me, strength) {
            call.log.push(call.time, error.to_string());
        }
        none()
    });
    method!(vm, w, "ImageSpaceModifier", "Remove", |world, _call, me| {
        world.remove_image_space(me);
        none()
    });
    method!(vm, w, "ImageSpaceModifier", "PopTo", |world, call, me| {
        let Some(next) = world.form_arg(&call.arg(0)) else {
            return none();
        };
        let strength = call.args.get(1).map_or(1.0, Value::as_float);
        if let Err(error) = world.pop_image_space(me, next, strength) {
            call.log.push(call.time, error.to_string());
        }
        none()
    });
    method!(
        vm,
        w,
        "ImageSpaceModifier",
        "ApplyCrossFade",
        |world, call, me| {
            let seconds = call.args.first().map_or(1.0, Value::as_float);
            if let Err(error) = world.crossfade_image_space(Some(me), seconds) {
                call.log.push(call.time, error.to_string());
            }
            none()
        }
    );
    global!(
        vm,
        w,
        "ImageSpaceModifier",
        "RemoveCrossFade",
        |world, call| {
            let seconds = call.args.first().map_or(1.0, Value::as_float);
            if let Err(error) = world.crossfade_image_space(None, seconds) {
                call.log.push(call.time, error.to_string());
            }
            none()
        }
    );
}

/// Shipped EffectShader calls retain target and duration in the shared world.
fn effect_shader(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "EffectShader", "Play", |world, call, me| {
        let Some(target) = world.form_arg(&call.arg(0)) else {
            call.log.push(
                call.time,
                "EffectShader.Play: target must be a placed reference",
            );
            return none();
        };
        let duration = match call.args.get(1) {
            None | Some(Value::None) => -1.0,
            Some(Value::Float(value)) => *value,
            Some(Value::Int(value)) => *value as f32,
            Some(_) => {
                call.log
                    .push(call.time, "EffectShader.Play: duration must be numeric");
                return none();
            }
        };
        if let Err(error) = world.play_effect_shader(me, target, duration) {
            call.log
                .push(call.time, format!("EffectShader.Play: {error}"));
        }
        none()
    });
    method!(vm, w, "EffectShader", "Stop", |world, call, me| {
        let Some(target) = world.form_arg(&call.arg(0)) else {
            call.log.push(
                call.time,
                "EffectShader.Stop: target must be a placed reference",
            );
            return none();
        };
        if let Err(error) = world.stop_effect_shader(me, target) {
            call.log
                .push(call.time, format!("EffectShader.Stop: {error}"));
        }
        none()
    });
}

fn weapon(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "Weapon", "Fire", |world, call, me| {
        let Some(source) = world.form_arg(&call.arg(0)) else {
            call.log.push(
                call.time,
                crate::combat::FireError::SourceArgument.to_string(),
            );
            return none();
        };
        let ammo_arg = call.arg(1);
        let ammo = if matches!(ammo_arg, Value::None) {
            None
        } else if let Some(ammo) = world.form_arg(&ammo_arg) {
            Some(ammo)
        } else {
            call.log.push(
                call.time,
                crate::combat::FireError::AmmoArgument.to_string(),
            );
            return none();
        };
        if let Err(error) = world.fire_weapon(me, source, ammo) {
            call.log.push(call.time, format!("Weapon.Fire: {error}"));
        }
        none()
    });
}

/// Weather requests are retained by the world for the host sky implementation.
fn weather(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "Weather", "SetActive", |world, call, me| {
        let override_weather = call.args.first().is_some_and(Value::as_bool);
        let accelerate = call.args.get(1).is_some_and(Value::as_bool);
        if let Err(error) = world.set_active_weather(me, override_weather, accelerate) {
            call.log
                .push(call.time, format!("Weather.SetActive: {error}"));
        }
        none()
    });
}

/// CK active-effect identity, queries and dispelling contracts.
fn active_magic(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "Actor", "HasMagicEffect", |world, call, me| {
        b(world
            .form_arg(&call.arg(0))
            .is_some_and(|effect| world.has_magic_effect(me, effect)))
    });
    method!(
        vm,
        w,
        "Actor",
        "HasMagicEffectWithKeyword",
        |world, call, me| {
            b(world
                .form_arg(&call.arg(0))
                .is_some_and(|keyword| world.has_magic_effect_with_keyword(me, keyword)))
        }
    );
    method!(vm, w, "Actor", "DispelSpell", |world, call, me| {
        b(world
            .form_arg(&call.arg(0))
            .is_some_and(|spell| world.dispel_spell(me, spell, alloc!(call))))
    });
    method!(vm, w, "Actor", "DispelAllSpells", |world, call, me| {
        world.dispel_all_spells(me, alloc!(call));
        none()
    });
    method!(vm, w, "Actor", "UnequipAll", |world, call, me| {
        let items: Vec<_> = world
            .reference(me)
            .map(|reference| reference.inventory.iter().map(|(item, _)| *item).collect())
            .unwrap_or_default();
        for item in items {
            if let Err(error) = world.unequip_item(me, item) {
                call.log
                    .push(call.time, format!("Actor.UnequipAll: {error}"));
                return none();
            }
        }
        if let Some(actor) = world.actor(me) {
            actor.equipped.clear();
            actor.left_weapon = None;
            actor.right_weapon = None;
            actor.left_spell = None;
            actor.right_spell = None;
            actor.equipped_shout = None;
        }
        none()
    });
    method!(vm, w, "Actor", "RemoveAllSpells", |world, _call, me| {
        if let Some(actor) = world.actor(me) {
            let spells = actor.spells.clone();
            actor.spells.clear();
            actor.left_spell = None;
            actor.right_spell = None;
            actor.equipped.retain(|item| !spells.contains(item));
        }
        none()
    });
    method!(vm, w, "Actor", "StartVampireFeed", |world, call, me| {
        if let Some(target) = world.form_arg(&call.arg(0)) {
            if world.actor(target).is_some_and(|actor| actor.ai.sleeping) {
                let target = world.form_value(Some(target), alloc!(call));
                world.form_event(me, "OnVampireFeed", vec![target], alloc!(call));
            }
        }
        none()
    });
    self_method!(
        vm,
        w,
        "ActiveMagicEffect",
        "GetBaseObject",
        |world, call, me| {
            let id = world.active_effect_of(me).map(|effect| effect.effect);
            form_value(world, call, id)
        }
    );
    self_method!(
        vm,
        w,
        "ActiveMagicEffect",
        "GetTargetActor",
        |world, call, me| {
            let id = world.active_effect_of(me).map(|effect| effect.target);
            form_value(world, call, id)
        }
    );
    self_method!(
        vm,
        w,
        "ActiveMagicEffect",
        "GetCasterActor",
        |world, call, me| {
            let id = world.active_effect_of(me).and_then(|effect| effect.caster);
            form_value(world, call, id)
        }
    );
    self_method!(vm, w, "ActiveMagicEffect", "Dispel", |world, call, me| {
        if let Some(id) = world.active_effect_of(me).map(|effect| effect.id) {
            world.dispel_effect(id, alloc!(call));
        }
        none()
    });
    self_method!(
        vm,
        w,
        "ActiveMagicEffect",
        "AddInventoryEventFilter",
        |world, call, me| {
            if let Some(filter) = world.form_arg(&call.arg(0)) {
                world.inventory_filters.entry(me).or_default().push(filter);
            }
            none()
        }
    );
    self_method!(
        vm,
        w,
        "ActiveMagicEffect",
        "RemoveInventoryEventFilter",
        |world, call, me| {
            let filter = world.form_arg(&call.arg(0));
            if let Some(list) = world.inventory_filters.get_mut(&me) {
                list.retain(|entry| Some(*entry) != filter);
            }
            none()
        }
    );
    self_method!(
        vm,
        w,
        "ActiveMagicEffect",
        "RemoveAllInventoryEventFilters",
        |world, _call, me| {
            world.inventory_filters.remove(&me);
            none()
        }
    );
}

/// Papyrus VisualEffect.Play and Stop, routed through transient World state.
fn visual_effect(vm: &mut Vm, w: &Shared) {
    method!(vm, w, "VisualEffect", "Play", |world, call, me| {
        let Some(target) = call.args.first().and_then(|value| world.form_arg(value)) else {
            call.log
                .push(call.time, "VisualEffect.Play: missing target reference");
            return none();
        };
        let duration = call.args.get(1).map_or(-1.0, Value::as_float);
        let facing = match call.args.get(2) {
            None | Some(Value::None) => None,
            Some(value) => match world.form_arg(value) {
                Some(form) => Some(form),
                None => {
                    call.log
                        .push(call.time, "VisualEffect.Play: invalid facing reference");
                    return none();
                }
            },
        };
        if let Err(error) = world.play_visual_effect(me, target, f64::from(duration), facing) {
            call.log
                .push(call.time, format!("VisualEffect.Play: {error}"));
        }
        none()
    });
    method!(vm, w, "VisualEffect", "Stop", |world, call, me| {
        let Some(target) = call.args.first().and_then(|value| world.form_arg(value)) else {
            call.log
                .push(call.time, "VisualEffect.Stop: missing target reference");
            return none();
        };
        if let Err(error) = world.stop_visual_effect(me, target) {
            call.log
                .push(call.time, format!("VisualEffect.Stop: {error}"));
        }
        none()
    });
}

/// What Dawnguard, HearthFires and Dragonborn added to the base classes.
/// Ordered by how often the DLC scripts call each one.
fn dlc(vm: &mut Vm, w: &Shared) {
    // "Set a variable on all of an actor's sub graphs." The behavior graph
    // reads these, the engine keeps them per actor under a lowercase name.
    method!(
        vm,
        w,
        "Actor",
        "SetSubGraphFloatVariable",
        |world, call, me| {
            let name = text(&call.arg(0)).to_ascii_lowercase();
            let value = call.arg(1).as_float();
            if let Some(a) = world.actor(me) {
                a.graph_floats.insert(name, value);
            }
            none()
        }
    );
    // "Sends the event passed to the actor's sub graphs", which is the same
    // delivery an animation event takes, so listeners hear it as one.
    method!(
        vm,
        w,
        "Actor",
        "PlaySubGraphAnimation",
        |world, call, me| {
            let name = text(&call.arg(0));
            world.animation_event(me, &name, alloc!(call));
            none()
        }
    );
    // "Sets this actor's current critical stage (applying any related
    // effects)." The effects are the death animations, which do not exist.
    method!(vm, w, "Actor", "SetCriticalStage", |world, call, me| {
        let stage = call.arg(0).as_int();
        if let Some(a) = world.actor(me) {
            a.critical_stage = stage;
        }
        none()
    });
    // "Sets the actor's race", read back by the existing GetRace.
    method!(vm, w, "Actor", "SetRace", |world, call, me| {
        let race = world.form_arg(&call.arg(0));
        if let Some(a) = world.actor(me) {
            a.race = race;
        }
        none()
    });
    // "Forces this actor to equip thes spell, in the specified source."
    // Sources 0 and 1 are the two casting hands (ck-wiki EquipSpell -
    // Actor), routed through the same `World::equip_hand_spell` the native
    // magic menu uses (`crates/godot-host/src/magic_menu.rs`), so a script
    // equip and a menu equip are one state. Sources 2 (other) and 3
    // (instant) have no per-hand slot here.
    //
    // `Actor.equipped` still gets the generic insert or remove on every
    // source, matching the pre-hand-routing contract other natives
    // (`IsEquipped`, dispel immunity) already read. A source of 0 or 1 also
    // tries the per-hand slot, best effort: a spell the hand rejects (wrong
    // type, not known, no cost data) still counts as equipped for those
    // generic readers, it just never shows up in a hand.
    method!(vm, w, "Actor", "EquipSpell", |world, call, me| {
        if let Some(spell) = world.form_arg(&call.arg(0)) {
            let hand = match call.arg(1).as_int() {
                0 => Some(Hand::Left),
                1 => Some(Hand::Right),
                _ => None,
            };
            if let Some(hand) = hand {
                if let Err(error) = world.equip_hand_spell(me, hand, spell) {
                    call.log
                        .push(call.time, format!("Actor.EquipSpell: {error}"));
                }
            }
            if let Some(a) = world.actor(me) {
                a.equipped.insert(spell);
            }
        }
        none()
    });
    method!(vm, w, "Actor", "UnequipSpell", |world, call, me| {
        if let Some(spell) = world.form_arg(&call.arg(0)) {
            match call.arg(1).as_int() {
                0 => {
                    let _ = world.unequip_hand_spell(me, Hand::Left, spell);
                }
                1 => {
                    let _ = world.unequip_hand_spell(me, Hand::Right, spell);
                }
                _ => {}
            }
            if let Some(a) = world.actor(me) {
                a.equipped.remove(&spell);
            }
        }
        none()
    });
    // "Removes the specified shout from this actor", true when it had it.
    method!(vm, w, "Actor", "RemoveShout", |world, call, me| {
        let Some(shout) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        let Some(a) = world.actor(me) else {
            return b(false);
        };
        if a.equipped_shout == Some(shout) {
            a.equipped_shout = None;
        }
        b(a.shouts.remove(&shout))
    });
    // "Adds the specified shout to this actor", true when it is new.
    method!(vm, w, "Actor", "AddShout", |world, call, me| {
        let Some(shout) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        b(world.actor(me).is_some_and(|a| a.shouts.insert(shout)))
    });
    // "Forces this actor to equip this shout."
    method!(vm, w, "Actor", "EquipShout", |world, call, me| {
        let Some(shout) = world.form_arg(&call.arg(0)) else {
            return none();
        };
        if let Some(a) = world.actor(me) {
            a.shouts.insert(shout);
            a.equipped_shout = Some(shout);
        }
        none()
    });
    method!(vm, w, "Actor", "UnequipShout", |world, call, me| {
        let shout = world.form_arg(&call.arg(0));
        if let Some(a) = world.actor(me) {
            if a.equipped_shout == shout {
                a.equipped_shout = None;
            }
        }
        none()
    });
    // "Obtains the currently equipped shout on this actor."
    method!(vm, w, "Actor", "GetEquippedShout", |world, call, me| {
        let shout = world.actor(me).and_then(|a| a.equipped_shout);
        form_value(world, call, shout)
    });
    method!(
        vm,
        w,
        "Actor",
        "GetVoiceRecoveryTime",
        |world, _call, me| f(world.voice_recovery_time(me))
    );
    method!(vm, w, "Actor", "SetVoiceRecoveryTime", |world, call, me| {
        world.set_voice_recovery_time(me, call.arg(0).as_float());
        none()
    });
    // "Flags this actor as whether they are allowed to fly or not (lands
    // them if they are flying and not allowed to)." There is no flight to
    // land, so the flag is the whole of it. The Ex form adds the crash and
    // search permissions, which need the same flight the flag waits on.
    for name in ["SetAllowFlying", "SetAllowFlyingEx"] {
        method!(vm, w, "Actor", name, |world, call, me| {
            let allowed = call.arg(0).as_bool();
            if let Some(a) = world.actor(me) {
                a.allow_flying = allowed;
            }
            none()
        });
    }
    method!(vm, w, "Actor", "IsAllowedToFly", |world, call, me| b(world
        .actor(me)
        .is_some_and(|a| a.allow_flying)));
    // The Hover package owns the only flight state implemented by the driver.
    method!(vm, w, "Actor", "GetFlyingState", |world, _call, me| i(
        i32::from(
            world
                .actor(me)
                .is_some_and(|actor| actor.ai.procedure == crate::ai::Procedure::Hover)
        ) * 3
    ));
    method!(vm, w, "Actor", "IsFlying", |world, _call, me| b(world
        .actor(me)
        .is_some_and(
            |actor| actor.ai.procedure == crate::ai::Procedure::Hover
        )));
    // "Can this actor fly here?" Allowed to fly, and out under the sky.
    method!(vm, w, "Actor", "CanFlyHere", |world, call, me| {
        if !world.actor(me).is_some_and(|a| a.allow_flying) {
            return b(false);
        }
        let cell = world.reference(me).and_then(|r| r.cell);
        b(cell.is_some_and(|c| !world.cell_is_interior(c)))
    });
    // "Enables or disables this actor's AI." Stored inverted so an actor
    // nobody touched still thinks.
    method!(vm, w, "Actor", "EnableAI", |world, call, me| {
        let enable = call.arg(0).as_bool();
        if let Some(a) = world.actor(me) {
            a.ai_disabled = !enable;
        }
        none()
    });
    // "Attaches an ash pile to this actor using the passed-in form (or
    // leveled item list) as a base", the Dragonborn disintegration.
    method!(vm, w, "Actor", "AttachAshPile", |world, call, me| {
        let base = world.form_arg(&call.arg(0));
        if let Some(a) = world.actor(me) {
            a.ash_pile = base;
        }
        none()
    });
    // "Sets or clears the no bleedout recovery flag on this actor."
    method!(
        vm,
        w,
        "Actor",
        "SetNoBleedoutRecovery",
        |world, call, me| {
            let flag = call.arg(0).as_bool();
            if let Some(a) = world.actor(me) {
                a.no_bleedout_recovery = flag;
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "Actor",
        "GetNoBleedoutRecovery",
        |world, call, me| b(world.actor(me).is_some_and(|a| a.no_bleedout_recovery))
    );
    // "Removes the actor from a Deferred Kill state." While the state is
    // on, a kill is held back, which is what the pair is for.
    method!(vm, w, "Actor", "StartDeferredKill", |world, call, me| {
        world.start_deferred_kill(me);
        none()
    });
    method!(vm, w, "Actor", "EndDeferredKill", |world, call, me| {
        world.end_deferred_kill(me);
        none()
    });
    method!(vm, w, "Actor", "KillSilent", |world, call, me| {
        let killer = world.form_arg(&call.arg(0));
        world.kill_actor_silent(me, killer);
        none()
    });
    // "Tells anyone who cares that the vampirism state of this actor has
    // changed", which is OnVampirismStateChanged to the actor's scripts.
    method!(
        vm,
        w,
        "Actor",
        "SendVampirismStateChanged",
        |world, call, me| {
            let is_vampire = call.arg(0).as_bool();
            if let Some(a) = world.actor(me) {
                a.vampire = is_vampire;
            }
            world.form_event(
                me,
                "OnVampirismStateChanged",
                vec![Value::Bool(is_vampire)],
                alloc!(call),
            );
            none()
        }
    );
    // "Tells anyone who cares that the lycanthropy state of this actor has
    // changed."
    method!(
        vm,
        w,
        "Actor",
        "SendLycanthropyStateChanged",
        |world, call, me| {
            let is_werewolf = call.arg(0).as_bool();
            if let Some(a) = world.actor(me) {
                a.werewolf = is_werewolf;
            }
            world.form_event(
                me,
                "OnLycanthropyStateChanged",
                vec![Value::Bool(is_werewolf)],
                alloc!(call),
            );
            none()
        }
    );
    // "Forces a new eye texture on the actor", the Dawnguard vampire eyes.
    method!(vm, w, "Actor", "SetEyeTexture", |world, call, me| {
        let texture = world.form_arg(&call.arg(0));
        if let Some(a) = world.actor(me) {
            a.eye_texture = texture;
        }
        none()
    });
    // Whether any equipped item carries the keyword.
    method!(vm, w, "Actor", "WornHasKeyword", |world, call, me| {
        let Some(keyword) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        let worn: Vec<GlobalFormId> = world
            .actor(me)
            .map(|a| a.equipped.iter().copied().collect())
            .unwrap_or_default();
        b(worn.into_iter().any(|i| world.has_keyword(i, keyword)))
    });
    method!(
        vm,
        w,
        "Actor",
        "IsPlayersLastRiddenHorse",
        |world, call, me| b(world.last_ridden_horse == Some(me))
    );
    // "Adds the given number of perk points to the player. Clamped at 255."
    global!(vm, w, "Game", "AddPerkPoints", |world, call| {
        let added = call.arg(0).as_int();
        world.perk_points = world.perk_points.saturating_add(added).clamp(0, 255);
        none()
    });

    // "Sets the value of a variable on the reference's animation graph."
    // The graph holds one value per name, so the three typed pairs read and
    // write one store each and a name belongs to the type that set it.
    method!(
        vm,
        w,
        "ObjectReference",
        "SetAnimationVariableFloat",
        |world, call, me| {
            let name = text(&call.arg(0)).to_ascii_lowercase();
            let value = call.arg(1).as_float();
            if let Some(r) = world.reference(me) {
                r.anim_floats.insert(name, value);
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "SetAnimationVariableInt",
        |world, call, me| {
            let name = text(&call.arg(0)).to_ascii_lowercase();
            let value = call.arg(1).as_int();
            if let Some(r) = world.reference(me) {
                r.anim_ints.insert(name, value);
            }
            none()
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "SetAnimationVariableBool",
        |world, call, me| {
            let name = text(&call.arg(0)).to_ascii_lowercase();
            let value = call.arg(1).as_bool();
            if let Some(r) = world.reference(me) {
                r.anim_bools.insert(name, value);
            }
            none()
        }
    );
    // "Fetches the value of a variable on the reference's animation graph."
    // A name the graph never saw reads as zero, the quiet value.
    method!(
        vm,
        w,
        "ObjectReference",
        "GetAnimationVariableFloat",
        |world, call, me| {
            let name = text(&call.arg(0)).to_ascii_lowercase();
            f(world
                .reference(me)
                .and_then(|r| r.anim_floats.get(&name).copied())
                .unwrap_or(0.0))
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetAnimationVariableInt",
        |world, call, me| {
            let name = text(&call.arg(0)).to_ascii_lowercase();
            i(world
                .reference(me)
                .and_then(|r| r.anim_ints.get(&name).copied())
                .unwrap_or(0))
        }
    );
    method!(
        vm,
        w,
        "ObjectReference",
        "GetAnimationVariableBool",
        |world, call, me| {
            let name = text(&call.arg(0)).to_ascii_lowercase();
            b(world
                .reference(me)
                .and_then(|r| r.anim_bools.get(&name).copied())
                .unwrap_or(false))
        }
    );
    // SetAlpha "(Float afTargetAlpha, Bool abFade)". Without a fade to run
    // over frames the target is reached at once.
    method!(vm, w, "Actor", "SetAlpha", |world, call, me| {
        let alpha = call.arg(0).as_float();
        if let Some(r) = world.reference(me) {
            r.alpha = alpha;
        }
        none()
    });
    // PlayIdle sends an idle to the actor's graph, the same delivery an
    // animation event takes, named after the idle's editor id.
    method!(vm, w, "Actor", "PlayIdle", |world, call, me| {
        let Some(idle) = world.form_arg(&call.arg(0)) else {
            return b(false);
        };
        let name = world.editor_id(idle).unwrap_or_default();
        world.animation_event(me, &name, alloc!(call));
        b(true)
    });
    // "Returns the nth linked ref of this object", the keyword filtered
    // list GetLinkedRef reads the head of.
    method!(
        vm,
        w,
        "ObjectReference",
        "GetNthLinkedRef",
        |world, call, me| {
            let index = call.arg(0).as_int().max(0) as usize;
            let keyword = world.form_arg(&call.arg(1));
            let linked = world.reference(me).and_then(|r| {
                r.linked
                    .iter()
                    .filter(|(k, _)| keyword.is_none() || *k == keyword)
                    .nth(index)
                    .map(|(_, l)| *l)
            });
            form_value(world, call, linked)
        }
    );
    // "Places an actor at this reference", PlaceAtMe with the actor flag
    // the leveled actor lists want.
    method!(
        vm,
        w,
        "ObjectReference",
        "PlaceActorAtMe",
        |world, call, me| {
            let Some(base) = world.form_arg(&call.arg(0)) else {
                return none();
            };
            let disabled = call.args.get(2).is_some_and(Value::as_bool);
            let placed = world.place_at(base, me, disabled);
            form_value(world, call, Some(placed))
        }
    );
}

/// The quest-owned fragment classes and `ActiveMagicEffect`. Both groups
/// are what the DLC scripts reach for most after the base classes.
fn fragments_and_effects(vm: &mut Vm, w: &Shared) {
    // CONTEXT: INFO ownership follows its DIAL group, not previous-topic links.
    for class in ["Scene", "Package", "TopicInfo"] {
        method!(vm, w, class, "GetOwningQuest", |world, call, me| {
            let quest = world.owning_quest(me);
            form_value(world, call, quest)
        });
    }
    method!(vm, w, "Actor", "GetDialogueTarget", |world, call, me| {
        let target = world.dialogue_target(me);
        form_value(world, call, target)
    });
    method!(
        vm,
        w,
        "ObjectReference",
        "IsInDialogueWithPlayer",
        |world, _call, me| {
            b(world
                .dialogue_target(me)
                .is_some_and(|target| world.is_player(target)))
        }
    );
    global!(vm, w, "Game", "GetDialogueTarget", |world, call| {
        let target = world
            .dialogue_session()
            .filter(|session| world.is_player(session.listener))
            .map(|session| session.speaker);
        form_value(world, call, target)
    });

    // An ActiveMagicEffect is not a Form, so the registration natives the
    // registry holds on Form are out of its reach. They key off the calling
    // object rather than a form, so the same behaviour registers here.
    for (name, repeat, game_time) in [
        ("RegisterForSingleUpdate", false, false),
        ("RegisterForUpdate", true, false),
        ("RegisterForSingleUpdateGameTime", false, true),
        ("RegisterForUpdateGameTime", true, true),
    ] {
        let w = w.clone();
        vm.natives.register_method(
            "ActiveMagicEffect",
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
            "ActiveMagicEffect",
            name,
            Box::new(move |call| {
                if let Some(obj) = call.self_object {
                    w.borrow_mut().unregister_update(obj, game_time);
                }
                none()
            }),
        );
    }
    self_method!(
        vm,
        w,
        "ActiveMagicEffect",
        "RegisterForAnimationEvent",
        |world, call, me| {
            let Some(sender) = world.form_arg(&call.arg(0)) else {
                return b(false);
            };
            let event = text(&call.arg(1));
            let l = AnimationListener {
                listener: me,
                sender,
                event,
            };
            if !world.animation_listeners.contains(&l) {
                world.animation_listeners.push(l);
            }
            b(true)
        }
    );
    self_method!(
        vm,
        w,
        "ActiveMagicEffect",
        "UnregisterForAnimationEvent",
        |world, call, me| {
            let sender = world.form_arg(&call.arg(0));
            let event = text(&call.arg(1));
            world.animation_listeners.retain(|l| {
                !(l.listener == me
                    && Some(l.sender) == sender
                    && l.event.eq_ignore_ascii_case(&event))
            });
            none()
        }
    );
}

/// Natives whose systems the engine does not have yet. They return the
/// value the game gives in the quiet case so vanilla scripts run past them.
fn stubs(vm: &mut Vm) -> usize {
    let mut n = 0;
    let bools: &[(&str, &str, bool)] = &[
        ("ObjectReference", "IsFurnitureInUse", false),
        ("Actor", "IsRunning", false),
        ("Actor", "IsSprinting", false),
        ("Actor", "IsHostileToActor", false),
        ("Actor", "IsChild", false),
        ("Actor", "IsCommandedActor", false),
        ("Actor", "IsOnMount", false),
        ("Actor", "IsTrespassing", false),
        ("Actor", "IsDetectedBy", true),
        ("Location", "IsLoaded", false),
        ("Location", "HasRefType", false),
        ("Scene", "IsActionComplete", false),
        ("Keyword", "SendStoryEventAndWait", false),
        ("ObjectReference", "IsFurnitureMarkerInUse", false),
        ("Utility", "IsInMenuMode", false),
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
        ("Actor", "GetSitState", 0),
        ("Location", "GetRefTypeAliveCount", 0),
        ("Location", "GetRefTypeDeadCount", 0),
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
        ("ObjectReference", "PlayAnimationAndWait"),
        ("ObjectReference", "PlayGamebryoAnimation"),
        ("ObjectReference", "SetMotionType"),
        ("ObjectReference", "ApplyHavokImpulse"),
        ("ObjectReference", "SetNoFavorAllowed"),
        ("Actor", "SetEssential"),
        ("Actor", "SetVehicle"),
        ("Actor", "SetLookAt"),
        ("Actor", "ClearLookAt"),
        ("Actor", "SetHeadTracking"),
        ("Actor", "ShowBarterMenu"),
        ("Actor", "OpenInventory"),
        ("Actor", "SendTrespassAlarm"),
        ("Actor", "StopCombatAlarm"),
        ("Actor", "ClearForcedMovement"),
        ("Actor", "ClearKeepOffsetFromActor"),
        ("Actor", "KeepOffsetFromActor"),
        ("Actor", "SetForcedLandingMarker"),
        ("Actor", "GetEquippedWeapon"),
        ("Actor", "GetEquippedShield"),
        ("Faction", "SendAssaultAlarm"),
        ("Faction", "SendPlayerToJail"),
        ("Keyword", "SendStoryEvent"),
        ("Actor", "PlayIdleWithTarget"),
        ("Spell", "Preload"),
        ("Spell", "Unload"),
        ("Actor", "Dismount"),
        ("Actor", "Trespass"),
        ("Actor", "StartCannibal"),
        ("Actor", "ClearExtraArrows"),
        // Attached-arrow state is owned by the host renderer.
        // The remaining music and particle systems do not have runtime owners.
        ("ShaderParticleGeometry", "Apply"),
        ("ShaderParticleGeometry", "Remove"),
        ("MusicType", "Add"),
        ("MusicType", "Remove"),
        ("Topic", "Add"),
        ("Scroll", "Cast"),
    ];
    for (class, name) in methods_none {
        vm.natives
            .register_method(class, name, Box::new(|_| none()));
        n += 1;
    }
    // No warmth system (the survival plugin is not loaded) and no sun to
    // take a position from.
    let floats: &[(&str, &str, f32)] = &[("Actor", "GetWarmthRating", 0.0)];
    for (class, name, value) in floats {
        let global = *class == "Game";
        let v = *value;
        let native: soe_papyrus::NativeFn = Box::new(move |_| f(v));
        if global {
            vm.natives.register_global(class, name, native);
        } else {
            vm.natives.register_method(class, name, native);
        }
        n += 1;
    }
    let globals_none: &[(&str, &str)] = &[
        // Werewolf crime, jail time and sun-gaze image space are not implemented.
        ("Game", "SendWereWolfTransformation"),
        ("Game", "SetSunGazeImageSpaceModifier"),
        ("Message", "ResetHelpMessage"),
        ("Game", "ShakeController"),
        ("Utility", "CaptureFrameRate"),
        ("Utility", "StartFrameRateCapture"),
        ("Utility", "EndFrameRateCapture"),
        ("Utility", "GetAverageFrameRate"),
        ("Utility", "GetMinFrameRate"),
        ("Utility", "GetMaxFrameRate"),
        ("Utility", "GetCurrentMemory"),
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
        assert!(vm.natives.has_global("game", "getcurrentcrosshairref"));
        assert!(vm.natives.has_global("game", "getplayergrabbedref"));
        assert!(vm.natives.has_method("alias", "getowningquest"));
        assert!(vm.natives.has_global("debug", "setgodmode"));
        assert!(vm.natives.has_global("debug", "toggleai"));
        assert!(vm.natives.has_global("debug", "togglecollisions"));
        assert!(vm.natives.has_global("debug", "dumpaliasdata"));
        assert!(vm.natives.has_global("debug", "debugchannelnotify"));
    }

    #[test]
    fn hst_18_world_exposes_current_crosshair_and_grabbed_references() {
        let mut world = World::new(LoadOrder::new());
        let crosshair = GlobalFormId {
            file: 1,
            object: 0x123,
        };
        let grabbed = GlobalFormId {
            file: 2,
            object: 0x456,
        };
        world.set_crosshair_reference(Some(crosshair));
        world.set_grabbed_reference(Some(grabbed));
        assert_eq!(world.crosshair_reference(), Some(crosshair));
        assert_eq!(world.grabbed_reference(), Some(grabbed));
        world.set_crosshair_reference(None);
        world.set_grabbed_reference(None);
        assert_eq!(world.crosshair_reference(), None);
        assert_eq!(world.grabbed_reference(), None);

        let mut vm = Vm::new();
        let shared = Rc::new(RefCell::new(world));
        register(&mut vm, &shared);
        assert!(vm.natives.has_global("game", "getcurrentcrosshairref"));
        assert!(vm.natives.has_global("game", "getplayergrabbedref"));
    }

    #[test]
    fn scr_60_registry_inventory_matches_formlist_and_game_contracts() {
        let mut vm = Vm::new();
        let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
        register(&mut vm, &world);

        for name in ["GetSize", "GetAt", "HasForm", "Find", "AddForm"] {
            assert!(vm.natives.has_method("FormList", name), "FormList.{name}");
        }

        let implemented = [
            "GetPlayer",
            "GetForm",
            "GetFormEx",
            "GetFormFromFile",
            "EnablePlayerControls",
            "DisablePlayerControls",
            "GetGameSettingFloat",
            "GetGameSettingInt",
            "GetGameSettingString",
            "SetGameSettingFloat",
            "SetGameSettingInt",
            "SetGameSettingBool",
            "SetGameSettingString",
            "GetRealHoursPassed",
            "IncrementStat",
            "QueryStat",
            "IsFastTravelEnabled",
            "EnableFastTravel",
            "SetPlayerAIDriven",
            "SetInChargen",
            "SetBeastForm",
            "TeachWord",
            "UnlockWord",
            "IsWordUnlocked",
            "AddAchievement",
            "GetPlayersLastRiddenHorse",
            "AddPerkPoints",
            "GetDialogueTarget",
        ];
        for name in implemented {
            assert!(vm.natives.has_global("Game", name), "Game.{name}");
        }

        let stubbed = [
            "IsPlayerSungazing",
            "GetSunPositionX",
            "GetSunPositionY",
            "GetSunPositionZ",
            "ShakeCamera",
            "SendWereWolfTransformation",
            "ServeTime",
            "ClearPrison",
            "SetSunGazeImageSpaceModifier",
            "ShakeController",
            "FadeOutGame",
            "RequestSave",
            "RequestAutoSave",
            "ForceFirstPerson",
            "ForceThirdPerson",
            "SetHudCartMode",
            "ShowRaceMenu",
            "TriggerScreenBlood",
            "PlayBink",
            "AdvanceSkill",
            "PrecacheCharGen",
            "PrecacheCharGenClear",
            "ShowTrainingMenu",
            "ShowFirstPersonGeometry",
            "SetCameraTarget",
            "QuitToMainMenu",
            "SetSittingRotation",
            "StartTitleSequence",
            "GetCurrentCrosshairRef",
            "GetPlayerGrabbedRef",
            "FindClosestActor",
            "FindRandomActor",
            "FindClosestReferenceOfType",
            "FindRandomReferenceOfType",
            "GetFormEx2",
        ];
        for name in stubbed {
            assert!(vm.natives.has_global("Game", name), "Game.{name}");
        }
    }

    #[test]
    fn scr_60_contract_fixes_name_existing_backlog_owners() {
        let backlog = include_str!("../../../BACKLOG.csv");
        let audit = include_str!("../../../docs/game.md");
        for row in [
            "SCR-26", "SCR-27", "SCR-28", "SCR-29", "SCR-30", "SCR-31", "SCR-32",
        ] {
            assert!(backlog.contains(&format!("\"{row}\"")), "missing {row}");
            assert!(audit.contains(&format!("- {row}:")), "unassigned {row}");
        }
        assert!(audit.contains("assigned to SCR-60"));
    }

    #[test]
    fn scr_62_registry_contains_every_audited_binding() {
        let mut vm = Vm::new();
        let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
        register(&mut vm, &world);
        for class in ["Alias", "Scene", "Package", "TopicInfo"] {
            assert!(
                vm.natives.has_method(class, "GetOwningQuest"),
                "missing {class}.GetOwningQuest"
            );
        }
        for name in ["GetValue", "GetValueInt", "SetValue", "SetValueInt", "Mod"] {
            assert!(
                vm.natives.has_method("GlobalVariable", name),
                "missing GlobalVariable.{name}"
            );
        }
    }

    #[test]
    fn scr_66_location_alias_bindings_match_ck_pages() {
        let mut vm = Vm::new();
        let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
        register(&mut vm, &world);
        let candidates = ["GetLocation", "ForceLocationTo", "Clear", "GetID"];
        let inventory: Vec<_> = candidates
            .into_iter()
            .filter(|name| vm.natives.has_method("LocationAlias", name))
            .collect();
        assert_eq!(
            inventory,
            ["GetLocation", "ForceLocationTo"],
            "LocationAlias direct bindings must match the CK contract"
        );
        assert!(
            vm.natives.has_method("Alias", "GetID"),
            "GetID is inherited from Alias, not registered on LocationAlias"
        );
    }

    #[test]
    fn scr_66_message_bindings_match_ck_pages() {
        let mut vm = Vm::new();
        let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
        register(&mut vm, &world);

        let methods = ["Show", "ShowAsHelpMessage", "ResetHelpMessage"];
        let method_inventory: Vec<_> = methods
            .into_iter()
            .filter(|name| vm.natives.has_method("Message", name))
            .collect();
        assert_eq!(
            method_inventory,
            ["Show", "ShowAsHelpMessage"],
            "Message instance bindings must match the CK contract"
        );
        let globals = ["Show", "ShowAsHelpMessage", "ResetHelpMessage"];
        let global_inventory: Vec<_> = globals
            .into_iter()
            .filter(|name| vm.natives.has_global("Message", name))
            .collect();
        assert_eq!(
            global_inventory,
            ["ResetHelpMessage"],
            "Message global bindings must match the CK contract"
        );
    }

    #[test]
    fn scr_55_actorbase_alias_inventory_matches_registry() {
        let mut vm = Vm::new();
        let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
        register(&mut vm, &world);

        let form_methods = [
            "GetFormID",
            "GetName",
            "GetType",
            "HasKeyword",
            "GetKeywords",
            "RegisterForSingleUpdate",
            "RegisterForUpdate",
            "RegisterForSingleUpdateGameTime",
            "RegisterForUpdateGameTime",
            "UnregisterForUpdate",
            "UnregisterForUpdateGameTime",
            "RegisterForSleep",
            "UnregisterForSleep",
            "RegisterForTrackedStatsEvent",
            "UnregisterForTrackedStatsEvent",
            "RegisterForAnimationEvent",
            "UnregisterForAnimationEvent",
            "RegisterForLOS",
            "RegisterForSingleLOSGain",
            "RegisterForSingleLOSLost",
            "UnregisterForLOS",
        ];
        let actor_base_methods = [
            "GetSkin",
            "SetSkin",
            "GetRace",
            "GetClass",
            "GetSex",
            "GetDeadCount",
            "IsEssential",
            "IsProtected",
            "IsInvulnerable",
            "IsUnique",
            "SetEssential",
            "SetProtected",
            "SetInvulnerable",
            "SetOutfit",
            "GetGiftFilter",
        ];
        let alias_methods = [
            "GetOwningQuest",
            "GetID",
            "GetName",
            "GetReference",
            "GetRef",
            "GetActorReference",
            "GetActorRef",
            "ForceRefTo",
            "Clear",
        ];

        for name in actor_base_methods {
            assert!(
                vm.natives.has_method("ActorBase", name),
                "ActorBase.{name} is missing from the registry"
            );
            assert!(
                !vm.natives.has_global("ActorBase", name),
                "ActorBase.{name} was registered as a global"
            );
        }
        for name in alias_methods {
            assert!(
                vm.natives.has_method("Alias", name),
                "Alias.{name} is missing from the registry"
            );
            assert!(
                !vm.natives.has_global("Alias", name),
                "Alias.{name} was registered as a global"
            );
        }
        for name in form_methods {
            assert!(
                vm.natives.has_method("Form", name),
                "inherited Form.{name} is missing from the registry"
            );
        }

        assert!(!vm.natives.has_global("ActorBase", "GetGiftFilter"));
        assert!(!vm.natives.has_global("Alias", "GetOwningQuest"));

        let audit = include_str!("../../../docs/reference-audit.md");
        for name in actor_base_methods {
            assert!(audit.contains(&format!("`ActorBase.{name}`")));
        }
        for name in alias_methods {
            assert!(audit.contains(&format!("`Alias.{name}`")));
        }
        for name in form_methods {
            assert!(audit.contains(&format!("`Form.{name}`")));
        }
        assert!(audit.contains("`ActorBase` and `Alias` have no registered global bindings"));
        assert!(audit.contains("assigns the correction to SCR-18"));
        assert!(audit.contains("SCR-68 owns optional validation and its diagnostic"));
        assert!(audit.contains("Assign reference-type validation to SCR-59"));
        let backlog = include_str!("../../../BACKLOG.csv");
        assert!(backlog.contains("\"SCR-18\""));
        assert!(backlog.contains("\"SCR-68\""));
        assert!(backlog.contains("\"SCR-59\""));
    }

    #[test]
    fn scr_56_registry_inventory_matches_ck_contracts() {
        let mut vm = Vm::new();
        let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
        register(&mut vm, &world);
        let mut inventory: Vec<_> = vm
            .natives
            .method_bindings()
            .filter(|(class, name)| *class == "cell" || *name == "clear")
            .collect();
        inventory.sort_unstable();
        let global_inventory: Vec<_> = vm
            .natives
            .global_bindings()
            .filter(|(class, name)| *class == "cell" || *name == "clear")
            .collect();
        let mut compared = vec![
            ("alias", "clear"),
            ("cell", "getactorowner"),
            ("cell", "getfactionowner"),
            ("cell", "isattached"),
            ("cell", "isinterior"),
            ("cell", "reset"),
            ("cell", "setactorowner"),
            ("cell", "setfactionowner"),
            ("cell", "setpublic"),
        ];
        compared.sort_unstable();
        // Each binding is compared with its CK page or the corresponding
        // vanilla PSC declaration. Unsupported effects are assigned in
        // docs/reference-audit.md to the existing owning rows.
        assert_eq!(inventory, compared);
        assert!(global_inventory.is_empty());
    }

    #[test]
    fn scr_56_cell_owner_setters_require_the_ck_record_type() {
        assert!(cell_owner_tag_matches(
            Some(Tag::new(b"NPC_")),
            Tag::new(b"NPC_")
        ));
        assert!(cell_owner_tag_matches(
            Some(Tag::new(b"FACT")),
            Tag::new(b"FACT")
        ));
        assert!(!cell_owner_tag_matches(
            Some(Tag::new(b"FACT")),
            Tag::new(b"NPC_")
        ));
        assert!(!cell_owner_tag_matches(
            Some(Tag::new(b"NPC_")),
            Tag::new(b"FACT")
        ));
        assert!(!cell_owner_tag_matches(
            Some(Tag::new(b"ACHR")),
            Tag::new(b"NPC_")
        ));
        assert!(!cell_owner_tag_matches(None, Tag::new(b"FACT")));
    }

    #[test]
    fn scr_58_faction_registry_matches_the_contract_inventory() {
        let mut vm = Vm::new();
        let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
        register(&mut vm, &world);

        let implemented = [
            "SetAlly",
            "SetEnemy",
            "GetReaction",
            "SetReaction",
            "GetCrimeGold",
            "GetCrimeGoldViolent",
            "GetCrimeGoldNonViolent",
            "SetCrimeGold",
            "SetCrimeGoldViolent",
            "ModCrimeGold",
            "GetInfamy",
            "GetInfamyViolent",
            "GetInfamyNonViolent",
            "CanPayCrimeGold",
            "PlayerPayCrimeGold",
            "IsPlayerExpelled",
            "SetPlayerExpelled",
            "SetPlayerEnemy",
            "IsFactionInCrimeGroup",
        ];
        let stubs = ["SendAssaultAlarm", "SendPlayerToJail", "ModReaction"];

        for name in implemented.into_iter().chain(stubs) {
            assert!(vm.natives.has_method("Faction", name), "Faction.{name}");
            assert!(
                !vm.natives.has_global("Faction", name),
                "Faction.{name} is a method, not a global"
            );
        }
        assert_eq!(implemented.len(), 19);
        assert_eq!(stubs.len(), 3);
        assert_eq!(implemented.len() + stubs.len(), 22);
    }

    #[test]
    fn scr_58_force_ref_if_empty_is_scoped_to_reference_alias() {
        let mut vm = Vm::new();
        let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
        register(&mut vm, &world);

        assert!(vm.natives.has_method("ReferenceAlias", "ForceRefIfEmpty"));
        assert!(!vm.natives.has_method("Alias", "ForceRefIfEmpty"));
        assert!(!vm.natives.has_method("LocationAlias", "ForceRefIfEmpty"));
        assert!(!vm.natives.has_global("ReferenceAlias", "ForceRefIfEmpty"));
    }
}
