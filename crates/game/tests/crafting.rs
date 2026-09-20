// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Recipe transactions and CK inventory notifications, including script callbacks.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{crafting::Error, quest, Game, Handle, Pending};
use soe_papyrus::asm::{b, i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

const PLAYER: u32 = 0x14;
const BASE: u32 = 0x800;
const BENCH: u32 = 0x801;
const FURN: u32 = 0x802;
const KEYWORD: u32 = 0x803;
const IRON: u32 = 0x804;
const NAILS: u32 = 0x805;
const RECIPE: u32 = 0x806;
const QUEST: u32 = 0x807;
const EMPTY: u32 = 0x808;
const ITEMS: u32 = 0x809;
const NESTED: u32 = 0x80a;

fn recipe(ingredients: &[(u32, i32)], output: u32, count: u16) -> Record {
    let mut record = Record::new(Tag(*b"COBJ"), RECIPE)
        .sub(Tag(*b"CNAM"), output.to_le_bytes())
        .sub(Tag(*b"BNAM"), KEYWORD.to_le_bytes())
        .sub(Tag(*b"NAM1"), count.to_le_bytes());
    for &(item, count) in ingredients {
        let mut bytes = item.to_le_bytes().to_vec();
        bytes.extend(count.to_le_bytes());
        record = record.sub(Tag(*b"CNTO"), bytes);
    }
    record
}

fn game(record: Record, bench_type: u8) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for (tag, id) in [
        (*b"NPC_", BASE),
        (*b"KYWD", KEYWORD),
        (*b"MISC", IRON),
        (*b"MISC", NAILS),
        (*b"FLST", EMPTY),
    ] {
        plugin.push_top(Record::new(Tag(tag), id));
    }
    plugin.push_top(Record::new(Tag(*b"FLST"), ITEMS).sub(Tag(*b"LNAM"), NAILS.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"FLST"), NESTED).sub(Tag(*b"LNAM"), ITEMS.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), FURN)
            .sub(Tag(*b"WBDT"), [bench_type, 0xff])
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), BENCH).sub(Tag(*b"NAME"), FURN.to_le_bytes()));
    plugin.push_top(quest::build_record(QUEST, "CraftQuest", 0));
    plugin.push_top(record);
    let mut order = LoadOrder::new();
    order.add("Craft.esm", plugin).unwrap();
    Game::new(order)
}

fn id(game: &Game, raw: u32) -> GlobalFormId {
    game.world().order().resolve(0, raw)
}

fn stock(game: &Game, item: u32, count: i32) {
    let (player, item) = (id(game, PLAYER), id(game, item));
    game.world_mut().add_item(player, item, count);
}

fn count(game: &Game, item: u32) -> i32 {
    let (player, item) = (id(game, PLAYER), id(game, item));
    game.world_mut().item_count(player, item)
}

fn craft(game: &mut Game) -> Result<soe_game::crafting::Outcome, Error> {
    game.craft(id(game, PLAYER), id(game, BENCH), id(game, RECIPE))
}

fn pending(game: &Game) -> Vec<(soe_papyrus::ObjectId, String, Vec<Value>)> {
    game.world()
        .pending
        .iter()
        .map(|event| match event {
            Pending::Event { object, name, args } => (*object, name.clone(), args.clone()),
            Pending::Call { .. } => panic!("unexpected function call"),
            Pending::BindEffect { .. } => panic!("unexpected magic effect binding"),
            Pending::DeferredDeath { .. } | Pending::ActorDeath { .. } => {
                panic!("unexpected actor death")
            }
        })
        .collect()
}

#[test]
fn duplicate_ingredients_are_gross_requirements_and_failure_is_atomic() {
    let mut g = game(recipe(&[(IRON, 2), (IRON, 3)], IRON, 10), 1);
    stock(&g, IRON, 4);
    let player = id(&g, PLAYER);
    let bench = id(&g, BENCH);
    let recipe = id(&g, RECIPE);
    let expected = Error::MissingIngredient {
        item: id(&g, IRON),
        needed: 5,
        available: 4,
    };
    assert_eq!(g.check_craft(player, bench, recipe), Err(expected));
    assert!(craft(&mut g).is_err());
    assert_eq!(count(&g, IRON), 4);
    assert!(pending(&g).is_empty());
    stock(&g, IRON, 1);
    let preview = g.check_craft(player, bench, recipe).unwrap();
    assert_eq!(count(&g, IRON), 5);
    assert!(pending(&g).is_empty());
    assert_eq!(craft(&mut g).unwrap(), preview);
    assert_eq!(count(&g, IRON), 10);
    let events = pending(&g);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].1, "OnItemRemoved");
    assert_eq!(events[0].2[1..], [Value::Int(5), Value::None, Value::None]);
    assert_eq!(events[1].1, "OnItemAdded");
    assert_eq!(events[1].2[1], Value::Int(10));
}

