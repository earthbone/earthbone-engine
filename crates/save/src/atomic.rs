// SPDX-License-Identifier: GPL-3.0-or-later

//! Validated save replacement using Linux rename/fsync ordering. OpenMW's
//! memory-first serialization and FallrimTools' prior-file backup informed
//! this path. See docs/save-resilience.md for the persistence failure model.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// A file operation at which persistence can fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Directory,
    ProbeDirectorySync,
    Lock,
    InspectDestination,
    CreateTemporary,
    WriteTemporary,
    SyncTemporary,
    ValidateTemporary,
    SyncTemporaryDirectory,
    OpenPrevious,
    SyncPrevious,
    LinkPrevious,
    SyncPreviousDirectory,
    Replace,
    SyncReplacementDirectory,
    RemovePrevious,
    SyncCleanupDirectory,
}

/// The writer preserves the destination until it has validated the new bytes.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("save serialization failed: {0}")]
    Serialize(String),
    #[error("save validation failed: {0}")]
    Validate(String),
    #[error("save file operation {step:?} failed: {source}")]
    Io {
        step: Step,
        #[source]
        source: io::Error,
        /// A staging path may contain incomplete or unvalidated bytes.
        staged: Option<PathBuf>,
        /// A prior link is retained after replacement or synchronization errors.
        previous: Option<PathBuf>,
        /// Replacement may have happened even though a later sync failed.
        replaced: bool,
    },
}

#[derive(Default)]
struct Transaction {
    staged: Option<PathBuf>,
    previous: Option<PathBuf>,
    replaced: bool,
}

impl Transaction {
    fn error(&self, step: Step, source: io::Error) -> WriteError {
        WriteError::Io {
            step,
            source,
            staged: self.staged.clone(),
            previous: self.previous.clone(),
            replaced: self.replaced,
        }
    }

    fn run<T>(
        &mut self,
        step: Step,
        action: impl FnOnce(&mut Self) -> io::Result<T>,
    ) -> Result<T, WriteError> {
        #[cfg(feature = "fault-injection")]
        fault::checkpoint(step, false).map_err(|error| self.error(step, error))?;
        let value = action(self).map_err(|error| self.error(step, error))?;
        #[cfg(feature = "fault-injection")]
        fault::checkpoint(step, true).map_err(|error| self.error(step, error))?;
        Ok(value)
    }
}

