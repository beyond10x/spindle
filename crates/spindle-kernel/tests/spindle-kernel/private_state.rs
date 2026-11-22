//! A SQLite store keeps its state private (`story:private-local-state`): a state directory it
//! creates is `0700`, and the database and every journal or WAL file beside it `0600`, whatever
//! the umask. In 0.17.0, under umask `0022`, the databases were created `0644`. A file that exists
//! keeps the mode it has.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::Mutex;

use rustix::fs::Mode;
use spindle_kernel::SqliteStore;

fn mode(path: &Path) -> String {
    format!(
        "{:o}",
        fs::metadata(path)
            .expect("the path exists")
            .permissions()
            .mode()
            & 0o777
    )
}

/// Held while a case of this module has the process umask changed, so two never interleave.
static UMASK: Mutex<()> = Mutex::new(());

/// Runs `body` with the process umask set to `umask`, restoring the one before.
fn under_umask<T>(umask: u32, body: impl FnOnce() -> T) -> T {
    let _held = UMASK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let before = rustix::process::umask(Mode::from_raw_mode(umask));
    let result = body();
    rustix::process::umask(before);
    result
}

#[test]
fn a_new_sqlite_store_is_private() {
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let outer = root.path().join("state");
    let directory = outer.join("components");
    let path = directory.join("rally.sqlite");
    let store = under_umask(0o022, || SqliteStore::open(&path)).expect("the store opens");
    assert_eq!(mode(&outer), "700", "{} is private", outer.display());
    assert_eq!(
        mode(&directory),
        "700",
        "{} is private",
        directory.display()
    );
    let mut created: Vec<String> = Vec::new();
    for entry in fs::read_dir(&directory).expect("the directory reads") {
        let entry = entry.expect("an entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        assert_eq!(
            mode(&entry.path()),
            "600",
            "{name} is readable by its owner only"
        );
        created.push(name);
    }
    created.sort();
    assert_eq!(
        created,
        ["rally.sqlite", "rally.sqlite-shm", "rally.sqlite-wal"],
        "the store created its database and its WAL files"
    );
    drop(store);
}

#[test]
fn a_new_sqlite_store_is_private_under_an_open_umask() {
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let directory = root.path().join("state");
    let path = directory.join("rally.sqlite");
    let store = under_umask(0o000, || SqliteStore::open(&path)).expect("the store opens");
    assert_eq!(mode(&directory), "700");
    for entry in fs::read_dir(&directory).expect("the directory reads") {
        let entry = entry.expect("an entry");
        assert_eq!(mode(&entry.path()), "600", "{}", entry.path().display());
    }
    drop(store);
}

#[test]
fn a_new_database_in_an_existing_directory_is_private() {
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = root.path().join("rally.sqlite");
    let store = under_umask(0o022, || SqliteStore::open(&path)).expect("the store opens");
    for name in ["rally.sqlite", "rally.sqlite-shm", "rally.sqlite-wal"] {
        assert_eq!(mode(&root.path().join(name)), "600", "{name}");
    }
    drop(store);
}

#[test]
fn an_existing_sqlite_file_keeps_its_mode() {
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o750)).expect("chmod");
    let path = root.path().join("shared.sqlite");
    fs::write(&path, b"").expect("an empty file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("chmod");
    let store = SqliteStore::open(&path).expect("the store opens");
    assert_eq!(mode(&path), "640", "an existing database keeps its mode");
    assert_eq!(
        mode(root.path()),
        "750",
        "an existing directory keeps its mode"
    );
    drop(store);
}
