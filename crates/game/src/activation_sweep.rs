// SPDX-License-Identifier: GPL-3.0-or-later

//! Activation evidence from CK Activate/OnActivate, xEdit REFR/ACTI/LIGH,
//! and nifly NiControllerSequence. Unknown script effects never count as passes.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value as Json};
use soe_esm::{CellIndex, GlobalFormId, LoadOrder, Tag};
use soe_papyrus::asm::{i, ident, none, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{IAdd, Return};

use crate::Game;

const OBSERVER: &str = "SoeSweepObserver";

/// Attach an independent event counter without replacing authored instances.
pub fn observe(game: &mut Game, target: GlobalFormId) -> Result<(), String> {
    let bytes = ScriptBuilder::new(OBSERVER, "ObjectReference")
        .variable("activations", "Int", i(0))
        .function(
            FunctionBuilder::new("OnActivate", "None")
                .param("activator", "ObjectReference")
                .ins(IAdd, vec![ident("activations"), ident("activations"), i(1)])
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).map_err(|e| e.to_string())?;
    let object = game.object_of_form(target);
    game.vm
        .attach(object, OBSERVER)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Activation eligibility from base records. Disabled placements are reported separately.
pub fn eligible(record: &soe_esm::Record) -> bool {
    match record.tag.as_bytes() {
        b"ACTI" | b"DOOR" | b"CONT" | b"FURN" | b"FLOR" | b"NPC_" | b"ALCH" | b"INGR" | b"MISC"
        | b"BOOK" | b"WEAP" | b"ARMO" | b"AMMO" | b"KEYM" | b"SLGM" | b"SCRL" => true,
        b"TREE" => record
            .find(Tag(*b"PFIG"))
            .is_some_and(|raw| raw.iter().any(|b| *b != 0)),
        b"LIGH" => record
            .find(Tag(*b"DATA"))
            .and_then(|raw| raw.get(12..16))
            .is_some_and(|raw| u32::from_le_bytes(raw.try_into().unwrap_or_default()) & 2 != 0),
        _ => false,
    }
}

/// The requested preview suite, followed by populated Whiterun exterior cells.
pub fn selected_cells(order: &LoadOrder, index: &CellIndex, all: bool) -> Vec<GlobalFormId> {
    let names = [
        "QASmoke",
        "WarehouseFurniture",
        "WarehousePrefabs",
        "WarehouseTraps",
        "WarehouseCrafting",
        "WarehouseCarry",
        "testL",
        "DLC2RieklingFurniture",
        "DLC1DBTest",
        "DLC2WarehouseSpiders",
        "WhiterunBanneredMare",
        "WhiterunDragonsreach",
    ];
    let whiterun = index
        .cell_by_editor_id("WhiterunOrigin")
        .and_then(|id| index.grid_of(id));
    let mut cells: Vec<_> = index
        .cells()
        .filter_map(|(id, _)| {
            let name = order
                .winner(id)
                .and_then(soe_esm::Record::editor_id)
                .unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            let selected = all
                || names.iter().any(|n| name.eq_ignore_ascii_case(n))
                || lower.starts_with("xtestgrantint")
                || lower.starts_with("mzinchaleft")
                || lower.starts_with("whiterunexterior")
                || (lower.starts_with("dlc") && lower.contains("test"))
                || whiterun.is_some_and(|origin| {
                    index.grid_of(id).is_some_and(|grid| {
                        grid.world == origin.world
                            && (grid.x - origin.x).abs() <= 2
                            && (grid.y - origin.y).abs() <= 2
                    })
                });
            selected.then_some(id)
        })
        .collect();
    cells.sort_by_key(|id| {
        let name = order
            .winner(*id)
            .and_then(soe_esm::Record::editor_id)
            .unwrap_or_default();
        (
            if name.to_ascii_lowercase().starts_with("mzinchaleft") {
                4
            } else if name.eq_ignore_ascii_case("WhiterunBanneredMare") {
                1
            } else if name.eq_ignore_ascii_case("WhiterunDragonsreach") {
                2
            } else if name.to_ascii_lowercase().starts_with("whiterun")
                || index
                    .grid_of(*id)
                    .is_some_and(|grid| whiterun.is_some_and(|origin| grid.world == origin.world))
            {
                3
            } else {
                0
            },
            names
                .iter()
                .position(|n| name.eq_ignore_ascii_case(n))
                .unwrap_or(names.len()),
            *id,
        )
    });
    cells
}

/// Load the five vanilla masters through the existing plugin cache reader.
pub fn installed_order(data: &Path) -> Result<Arc<LoadOrder>, String> {
    let mut order = LoadOrder::new();
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/soe-plugin-cache/sweep");
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let plugin = soe_esm::read_plugin_cached(data.join(name), &cache)
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        order.add(name, plugin).map_err(|e| e.to_string())?;
    }
    Ok(Arc::new(order))
}

/// Read vanilla archives in plugin order, with loose data taking precedence.
pub fn installed_vfs(data: &Path, order: &LoadOrder) -> Result<soe_bsa::Vfs, String> {
    let mut paths = std::fs::read_dir(data)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    paths.sort();
    let mut vfs = soe_bsa::Vfs::new();
    for file in order.files() {
        let stem = Path::new(&file.name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        for path in &paths {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if (name == format!("{stem}.bsa") || name.starts_with(&format!("{stem} - ")))
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("bsa"))
            {
                vfs.add_archive_path(path).map_err(|e| e.to_string())?;
            }
        }
    }
    vfs.add_loose_root(data).map_err(|e| e.to_string())?;
    Ok(vfs)
}

fn label(order: &LoadOrder, id: GlobalFormId) -> Json {
    json!({"file":id.file,"object":id.object,"id":id.to_string(),
        "editor_id":order.winner(id).and_then(soe_esm::Record::editor_id)})
}

fn state(game: &mut Game, target: GlobalFormId, item: GlobalFormId) -> Json {
    let player = game.player();
    let mut world = game.world_mut();
    let count = world.item_count(player, item);
    let seat = world
        .furniture_seat(player)
        .map(|(id, marker)| json!([id.to_string(), marker]));
    let player_cell = world
        .reference(player)
        .and_then(|r| r.cell)
        .map(|id| id.to_string());
    let reference = world.reference(target);
    json!({"inventory":count,"seat":seat,"player_cell":player_cell,
        "enabled":reference.as_ref().map(|r| r.enabled),"opened":reference.as_ref().map(|r|r.opened),
        "blocked":reference.as_ref().map(|r|r.activation_blocked),"locked":reference.as_ref().map(|r|r.locked),
        "loose_count":reference.as_ref().and_then(|r|r.item.as_ref()).map_or(0,crate::inventory::Stack::count)})
}

fn animation_asset(vfs: &soe_bsa::Vfs, record: &soe_esm::Record) -> (Vec<String>, Option<String>) {
    let Some(path) = record.find(Tag(*b"MODL")) else {
        return (Vec::new(), None);
    };
    let path = String::from_utf8_lossy(path)
        .trim_end_matches('\0')
        .to_string();
    let path = format!("meshes/{}", path.replace('\\', "/"));
    match vfs
        .read(&path)
        .map_err(|e| e.to_string())
        .and_then(|raw| soe_nif::parse(&raw).map_err(|e| e.to_string()))
        .and_then(|nif| {
            soe_nif::object_animation::ObjectAnimation::from_file(&nif).map_err(|e| e.to_string())
        }) {
        Ok(asset) => (asset.sequence_names().map(str::to_owned).collect(), None),
        Err(error) => (Vec::new(), Some(format!("{path}: {error}"))),
    }
}

fn reference_states(
    game: &Game,
    cell: GlobalFormId,
    authored: &[GlobalFormId],
) -> BTreeMap<GlobalFormId, Json> {
    let world = game.world();
    let mut ids = authored.to_vec();
    ids.extend(world.runtime_references_in_cell(cell));
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter().filter_map(|id|world.reference_ref(id).map(|reference|(id,json!({
        "base":reference.base.map(|id|id.to_string()),"enabled":reference.enabled,"opened":reference.opened,
        "position":reference.position,"rotation":reference.rotation,"deleted":reference.deleted})))) .collect()
}

fn script_calls(vfs: &soe_bsa::Vfs, names: &[String]) -> Vec<Json> {
    let mut result = Vec::new();
    let mut pending = names.to_vec();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !seen.insert(name.to_ascii_lowercase()) {
            continue;
        }
        let Ok(raw) = vfs.read(&format!("scripts/{name}.pex")) else {
            continue;
        };
        let Ok(script) = soe_papyrus::pex::parse(&raw) else {
            continue;
        };
        for object in script.objects {
            if !object.parent.is_empty()
                && !["objectreference", "form", "actor"]
                    .contains(&object.parent.to_ascii_lowercase().as_str())
            {
                pending.push(object.parent.clone());
            }
            for state in object.states {
                for function in state.functions {
                    for instruction in function.code {
                        let Some(method) = instruction
                            .args
                            .first()
                            .and_then(soe_papyrus::pex::Operand::as_ident)
                        else {
                            continue;
                        };
                        if [
                            "PlayAnimation",
                            "PlayGamebryoAnimation",
                            "PlayAnimationAndWait",
                            "Activate",
                            "GetLinkedRef",
                            "SetOpen",
                            "Enable",
                            "Disable",
                        ]
                        .iter()
                        .any(|name| method.eq_ignore_ascii_case(name))
                        {
                            result.push(json!({"script":name,"state":state.name,"function":function.name,"call":method,
                                "animation_literals":if method.to_ascii_lowercase().starts_with("play") { instruction.args.iter().skip(3).filter_map(|arg|if let soe_papyrus::pex::Operand::Str(text)=arg {Some(text)}else{None}).collect::<Vec<_>>() } else {Vec::new()}}));
                        }
                    }
                }
            }
        }
    }
    result
}

