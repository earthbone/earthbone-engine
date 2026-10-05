// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared installed master load orders for the single integration test binary.

#![allow(dead_code, clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

use soe_esm::{GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::Game;

type CachedOrders = HashMap<Vec<String>, Result<Arc<LoadOrder>, String>>;
type CachedPlugins = HashMap<(PathBuf, String), Result<Plugin, String>>;

static ORDERS: OnceLock<Mutex<CachedOrders>> = OnceLock::new();
static PLUGINS: OnceLock<Mutex<CachedPlugins>> = OnceLock::new();

/// Placed containers with fixed CNTO items for equality checks across resets.
/// LVLI contents are selected again on reset and can differ from the first draw.
pub fn fixed_container_references(order: &LoadOrder) -> impl Iterator<Item = GlobalFormId> + '_ {
    order.winners_of(Tag(*b"REFR")).filter_map(|(id, record)| {
        let raw = u32::from_le_bytes(record.find(Tag(*b"NAME"))?.try_into().ok()?);
        let base = order.resolve(order.winner_ref(id)?.file, raw);
        let base_ref = order.winner_ref(base)?;
        let base_record = order.record(base_ref);
        (base_record.tag == Tag(*b"CONT")
            && !record.is_deleted()
            && !base_record.is_deleted()
            && base_record.find(Tag(*b"CNTO")).is_some()
            && base_record.all(Tag(*b"CNTO")).all(|cnto| {
                let Some(raw) = cnto
                    .get(..4)
                    .and_then(|bytes| bytes.try_into().ok())
                    .map(u32::from_le_bytes)
                else {
                    return false;
                };
                order
                    .winner(order.resolve(base_ref.file, raw))
                    .is_some_and(|item| item.tag != Tag(*b"LVLI"))
            }))
        .then_some(id)
    })
}

/// Return the installed data directory when this test process has one.
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

fn plugin_cache_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("SOE_PLUGIN_CACHE") {
        return PathBuf::from(path);
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    target.join("soe-plugin-cache").join("game-installed")
}

/// Return a shared parsed load order, caching each ordered master set once.
pub fn load_order(masters: &[&str]) -> Option<Arc<LoadOrder>> {
    data_dir()?;
    let key: Vec<String> = masters
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    let cache = ORDERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().expect("installed order cache lock poisoned");
    let inserted = !cache.contains_key(&key);
    let entry = cache.entry(key).or_insert_with(|| {
        let mut order = LoadOrder::new();
        for name in masters {
            let plugin = plugin(name).expect("installed data directory was checked");
            order
                .add(name, plugin)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
        }
        Ok(Arc::new(order))
    });
    if inserted && entry.is_ok() {
        eprintln!(
            "tst_14: parsed installed master set once ({} masters)",
            masters.len()
        );
    }
    Some(
        entry
            .as_ref()
            .unwrap_or_else(|error| panic!("installed load order: {error}"))
            .clone(),
    )
}

/// Build a mutable load order from the cached parsed plugins.
pub fn owned_order(masters: &[&str]) -> Option<LoadOrder> {
    data_dir()?;
    let mut order = LoadOrder::new();
    for name in masters {
        let plugin = plugin(name).expect("installed data directory was checked");
        order
            .add(name, plugin)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    Some(order)
}

/// Build a fresh game and world over the cached immutable form table.
pub fn game(masters: &[&str]) -> Option<Game> {
    load_order(masters).map(Game::from_shared_order)
}

/// Parse one plugin for a test that needs a standalone installed record.
pub fn plugin(name: &str) -> Option<Plugin> {
    let data = data_dir()?;
    let key = (data.clone(), name.to_ascii_lowercase());
    let cache = PLUGINS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().expect("installed plugin cache lock poisoned");
    let cache_dir = plugin_cache_dir();
    let entry = cache.entry(key).or_insert_with(|| {
        let path = data.join(name);
        soe_esm::read_plugin_cached(&path, &cache_dir)
            .map_err(|error| format!("{}: {error}", path.display()))?
            .map_err(|error| format!("{name}: {error}"))
    });
    Some(
        entry
            .as_ref()
            .unwrap_or_else(|error| panic!("{error}"))
            .clone(),
    )
}