#[test]
fn missing_later_ingredient_and_output_overflow_do_not_consume_inputs() {
    let mut g = game(recipe(&[(IRON, 1), (NAILS, 2)], IRON, 1), 1);
    stock(&g, IRON, 1);
    assert!(matches!(
        craft(&mut g),
        Err(Error::MissingIngredient { .. })
    ));
    assert_eq!(count(&g, IRON), 1);
    assert!(pending(&g).is_empty());

    let mut g = game(recipe(&[(IRON, 1)], NAILS, 10), 1);
    stock(&g, IRON, 1);
    stock(&g, NAILS, i32::MAX - 5);
    assert_eq!(craft(&mut g), Err(Error::Quantity));
    assert_eq!(count(&g, IRON), 1);
    assert_eq!(count(&g, NAILS), i32::MAX - 5);
    assert!(pending(&g).is_empty());
}

#[test]
fn invalid_quantities_and_improvement_benches_are_rejected() {
    for recipe in [
        recipe(&[(IRON, i32::MAX), (IRON, 1)], NAILS, 1),
        recipe(&[(IRON, -1)], NAILS, 1),
        recipe(&[(IRON, 1)], NAILS, 0),
    ] {
        let mut g = game(recipe, 1);
        stock(&g, IRON, 10);
        assert_eq!(craft(&mut g), Err(Error::Quantity));
        assert_eq!(count(&g, IRON), 10);
        assert!(pending(&g).is_empty());
    }
    for mode in [0, 2, 3, 4, 5, 6, 7] {
        let mut g = game(recipe(&[(IRON, 1)], NAILS, 10), mode);
        stock(&g, IRON, 1);
        assert_eq!(craft(&mut g), Err(Error::BenchType(mode)));
        assert_eq!(count(&g, IRON), 1);
    }
}

#[test]
fn reference_and_alias_filters_are_independent_and_apply_at_enqueue_time() {
    let mut g = game(recipe(&[(IRON, 1)], NAILS, 10), 1);
    stock(&g, IRON, 3);
    let player = id(&g, PLAYER);
    let quest = id(&g, QUEST);
    let direct = g.object_of_form(player);
    let alias = g.object_of(Handle::Alias { quest, id: 3 });
    let nested = g.object_of(Handle::Alias { quest, id: 4 });
    let empty = id(&g, EMPTY);
    let items = id(&g, ITEMS);
    let nested_list = id(&g, NESTED);
    {
        let mut w = g.world_mut();
        let q = w.quest_state(quest).unwrap();
        q.running = true;
        q.aliases.insert(3, Some(player));
        q.aliases.insert(4, Some(player));
        w.inventory_filters.insert(direct, vec![empty]);
        w.inventory_filters.insert(alias, vec![items]);
        w.inventory_filters.insert(nested, vec![nested_list]);
    }
    craft(&mut g).unwrap();
    let events = pending(&g);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, alias);
    assert_eq!(events[0].1, "OnItemAdded");
    g.world_mut().inventory_filters.insert(alias, vec![empty]);
    assert_eq!(pending(&g), events, "queued calls survive filter changes");
    g.world_mut().pending.clear();
    craft(&mut g).unwrap();
    assert!(pending(&g).is_empty());
    g.world_mut().inventory_filters.remove(&direct);
    craft(&mut g).unwrap();
    assert_eq!(pending(&g).len(), 2);
    assert!(pending(&g).iter().all(|event| event.0 == direct));
}