/// Run one cell in placement order. Each cell starts with fresh script and world state.
/// Script effects between references are retained, as during a player visit.
pub fn run_cell(
    order: &Arc<LoadOrder>,
    vfs: &soe_bsa::Vfs,
    cell: GlobalFormId,
    data: &Path,
) -> Result<Json, String> {
    let mut game = Game::from_shared_order(Arc::clone(order));
    let loaded = game.load_scripts_from_vfs(vfs);
    game.set_headless_animations(
        crate::anim_driver::Driver::open(data, order)
            .map_err(|error| error.to_string())?
            .objects_only(),
    );
    game.start();
    let player = game.player();
    if let Some(reference) = game.world_mut().reference(player) {
        reference.cell = Some(cell);
        reference.position = [0.0, 0.0, 500.0];
    }
    game.world_mut().prefetch_cell(cell);
    let ids = game.world_mut().cell_reference_ids(cell);
    let mut bind = crate::BindReport::default();
    for target in &ids {
        game.bind_reference(*target, &mut bind);
    }
    game.change_cell(Some(cell));
    game.run_until_idle(64);
    let mut rows = Vec::new();
    let initial_errors: Vec<_> = game
        .vm
        .log
        .entries
        .iter()
        .filter(|entry| {
            entry.message.starts_with("Headless animation ")
                || entry.message.contains("error:")
                || entry.message.contains("ERROR")
                || entry.message.contains("has no function")
        })
        .map(|entry| entry.message.clone())
        .collect();
    let initial_native_errors = game.world().native_errors.clone();
    let mut excluded = Vec::new();
    let mut assets = BTreeMap::new();
    for target in ids.iter().copied() {
        let Some(base) = game.world_mut().reference(target).and_then(|r| r.base) else {
            continue;
        };
        let Some(record) = order.winner(base).filter(|r| eligible(r)) else {
            continue;
        };
        if let Some(reference) = game.world_mut().reference(player) {
            reference.cell = Some(cell);
        }
        if !game.world().reference_enabled(target) {
            excluded.push(json!({"reference":label(order,target),"reason":"disabled or deleted"}));
            continue;
        }
        let kind = record.tag.to_string();
        let item = record
            .find(Tag(*b"PFIG"))
            .and_then(|raw| raw.try_into().ok())
            .map(u32::from_le_bytes)
            .filter(|raw| *raw != 0)
            .map_or(base, |raw| {
                order.resolve(order.winner_ref(base).map_or(0, |r| r.file), raw)
            });
        let object = game.object_of_form(target);
        let scripts = game.vm.scripts_on(object);
        let script_before: BTreeMap<_, _> = scripts
            .iter()
            .map(|script| (script.clone(), game.vm.state_of(object, script)))
            .collect();
        let calls = script_calls(vfs, &scripts);
        observe(&mut game, target)?;
        let links = game
            .world_mut()
            .reference(target)
            .map(|r| r.linked.clone())
            .unwrap_or_default();
        let linked_before: Vec<_> = links
            .iter()
            .map(|(_, id)| (id.to_string(), state(&mut game, *id, item)))
            .collect();
        let before = state(&mut game, target, item);
        let teleport = game.world_mut().reference(target).and_then(|r| r.teleport);
        let destination = teleport
            .and_then(|(far, _)| game.world_mut().reference(far).and_then(|r| r.cell))
            .map(|id| id.to_string());
        let request_start = game.world().animation_requests.len();
        let log_start = game.vm.log.entries.len();
        let native_start = game.world().native_errors.len();
        let event_start = game.world().animation_events.len();
        let world_before = reference_states(&game, cell, &ids);
        let accepted = game.activate(target, player);
        for _ in 0..12 {
            game.tick(0.25);
        }
        let after = state(&mut game, target, item);
        let script_after: BTreeMap<_, _> = scripts
            .iter()
            .map(|script| (script.clone(), game.vm.state_of(object, script)))
            .collect();
        let world_after = reference_states(&game, cell, &ids);
        let changed_references: Vec<_> = world_after.keys().chain(world_before.keys()).copied().collect::<std::collections::BTreeSet<_>>()
            .into_iter().filter(|id|world_before.get(id)!=world_after.get(id))
            .map(|id|json!({"reference":id.to_string(),"before":world_before.get(&id),"after":world_after.get(&id)})).collect();
        let emitted: Vec<_> = game.world().animation_events[event_start..]
            .iter()
            .map(|(id, name)| json!({"reference":id.to_string(),"name":name}))
            .collect();
        let linked_after: Vec<_> = links
            .iter()
            .map(|(_, id)| (id.to_string(), state(&mut game, *id, item)))
            .collect();
        let events = game
            .vm
            .get_var(object, OBSERVER, "activations")
            .map_or(0, |v| v.as_int());
        let requests: Vec<_> = game.world().animation_requests[request_start..]
            .iter()
            .map(|r| json!({"target":label(order,r.target),"name":r.animation}))
            .collect();
        let logs: Vec<_> = game.vm.log.entries[log_start..]
            .iter()
            .filter(|entry| {
                let message = entry.message.to_ascii_lowercase();
                [
                    "error",
                    "cannot",
                    "unsupported",
                    "missing",
                    "unknown",
                    "headless animation",
                ]
                .iter()
                .any(|word| message.contains(word))
            })
            .map(|e| e.message.clone())
            .collect();
        let native_errors = game.world().native_errors[native_start..].to_vec();
        let pending_waits = game
            .world()
            .animation_waiters
            .iter()
            .filter(|wait| wait.source == target)
            .count();
        let (sequences, asset_error) = assets
            .entry(base)
            .or_insert_with(|| animation_asset(vfs, record))
            .clone();
        let effect = before != after || linked_before != linked_after || !requests.is_empty();
        let state_contract = match record.tag.as_bytes() {
            b"DOOR" => destination.map_or_else(
                || after["opened"] != before["opened"],
                |id| after["player_cell"] == id,
            ),
            b"CONT" => after["opened"] == true,
            b"FURN" => after["seat"]
                .as_array()
                .is_some_and(|seat| seat.first().is_some_and(|id| *id == target.to_string())),
            b"FLOR" | b"TREE" => after["inventory"].as_i64() > before["inventory"].as_i64(),
            _ if before["loose_count"].as_i64().unwrap_or_default() > 0 => {
                after["enabled"] == false
                    && after["inventory"].as_i64().unwrap_or_default()
                        == before["inventory"].as_i64().unwrap_or_default()
                            + before["loose_count"].as_i64().unwrap_or_default()
            }
            _ => false,
        };
        let (status, cause) = if !native_errors.is_empty()
            || logs.iter().any(|line| {
                line.starts_with("Headless animation ")
                    || line.contains("error:")
                    || line.contains("ERROR")
                    || line.contains("has no function")
            }) {
            ("error", "papyrus_error")
        } else if events != 1 {
            ("wrong outcome", "activation_event")
        } else if before["locked"] == true && after["locked"] == true {
            ("pass", "locked_default_blocked")
        } else if matches!(record.tag.as_bytes(), b"FLOR" | b"TREE")
            && before["inventory"] == after["inventory"]
        {
            ("no outcome", "harvest_default_missing")
        } else if record.tag.as_bytes() == b"NPC_" {
            ("no outcome", "dialogue_requires_host")
        } else if record.tag.as_bytes() == b"FURN" && after["seat"].is_null() {
            ("no outcome", "furniture_entry_missing")
        } else if pending_waits > 0 {
            ("no outcome", "animation_wait_not_completed")
        } else if !scripts.is_empty() || !sequences.is_empty() {
            (
                if effect {
                    "wrong outcome"
                } else {
                    "no outcome"
                },
                "script_or_animation_unverified",
            )
        } else if state_contract {
            ("pass", "state_changed")
        } else {
            ("no outcome", "no_observable_effect")
        };
        rows.push(json!({"reference":label(order,target),"base":label(order,base),"base_type":kind,
            "status":status,"cause":cause,"accepted":accepted,"on_activate":events,
            "scripts":scripts,"script_calls":calls,"sequences":sequences,"asset_error":asset_error,
            "model":record.find(Tag(*b"MODL")).map(|raw|String::from_utf8_lossy(raw).trim_end_matches('\0').to_string()),
            "animation_requests":requests,"before":before,"after":after,
            "state_contract":state_contract,"pending_animation_waits":pending_waits,
            "changed_references":changed_references,"script_state_before":script_before,"script_state_after":script_after,"animation_events":emitted,
            "linked_before":linked_before,"linked_after":linked_after,"logs":logs,"native_errors":native_errors,
            "visual":{"status":"not run"}}));
    }
    Ok(
        json!({"cell":label(order,cell),"references":rows,"excluded":excluded,
        "initial":{"papyrus_errors":initial_errors,"native_errors":initial_native_errors},
        "scripts_loaded":loaded.scripts_loaded,"script_load_errors":loaded.scripts_failed,
        "missing_scripts":bind.scripts_missing.iter().map(|(s,id)|json!([s,id.to_string()])).collect::<Vec<_>>()}),
    )
}

