//! The exclusive lock `run --state <path>` holds on its state until it exits
//! (`story:state-directory-lock`): an advisory lock on a lock file beside the state, taken before
//! any store is opened. A second run on the same state is refused, naming the path and the
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
}

/// Locks the state directory `directory`, which exists, by `spindle.lock` inside it.
pub(crate) fn directory(directory: &Path) -> Result<StateLock> {
    acquire(directory, directory.join(IN_DIRECTORY))
}

/// Locks the state file `file` by `<file>.lock` beside it.
pub(crate) fn file(file: &Path) -> Result<StateLock> {
    let mut lock = file.as_os_str().to_owned();
    lock.push(".lock");
    acquire(file, PathBuf::from(lock))
}

/// Takes the lock on `lock` for `state` without waiting, then writes this process's id into it;
/// where another process holds it, an error naming `state` and the id that process wrote.
fn acquire(state: &Path, lock: PathBuf) -> Result<StateLock> {
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
    Ok(StateLock { _file: file })
}
