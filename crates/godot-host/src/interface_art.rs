// SPDX-License-Identifier: GPL-3.0-or-later

//! FFDec discovery follows SkyUI-Community/tools/FFDec's ffdec-cli and ffdec.sh.
//! The converter owns export arguments and runs the selected tool out of process.

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

use soe_swf_convert::Decompiler;

/// File conversion stays off the Godot thread, including external FFDec waits.
pub(crate) struct CacheJob(mpsc::Receiver<Result<String, String>>);

impl CacheJob {
    fn spawn(
        work: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("interface-cache".into())
            .spawn(move || {
                let _ = sender.send(work());
            })?;
        Ok(Self(receiver))
    }

    pub(crate) fn start(
        source: Arc<crate::data_source::DataSource>,
        root: PathBuf,
        decompiler: Decompiler,
    ) -> std::io::Result<Self> {
        Self::spawn(move || {
            let plugins = source
                .order()
                .files()
                .iter()
                .map(|file| file.name.clone())
                .collect::<Vec<_>>();
            soe_swf_convert::build_cache(source.vfs(), &plugins, &root, &decompiler)
                .map(|cache| cache.directory.to_string_lossy().into_owned())
                .map_err(|error| error.to_string())
        })
    }

    pub(crate) fn poll(&self) -> Option<Result<String, String>> {
        match self.0.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("interface cache worker disconnected".into()))
            }
        }
    }
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    path.is_file()
}

fn program(path: &Path, search_path: &[PathBuf]) -> Option<PathBuf> {
    if path.components().count() > 1 || path.is_absolute() {
        return executable(path).then(|| path.to_path_buf());
    }
    search_path
        .iter()
        .map(|directory| directory.join(path))
        .find(|candidate| executable(candidate))
}

fn tool(path: &Path, java: bool) -> Option<Decompiler> {
    if path.extension().is_some_and(|extension| extension == "jar") {
        (java && path.is_file()).then(|| Decompiler::Jar(path.to_path_buf()))
    } else {
        executable(path).then(|| Decompiler::Program(path.to_path_buf()))
    }
}

fn discover(
    jar: Option<&Path>,
    launcher: Option<&Path>,
    tools: &[PathBuf],
    search_path: &[PathBuf],
) -> Option<Decompiler> {
    let java = program(Path::new("java"), search_path).is_some();
    if let Some(selected) = jar.and_then(|path| tool(path, java)) {
        return Some(selected);
    }
    if let Some(selected) = launcher.and_then(|path| program(path, search_path)) {
        return Some(Decompiler::Program(selected));
    }
    for directory in tools {
        if let Some(selected) = tool(directory, java) {
            return Some(selected);
        }
        for child in [
            "",
            "FFDec",
            "ffdec",
            "tools/FFDec",
            "SkyUI-Community/tools/FFDec",
        ] {
            for name in [
                "ffdec-cli.jar",
                "ffdec.jar",
                "ffdec-cli",
                "ffdec",
                "ffdec.sh",
            ] {
                if let Some(selected) = tool(&directory.join(child).join(name), java) {
                    return Some(selected);
                }
            }
        }
    }
    for directory in search_path {
        for name in ["ffdec-cli", "ffdec", "ffdec.sh"] {
            if let Some(selected) = tool(&directory.join(name), java) {
                return Some(selected);
            }
        }
    }
    None
}

pub(crate) fn from_env(configured_tools: &str) -> Option<Decompiler> {
    let jar = std::env::var_os("SOE_FFDEC_JAR").map(PathBuf::from);
    let launcher = std::env::var_os("SOE_FFDEC").map(PathBuf::from);
    let mut search_path = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut tools = std::env::var_os("SOE_TOOLS_PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    if !configured_tools.is_empty() {
        tools.push(PathBuf::from(configured_tools));
    }
    tools.extend([
        PathBuf::from("tools"),
        PathBuf::from("../tools"),
        PathBuf::from("../engine-refs"),
        PathBuf::from("../../engine-refs"),
        PathBuf::from("/usr/share/java"),
        PathBuf::from("/usr/share/java/ffdec"),
        PathBuf::from("/usr/share/java/jpexs-decompiler"),
    ]);
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        tools.extend(
            home.ancestors().map(|directory| {
                directory.join("Projects/Programming/skyrim-modclients/engine-refs")
            }),
        );
    }
    let selected = soe_swf_convert::find_java().and_then(|java| {
        if let Some(directory) = java.parent() {
            search_path.insert(0, directory.to_path_buf());
        }
        discover(jar.as_deref(), launcher.as_deref(), &tools, &search_path)
    });
    static LOG_SELECTION: std::sync::Once = std::sync::Once::new();
    LOG_SELECTION.call_once(|| match &selected {
        Some(Decompiler::Jar(path)) => {
            soe_trace::info!(target: "host::ui", system = "host::ui", "interface art: FFDec {} with installed Java", path.display());
        }
        Some(Decompiler::Program(path)) => {
            soe_trace::info!(target: "host::ui", system = "host::ui", "interface art: FFDec launcher {}", path.display());
        }
        None => {
            soe_trace::info!(target: "host::ui", system = "host::ui", "interface art: FFDec unavailable, using bundled menu theme");
        }
    });
    selected
}

pub(crate) fn unavailable_notice() -> &'static str {
    if soe_swf_convert::find_java().is_none() {
        "Interface art: Java unavailable. Set JAVA_HOME or install Java."
    } else {
        "Interface art: JPEXS FFDec unavailable. Set SOE_FFDEC_JAR or SOE_TOOLS_PATH."
    }
}

#[cfg(test)]
#[path = "interface_art/tests.rs"]
mod tests;
