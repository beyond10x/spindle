//! The exclusive lock `run --state <path>` holds on its state until it exits
//! (`story:state-directory-lock`): an advisory lock on a lock file beside the state, and for a
//! state file on the database file itself, so a link to it meets the same lock, taken before any
//! store is opened. A second run on the same state is refused, naming the path and the
//! process that holds it, so two runs never keep two outboxes and event lists over one database.
//! `state move --from <directory>` takes the same lock before it reads the directory, so it never
//! reads files a run still writes. The operating system releases the lock with the process, after
//! a SIGKILL too.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// The lock file inside a state directory.
const IN_DIRECTORY: &str = "spindle.lock";

/// A held lock on one state, released when dropped or when the process ends.
#[derive(Debug)]
pub(crate) struct StateLock {
    _file: File,
    _database: Option<File>,
}

/// Locks the state directory `directory`, which exists, by `spindle.lock` inside it.
pub(crate) fn directory(directory: &Path) -> Result<StateLock> {
    Ok(StateLock {
        _file: acquire(directory, directory.join(IN_DIRECTORY))?,
        _database: None,
    })
}

/// Locks the state file `file`, created empty where it does not exist yet: the database file
/// itself, so every path to it, a symbolic or a hard link included, meets one lock; then
/// `<path>.lock` beside the path it resolves to, symbolic links followed, into which this
/// process writes its id, so a second run reached through a link names the holder too (one
/// reached through a hard link, which resolves to a path of its own, names "another process").
/// The database's lock is a whole-file `flock`, which on Linux is of another kind than SQLite's
/// byte-range locks on the file, so neither waits on the other. Drop the lock after the store:
/// closing the file releases this process's byte-range locks on it. On macOS an `flock` and a
/// byte-range lock on one file exclude each other, so SQLite would find its own database locked:
/// there only `<path>.lock` is taken, and a run reached through a hard link takes a lock file of
/// its own.
pub(crate) fn file(file: &Path) -> Result<StateLock> {
    let database = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(file)
        .with_context(|| format!("opening the state file {}", file.display()))?;
    let resolved = file
        .canonicalize()
        .with_context(|| format!("resolving the state file {}", file.display()))?;
    let mut lock = resolved.clone().into_os_string();
    lock.push(".lock");
    let lock = PathBuf::from(lock);
    let database = if cfg!(target_os = "linux") {
        match database.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => bail!(
                "the state {} is in use by {}, which holds {}: one state is run by one process \
                 at a time",
                file.display(),
                holder(&lock),
                resolved.display()
            ),
            Err(TryLockError::Error(error)) => {
                return Err(error).with_context(|| format!("locking {}", resolved.display()));
            }
        }
        Some(database)
    } else {
        None
    };
    Ok(StateLock {
        _file: acquire(file, lock)?,
        _database: database,
    })
}

/// Who holds a state file whose lock file is `lock`: `process <id>`, the id written into `lock`,
/// while a process holds `lock`; else "another process", one that reached the file by another
/// path, and an id a process that ended left behind is not named.
fn holder(lock: &Path) -> String {
    let Ok(mut file) = File::open(lock) else {
        return "another process".to_owned();
    };
    if !matches!(file.try_lock_shared(), Err(TryLockError::WouldBlock)) {
        return "another process".to_owned();
    }
    let mut written = String::new();
    match file.read_to_string(&mut written) {
        Ok(_) if !written.trim().is_empty() => format!("process {}", written.trim()),
        _ => "another process".to_owned(),
    }
}

/// Takes the lock on `lock` for `state` without waiting, then writes this process's id into it;
/// where another process holds it, an error naming `state` and the id that process wrote.
fn acquire(state: &Path, lock: PathBuf) -> Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock)
        .with_context(|| format!("opening the lock file {}", lock.display()))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            let mut written = String::new();
            let holder = match file.read_to_string(&mut written) {
                Ok(_) if !written.trim().is_empty() => format!("process {}", written.trim()),
                _ => "another process".to_owned(),
            };
            bail!(
                "the state {} is in use by {holder}, which holds {}: one state is run by one \
                 process at a time",
                state.display(),
                lock.display()
            );
        }
        Err(TryLockError::Error(error)) => {
            return Err(error).with_context(|| format!("locking {}", lock.display()));
        }
    }
    file.set_len(0)
        .and_then(|()| file.rewind())
        .and_then(|()| writeln!(file, "{}", std::process::id()))
        .and_then(|()| file.flush())
        .with_context(|| format!("writing this process's id into {}", lock.display()))?;
    Ok(file)
}
