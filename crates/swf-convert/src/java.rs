// SPDX-License-Identifier: GPL-3.0-or-later

//! Java discovery for JPEXS, following the local FFDec ffdec.sh launcher.

use std::path::{Path, PathBuf};

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    path.is_file()
}

fn discover(
    home: Option<&Path>,
    search_path: &[PathBuf],
    locations: &[PathBuf],
) -> Option<PathBuf> {
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    home.map(|home| home.join("bin").join(name))
        .into_iter()
        .chain(search_path.iter().map(|path| path.join(name)))
        .chain(locations.iter().cloned())
        .find(|path| executable(path))
}

/// Locate Java through SOE_JAVA, JAVA_HOME, PATH and conventional Linux installs.
/// An explicit SOE_JAVA path must be executable. An invalid override returns None.
pub fn find_java() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SOE_JAVA").map(PathBuf::from) {
        return executable(&path).then_some(path);
    }
    let home = std::env::var_os("JAVA_HOME").map(PathBuf::from);
    let search_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut locations: Vec<PathBuf> = [
        "/home/linuxbrew/.linuxbrew/bin/java",
        "/var/home/linuxbrew/.linuxbrew/bin/java",
        "/usr/bin/java",
        "/usr/local/bin/java",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    for directory in ["/usr/lib/jvm", "/usr/lib64/jvm", "/opt"] {
        let mut children = std::fs::read_dir(directory)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("bin/java"))
            .collect::<Vec<_>>();
        children.sort();
        locations.extend(children);
    }
    discover(home.as_deref(), &search_path, &locations)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn java(directory: &Path) -> PathBuf {
        let path = directory.join(if cfg!(windows) { "java.exe" } else { "java" });
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(&path, "fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    #[test]
    fn ui_30_java_home_precedes_path_then_install_locations() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("jdk");
        let first = java(&home.join("bin"));
        let path = temp.path().join("path");
        let second = java(&path);
        let third = java(&temp.path().join("linuxbrew"));
        let locations = [third.clone()];
        assert_eq!(
            discover(Some(&home), std::slice::from_ref(&path), &locations),
            Some(first)
        );
        assert_eq!(discover(None, &[path], &locations), Some(second));
        assert_eq!(discover(None, &[], &locations), Some(third));
    }

    #[test]
    fn ui_30_invalid_java_home_and_nonexecutables_fall_through() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");
        assert_eq!(discover(Some(&missing), &[], &[]), None);
        let valid = java(&temp.path().join("system"));
        let text = temp.path().join("not-java");
        std::fs::write(&text, "fixture").unwrap();
        assert_eq!(
            discover(Some(&missing), &[], &[text, valid.clone()]),
            Some(valid)
        );
    }
}
