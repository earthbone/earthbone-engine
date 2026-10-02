// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded authored sequences, using CK Quest, Utility, Debug, Actor and Form natives.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::Error;

/// A sequence and the conditions under which it should be repeated.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    pub name: String,
    pub load_order: Vec<String>,
    pub timescale: f32,
    pub tick_seconds: f64,
    pub timeout_seconds: f64,
    pub tolerances: Tolerances,
    #[serde(default)]
    pub forms: BTreeMap<String, Form>,
    #[serde(default)]
    pub menus: Vec<Menu>,
    pub steps: Vec<Step>,
}

/// Absolute tolerances, established in the authored file before execution.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tolerances {
    pub real_seconds: f64,
    pub game_seconds: f64,
    pub timescale: f64,
    #[serde(default)]
    pub result: BTreeMap<String, f64>,
}

/// A stable identity. Object IDs omit the runtime load-order prefix.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Form {
    pub plugin: String,
    pub object: u32,
    pub kind: FormKind,
}

/// Papyrus types accepted by the command set.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormKind {
    Quest,
    Actor,
    ObjectReference,
    Spell,
    MagicEffect,
    GlobalVariable,
    Form,
}

impl FormKind {
    pub(crate) fn papyrus(self) -> &'static str {
        match self {
            Self::Quest => "Quest",
            Self::Actor => "Actor",
            Self::ObjectReference => "ObjectReference",
            Self::Spell => "Spell",
            Self::MagicEffect => "MagicEffect",
            Self::GlobalVariable => "GlobalVariable",
            Self::Form => "Form",
        }
    }
}

/// A host menu-state change at sequence-relative real seconds.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Menu {
    pub at: f64,
    pub paused: bool,
}

/// Each command has a unique request ID and produces an end observation.
/// Waits also produce a begin observation with the same ID.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    SetStage {
        id: String,
        quest: String,
        stage: u16,
    },
    ResetQuest {
        id: String,
        quest: String,
    },
    QuestStage {
        id: String,
        quest: String,
    },
    WaitReal {
        id: String,
        seconds: f32,
    },
    WaitMenu {
        id: String,
        seconds: f32,
    },
    WaitGame {
        id: String,
        seconds: f32,
    },
    Animation {
        id: String,
        actor: String,
        event: String,
    },
    WaitAnimation {
        id: String,
        actor: String,
        event: String,
    },
    Cast {
        id: String,
        spell: String,
        source: String,
        target: String,
    },
    ActorValue {
        id: String,
        actor: String,
        value: String,
    },
    HasEffect {
        id: String,
        actor: String,
        effect: String,
    },
    FormProperty {
        id: String,
        form: String,
        property: String,
    },
    Save {
        id: String,
    },
}