fn callback_script() -> Vec<u8> {
    ScriptBuilder::new("CraftReceiver", "ObjectReference")
        .variable("removed", "Int", i(0))
        .function(
            FunctionBuilder::new("OnItemAdded", "None")
                .param("item", "Form")
                .param("count", "Int")
                .param("reference", "ObjectReference")
                .param("source", "ObjectReference")
                .ins(
                    CallMethod,
                    vec![
                        s("RemoveItem"),
                        ident("self"),
                        ident("::nonevar"),
                        ident("item"),
                        i(1),
                        b(true),
                        none(),
                    ],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("OnItemRemoved", "None")
                .param("item", "Form")
                .param("count", "Int")
                .param("reference", "ObjectReference")
                .param("destination", "ObjectReference")
                .ins(
                    IAdd,
                    vec![ident("removed"), ident("removed"), ident("count")],
                )
                .build(),
        )
        .bytes()
}

#[test]
fn crafted_item_callback_can_remove_it_and_pending_events_survive_save_load() {
    let mut g = game(recipe(&[(IRON, 2)], NAILS, 10), 1);
    stock(&g, IRON, 2);
    g.load_script(&callback_script()).unwrap();
    let object = g.object_of_form(id(&g, PLAYER));
    g.vm.attach(object, "CraftReceiver").unwrap();
    craft(&mut g).unwrap();
    let saved = g.save_state().unwrap();
    assert!(g.run_until_idle(64));
    assert_eq!(count(&g, IRON), 0);
    assert_eq!(count(&g, NAILS), 9);
    assert_eq!(
        g.vm.get_var(object, "CraftReceiver", "removed"),
        Some(Value::Int(3))
    );
    let expected = g.state_hash();

    let mut resumed = game(recipe(&[(IRON, 2)], NAILS, 10), 1);
    resumed.load_script(&callback_script()).unwrap();
    resumed.load_state(&saved).unwrap();
    assert!(resumed.run_until_idle(64));
    assert_eq!(count(&resumed, IRON), 0);
    assert_eq!(count(&resumed, NAILS), 9);
    assert_eq!(
        resumed.vm.get_var(object, "CraftReceiver", "removed"),
        Some(Value::Int(3))
    );
    assert!(pending(&resumed).is_empty());
    assert_eq!(resumed.state_hash(), expected);
}

fn condition(function: u16, parameter: u32, expected: f32, flags: u8) -> Vec<u8> {
    let mut bytes = vec![flags, 0, 0, 0];
    bytes.extend(expected.to_le_bytes());
    bytes.extend(function.to_le_bytes());
    bytes.extend([0, 0]);
    bytes.extend(parameter.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend((-1i32).to_le_bytes());
    bytes
}

#[test]
fn recipe_conditions_are_rechecked_and_unknown_functions_fail_closed() {
    let recipe = recipe(&[(IRON, 1)], NAILS, 10).sub(Tag(*b"CTDA"), condition(47, IRON, 3.0, 0x60));
    let mut g = game(recipe, 1);
    stock(&g, IRON, 3);
    let preview = g.check_craft(id(&g, PLAYER), id(&g, BENCH), id(&g, RECIPE));
    assert!(preview.is_ok());
    craft(&mut g).unwrap();
    let events = pending(&g);
    assert_eq!(craft(&mut g), Err(Error::Ineligible));
    assert_eq!(count(&g, IRON), 2);
    assert_eq!(pending(&g), events);

    let mut g = game(
        crate::recipe(&[(IRON, 1)], NAILS, 10).sub(Tag(*b"CTDA"), condition(0xffff, 0, 1.0, 0)),
        1,
    );
    stock(&g, IRON, 1);
    assert_eq!(
        craft(&mut g),
        Err(Error::Condition(soe_game::conditions::Error::Function(
            0xffff
        )))
    );
    assert_eq!(count(&g, IRON), 1);
    assert!(pending(&g).is_empty());
}

#[test]
fn or_groups_use_current_state_and_short_circuit_only_inside_the_group() {
    let record = recipe(&[(IRON, 1)], NAILS, 10)
        .sub(Tag(*b"CTDA"), condition(47, IRON, 5.0, 0x61))
        .sub(Tag(*b"CTDA"), condition(47, NAILS, 10.0, 0x60))
        .sub(Tag(*b"CTDA"), condition(56, QUEST, 1.0, 0));
    let mut g = game(record, 1);
    stock(&g, IRON, 1);
    stock(&g, NAILS, 10);
    assert_eq!(craft(&mut g), Err(Error::Ineligible));
    let quest = id(&g, QUEST);
    g.world_mut().quest_state(quest).unwrap().running = true;
    craft(&mut g).unwrap();
    assert_eq!(count(&g, NAILS), 20);

    let record = recipe(&[(IRON, 1)], NAILS, 10)
        .sub(Tag(*b"CTDA"), condition(47, IRON, 1.0, 0x61))
        .sub(Tag(*b"CTDA"), condition(0xffff, 0, 0.0, 0));
    let mut g = game(record, 1);
    stock(&g, IRON, 1);
    craft(&mut g).unwrap();
}

#[test]
fn quest_conditions_read_live_conditional_pex_variables() {
    let record = recipe(&[(IRON, 1)], NAILS, 10)
        .sub(Tag(*b"CTDA"), condition(629, QUEST, 2.0, 0))
        .zsub(Tag(*b"CIS2"), "::house_var");
    let mut g = game(record, 1);
    stock(&g, IRON, 2);
    let mut script = ScriptBuilder::new("HouseQuest", "Quest")
        .auto_property("House", "Int", i(1))
        .build();
    script.user_flags.push(soe_papyrus::pex::UserFlag {
        name: "conditional".into(),
        bit: 3,
    });
    script.objects[0].user_flags = 8;
    script.objects[0].variables[0].user_flags = 8;
    g.vm.load(script);
    let quest = g.object_of_form(id(&g, QUEST));
    g.vm.attach(quest, "HouseQuest").unwrap();
    assert_eq!(craft(&mut g), Err(Error::Ineligible));
    g.vm.set_var(quest, "HouseQuest", "::House_var", Value::Int(2))
        .unwrap();
    craft(&mut g).unwrap();
    g.vm.set_var(quest, "HouseQuest", "::House_var", Value::Int(1))
        .unwrap();
    assert_eq!(craft(&mut g), Err(Error::Ineligible));
    assert_eq!(count(&g, NAILS), 10);
}

#[test]
fn winning_recipe_resolves_links_in_its_own_master_order() {
    let base = game(recipe(&[(IRON, 1)], NAILS, 10), 1);
    let mut order = LoadOrder::new();
    let plugin = Plugin::parse(&base.world().order().files()[0].plugin.to_bytes()).unwrap();
    order.add("Craft.esm", plugin).unwrap();
    let mut addon = Plugin::new(FileHeader::default());
    addon.push_top(Record::new(Tag(*b"MISC"), NAILS));
    order.add("Addon.esm", addon).unwrap();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Addon.esm".into(), "Craft.esm".into()],
        ..FileHeader::default()
    });
    let mut changed = recipe(&[(0x0100_0000 | IRON, 2)], NAILS, 7);
    changed.form_id = 0x0100_0000 | RECIPE;
    for sub in &mut changed.subrecords {
        if sub.tag == Tag(*b"BNAM") {
            sub.data = (0x0100_0000 | KEYWORD).to_le_bytes().to_vec();
        }
    }
    patch.push_top(changed);
    order.add("Patch.esp", patch).unwrap();
    let mut g = Game::new(order);
    stock(&g, IRON, 2);
    let output = craft(&mut g).unwrap();
    assert_eq!(
        output.created,
        GlobalFormId {
            file: 1,
            object: NAILS
        }
    );
    assert_eq!(output.count, 7);
    assert_eq!(count(&g, IRON), 0);
    assert_eq!(count(&g, NAILS), 0);
    let player = id(&g, PLAYER);
    assert_eq!(g.world_mut().item_count(player, output.created), 7);
}

