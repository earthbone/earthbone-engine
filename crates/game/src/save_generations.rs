// SPDX-License-Identifier: GPL-3.0-or-later

//! Verified native save generations. OpenMW's bounded quicksave slots and
//! FallrimTools' backup copies informed the policy. See docs/save-resilience.md.

use crate::Game;
use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::VmError;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"SOEB";
const VERSION: u16 = 1;
const LOCK: &str = ".soe-generations.lock";

/// A dedicated trusted directory and the number of generations to retain.
#[derive(Clone, Debug)]
pub struct GenerationStore {
    directory: PathBuf,
    retain: usize,
}

/// Distinguishes damaged storage from a save incompatible with current definitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RejectionReason {
    Integrity(String),
    Definitions(String),
    Read(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedGeneration {
    pub generation: u64,
    pub path: PathBuf,
    pub reason: RejectionReason,
}

/// A verified selection owns its bytes, so later file edits cannot change them.
#[derive(Debug)]
pub struct SelectedGeneration {
    pub generation: u64,
    pub path: PathBuf,
    pub rejected: Vec<RejectedGeneration>,
    bytes: Vec<u8>,
}

impl SelectedGeneration {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug)]
pub struct GenerationWrite {
    pub generation: u64,
    pub path: PathBuf,
    pub retained: Vec<u64>,
    pub removed: Vec<PathBuf>,
    pub rejected: Vec<RejectedGeneration>,
}

/// Persistence boundaries exposed for explicit fault tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    OpenDirectory,
    SyncInitialDirectory,
    Lock,
    Scan,
    WriteGeneration,
    VerifyGeneration,
    SyncGeneration,
    SyncPublishedDirectory,
    InspectRetention,
    RemoveGeneration,
    SyncRemovalDirectory,
    RemoveArtifact,
    SyncArtifactDirectory,
    ReadRecovery,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("retention must be between 2 and 64 generations, got {0}")]
    Retention(usize),
    #[error("generation counter exhausted")]
    CounterExhausted,
    #[error("generation operation {step:?} failed: {source}")]
    Io {
        step: Step,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Write(#[from] soe_save::atomic::WriteError),
    #[error(transparent)]
    State(#[from] VmError),
    #[error("new generation {path:?} failed verification: {reason:?}")]
    Verification {
        path: PathBuf,
        reason: RejectionReason,
    },
    #[error("no generation loads with the current definitions ({count} rejected)", count = .rejected.len())]
    NoLoadable { rejected: Vec<RejectedGeneration> },
}

fn boundary<T>(step: Step, action: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    #[cfg(not(feature = "save-fault-tests"))]
    let _ = step;
    #[cfg(feature = "save-fault-tests")]
    fault::checkpoint(step, false).map_err(|source| Error::Io { step, source })?;
    let result = action()?;
    #[cfg(feature = "save-fault-tests")]
    fault::checkpoint(step, true).map_err(|source| Error::Io { step, source })?;
    Ok(result)
}

fn operation<T>(step: Step, action: impl FnOnce() -> io::Result<T>) -> Result<T, Error> {
    boundary(step, || {
        action().map_err(|source| Error::Io { step, source })
    })
}

fn regular(path: &Path) -> io::Result<()> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "generation path must be a regular file",
        ));
    }
    Ok(())
}

fn filename(id: u64) -> String {
    format!("generation-{id:020}.soeb")
}

fn generation_id(name: &str) -> Option<u64> {
    let number = name.strip_prefix("generation-")?.strip_suffix(".soeb")?;
    if number.len() != 20 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse().ok().filter(|id| *id > 0)
}