fn sibling_name(name: &std::ffi::OsStr, suffix: &str) -> OsString {
    let mut result = OsString::from(".");
    result.push(name);
    result.push(suffix);
    result
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

fn regular_file(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "save paths must be regular files, not links or directories",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Write, synchronize and validate a sibling before replacing a save file.
///
/// `validate` must reject bytes the caller cannot load. No existing file is
/// truncated. On errors, retained staging and prior paths are reported in
/// [`WriteError`]. Staging files must be validated before recovery. An initial
/// save has no prior generation until the first write finishes.
///
/// The parent directory must already exist and remain stable. Writers using
/// this function coordinate through a persistent sibling lock file. This
/// protocol requires local filesystem support for locks, hard links, atomic
/// replacement and directory synchronization. See the resilience document.
///
/// # Errors
/// Serialization validation, path inspection, locking, writing, read-back,
/// hard-link, rename and synchronization errors. A post-replacement error
/// reports `replaced: true` and retains the prior link when it still exists.
pub fn write_validated(
    path: &Path,
    bytes: &[u8],
    validate: impl Fn(&[u8]) -> Result<(), String>,
) -> Result<(), WriteError> {
    validate(bytes).map_err(WriteError::Validate)?;
    let mut tx = Transaction::default();
    let (directory, destination, name) = tx.run(Step::Directory, |_| {
        let name = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "save path has no filename")
        })?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = fs::canonicalize(parent)?;
        Ok((parent.clone(), parent.join(name), name.to_os_string()))
    })?;
    let directory_file = tx.run(Step::ProbeDirectorySync, |_| {
        let file = File::open(&directory)?;
        file.sync_all()?;
        Ok(file)
    })?;
    let _lock = tx.run(Step::Lock, |_| {
        let path = directory.join(sibling_name(&name, ".soe-lock"));
        regular_file(&path)?;
        let file = private_options().create(true).truncate(false).open(path)?;
        file.try_lock().map_err(io::Error::from)?;
        Ok(file)
    })?;
    let has_previous = tx.run(Step::InspectDestination, |_| regular_file(&destination))?;
    let (mut file, temporary) = tx.run(Step::CreateTemporary, |tx| {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..1024 {
            let suffix = format!(
                ".soe-stage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let path = directory.join(sibling_name(&name, &suffix));
            match private_options().create_new(true).open(&path) {
                Ok(file) => {
                    tx.staged = Some(path.clone());
                    return Ok((file, path));
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "save staging names exhausted",
        ))
    })?;
    for chunk in bytes.chunks(64 * 1024) {
        tx.run(Step::WriteTemporary, |_| file.write_all(chunk))?;
    }
    tx.run(Step::SyncTemporary, |_| file.sync_all())?;
    tx.run(Step::ValidateTemporary, |_| {
        file.rewind()?;
        let mut read_back = Vec::new();
        file.read_to_end(&mut read_back)?;
        if read_back != bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "saved bytes differ on read-back",
            ));
        }
        validate(&read_back).map_err(|reason| io::Error::new(io::ErrorKind::InvalidData, reason))
    })?;
    tx.run(Step::SyncTemporaryDirectory, |_| directory_file.sync_all())?;
    if has_previous {
        let previous_file = tx.run(Step::OpenPrevious, |_| File::open(&destination))?;
        tx.run(Step::SyncPrevious, |_| previous_file.sync_all())?;
        let mut previous_name = temporary.as_os_str().to_os_string();
        previous_name.push(".prior");
        let previous = PathBuf::from(previous_name);
        tx.run(Step::LinkPrevious, |tx| {
            fs::hard_link(&destination, &previous)?;
            tx.previous = Some(previous);
            Ok(())
        })?;
        tx.run(Step::SyncPreviousDirectory, |_| directory_file.sync_all())?;
    }
    drop(file);
    tx.run(Step::Replace, |tx| {
        fs::rename(&temporary, &destination)?;
        tx.staged = None;
        tx.replaced = true;
        Ok(())
    })?;
    tx.run(Step::SyncReplacementDirectory, |_| {
        directory_file.sync_all()
    })?;
    if let Some(previous) = tx.previous.clone() {
        tx.run(Step::RemovePrevious, |tx| {
            fs::remove_file(&previous)?;
            tx.previous = None;
            Ok(())
        })?;
        tx.run(Step::SyncCleanupDirectory, |_| directory_file.sync_all())?;
    }
    Ok(())
}

/// Test support enabled only by the explicit fault-injection feature.
#[cfg(feature = "fault-injection")]
#[doc(hidden)]
pub mod fault {
    use super::{io, Step};
    use std::cell::RefCell;

    type Hook = Box<dyn FnMut(Step, bool) -> io::Result<()>>;
    thread_local! {
        static HOOK: RefCell<Option<Hook>> = RefCell::new(None);
    }

    pub fn with_hook<T>(
        hook: impl FnMut(Step, bool) -> io::Result<()> + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Restore(Option<Hook>);
        impl Drop for Restore {
            fn drop(&mut self) {
                HOOK.with(|slot| {
                    *slot.borrow_mut() = self.0.take();
                });
            }
        }
        let _restore = Restore(HOOK.with(|slot| slot.replace(Some(Box::new(hook)))));
        action()
    }

    pub(super) fn checkpoint(step: Step, after: bool) -> io::Result<()> {
        HOOK.with(|slot| match slot.borrow_mut().as_mut() {
            Some(hook) => hook(step, after),
            None => Ok(()),
        })
    }
}