/// Count outcomes by base type and cause without hiding unverified contracts.
pub fn counts(cells: &[Json]) -> Json {
    let mut types: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut causes: BTreeMap<String, usize> = BTreeMap::new();
    for row in cells
        .iter()
        .filter_map(|c| c["references"].as_array())
        .flatten()
    {
        *types
            .entry(row["base_type"].as_str().unwrap_or_default().into())
            .or_default()
            .entry(row["status"].as_str().unwrap_or_default().into())
            .or_default() += 1;
        *causes
            .entry(row["cause"].as_str().unwrap_or_default().into())
            .or_default() += 1;
    }
    json!({"by_base_type":types,"by_cause":causes})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_sweep_carry_flag_and_tree_produce_bound_eligibility() {
        let light = soe_esm::Record::new(Tag(*b"LIGH"), 42).sub(b"DATA", [0; 16]);
        assert!(!eligible(&light));
        let mut data = [0; 16];
        data[12] = 2;
        assert!(eligible(
            &soe_esm::Record::new(Tag(*b"LIGH"), 42).sub(b"DATA", data)
        ));
        assert!(!eligible(&soe_esm::Record::new(Tag(*b"TREE"), 42)));
        assert!(eligible(
            &soe_esm::Record::new(Tag(*b"TREE"), 42).sub(b"PFIG", 42_u32.to_le_bytes())
        ));
    }
}
