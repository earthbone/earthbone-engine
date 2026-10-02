// SPDX-License-Identifier: GPL-3.0-or-later

//! WebAssembly component mods using the versioned WIT interface in
//! `wit/mod-api.wit`, embedded with Wasmtime's component model.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use soe_esm::GlobalFormId;
use thiserror::Error;
use wasmtime::component::{Component, Instance, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

const FUEL_PER_CALL: u64 = 100_000;

/// A host operation requested by a component callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Apply a quest stage through the game's shared quest and Papyrus state.
    SetStage(GlobalFormId, u16),
}

/// Errors returned while loading or running a component mod.
#[derive(Debug, Error)]
pub enum Error {
    /// The component could not be compiled, linked, or invoked.
    #[error("WebAssembly mod error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
    /// The component does not implement a required ABI export.
    #[error("WebAssembly mod is missing export `{0}`")]
    MissingExport(&'static str),
}

struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    subscriptions: BTreeSet<String>,
    commands: Arc<Mutex<Vec<Command>>>,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

struct ModInstance {
    store: Store<HostState>,
    instance: Instance,
    commands: Arc<Mutex<Vec<Command>>>,
}

/// Loaded component mods and their event subscriptions.
pub struct WasmMods {
    engine: Option<Engine>,
    instances: Vec<ModInstance>,
}

impl WasmMods {
    /// Create an empty component-mod host.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: None,
            instances: Vec::new(),
        }
    }

    /// Compile, instantiate, and initialize one component against the game world.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if self.engine.is_none() {
            let mut config = wasmtime::Config::new();
            config.consume_fuel(true);
            self.engine = Some(Engine::new(&config)?);
        }
        let engine = self.engine.as_ref().ok_or_else(|| {
            wasmtime::Error::msg("WebAssembly component engine initialization failed")
        })?;
        let component = Component::new(engine, bytes)?;
        let mut linker = Linker::<HostState>::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        let mut engine_api = linker.instance("soe:mod-api/engine@0.1.0")?;
        engine_api.func_wrap(
            "subscribe-on-activate",
            |mut store, (): ()| -> wasmtime::Result<(bool,)> {
                store.data_mut().subscriptions.insert("OnActivate".into());
                Ok((true,))
            },
        )?;
        engine_api.func_wrap(
            "set-stage",
            |store, (file, object, stage): (u32, u32, u16)| -> wasmtime::Result<(bool,)> {
                let Ok(file) = u16::try_from(file) else {
                    return Ok((false,));
                };
                let quest = GlobalFormId { file, object };
                let commands = Arc::clone(&store.data().commands);
                let Ok(mut commands) = commands.lock() else {
                    return Ok((false,));
                };
                commands.push(Command::SetStage(quest, stage));
                Ok((true,))
            },
        )?;

        let commands = Arc::new(Mutex::new(Vec::new()));
        let wasi = WasiCtx::builder().build();
        let mut store = Store::new(
            engine,
            HostState {
                wasi,
                table: ResourceTable::new(),
                subscriptions: BTreeSet::new(),
                commands: Arc::clone(&commands),
            },
        );
        store.set_fuel(FUEL_PER_CALL)?;
        let instance = linker.instantiate(&mut store, &component)?;
        call_export(&mut store, &instance, "initialize", &[])?;
        if let Ok(mut pending) = commands.lock() {
            pending.clear();
        }
        self.instances.push(ModInstance {
            store,
            instance,
            commands,
        });
        Ok(())
    }

    /// Invoke `on-activate` for components subscribed to that event.
    pub fn activate(&mut self, target: GlobalFormId, activator: GlobalFormId) -> Vec<Command> {
        let mut commands = Vec::new();
        for component in &mut self.instances {
            if !component.store.data().subscriptions.contains("OnActivate") {
                continue;
            }
            if component.store.set_fuel(FUEL_PER_CALL).is_err() {
                continue;
            }
            if let Ok(mut pending) = component.commands.lock() {
                pending.clear();
            }
            let params = [
                wasmtime::component::Val::U32(u32::from(target.file)),
                wasmtime::component::Val::U32(target.object),
                wasmtime::component::Val::U32(u32::from(activator.file)),
                wasmtime::component::Val::U32(activator.object),
            ];
            if call_export(
                &mut component.store,
                &component.instance,
                "on-activate",
                &params,
            )
            .is_ok()
            {
                if let Ok(mut pending) = component.commands.lock() {
                    commands.append(&mut pending);
                }
            } else if let Ok(mut pending) = component.commands.lock() {
                pending.clear();
            }
        }
        commands
    }
}

impl Default for WasmMods {
    fn default() -> Self {
        Self::new()
    }
}

fn call_export(
    store: &mut Store<HostState>,
    instance: &Instance,
    name: &'static str,
    params: &[wasmtime::component::Val],
) -> Result<(), Error> {
    let func = instance
        .get_func(&mut *store, name)
        .ok_or(Error::MissingExport(name))?;
    let mut results = [];
    func.call(&mut *store, params, &mut results)?;
    func.post_return(store)?;
    Ok(())
}