fn artifact_id(name: &str) -> Option<u64> {
    let (stem, suffix) = name.strip_prefix('.')?.split_once(".soeb.")?;
    let id = generation_id(&format!("{stem}.soeb"))?;
    if suffix == "soe-lock" {
        return Some(id);
    }
    let suffix = suffix.strip_prefix("soe-stage-")?;
    let suffix = suffix.strip_suffix(".prior").unwrap_or(suffix);
    let (pid, serial) = suffix.split_once('-')?;
    if pid.is_empty()
        || serial.is_empty()
        || !pid
            .bytes()
            .chain(serial.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    pid.parse::<u32>().ok()?;
    serial.parse::<u64>().ok()?;
    Some(id)
}

fn envelope(id: u64, bytes: &[u8]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.out.extend_from_slice(MAGIC);
    writer.u16(VERSION);
    writer.u64(id);
    writer.u64(bytes.len() as u64);
    let mut checksum = crc32fast::Hasher::new();
    checksum.update(&writer.out);
    checksum.update(bytes);
    writer.u32(checksum.finalize());
    writer.out.extend_from_slice(bytes);
    writer.out
}

fn payload(expected: u64, bytes: &[u8]) -> Result<&[u8], String> {
    fn decode(expected: u64, bytes: &[u8]) -> Result<&[u8], VmError> {
        let mut reader = Reader::new(bytes);
        if reader.take(4)? != MAGIC || reader.u16()? != VERSION || reader.u64()? != expected {
            return Err(VmError::State("wrong generation header or identity".into()));
        }
        let length = usize::try_from(reader.u64()?)
            .map_err(|_| VmError::State("generation length exceeds address space".into()))?;
        let expected_crc = reader.u32()?;
        let saved = reader.take(length)?;
        reader.expect_end()?;
        let mut checksum = crc32fast::Hasher::new();
        checksum.update(&bytes[..22]);
        checksum.update(saved);
        if checksum.finalize() != expected_crc {
            return Err(VmError::State("generation checksum mismatch".into()));
        }
        Ok(saved)
    }
    decode(expected, bytes).map_err(|error| error.to_string())
}

fn read_verified(game: &Game, id: u64, path: &Path) -> Result<Vec<u8>, RejectionReason> {
    regular(path).map_err(|error| RejectionReason::Read(error.to_string()))?;
    let bytes = fs::read(path).map_err(|error| RejectionReason::Read(error.to_string()))?;
    let saved = payload(id, &bytes).map_err(RejectionReason::Integrity)?;
    game.validate_saved_state(saved)
        .map_err(|error| RejectionReason::Definitions(error.to_string()))?;
    Ok(saved.to_vec())
}

struct Session {
    directory: PathBuf,
    directory_file: File,
    lock: File,
}

impl Drop for Session {
    fn drop(&mut self) {
        // A concurrent launch or fork can inherit the descriptor until exec.
        // Closing our copy alone would keep the shared file lock held.
        let _ = self.lock.unlock();
    }
}

impl GenerationStore {
    /// Configure native generations in an existing dedicated directory.
    /// Successful rotation retains 2 to 64 generations. Initial saves can have
    /// fewer. Interrupted rotations may retain extras until a successful retry.
    ///
    /// # Errors
    /// A retention value outside the supported range.
    pub fn new(directory: impl Into<PathBuf>, retain: usize) -> Result<Self, Error> {
        if !(2..=64).contains(&retain) {
            return Err(Error::Retention(retain));
        }
        Ok(Self {
            directory: directory.into(),
            retain,
        })
    }

    fn session(&self) -> Result<Session, Error> {
        let (directory, directory_file) = operation(Step::OpenDirectory, || {
            let directory = fs::canonicalize(&self.directory)?;
            if !directory.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    "generation store is not a directory",
                ));
            }
            let file = File::open(&directory)?;
            Ok((directory, file))
        })?;
        operation(Step::SyncInitialDirectory, || directory_file.sync_all())?;
        let lock = operation(Step::Lock, || {
            let path = directory.join(LOCK);
            match fs::symlink_metadata(&path) {
                Ok(_) => regular(&path)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true).truncate(false);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(path)?;
            file.try_lock().map_err(io::Error::from)?;
            Ok(file)
        })?;
        Ok(Session {
            directory,
            directory_file,
            lock,
        })
    }
}

impl Session {
    fn scan(&self) -> Result<Vec<(u64, PathBuf)>, Error> {
        operation(Step::Scan, || {
            let mut entries = Vec::new();
            for entry in fs::read_dir(&self.directory)? {
                let entry = entry?;
                if let Some(id) = entry.file_name().to_str().and_then(generation_id) {
                    entries.push((id, entry.path()));
                }
            }
            entries.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
            Ok(entries)
        })
    }

    fn remove(&self, path: &Path, artifact: bool) -> Result<(), Error> {
        let (remove, sync) = if artifact {
            (Step::RemoveArtifact, Step::SyncArtifactDirectory)
        } else {
            (Step::RemoveGeneration, Step::SyncRemovalDirectory)
        };
        operation(remove, || {
            regular(path)?;
            fs::remove_file(path)
        })?;
        operation(sync, || self.directory_file.sync_all())
    }
}

