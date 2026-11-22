//! Local state kept private to the user who runs spindle (`story:private-local-state`): a state
//! directory it creates is `0700` and a state file it creates `0600` on Unix, whatever the umask,
//! since a store's rows and the identities in them are no one else's to read. A directory or file
//! that exists keeps the mode it has. Elsewhere the operating system's defaults apply.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

/// The mode of a state directory spindle creates.
const DIRECTORY: u32 = 0o700;
/// The mode of a state file spindle creates.
const FILE: u32 = 0o600;

/// Creates `directory` and every parent of it that is missing, each one it creates `0700`; one
/// that exists keeps its mode.
///
/// # Errors
/// The error of the first directory that could not be created or given its mode.
pub fn create_dir_all(directory: &Path) -> io::Result<()> {
    if directory.as_os_str().is_empty() || directory.is_dir() {
        return Ok(());
    }
    if let Some(parent) = directory.parent() {
        create_dir_all(parent)?;
    }
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, DIRECTORY);
    match builder.create(directory) {
        Ok(()) => exactly(directory, DIRECTORY),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && directory.is_dir() => Ok(()),
        Err(error) => Err(error),
    }
}

/// Opens `file` to read and write, never truncated, creating it `0600` where it is absent; a file
/// that exists keeps its mode.
///
/// # Errors
/// The error of opening or creating the file, or of giving a file it created its mode.
pub fn open_or_create(file: &Path) -> io::Result<File> {
    let mut creating = OpenOptions::new();
    creating.read(true).write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut creating, FILE);
    match creating.open(file) {
        Ok(opened) => {
            exactly(file, FILE)?;
            Ok(opened)
        }
        // It exists, or is a symbolic link: opened as it is, and created through a link that
        // names no file yet, as an open with `create` would.
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let mut existing = OpenOptions::new();
            existing.read(true).write(true).create(true).truncate(false);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut existing, FILE);
            existing.open(file)
        }
        Err(error) => Err(error),
    }
}

/// Gives `path`, which this process just created, exactly `mode`: the mode it was created with
/// is narrowed by the umask, and this one is not.
#[cfg(unix)]
fn exactly(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

/// Off Unix a created path keeps the operating system's default.
#[cfg(not(unix))]
fn exactly(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}
