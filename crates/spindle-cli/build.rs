//! Embeds the observe panel, `ui/dist-panel/panel.html` as `pnpm build:panel` writes it, into
//! `b10x-spindle`.
//!
//! `$OUT_DIR/panel.rs` holds `PANEL`, the page as one `&'static str`, or `None` without it: the
//! build goes on, and `run --observe` then answers `GET /__spindle/ui` with 503, naming the page
//! (`task build` and `task check` build `ui/` before cargo).

use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let page = manifest.join("../../ui/dist-panel/panel.html");
    println!("cargo:rerun-if-changed={}", page.display());
    let panel = if page.is_file() {
        let absolute = fs::canonicalize(&page).expect("the page exists");
        format!(
            "pub(crate) static PANEL: Option<&str> = Some(include_str!({:?}));\n",
            absolute.to_string_lossy()
        )
    } else {
        println!(
            "cargo:warning=ui/dist-panel/panel.html is missing: run --observe serves no panel; build ui/ first (task check:ui)"
        );
        "pub(crate) static PANEL: Option<&str> = None;\n".to_owned()
    };
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets it"));
    fs::write(out.join("panel.rs"), panel).expect("writing panel.rs");
}
