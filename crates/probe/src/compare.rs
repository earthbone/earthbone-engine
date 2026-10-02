// SPDX-License-Identifier: GPL-3.0-or-later

//! Structured log readback and paired execution through Game's existing VM.
//! Request IDs and explicit results follow tes4skyrim game_bridge/docs/protocol.md.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};
use soe_esm::{LoadOrder, Plugin, Tag};
use soe_game::Game;
use soe_papyrus::{asm, NativeReturn, Value};

use crate::{runner::SCRIPT, Error, FormKind, Probe};

/// One boundary sample emitted by Debug.Trace. Boolean results use zero or one.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub schema: u32,
    pub sequence: String,
    pub request_id: String,
    pub phase: String,
    pub runtime_version: String,
    pub skse_version: Option<String>,
    pub load_order: Vec<String>,
    pub forms: BTreeMap<String, i32>,
    pub timescale: f64,
    pub real_seconds: f64,
    pub game_seconds: f64,
    pub paused: u8,
    pub result: BTreeMap<String, f64>,
}

impl Observation {
    fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || !["begin", "end"].contains(&self.phase.as_str())
            || self.sequence.is_empty()
            || self.request_id.is_empty()
            || self.runtime_version.is_empty()
            || self.paused > 1
            || self.timescale <= 0.0
            || self.real_seconds < 0.0
            || self.game_seconds < 0.0
        {
            return Err("invalid observation context".into());
        }
        if !self.timescale.is_finite()
            || !self.real_seconds.is_finite()
            || !self.game_seconds.is_finite()
            || self.result.values().any(|n| !n.is_finite())
        {
            return Err("non-finite observation number".into());
        }
        Ok(())
    }
}

/// Parse only SOEPROBE trace lines from a Papyrus log, preserving their order.
///
/// # Errors
/// Rejects malformed marker lines, unknown fields, duplicate boundaries or empty logs.
pub fn parse_log(log: &str) -> Result<Vec<Observation>, Error> {
    let mut observations = Vec::new();
    let mut seen = BTreeSet::new();
    for (line, text) in log.lines().enumerate() {
        let Some((_, json)) = text.split_once("SOEPROBE ") else {
            continue;
        };
        let invalid = |reason: String| Error::Observation {
            line: line + 1,
            reason,
        };
        let observation: Observation =
            serde_json::from_str(json).map_err(|error| invalid(error.to_string()))?;
        observation.validate().map_err(invalid)?;
        if !seen.insert((
            observation.sequence.clone(),
            observation.request_id.clone(),
            observation.phase.clone(),
        )) {
            return Err(invalid(
                "duplicate request boundary, isolate one run per log".into(),
            ));
        }
        observations.push(observation);
    }
    if observations.is_empty() {
        return Err(Error::Observation {
            line: 0,
            reason: "no SOEPROBE observations".into(),
        });
    }
    Ok(observations)
}

/// A difference retains both values, its request boundary and the declared tolerance.
#[derive(Debug, Serialize)]
pub struct Difference {
    pub request_id: String,
    pub phase: String,
    pub field: String,
    pub engine: serde_json::Value,
    pub observed: serde_json::Value,
    pub tolerance: f64,
}

/// Runtime versions are provenance. All other fields participate in comparison.
#[derive(Debug, Serialize)]
pub struct Comparison {
    pub matched: bool,
    pub engine_version: String,
    pub observed_version: String,
    pub differences: Vec<Difference>,
}

fn diff(
    report: &mut Comparison,
    boundary: &Observation,
    field: &str,
    engine: serde_json::Value,
    observed: serde_json::Value,
    tolerance: f64,
) {
    let same = match (engine.as_f64(), observed.as_f64()) {
        (Some(a), Some(b)) => (a - b).abs() <= tolerance,
        _ => engine == observed,
    };
    if !same {
        report.differences.push(Difference {
            request_id: boundary.request_id.clone(),
            phase: boundary.phase.clone(),
            field: field.into(),
            engine,
            observed,
            tolerance,
        });
    }
}

