// SPDX-License-Identifier: GPL-3.0-or-later

//! Content-addressed interface generations over soe-bsa's resolved VFS.
//! Publication is an atomic directory rename after resources and AS2 sources exist.

use std::collections::BTreeMap;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use soe_bsa::{Source, Vfs};

use crate::{convert, resource, Error};

const FORMAT: &str = "soe-interface-1";

/// The external JPEXS command used to preserve AS2 source for reference.
#[derive(Debug, Clone)]
pub enum Decompiler {
    /// Execute an installed ffdec launcher.
    Program(PathBuf),
    /// Execute a JPEXS jar with Java.
    Jar(PathBuf),
}

impl Decompiler {
    /// Use SOE_FFDEC_JAR, otherwise SOE_FFDEC or the ffdec launcher on PATH.
    pub fn from_env() -> Self {
        std::env::var_os("SOE_FFDEC_JAR").map_or_else(
            || {
                Self::Program(
                    std::env::var_os("SOE_FFDEC")
                        .map_or_else(|| PathBuf::from("ffdec"), PathBuf::from),
                )
            },
            |path| Self::Jar(PathBuf::from(path)),
        )
    }

    fn identity(&self) -> Result<String, Error> {
        match self {
            Self::Program(path) => Ok(format!("program:{}", path.display())),
            Self::Jar(path) => Ok(format!(
                "jar:{}:{}",
                path.display(),
                digest(&std::fs::read(path)?)
            )),
        }
    }

