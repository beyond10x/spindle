//! Embeds the observe UI bundle, `ui/dist` as `pnpm build` writes it, into the crate.
//!
//! Every file under `ui/dist` becomes one entry of `FILES` in `$OUT_DIR/ui.rs`, by its path
//! relative to `ui/dist` with `/` separators. Without `ui/dist` the table is empty and the build
//! goes on: the control process then answers `GET /` with 503, naming the missing bundle
//! (`task build` and `task check` build `ui/` before cargo).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let dist = manifest.join("../../ui/dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    let mut files = Vec::new();
    if dist.is_dir() {
        collect(&dist, &dist, &mut files);
    } else {
        println!(
            "cargo:warning=ui/dist is missing: this build serves no UI; build ui/ first (task check:ui)"
        );
    }
    files.sort();
    let mut table = String::from("pub(crate) static FILES: &[(&str, &[u8])] = &[\n");
    for (name, path) in &files {
        let _ = writeln!(table, "    ({name:?}, include_bytes!({path:?})),");
    }
    table.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets it"));
    fs::write(out.join("ui.rs"), table).expect("writing ui.rs");
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("below the root")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let absolute = fs::canonicalize(&path).expect("the file exists");
            files.push((relative, absolute.to_string_lossy().into_owned()));
        }
    }
}