/// Compare ordered boundaries with exact identity/context checks and numeric tolerances.
///
/// # Errors
/// Rejects invalid authored files, invalid observations or engine output with the wrong shape.
pub fn compare(
    probe: &Probe,
    engine: &[Observation],
    observed: &[Observation],
) -> Result<Comparison, Error> {
    probe.validate()?;
    let expected: Vec<_> = probe
        .steps
        .iter()
        .flat_map(|step| {
            let mut boundaries = Vec::new();
            if step.waits() {
                boundaries.push((step.id(), "begin"));
            }
            boundaries.push((step.id(), "end"));
            boundaries
        })
        .collect();
    if engine.len() != expected.len()
        || engine.iter().zip(&expected).any(|(o, (id, phase))| {
            o.request_id != *id || o.phase != *phase || o.sequence != probe.name
        })
    {
        return Err(Error::Execution(
            "engine observations do not match the authored sequence".into(),
        ));
    }
    for o in engine.iter().chain(observed) {
        o.validate()
            .map_err(|reason| Error::Observation { line: 0, reason })?;
    }
    let engine_version = engine
        .first()
        .map_or_else(String::new, |o| o.runtime_version.clone());
    if engine.iter().any(|o| o.runtime_version != engine_version) {
        return Err(Error::Execution(
            "engine runtime version changed within the run".into(),
        ));
    }
    let observed_version = observed
        .first()
        .map_or_else(String::new, |o| o.runtime_version.clone());
    let mut report = Comparison {
        matched: false,
        engine_version,
        observed_version,
        differences: Vec::new(),
    };
    let forms = probe.all_forms();
    for (index, e) in engine.iter().enumerate() {
        diff(
            &mut report,
            e,
            "authored.load_order",
            serde_json::to_value(&probe.load_order)?,
            serde_json::to_value(&e.load_order)?,
            0.0,
        );
        diff(
            &mut report,
            e,
            "authored.forms.aliases",
            serde_json::to_value(forms.keys().collect::<Vec<_>>())?,
            serde_json::to_value(e.forms.keys().collect::<Vec<_>>())?,
            0.0,
        );
        for (alias, form) in &forms {
            let object = e.forms.get(alias).map(|id| {
                let raw = *id as u32;
                raw & if raw >> 24 == 0xfe { 0xfff } else { 0xff_ffff }
            });
            diff(
                &mut report,
                e,
                &format!("authored.forms.{alias}.object"),
                serde_json::json!(form.object),
                serde_json::to_value(object)?,
                0.0,
            );
        }
        let Some(o) = observed.get(index) else {
            diff(
                &mut report,
                e,
                "boundary",
                serde_json::to_value(e)?,
                serde_json::Value::Null,
                0.0,
            );
            continue;
        };
        let ev = serde_json::to_value(e)?;
        let ov = serde_json::to_value(o)?;
        for field in [
            "schema",
            "sequence",
            "request_id",
            "phase",
            "skse_version",
            "load_order",
            "forms",
            "paused",
        ] {
            diff(
                &mut report,
                e,
                field,
                ev[field].clone(),
                ov[field].clone(),
                0.0,
            );
        }
        for (field, tolerance) in [
            ("real_seconds", probe.tolerances.real_seconds),
            ("game_seconds", probe.tolerances.game_seconds),
            ("timescale", probe.tolerances.timescale),
        ] {
            diff(
                &mut report,
                e,
                field,
                ev[field].clone(),
                ov[field].clone(),
                tolerance,
            );
        }
        // CONTEXT: Runtime versions identify the two different implementations.
        let version = serde_json::json!(report.observed_version);
        diff(
            &mut report,
            e,
            "runtime_version",
            version,
            serde_json::json!(o.runtime_version),
            0.0,
        );
        let keys: BTreeSet<_> = e.result.keys().chain(o.result.keys()).collect();
        for key in keys {
            diff(
                &mut report,
                e,
                &format!("result.{key}"),
                serde_json::to_value(e.result.get(key))?,
                serde_json::to_value(o.result.get(key))?,
                probe.tolerances.result.get(key).copied().unwrap_or(0.0),
            );
        }
    }
    for o in observed.iter().skip(engine.len()) {
        diff(
            &mut report,
            o,
            "unexpected_boundary",
            serde_json::Value::Null,
            serde_json::to_value(o)?,
            0.0,
        );
    }
    report.matched = report.differences.is_empty();
    Ok(report)
}

fn kind_matches(kind: FormKind, tag: Tag) -> bool {
    match kind {
        FormKind::Quest => tag == Tag(*b"QUST"),
        FormKind::Actor | FormKind::Player => tag == Tag(*b"ACHR"),
        FormKind::ObjectReference => [Tag(*b"REFR"), Tag(*b"ACHR")].contains(&tag),
        FormKind::Spell => tag == Tag(*b"SPEL"),
        FormKind::MagicEffect => tag == Tag(*b"MGEF"),
        FormKind::GlobalVariable => tag == Tag(*b"GLOB"),
        FormKind::Form => true,
    }
}

/// Run the generated runner through soe-game over an existing external load order.
/// The order excludes SoeProbe.esp, which is generated and appended here.
///
/// # Errors
/// Rejects a different load order, missing/mistyped forms, VM faults or unfinished runs.
pub fn run(probe: &Probe, order: LoadOrder) -> Result<Vec<Observation>, Error> {
    if probe.animation_graph.is_some() {
        return Err(Error::Execution(
            "animation comparison requires installed assets".into(),
        ));
    }
    run_with_assets(probe, order, None)
}