    fn export(&self, bytes: &[u8], directory: &Path) -> Result<(), Error> {
        // CONTEXT: Godot retains SIGPIPE, so an early child exit must not close a write pipe.
        let mut input = tempfile::tempfile()?;
        input.write_all(bytes)?;
        input.rewind()?;
        let mut command = match self {
            Self::Program(program) => Command::new(program),
            Self::Jar(jar) => {
                let mut c = Command::new("java");
                c.arg("-Djava.awt.headless=true").arg("-jar").arg(jar);
                c
            }
        };
        let child = command
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
            .arg(directory)
            .arg("/dev/stdin")
            .stdin(Stdio::from(input))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error::Decompiler(format!(
                    "start JPEXS FFDec: {e}. Set SOE_FFDEC_JAR or SOE_FFDEC"
                ))
            })?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(Error::Decompiler(format!(
                "{}: {} {}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        let files = collect(directory)?;
        if !files.keys().any(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("as"))
        }) {
            return Err(Error::Decompiler(
                "JPEXS returned no ActionScript source for a movie with actions".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn ui_27_decompiler_early_exit_returns_export_error() {
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("ffdec");
        std::fs::write(&program, "#!/bin/sh\nexit 7\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let error = Decompiler::Program(program)
            .export(&vec![0; 8 * 1024 * 1024], temp.path())
            .unwrap_err();
        assert!(matches!(error, Error::Decompiler(message) if message.contains("exit status: 7")));
    }
}

/// A completed immutable cache generation.
#[derive(Debug, Clone)]
pub struct Cache {
    /// External directory of this load order's converted resources.
    pub directory: PathBuf,
    /// Normalized SWF paths to native movie resources.
    pub movies: BTreeMap<String, PathBuf>,
    /// True when all output hashes matched an existing generation.
    pub reused: bool,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    key: String,
    movies: BTreeMap<String, String>,
    files: BTreeMap<String, String>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hash_part(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn is_git_checkout(path: &Path) -> bool {
    let marker = path.join(".git");
    marker.is_file() || marker.join("HEAD").is_file()
}

fn collect(directory: &Path) -> Result<BTreeMap<String, String>, Error> {
    let mut files = BTreeMap::new();
    if !directory.exists() {
        return Ok(files);
    }
    let mut pending = vec![directory.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                return Err(Error::Invalid("symlink in interface cache".into()));
            }
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else {
                let relative = entry
                    .path()
                    .strip_prefix(directory)
                    .map_err(|e| Error::Invalid(e.to_string()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative != "manifest.json" {
                    files.insert(relative, digest(&std::fs::read(entry.path())?));
                }
            }
        }
    }
    Ok(files)
}

fn cached(directory: &Path, key: &str) -> Option<Cache> {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).ok()?).ok()?;
    if manifest.format != FORMAT
        || manifest.key != key
        || collect(directory).ok()? != manifest.files
    {
        return None;
    }
    if manifest
        .movies
        .values()
        .any(|path| !manifest.files.contains_key(path))
    {
        return None;
    }
    Some(Cache {
        directory: directory.to_path_buf(),
        movies: manifest
            .movies
            .into_iter()
            .map(|(name, path)| (name, directory.join(path)))
            .collect(),
        reused: true,
    })
}

/// Convert all winning interface SWFs and publish their native resources.
///
/// The caller supplies active plugin names in load order and the VFS built from
/// that profile. Output must be outside a git checkout. JPEXS is only invoked
/// for movies containing actions, and its source output is never executed.
///
/// # Errors
/// Returns an asset error on parsing or decompilation failure, or a cache I/O
/// error. A failed build does not publish a generation.
pub fn build_cache(
    vfs: &Vfs,
    load_order: &[String],
    root: &Path,
    decompiler: &Decompiler,
) -> Result<Cache, Error> {
    std::fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    if root.ancestors().any(is_git_checkout) {
        return Err(Error::Invalid(
            "interface caches must be outside git checkouts".into(),
        ));
    }
    let mut hash = Sha256::new();
    hash_part(&mut hash, FORMAT.as_bytes());
    for plugin in load_order {
        hash_part(&mut hash, plugin.to_lowercase().as_bytes());
    }
    for archive in vfs.archives() {
        hash_part(&mut hash, archive.path().to_string_lossy().as_bytes());
    }
    for root in vfs.loose_roots() {
        hash_part(&mut hash, root.to_string_lossy().as_bytes());
    }
    let paths: Vec<_> = vfs
        .list("interface")
        .into_iter()
        .filter(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("swf"))
        })
        .collect();
    let mut inputs = vec![];
    for path in paths {
        let data = vfs.read(path).map_err(|e| Error::Asset {
            path: path.into(),
            source: Box::new(e.into()),
        })?;
        hash_part(&mut hash, path.as_bytes());
        hash_part(&mut hash, &data);
        match vfs.resolve(path) {
            Some(Source::Loose(file)) => hash_part(&mut hash, file.to_string_lossy().as_bytes()),
            Some(Source::Archived { archive, record }) => {
                hash_part(&mut hash, &(*archive as u64).to_le_bytes());
                hash_part(&mut hash, &(*record as u64).to_le_bytes());
            }
            None => {
                return Err(Error::Invalid(format!(
                    "winning source disappeared for {path}"
                )))
            }
        }
        inputs.push((path, data));
    }
    hash_part(&mut hash, decompiler.identity()?.as_bytes());
    let key = digest(&hash.finalize());
    let directory = root.join(&key);
    if let Some(cache) = cached(&directory, &key) {
        return Ok(cache);
    }
    let pending = tempfile::Builder::new()
        .prefix(".interface-")
        .tempdir_in(&root)?;
    let mut movies = BTreeMap::new();
    for (path, data) in inputs {
        let movie_dir = digest(path.as_bytes());
        let output = pending.path().join(&movie_dir);
        let converted = (|| {
            let movie = convert(&data)?;
            resource::write(&movie, &output)?;
            if movie.action_blocks > 0 {
                decompiler.export(&data, &output.join("as2-reference"))?;
            }
            Ok::<_, Error>(())
        })();
        converted.map_err(|source| Error::Asset {
            path: path.into(),
            source: Box::new(source),
        })?;
        movies.insert(path.to_string(), format!("{movie_dir}/movie.tres"));
    }
    let files = collect(pending.path())?;
    let manifest = Manifest {
        format: FORMAT.into(),
        key: key.clone(),
        movies,
        files,
    };
    std::fs::write(
        pending.path().join("manifest.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    if directory.exists() {
        std::fs::remove_dir_all(&directory)?;
    }
    std::fs::rename(pending.path(), &directory)?;
    cached(&directory, &key)
        .map(|mut cache| {
            cache.reused = false;
            cache
        })
        .ok_or_else(|| Error::Invalid("published interface generation did not validate".into()))
}
