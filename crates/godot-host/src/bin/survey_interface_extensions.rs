// SPDX-License-Identifier: GPL-3.0-or-later

//! UI-23 counts Ruffle tag envelopes and actual JPEXS AS2 exports.
//! Tag boundaries follow swf-convert's bounded reader, including SkyUI sprites.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

type Error = Box<dyn std::error::Error>;

fn tags(
    bytes: &[u8],
    version: u8,
    depth: usize,
    counts: &mut BTreeMap<u16, usize>,
) -> Result<(), Error> {
    if depth > 64 {
        return Err("sprite nesting exceeds 64".into());
    }
    let mut reader = swf::read::Reader::new(bytes, version);
    while !reader.get_ref().is_empty() {
        let (code, length) = reader.read_tag_code_and_length()?;
        let payload = reader.get_ref().get(..length).ok_or("truncated SWF tag")?;
        *counts.entry(code).or_default() += 1;
        if code == 39 {
            tags(
                payload.get(4..).ok_or("truncated sprite header")?,
                version,
                depth + 1,
                counts,
            )?;
        }
        *reader.get_mut() = &reader.get_ref()[length..];
        if code == 0 {
            break;
        }
    }
    Ok(())
}

fn sources(directory: &Path, output: &mut Vec<String>) -> Result<(), Error> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, output)?;
        } else if path.extension().is_some_and(|extension| extension == "as") {
            output.push(std::fs::read_to_string(path)?);
        }
    }
    Ok(())
}

fn literal_count(source: &str, needle: &str) -> usize {
    let word = |character: char| character.is_ascii_alphanumeric() || character == '_';
    source
        .match_indices(needle)
        .filter(|(index, _)| {
            !source[..*index].chars().next_back().is_some_and(word)
                && !source[*index + needle.len()..]
                    .chars()
                    .next()
                    .is_some_and(word)
        })
        .count()
}

fn apis(source: &str) -> serde_json::Value {
    let defined_classes = source
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix("class ")?;
            let name = line.split_whitespace().next()?;
            name.starts_with("gfx.")
                .then(|| name.trim_end_matches('{').to_owned())
        })
        .collect::<BTreeSet<_>>();
    let mut names = BTreeSet::new();
    let mut operations = BTreeMap::<String, usize>::new();
    for (index, _) in source.match_indices("gfx.io.GameDelegate.") {
        let tail = &source[index + "gfx.io.GameDelegate.".len()..];
        let Some((method, rest)) = tail.split_once('(') else {
            continue;
        };
        let method = method.trim();
        if !matches!(method, "call" | "addCallBack") {
            continue;
        }
        let rest = rest.trim_start();
        let Some(quote) = rest.chars().next().filter(|c| matches!(c, '\'' | '"')) else {
            continue;
        };
        if let Some(name) = rest[1..].split(quote).next().filter(|s| !s.is_empty()) {
            names.insert(name.to_owned());
            *operations.entry(method.to_owned()).or_default() += 1;
        }
    }
    let namespaces = ["io", "ui", "managers", "controls", "events"]
        .map(|name| (name, literal_count(source, &format!("gfx.{name}"))));
    let extensions = [
        "Stage.safeRect",
        "Stage.visibleRect",
        "Mouse.getTopMostEntity",
        ".textAutoSize",
        ".verticalAutoSize",
        ".getTextExtent",
        "TextField.prototype.SetText",
        "MovieClip.prototype.Lock",
    ]
    .map(|name| (name, source.matches(name).count()))
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    serde_json::json!({"namespaces": namespaces.into_iter().collect::<BTreeMap<_,_>>(),
        "gfxExtensions":literal_count(source,"_global.gfxExtensions"), "gfxPlayer":literal_count(source,"_global.gfxPlayer"),
        "operations":operations,"delegates":names,"defined_gfx_classes":defined_classes,"extensions":extensions})
}

fn survey(
    vfs: &soe_bsa::Vfs,
    root: &Path,
    java: &Path,
    jar: &Path,
) -> Result<serde_json::Value, Error> {
    std::fs::create_dir_all(root)?;
    let input = tempfile::tempdir_in("/tmp")?;
    let mut movies = BTreeMap::new();
    for path in vfs.list("interface") {
        if !Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "swf" || extension == "gfx")
        {
            continue;
        }
        let bytes = vfs.read(path)?;
        let buffer = swf::decompress_swf(bytes.as_slice())?;
        let mut counts = BTreeMap::new();
        tags(&buffer.data, buffer.header.version(), 0, &mut counts)?;
        let directory = root.join(path.replace('/', "_"));
        std::fs::create_dir_all(&directory)?;
        let mut scripts = Vec::new();
        {
            let file = input.path().join("input.swf");
            std::fs::write(&file, bytes)?;
            let result = Command::new(java)
                .arg("-Djava.awt.headless=true")
                .arg("-jar")
                .arg(jar)
                .args([
                    "-onerror",
                    "abort",
                    "-importAssets",
                    "local",
                    "-format",
                    "script:as",
                    "-export",
                    "script",
                ])
                .arg(&directory)
                .arg(file)
                .output()?;
            if !result.status.success() {
                return Err(
                    format!("JPEXS {path}: {}", String::from_utf8_lossy(&result.stderr)).into(),
                );
            }
            sources(&directory, &mut scripts)?;
        }
        let mut api = apis(&scripts.join("\n"));
        api["source_files"] = scripts.len().into();
        api["tags"] = serde_json::to_value(counts)?;
        movies.insert(path.to_owned(), api);
    }
    Ok(serde_json::to_value(movies)?)
}

fn required(name: &str) -> Result<PathBuf, Error> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("set {name}").into())
}

fn run() -> Result<(), Error> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("supply a new external output directory")?;
    if root.exists() {
        return Err("output directory already exists".into());
    }
    std::fs::create_dir_all(&root)?;
    let root = root.canonicalize()?;
    if root
        .ancestors()
        .any(|path| path.join(".git").is_file() || path.join(".git/HEAD").is_file())
    {
        return Err("decompiled sources must stay outside git checkouts".into());
    }
    let java = soe_swf_convert::find_java().ok_or("Java unavailable")?;
    let jar = required("SOE_FFDEC_JAR")?;
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_archive_path(&required("SKYRIM_DATA")?.join("Skyrim - Interface.bsa"))?;
    let vanilla = survey(&vfs, &root.join("vanilla"), &java, &jar)?;
    vfs.add_archive_path(&required("SOE_TEST_SKYUI")?.join("SkyUI_SE.bsa"))?;
    let skyui = survey(&vfs, &root.join("skyui"), &java, &jar)?;
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"vanilla":vanilla,"skyui":skyui}))?,
    )?;
    println!("{}", root.join("report.json").display());
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Interface extension survey: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