#[test]
fn crafting_does_not_invent_target_context_or_ignore_unknown_perk_parameters() {
    let mut target = condition(47, IRON, 1.0, 0x60);
    target[20..24].copy_from_slice(&1u32.to_le_bytes());
    let mut g = game(
        recipe(&[(IRON, 1)], NAILS, 10).sub(Tag(*b"CTDA"), target),
        1,
    );
    stock(&g, IRON, 1);
    assert_eq!(
        craft(&mut g),
        Err(Error::Condition(soe_game::conditions::Error::Target(1)))
    );
    assert_eq!(count(&g, IRON), 1);
    assert!(pending(&g).is_empty());

    let mut perk = condition(448, 0, 1.0, 0);
    perk[16..20].copy_from_slice(&1u32.to_le_bytes());
    let mut g = game(recipe(&[(IRON, 1)], NAILS, 10).sub(Tag(*b"CTDA"), perk), 1);
    stock(&g, IRON, 1);
    assert_eq!(
        craft(&mut g),
        Err(Error::Condition(soe_game::conditions::Error::Parameter(
            448, 1
        )))
    );
    assert_eq!(count(&g, IRON), 1);
    assert!(pending(&g).is_empty());
}

#[test]
fn notified_transfers_check_destination_capacity_and_keep_container_arguments() {
    let mut g = game(recipe(&[(IRON, 1)], NAILS, 10), 1);
    stock(&g, IRON, 3);
    let player = id(&g, PLAYER);
    let bench = id(&g, BENCH);
    let iron = id(&g, IRON);
    let list = id(&g, ITEMS);
    let player_object = g.object_of_form(player);
    let bench_object = g.object_of_form(bench);
    let shared = g.shared_world();
    let mut world = shared.borrow_mut();
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    for unsupported in [list, bench] {
        assert!(!world.add_item_notified(player, unsupported, 1, &mut alloc));
        assert_eq!(world.item_count(player, unsupported), 0);
    }
    world.add_item(bench, iron, i32::MAX);
    assert_eq!(
        world.remove_item_notified(player, iron, 2, Some(bench), &mut alloc),
        0
    );
    assert_eq!(world.item_count(player, iron), 3);
    assert!(world.pending.is_empty());
    world.reference(bench).unwrap().inventory.clear();
    assert_eq!(
        world.remove_item_notified(player, iron, 2, Some(bench), &mut alloc),
        2
    );
    assert_eq!(world.item_count(player, iron), 1);
    assert_eq!(world.item_count(bench, iron), 2);
    drop(world);
    let events = pending(&g);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, player_object);
    assert_eq!(events[0].1, "OnItemRemoved");
    assert_eq!(events[0].2[3], Value::Object(bench_object));
    assert_eq!(events[1].0, bench_object);
    assert_eq!(events[1].1, "OnItemAdded");
    assert_eq!(events[1].2[3], Value::Object(player_object));
}