impl Step {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::SetStage { id, .. }
            | Self::ResetQuest { id, .. }
            | Self::QuestStage { id, .. }
            | Self::WaitReal { id, .. }
            | Self::WaitMenu { id, .. }
            | Self::WaitGame { id, .. }
            | Self::Animation { id, .. }
            | Self::WaitAnimation { id, .. }
            | Self::Cast { id, .. }
            | Self::ActorValue { id, .. }
            | Self::HasEffect { id, .. }
            | Self::FormProperty { id, .. }
            | Self::Save { id } => id,
        }
    }

    pub(crate) fn waits(&self) -> bool {
        matches!(
            self,
            Self::WaitReal { .. }
                | Self::WaitMenu { .. }
                | Self::WaitGame { .. }
                | Self::WaitAnimation { .. }
        )
    }
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Probe {
    /// Read and validate an authored TOML sequence.
    ///
    /// # Errors
    /// Rejects malformed TOML, unknown fields and invalid command arguments.
    pub fn parse(source: &str) -> Result<Self, Error> {
        let probe: Self = toml::from_str(source)?;
        probe.validate()?;
        Ok(probe)
    }

    pub(crate) fn all_forms(&self) -> BTreeMap<String, Form> {
        let mut forms = self.forms.clone();
        forms.insert(
            "runner".into(),
            Form {
                plugin: "SoeProbe.esp".into(),
                object: 0x800,
                kind: FormKind::Quest,
            },
        );
        forms.insert(
            "timescale".into(),
            Form {
                plugin: "Skyrim.esm".into(),
                object: 0x3a,
                kind: FormKind::GlobalVariable,
            },
        );
        forms
    }

    /// Validate counts, identities, typed command arguments and tolerances.
    ///
    /// # Errors
    /// Returns the invalid field or request ID without building any output.
    pub fn validate(&self) -> Result<(), Error> {
        let invalid = |why: &str| Error::Invalid(why.into());
        if !token(&self.name)
            || self.steps.is_empty()
            || self.steps.len() > 64
            || self.forms.len() > 32
        {
            return Err(invalid("name or sequence size"));
        }
        if self.forms.contains_key("runner") || self.forms.contains_key("timescale") {
            return Err(invalid("runner and timescale are reserved form names"));
        }
        let mut plugins = BTreeSet::new();
        for name in &self.load_order {
            if name.len() > 128
                || name.contains(['"', '\\', '/', '\n', '\r'])
                || !name.is_ascii()
                || ![".esm", ".esp", ".esl"]
                    .iter()
                    .any(|ext| name.to_lowercase().ends_with(ext))
                || !plugins.insert(name.to_lowercase())
            {
                return Err(invalid("load_order plugin name"));
            }
        }
        let masters = [
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
        ];
        if self.load_order.len() > 254
            || !self
                .load_order
                .iter()
                .take(5)
                .map(String::as_str)
                .eq(masters)
            || self.load_order.last().map(String::as_str) != Some("SoeProbe.esp")
        {
            return Err(invalid(
                "load_order must begin with the five masters and end with SoeProbe.esp",
            ));
        }
        if !self.timescale.is_finite()
            || self.timescale <= 0.0
            || !self.tick_seconds.is_finite()
            || self.tick_seconds <= 0.0
            || !self.timeout_seconds.is_finite()
            || self.timeout_seconds < self.tick_seconds
            || self.timeout_seconds / self.tick_seconds > 1_000_000.0
        {
            return Err(invalid("clock configuration"));
        }
        for v in [
            self.tolerances.real_seconds,
            self.tolerances.game_seconds,
            self.tolerances.timescale,
        ]
        .into_iter()
        .chain(self.tolerances.result.values().copied())
        {
            if !v.is_finite() || v < 0.0 {
                return Err(invalid("tolerances must be finite and nonnegative"));
            }
        }
        if self
            .tolerances
            .result
            .keys()
            .any(|key| !["accepted", "stage", "issued", "value"].contains(&key.as_str()))
        {
            return Err(invalid("unknown result tolerance"));
        }
        let forms = self.all_forms();
        if self.load_order.iter().any(|plugin| {
            !forms
                .values()
                .any(|form| form.plugin.eq_ignore_ascii_case(plugin))
        }) {
            return Err(invalid(
                "every declared plugin needs a resolved form anchor",
            ));
        }
        let mut aliases = BTreeSet::new();
        for (name, form) in &forms {
            if !token(name)
                || !aliases.insert(name.to_ascii_lowercase())
                || form.object == 0
                || form.object > 0xffffff
                || !plugins.contains(&form.plugin.to_lowercase())
            {
                return Err(invalid("form name, plugin or local object ID"));
            }
        }
        let check = |name: &str, kinds: &[FormKind]| -> Result<(), Error> {
            if forms
                .get(name)
                .is_some_and(|form| kinds.is_empty() || kinds.contains(&form.kind))
            {
                Ok(())
            } else {
                Err(Error::Invalid(format!("unknown or mistyped form {name}")))
            }
        };
        let reference = [FormKind::Actor, FormKind::ObjectReference];
        let mut ids = BTreeSet::new();
        for step in &self.steps {
            if !token(step.id()) || !ids.insert(step.id()) {
                return Err(invalid("request IDs must be unique tokens"));
            }
            match step {
                Step::SetStage { quest, .. }
                | Step::ResetQuest { quest, .. }
                | Step::QuestStage { quest, .. } => check(quest, &[FormKind::Quest])?,
                Step::WaitReal { seconds, .. }
                | Step::WaitMenu { seconds, .. }
                | Step::WaitGame { seconds, .. } => {
                    if !seconds.is_finite() || *seconds <= 0.0 {
                        return Err(invalid("wait seconds must be positive and finite"));
                    }
                    if matches!(step, Step::WaitGame { .. }) && *seconds / 3600.0 == 0.0 {
                        return Err(invalid(
                            "game wait seconds underflow the native hour argument",
                        ));
                    }
                }
                Step::Animation { actor, event, .. } | Step::WaitAnimation { actor, event, .. } => {
                    check(actor, &reference)?;
                    if !token(event) {
                        return Err(invalid("animation event"));
                    }
                }
                Step::Cast {
                    spell,
                    source,
                    target,
                    ..
                } => {
                    check(spell, &[FormKind::Spell])?;
                    check(source, &reference)?;
                    check(target, &reference)?;
                }
                Step::ActorValue { actor, value, .. } => {
                    check(actor, &[FormKind::Actor])?;
                    if soe_game::actor_values::actor_value_index(value).is_none() {
                        return Err(invalid("unknown actor value"));
                    }
                }
                Step::HasEffect { actor, effect, .. } => {
                    check(actor, &[FormKind::Actor])?;
                    check(effect, &[FormKind::MagicEffect])?;
                }
                Step::FormProperty { form, property, .. } => {
                    check(form, &[])?;
                    if !["type", "form_id", "global_value"].contains(&property.as_str()) {
                        return Err(invalid("unsupported form property"));
                    }
                    if property == "global_value" {
                        check(form, &[FormKind::GlobalVariable])?;
                    }
                }
                Step::Save { .. } => {}
            }
        }
        let mut last = -1.0;
        if self.menus.len() > 128 {
            return Err(invalid("too many menu changes"));
        }
        for menu in &self.menus {
            if !menu.at.is_finite()
                || menu.at < 0.0
                || menu.at <= last
                || menu.at >= self.timeout_seconds
            {
                return Err(invalid("menu changes must be ordered within the timeout"));
            }
            last = menu.at;
        }
        Ok(())
    }
}