impl Game {
    /// Write a verified native generation, then apply bounded retention.
    /// Loadable generations take priority over intact incompatible ones. The
    /// new verified file is durable before any old file or artifact is removed.
    ///
    /// # Errors
    /// State validation, storage errors, counter overflow or failed cleanup.
    /// An error after publication can leave a complete new generation present.
    pub fn write_generation(&self, store: &GenerationStore) -> Result<GenerationWrite, Error> {
        let native = self.save_state()?;
        self.validate_saved_state(&native)?;
        let session = store.session()?;
        let existing = session.scan()?;
        let id = existing
            .first()
            .map_or(Some(1), |entry| entry.0.checked_add(1))
            .ok_or(Error::CounterExhausted)?;
        let path = session.directory.join(filename(id));
        let bytes = envelope(id, &native);
        boundary(Step::WriteGeneration, || {
            soe_save::atomic::write_validated(&path, &bytes, |written| {
                self.validate_saved_state(payload(id, written)?)
                    .map_err(|error| error.to_string())
            })
            .map_err(Error::Write)
        })?;
        boundary(Step::VerifyGeneration, || {
            read_verified(self, id, &path)
                .map(|_| ())
                .map_err(|reason| Error::Verification {
                    path: path.clone(),
                    reason,
                })
        })?;
        operation(Step::SyncGeneration, || File::open(&path)?.sync_all())?;
        operation(Step::SyncPublishedDirectory, || {
            session.directory_file.sync_all()
        })?;

        let mut candidates = vec![(id, path.clone())];
        candidates.extend(existing);
        let (retained, rejected) = boundary(Step::InspectRetention, || {
            let mut loadable = Vec::new();
            let mut incompatible = Vec::new();
            let mut rejected = Vec::new();
            for (candidate, path) in &candidates {
                match read_verified(self, *candidate, path) {
                    Ok(_) => loadable.push(*candidate),
                    Err(reason) => {
                        if matches!(reason, RejectionReason::Definitions(_)) {
                            incompatible.push(*candidate);
                        }
                        rejected.push(RejectedGeneration {
                            generation: *candidate,
                            path: path.clone(),
                            reason,
                        });
                    }
                }
            }
            if let Some(failed) = rejected.iter().find(|candidate| candidate.generation == id) {
                return Err(Error::Verification {
                    path: path.clone(),
                    reason: failed.reason.clone(),
                });
            }
            loadable.extend(incompatible);
            loadable.truncate(store.retain);
            Ok((loadable.into_iter().collect::<BTreeSet<_>>(), rejected))
        })?;
        let mut removed = Vec::new();
        // Oldest first gives deterministic progress if deletion is interrupted.
        for (candidate, path) in candidates.iter().rev() {
            if !retained.contains(candidate) {
                session.remove(path, false)?;
                removed.push(path.clone());
            }
        }
        // Only this protocol's exact sibling names are eligible. The store lock
        // excludes active generation writers, including readers selecting a save.
        let artifacts = operation(Step::Scan, || {
            let mut artifacts = Vec::new();
            for entry in fs::read_dir(&session.directory)? {
                let entry = entry?;
                if entry.file_name().to_str().and_then(artifact_id).is_some() {
                    artifacts.push(entry.path());
                }
            }
            artifacts.sort();
            Ok(artifacts)
        })?;
        for artifact in artifacts {
            session.remove(&artifact, true)?;
            removed.push(artifact);
        }
        Ok(GenerationWrite {
            generation: id,
            path,
            retained: retained.into_iter().rev().collect(),
            removed,
            rejected,
        })
    }

    /// Select the newest intact generation loadable under the current definitions.
    /// Selection is read-only and reports skipped candidates. It never runs
    /// callbacks, rotates backups or mutates the live world and VM.
    ///
    /// # Errors
    /// Storage errors or no generation passing both integrity and loadability.
    pub fn select_generation(&self, store: &GenerationStore) -> Result<SelectedGeneration, Error> {
        let session = store.session()?;
        let candidates = session.scan()?;
        let mut rejected = Vec::new();
        for (id, path) in candidates {
            let candidate = boundary(Step::ReadRecovery, || Ok(read_verified(self, id, &path)))?;
            match candidate {
                Ok(bytes) => {
                    return Ok(SelectedGeneration {
                        generation: id,
                        path,
                        rejected,
                        bytes,
                    })
                }
                Err(reason) => rejected.push(RejectedGeneration {
                    generation: id,
                    path,
                    reason,
                }),
            }
        }
        Err(Error::NoLoadable { rejected })
    }

    /// Load the newest verified generation after selection has validated both blobs.
    ///
    /// # Errors
    /// The same selection errors, or an error returned by the native loader.
    pub fn recover_generation(
        &mut self,
        store: &GenerationStore,
    ) -> Result<SelectedGeneration, Error> {
        let selected = self.select_generation(store)?;
        self.load_state(selected.bytes())?;
        Ok(selected)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn session_releases_lock_while_a_duplicate_descriptor_remains_open() {
        let directory =
            std::env::temp_dir().join(format!("soe-generation-session-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let store = GenerationStore::new(&directory, 2).unwrap();
        let session = store.session().unwrap();
        // A cloned descriptor shares the lock just like one inherited by fork.
        let inherited = session.lock.try_clone().unwrap();
        assert!(matches!(
            store.session(),
            Err(Error::Io { step: Step::Lock, source })
                if source.kind() == io::ErrorKind::WouldBlock
        ));
        drop(session);
        let next = store.session().map(drop);
        drop(inherited);
        fs::remove_dir_all(directory).unwrap();
        assert!(next.is_ok(), "session lock was retained: {next:?}");
    }
}

/// Explicit test hooks, absent from the default build.
#[cfg(feature = "save-fault-tests")]
#[doc(hidden)]
pub mod fault {
    use super::{io, Step};
    use std::cell::RefCell;
    type Hook = Box<dyn FnMut(Step, bool) -> io::Result<()>>;
    thread_local! { static HOOK: RefCell<Option<Hook>> = RefCell::new(None); }

    pub fn with_hook<T>(
        hook: impl FnMut(Step, bool) -> io::Result<()> + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Restore(Option<Hook>);
        impl Drop for Restore {
            fn drop(&mut self) {
                HOOK.with(|slot| *slot.borrow_mut() = self.0.take());
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