#[test]
fn implicit_player_keeps_reserved_identity_and_receives_forced_alias_events() {
    let fixture = game(recipe(&[(IRON, 1)], NAILS, 10), 1);
    let mut plugin = Plugin::new(FileHeader::default());
    for record in &fixture.world().order().files()[0].plugin.records {
        if record.form_id != PLAYER && record.form_id != QUEST {
            let mut record = record.clone();
            if record.form_id == BASE {
                record = record.zsub(Tag(*b"EDID"), "Player");
            }
            plugin.push_top(record);
        }
    }
    plugin.push_top(quest::with_alias(
        quest::build_record(QUEST, "PlayerAliasQuest", 0),
        0,
        "PlayerAlias",
        Some(PLAYER),
    ));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut g = Game::new(order);
    let reserved = id(&g, PLAYER);
    assert!(g.world().record(reserved).is_none());
    let player = g.world_mut().player();
    assert_eq!(player, reserved);
    let base = id(&g, BASE);
    assert_eq!(g.world_mut().base_of(player), Some(base));
    let quest = id(&g, QUEST);
    g.world_mut().quest_state(quest).unwrap().running = true;
    assert_eq!(g.world_mut().alias_ref(quest, 0), Some(player));
    let alias = g.object_of(Handle::Alias { quest, id: 0 });
    stock(&g, IRON, 1);
    craft(&mut g).unwrap();
    let alias_events: Vec<_> = pending(&g)
        .into_iter()
        .filter(|event| event.0 == alias)
        .collect();
    assert_eq!(alias_events.len(), 2);
    assert_eq!(alias_events[0].1, "OnItemRemoved");
    assert_eq!(alias_events[1].1, "OnItemAdded");
    let bytes = g.save_state().unwrap();
    g.load_state(&bytes).unwrap();
    assert_eq!(g.world_mut().player(), reserved);
    assert_eq!(g.world_mut().alias_ref(quest, 0), Some(reserved));
}

#[test]
fn crafting_consumes_distinct_ingredients_without_losing_unspent_instances() {
    use soe_game::inventory::{Extra, Field, Stack};
    let mut g = game(recipe(&[(IRON, 2)], NAILS, 10), 1);
    let (player, iron) = (id(&g, PLAYER), id(&g, IRON));
    let distinct = Stack::new(
        3,
        vec![Extra {
            kind: 33,
            fields: vec![Field::Form(Some(player))],
        }],
    )
    .unwrap();
    g.world_mut()
        .reference(player)
        .unwrap()
        .inventory
        .restore(iron, 3, vec![distinct.clone()])
        .unwrap();
    g.check_craft(player, id(&g, BENCH), id(&g, RECIPE))
        .unwrap();
    assert_eq!(
        g.world()
            .reference_ref(player)
            .unwrap()
            .inventory
            .stacks(iron),
        &[distinct]
    );
    craft(&mut g).unwrap();
    let inventory = g.world().reference_ref(player).unwrap().inventory.clone();
    assert_eq!(inventory.count(iron), 1);
    assert_eq!(inventory.stacks(iron)[0].count(), 1);
    assert_eq!(
        inventory.stacks(iron)[0].extras()[0].fields,
        vec![Field::Form(Some(player))]
    );
    let before = inventory.clone();
    assert!(craft(&mut g).is_err());
    assert_eq!(g.world().reference_ref(player).unwrap().inventory, before);
}
