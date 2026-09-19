// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Every non-deleted gameplay winner must decode. Non-null spell effects,
//! topic quests and recipe outputs/ingredients must resolve in the five masters.

use soe_esm::records::gameplay::*;
use soe_esm::{LoadOrder, Plugin, Tag};
use std::{collections::BTreeMap, path::PathBuf};

#[test]
fn the_five_masters_gameplay_winners_decode_and_links_resolve() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping gameplay record survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = std::fs::read(data.join(name)).unwrap();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let mut counts = BTreeMap::new();
    let mut conditions = 0;
    let mut responses = 0;
    let mut effects = 0;
    let mut ingredients = 0;
    let mut no_output = 0;
    let mut links = 0;
    let mut failures = Vec::new();
    for signature in [b"DIAL", b"INFO", b"SPEL", b"MGEF", b"COBJ"] {
        let mut count = 0;
        for (id, rec) in order.winners_of(Tag::new(signature)) {
            if rec.is_deleted() {
                continue;
            }
            count += 1;
            let file = order.winner_ref(id).unwrap().file;
            let localized = order.file(file).plugin.header.is_localized();
            let mut check = |raw: u32, expected: Option<Tag>| {
                if raw == 0 {
                    return;
                }
                links += 1;
                let resolved = order.resolve(file, raw);
                match order.winner(resolved) {
                    Some(target) if expected.is_none_or(|tag| tag == target.tag) => {}
                    other => failures.push(format!(
                        "{} {id}: link {raw:08x} resolved to {:?}",
                        rec.tag,
                        other.map(|r| r.tag)
                    )),
                }
            };
            let result = match signature {
                b"DIAL" => DialogueTopic::from_record(rec, localized).map(|view| {
                    if let Some(quest) = view.quest {
                        check(quest, Some(Tag::new(b"QUST")));
                    }
                }),
                b"INFO" => DialogueInfo::from_record(rec, localized).map(|view| {
                    conditions += view.conditions.len();
                    responses += view.responses.len();
                }),
                b"SPEL" => Spell::from_record(rec, localized).map(|view| {
                    effects += view.effects.len();
                    for effect in &view.effects {
                        conditions += effect.conditions.len();
                        check(effect.base_effect, Some(Tag::new(b"MGEF")));
                    }
                }),
                b"MGEF" => MagicEffect::from_record(rec, localized).map(|view| {
                    conditions += view.conditions.len();
                }),
                b"COBJ" => Recipe::from_record(rec).map(|view| {
                    ingredients += view.ingredients.len();
                    conditions += view.conditions.len();
                    if let Some(output) = view.created_object {
                        check(output, None);
                    } else {
                        no_output += 1;
                        eprintln!("COBJ {id} {:?} has no output", view.editor_id);
                    }
                    for ingredient in &view.ingredients {
                        check(ingredient.item, None);
                    }
                }),
                _ => unreachable!(),
            };
            if let Err(error) = result {
                failures.push(format!("{} {id}: {error}", rec.tag));
            }
        }
        assert!(count > 0, "no {} winners surveyed", Tag::new(signature));
        counts.insert(Tag::new(signature), count);
    }
    eprintln!("winning records: {counts:?}");
    eprintln!("{conditions} conditions, {responses} responses, {effects} effects, {ingredients} ingredients, {links} links");
    eprintln!("{no_output} recipes have no output object");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
