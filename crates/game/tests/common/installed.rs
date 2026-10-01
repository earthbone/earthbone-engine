// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared installed master load orders for the single integration test binary.

#![allow(dead_code, clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

use soe_esm::{LoadOrder, Plugin};
use soe_game::Game;

type CachedOrders = HashMap<Vec<String>, Result<Arc<LoadOrder>, String>>;
type CachedPlugins = HashMap<(PathBuf, String), Result<Plugin, String>>;

static ORDERS: OnceLock<Mutex<CachedOrders>> = OnceLock::new();
static PLUGINS: OnceLock<Mutex<CachedPlugins>> = OnceLock::new();

/// Return the installed data directory when this test process has one.
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
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
    let entry = cache.entry(key).or_insert_with(|| {
        let path = data.join(name);
        soe_esm::read_plugin(&path)
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
