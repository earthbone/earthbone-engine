// SPDX-License-Identifier: GPL-3.0-or-later

//! External SWF reference frames use JPEXS's local import and frame export CLI.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn required(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("set {name}").into())
}

fn materialize(
    vfs: &soe_bsa::Vfs,
    relative: &Path,
    root: &Path,
    seen: &mut BTreeSet<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !seen.insert(relative.to_path_buf()) {
        return Ok(());
    }
    let source = relative.to_string_lossy();
    if vfs.resolve(&source).is_none() {
        eprintln!("External font or movie has no VFS file: {source}");
        return Ok(());
    }
    let bytes = vfs.read(&source)?;
    let file = root.join(relative);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(file, &bytes)?;
    let buffer = swf::decompress_swf(bytes.as_slice())?;
    let mut reader = swf::read::Reader::new(&buffer.data, buffer.header.version());
    let mut imports = Vec::new();
    while !reader.get_ref().is_empty() {
        let start = reader.get_ref();
        let (code, length) = reader.read_tag_code_and_length()?;
        let rest = reader.get_ref().get(length..).ok_or("truncated SWF tag")?;
        if matches!(code, 57 | 71) {
            if let swf::Tag::ImportAssets { url, .. } =
                swf::read::Reader::new(start, buffer.header.version()).read_tag()?
            {
                imports.push(
                    url.to_string_lossy(swf::SwfStr::encoding_for_version(buffer.header.version())),
                );
            }
        }
        *reader.get_mut() = rest;
        if code == 0 {
            break;
        }
    }
    for url in &imports {
        if url.contains("://") {
            continue;
        }
        let path = relative.parent().unwrap_or(Path::new("")).join(url);
        let mut canonical = PathBuf::new();
        for part in path.components() {
            match part {
                std::path::Component::Normal(part) => canonical.push(part),
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir if canonical.pop() => {}
                _ => return Err(format!("import leaves the data tree: {url}").into()),
            }
        }
        materialize(vfs, &canonical, root, seen)?;
    }
    Ok(())
}

fn export(
    vfs: &soe_bsa::Vfs,
    java: &Path,
    jar: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let input = tempfile::tempdir_in("/tmp")?;
    let mut seen = BTreeSet::new();
    for menu in ["inventorymenu", "containermenu", "dialoguemenu"] {
        materialize(
            vfs,
            Path::new(&format!("interface/{menu}.swf")),
            input.path(),
            &mut seen,
        )?;
        let directory = output.join(menu);
        std::fs::create_dir_all(&directory)?;
        let result = Command::new(java)
            .arg("-Djava.awt.headless=true")
            .arg("-jar")
            .arg(jar)
            .args([
                "-onerror",
                "abort",
                "-importAssets",
                "yes,local",
                "-format",
                "frame:png",
                "-export",
                "frame",
            ])
            .arg(&directory)
            .arg(input.path().join(format!("interface/{menu}.swf")))
            .output()?;
        std::fs::write(
            directory.join("ffdec.log"),
            [result.stdout, result.stderr].concat(),
        )?;
        if !result.status.success() {
            return Err(format!("JPEXS {menu}: {}", result.status).into());
        }
        println!("{}", directory.display());
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("supply an output directory under target")?;
    let data = required("SKYRIM_DATA")?;
    let skyui = required("SOE_TEST_SKYUI")?;
    let nordic = required("SOE_TEST_NORDIC")?;
    let jar = required("SOE_FFDEC_JAR")?;
    let java = soe_swf_convert::find_java().ok_or("Java unavailable")?;
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_archive_path(&data.join("Skyrim - Interface.bsa"))?;
    vfs.add_archive_path(&skyui.join("SkyUI_SE.bsa"))?;
    export(&vfs, &java, &jar, &output.join("skyui"))?;
    vfs.add_loose_root(&nordic)?;
    export(&vfs, &java, &jar, &output.join("nordic"))
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Interface reference export: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
