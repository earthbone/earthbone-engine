// SPDX-License-Identifier: GPL-3.0-or-later
// Weekly mod corpus sweep. Point `SOE_MOD_CORPUS` at a directory containing
// one installed mod directory per child. The sweep reads local files only.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder};

#[derive(Debug, Default)]
struct ModReport {
    name: String,
    parser_failures: Vec<String>,
    unresolved_natives: Vec<String>,
    missing_assets: Vec<String>,
    files_scanned: usize,
}

const ASSET_EXTENSIONS: &[&str] = &[
    "nif", "dds", "hkx", "pex", "fuz", "lip", "wav", "xwm", "ogg",
];

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths.into_iter().rev() {
            if path.is_dir() {
                pending.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn mod_roots(root: &Path) -> Vec<PathBuf> {
    let mut children: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();
    if children.is_empty() {
        vec![root.to_path_buf()]
    } else {
        children
    }
}

fn relative_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn parse_asset(path: &str, bytes: &[u8]) -> Result<(), String> {
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "esp" | "esm" | "esl" => Plugin::parse(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        "nif" => soe_nif::parse(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        "hkx" => soe_hkx::Packfile::parse(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        "dds" => soe_dds::Dds::parse(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        "pex" => soe_papyrus::pex::parse(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        "fuz" => soe_audio::Fuz::split(bytes)
            .map(|fuz| {
                if !fuz.lip.is_empty() {
                    soe_audio::Lip::parse(fuz.lip).map_err(|error| error.to_string())?;
                }
                Ok(())
            })
            .map_err(|error| error.to_string())?,
        "lip" => soe_audio::Lip::parse(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        "xwm" => soe_audio::Xwma::parse(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        "wav" => soe_audio::riff::Riff::parse(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        _ => Ok(()),
    }
}

fn archive_error(
    path: &Path,
    report: &mut ModReport,
    plugins: &mut Vec<(String, Vec<u8>)>,
    scripts: &mut Vec<(String, Vec<u8>)>,
) {
    match soe_bsa::Archive::open(path) {
        Ok(archive) => {
            for (index, member) in archive.files().iter().enumerate() {
                let bytes = match archive.read_record(index) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        report
                            .parser_failures
                            .push(format!("{}!{member}: {error}", path.display()));
                        continue;
                    }
                };
                report.files_scanned += 1;
                if let Err(error) = parse_asset(member, &bytes) {
                    report
                        .parser_failures
                        .push(format!("{}!{member}: {error}", path.display()));
                } else {
                    let extension = Path::new(member)
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or_default();
                    if matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "esp" | "esm" | "esl"
                    ) {
                        plugins.push((format!("{}!{member}", path.display()), bytes.clone()));
                    }
                    if extension.eq_ignore_ascii_case("pex") {
                        scripts.push((format!("{}!{member}", path.display()), bytes));
                    }
                }
            }
        }
        Err(error) => report
            .parser_failures
            .push(format!("{}: {error}", path.display())),
    }
}

fn asset_references(plugin: &Plugin) -> Vec<String> {
    let mut references = BTreeSet::new();
    for record in &plugin.records {
        for subrecord in &record.subrecords {
            for value in subrecord.data.split(|byte| *byte == 0) {
                let Ok(value) = std::str::from_utf8(value) else {
                    continue;
                };
                let value = value.trim();
                let extension = Path::new(value)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if ASSET_EXTENSIONS.contains(&extension.as_str())
                    && value.contains(['/', '\\'])
                    && !value.starts_with('\\')
                {
                    references.insert(value.replace('\\', "/").to_ascii_lowercase());
                }
            }
        }
    }
    references.into_iter().collect()
}

fn scan_mod(root: &Path, available: &BTreeSet<String>) -> ModReport {
    let mut report = ModReport {
        name: root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        ..ModReport::default()
    };
    let mut plugins = Vec::new();
    let mut scripts = Vec::new();
    for path in files_under(root) {
        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        let extension = extension.to_ascii_lowercase();
        let key = relative_key(root, &path);
        if extension == "bsa" || extension == "ba2" {
            archive_error(&path, &mut report, &mut plugins, &mut scripts);
            continue;
        }
        if !matches!(
            extension.as_str(),
            "esp" | "esm" | "esl" | "nif" | "hkx" | "dds" | "pex" | "fuz" | "lip" | "wav" | "xwm"
        ) {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                report
                    .parser_failures
                    .push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        report.files_scanned += 1;
        match parse_asset(&key, &bytes) {
            Ok(()) => {
                if matches!(extension.as_str(), "esp" | "esm" | "esl") {
                    plugins.push((path.display().to_string(), bytes.clone()));
                }
                if extension == "pex" {
                    scripts.push((path.display().to_string(), bytes));
                }
            }
            Err(error) => report
                .parser_failures
                .push(format!("{}: {error}", path.display())),
        }
    }

    for (path, bytes) in plugins {
        if let Ok(plugin) = Plugin::parse(&bytes) {
            for reference in asset_references(&plugin) {
                if !available.contains(&soe_bsa::normalize(&reference)) {
                    report.missing_assets.push(format!("{path}: {reference}"));
                }
            }
        }
    }

    let mut game = Game::new(LoadOrder::new());
    for (path, bytes) in scripts {
        if let Err(error) = game.load_script(&bytes) {
            report.parser_failures.push(format!("{path}: {error}"));
        }
    }
    report.unresolved_natives = game
        .vm
        .unregistered_natives()
        .into_iter()
        .map(|(class, function, global)| {
            format!("{class}.{}{function}", if global { "::" } else { "" })
        })
        .collect();
    report.unresolved_natives.sort();
    report.unresolved_natives.dedup();
    report.parser_failures.sort();
    report.missing_assets.sort();
    report.missing_assets.dedup();
    report
}

fn run_corpus(root: &Path) -> Vec<ModReport> {
    let roots = mod_roots(root);
    let mut available = BTreeSet::new();
    for mod_root in &roots {
        for path in files_under(mod_root) {
            available.insert(relative_key(mod_root, &path));
            if path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("bsa") || ext.eq_ignore_ascii_case("ba2")
            }) {
                if let Ok(archive) = soe_bsa::Archive::open(&path) {
                    available.extend(archive.files().iter().map(|file| soe_bsa::normalize(file)));
                }
            }
        }
    }
    if let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) {
        for path in files_under(&data) {
            available.insert(relative_key(&data, &path));
            if path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("bsa") || ext.eq_ignore_ascii_case("ba2")
            }) {
                if let Ok(archive) = soe_bsa::Archive::open(&path) {
                    available.extend(archive.files().iter().map(|file| soe_bsa::normalize(file)));
                }
            }
        }
    }
    roots
        .iter()
        .map(|root| scan_mod(root, &available))
        .collect()
}

fn render(reports: &[ModReport]) -> String {
    let mut lines = Vec::new();
    for report in reports {
        lines.push(format!(
            "{}: {} files, {} parser failures, {} unresolved natives, {} missing assets",
            report.name,
            report.files_scanned,
            report.parser_failures.len(),
            report.unresolved_natives.len(),
            report.missing_assets.len()
        ));
        lines.extend(
            report
                .parser_failures
                .iter()
                .map(|entry| format!("  parser failure: {entry}")),
        );
        lines.extend(
            report
                .unresolved_natives
                .iter()
                .map(|entry| format!("  unresolved native: {entry}")),
        );
        lines.extend(
            report
                .missing_assets
                .iter()
                .map(|entry| format!("  missing asset: {entry}")),
        );
    }
    lines.join("\n")
}

struct TempDir(PathBuf);

impl TempDir {
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    fn new(case: &str) -> Self {
        let path = std::env::temp_dir().join(format!("soe-tst-12-{}-{case}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[test]
fn tst_12_reports_parser_failures_per_mod() {
    let temp = TempDir::new("parser");
    let mod_dir = temp.0.join("BrokenMod");
    std::fs::create_dir_all(mod_dir.join("meshes")).unwrap();
    std::fs::write(mod_dir.join("bad.esp"), b"not a plugin").unwrap();
    std::fs::write(mod_dir.join("meshes/bad.nif"), b"not a nif").unwrap();
    let reports = run_corpus(&temp.0);
    let rendered = render(&reports);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name, "BrokenMod");
    assert!(
        rendered.contains("BrokenMod: 2 files, 2 parser failures"),
        "{rendered}"
    );
    assert!(rendered.contains("bad.esp"), "{rendered}");
    assert!(rendered.contains("bad.nif"), "{rendered}");
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[test]
fn tst_12_reports_unresolved_natives_per_mod() {
    let temp = TempDir::new("native");
    let mod_dir = temp.0.join("NativeMod");
    std::fs::create_dir_all(mod_dir.join("Scripts")).unwrap();
    let bytes = ScriptBuilder::new("NativeModScript", "")
        .function(
            FunctionBuilder::new("MissingNative", "None")
                .native()
                .build(),
        )
        .bytes();
    std::fs::write(mod_dir.join("Scripts/NativeModScript.pex"), bytes).unwrap();
    let reports = run_corpus(&temp.0);
    let rendered = render(&reports);
    assert!(
        rendered.contains("NativeMod: 1 files, 0 parser failures, 1 unresolved natives"),
        "{rendered}"
    );
    assert!(
        rendered.contains("NativeModScript.MissingNative"),
        "{rendered}"
    );
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[test]
fn tst_12_reports_missing_assets_per_mod() {
    let temp = TempDir::new("asset");
    let mod_dir = temp.0.join("AssetMod");
    std::fs::create_dir_all(&mod_dir).unwrap();
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        Record::new(Tag::new(b"STAT"), 0x800).zsub(Tag::new(b"MODL"), "meshes/custom/missing.nif"),
    );
    std::fs::write(mod_dir.join("AssetMod.esp"), plugin.to_bytes()).unwrap();
    let reports = run_corpus(&temp.0);
    let rendered = render(&reports);
    assert!(
        rendered.contains(
            "AssetMod: 1 files, 0 parser failures, 0 unresolved natives, 1 missing assets"
        ),
        "{rendered}"
    );
    assert!(rendered.contains("meshes/custom/missing.nif"), "{rendered}");
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[test]
fn tst_12_reports_all_finding_categories_per_mod() {
    let temp = TempDir::new("finding-categories");
    let broken = temp.0.join("BrokenMod");
    let native = temp.0.join("NativeMod");
    let missing = temp.0.join("MissingAssetMod");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::create_dir_all(native.join("Scripts")).unwrap();
    std::fs::create_dir_all(&missing).unwrap();
    std::fs::write(broken.join("bad.esp"), b"not a plugin").unwrap();

    let bytes = ScriptBuilder::new("NativeModScript", "")
        .function(
            FunctionBuilder::new("MissingNative", "None")
                .native()
                .build(),
        )
        .bytes();
    std::fs::write(native.join("Scripts/NativeModScript.pex"), bytes).unwrap();

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"STAT"), 0x800).zsub(Tag::new(b"MODL"), "meshes/custom/missing.nif"),
    );
    std::fs::write(missing.join("MissingAsset.esp"), plugin.to_bytes()).unwrap();

    let reports = run_corpus(&temp.0);
    let rendered = render(&reports);
    assert_eq!(
        reports
            .iter()
            .map(|report| report.name.as_str())
            .collect::<Vec<_>>(),
        ["BrokenMod", "MissingAssetMod", "NativeMod"]
    );
    assert!(
        rendered.contains("BrokenMod: 1 files, 1 parser failures"),
        "{rendered}"
    );
    assert!(
        rendered.contains(
            "MissingAssetMod: 1 files, 0 parser failures, 0 unresolved natives, 1 missing assets"
        ),
        "{rendered}"
    );
    assert!(
        rendered.contains("NativeMod: 1 files, 0 parser failures, 1 unresolved natives"),
        "{rendered}"
    );
    assert!(rendered.contains("bad.esp"), "{rendered}");
    assert!(
        rendered.contains("NativeModScript.MissingNative"),
        "{rendered}"
    );
    assert!(rendered.contains("meshes/custom/missing.nif"), "{rendered}");
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[test]
fn tst_12_mod_corpus_sweep() {
    let Some(root) = std::env::var_os("SOE_MOD_CORPUS").map(PathBuf::from) else {
        eprintln!("SOE_MOD_CORPUS not set, skipping the weekly mod corpus sweep");
        return;
    };
    assert!(
        root.is_dir(),
        "SOE_MOD_CORPUS is not a directory: {}",
        root.display()
    );
    let reports = run_corpus(&root);
    assert!(
        !reports.is_empty(),
        "the corpus has no mod directories: {}",
        root.display()
    );
    eprintln!("{}", render(&reports));
}