fn run_with_assets(
    probe: &Probe,
    mut order: LoadOrder,
    assets: Option<&crate::animation::Assets>,
) -> Result<Vec<Observation>, Error> {
    probe.validate()?;
    if !order.files().iter().map(|file| file.name.as_str()).eq(probe
        .load_order
        .iter()
        .take(probe.load_order.len() - 1)
        .map(String::as_str))
    {
        return Err(Error::Invalid(
            "engine load order differs from authored load_order".into(),
        ));
    }
    order
        .add("SoeProbe.esp", crate::plugin(probe)?)
        .map_err(|e| Error::Execution(e.to_string()))?;
    let mut game = Game::new(order);
    let mut resolved = BTreeMap::new();
    for (name, form) in probe.all_forms() {
        if form.kind == FormKind::Player {
            resolved.insert(name, game.world_mut().player());
            continue;
        }
        let id = game
            .world()
            .form_from_file(form.object, &form.plugin)
            .ok_or_else(|| Error::Execution(format!("missing form {name}")))?;
        if !game
            .world()
            .tag_of(id)
            .is_some_and(|tag| kind_matches(form.kind, tag))
        {
            return Err(Error::Execution(format!("mistyped form {name}")));
        }
        resolved.insert(name, id);
    }
    let timescale = game
        .world()
        .form_from_file(0x3a, "Skyrim.esm")
        .ok_or_else(|| Error::Execution("missing TimeScale".into()))?;
    game.world_mut().set_global(timescale, probe.timescale);
    let mut animation = match (&probe.animation_graph, assets) {
        (Some(config), Some(assets)) => {
            let actor = resolved[&config.actor];
            Some(crate::animation::Host::new(assets, actor))
        }
        _ => None,
    };
    if let Some(host) = &mut animation {
        host.update(&mut game, 0.0)?;
    }
    let version = format!("soe-game/{}", env!("CARGO_PKG_VERSION"));
    if !game.vm.natives.register_global(
        "Debug",
        "GetVersionNumber",
        Box::new(move |_| NativeReturn::Value(Value::string(&version))),
    ) {
        return Err(Error::Execution(
            "version provider already registered".into(),
        ));
    }
    game.load_script(&asm::write(&crate::script(probe)?))
        .map_err(|e| Error::Execution(e.to_string()))?;
    game.bind();
    let runner = game
        .form("SoeProbe")
        .ok_or_else(|| Error::Execution("missing generated quest".into()))?;
    if !game.start_quest(runner) {
        return Err(Error::Execution("runner quest did not start".into()));
    }
    let mut elapsed = 0.0;
    let mut menu_index = 0;
    let mut paused = false;
    while elapsed <= probe.timeout_seconds {
        while probe
            .menus
            .get(menu_index)
            .is_some_and(|menu| menu.at <= elapsed)
        {
            paused = probe.menus[menu_index].paused;
            menu_index += 1;
        }
        game.tick_menu_mode(0.0, paused);
        game.run_until_idle(64);
        let log = game.traces();
        if let Some(error) = log.iter().find(|line| {
            line.starts_with("error:")
                || line.contains("SoeProbe missing")
                || line.contains("unsupported")
        }) {
            return Err(Error::Execution(error.clone()));
        }
        let count = log
            .iter()
            .filter(|line| line.starts_with("SOEPROBE "))
            .count();
        let expected = probe
            .steps
            .iter()
            .map(|step| if step.waits() { 2 } else { 1 })
            .sum::<usize>();
        if count >= expected {
            let observations = parse_log(&log.join("\n"))?;
            compare(probe, &observations, &observations)?;
            return Ok(observations);
        }
        if elapsed >= probe.timeout_seconds {
            break;
        }
        let next_menu = probe
            .menus
            .get(menu_index)
            .map_or(probe.timeout_seconds, |menu| menu.at);
        let dt = probe
            .tick_seconds
            .min(next_menu - elapsed)
            .min(probe.timeout_seconds - elapsed);
        if dt <= 0.0 {
            return Err(Error::Execution("clock made no progress".into()));
        }
        if !paused {
            if let Some(host) = &mut animation {
                host.update(&mut game, dt as f32)?;
            }
        }
        game.tick_menu_mode(dt, paused);
        elapsed += dt;
    }
    Err(Error::Execution(format!(
        "{SCRIPT} exceeded {} real seconds",
        probe.timeout_seconds
    )))
}

pub(crate) fn installed_order(probe: &Probe, data: &Path) -> Result<LoadOrder, Error> {
    let mut order = LoadOrder::new();
    for name in probe.load_order.iter().take(probe.load_order.len() - 1) {
        let bytes = std::fs::read(data.join(name))?;
        let plugin = Plugin::parse(&bytes).map_err(|e| Error::Execution(format!("{name}: {e}")))?;
        order
            .add(name, plugin)
            .map_err(|e| Error::Execution(e.to_string()))?;
    }
    Ok(order)
}

/// Run the authored load order from an installed Data directory.
///
/// # Errors
/// Reports missing plugins, malformed records or execution errors.
pub fn run_installed(probe: &Probe, data: &Path) -> Result<Vec<Observation>, Error> {
    probe.validate()?;
    let assets = crate::animation::Assets::load(probe, data)?;
    run_with_assets(probe, installed_order(probe, data)?, assets.as_ref())
}
