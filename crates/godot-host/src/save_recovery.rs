// SPDX-License-Identifier: GPL-3.0-or-later

//! Explicit host choice for vanilla ESS recovery. Dependency checks use the
//! game save report, while accepted sessions use the native generation store.
//! ESS type identities follow UESP's `Save_File_Format.wikitext` and
//! FallrimTools' `ChangeForm.java`. xEdit leaves projectile subtypes opaque.
//! CommonLib's `GrenadeProjectile.h` identifies PGRE as a projectile subtype.

use std::{borrow::Cow, collections::BTreeMap};

use soe_game::{
    save::{DependencyChange, DependencyFinding, DependencyReport},
    save_generations::{GenerationStore, GenerationWrite},
    Game,
};
use soe_save::Save;

/// An accepted vanilla recovery session written to a distinct native generation.
#[derive(Debug)]
pub struct AcceptedVanillaSession {
    pub report: DependencyReport,
    pub generation: GenerationWrite,
    /// Transient arrows, grenade projectiles and hazards omitted from recovery.
    /// Their original payloads remain in the source ESS.
    pub skipped_transient: BTreeMap<&'static str, usize>,
}

/// Why a vanilla ESS cannot start as an accepted recovery session.
#[derive(Debug, thiserror::Error)]
pub enum VanillaRecoveryError {
    #[error("could not inspect vanilla save dependencies: {0}")]
    Preflight(String),
    #[error("vanilla recovery refused: {reasons:?}")]
    Refused {
        report: DependencyReport,
        reasons: Vec<String>,
    },
    #[error("vanilla state restoration failed after dependency preflight: {detail}")]
    Restore {
        report: DependencyReport,
        detail: String,
    },
    #[error("could not write accepted recovery generation after dependency preflight: {detail}")]
    Write {
        report: DependencyReport,
        detail: String,
    },
}

fn unusable_finding(finding: &DependencyFinding) -> bool {
    matches!(
        finding.change,
        DependencyChange::Missing | DependencyChange::KindChanged
    )
}

fn is_transient_projectile(kind: &str) -> bool {
    matches!(kind, "PARW" | "PHZD" | "PGRE")
}

/// Inspect and explicitly accept a vanilla save into a separate native store.
///
/// Missing plugins or scripts and changed plugin kinds are reported before
/// restoration. PARW, PHZD and PGRE are transient and omitted with logged
/// counts. Other active projectiles and opaque PCON bytes refuse execution.
/// The source ESS path is never opened for writing by this operation.
///
/// # Errors
/// Returns the dependency report with refusal reasons, or a restoration or
/// generation-write error. Refusal happens before the game or store changes.
pub fn accept_vanilla_recovery_session(
    game: &mut Game,
    save: &Save,
    store: &GenerationStore,
) -> Result<AcceptedVanillaSession, VanillaRecoveryError> {
    let report = game
        .preflight_vanilla_dependencies(save)
        .map_err(|error| VanillaRecoveryError::Preflight(error.to_string()))?;
    let mut reasons: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| unusable_finding(finding))
        .map(|finding| {
            format!(
                "{:?} {} is {:?} for {:?} state",
                finding.kind, finding.identity, finding.change, finding.affects
            )
        })
        .collect();
    let mut skipped_transient = BTreeMap::new();
    for change in &save.change_forms {
        if is_transient_projectile(change.record_type()) {
            *skipped_transient.entry(change.record_type()).or_default() += 1;
        }
    }
    let mut opaque: Vec<_> = save
        .change_forms
        .iter()
        .filter(|change| matches!(change.record_type(), "PMIS" | "PBEA" | "PFLA" | "PBAR"))
        .map(|change| {
            format!(
                "unimported {} change form {:06X}",
                change.record_type(),
                change.form.raw()
            )
        })
        .collect();
    for change in save
        .change_forms
        .iter()
        .filter(|change| change.record_type() == "PCON")
    {
        let unresolved = change
            .reference_header(save.form_version)
            .map(|header| header.is_none_or(|header| header.remaining_bytes > 0))
            .unwrap_or(true);
        if unresolved {
            opaque.push(format!("opaque PCON change form {:06X}", change.form.raw()));
        }
    }
    reasons.extend(opaque);
    if !reasons.is_empty() {
        return Err(VanillaRecoveryError::Refused { report, reasons });
    }

    let mut recovery = Cow::Borrowed(save);
    if !skipped_transient.is_empty() {
        recovery
            .to_mut()
            .change_forms
            .retain(|change| !is_transient_projectile(change.record_type()));
    }
    game.restore_vanilla_world_and_vm(&recovery)
        .map_err(|error| VanillaRecoveryError::Restore {
            report: report.clone(),
            detail: error.to_string(),
        })?;
    let generation = game
        .write_generation(store)
        .map_err(|error| VanillaRecoveryError::Write {
            report: report.clone(),
            detail: error.to_string(),
        })?;
    for (kind, count) in &skipped_transient {
        soe_trace::warn!(target: "host::saves", system = "host::saves",
            "vanilla recovery skipped {count} transient {kind} change forms, source ESS retained"
        );
    }
    Ok(AcceptedVanillaSession {
        report,
        generation,
        skipped_transient,
    })
}
