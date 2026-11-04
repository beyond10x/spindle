//! The static guest binaries: `b10x-spindle`, this crate's init and its probe, built for
//! `x86_64-unknown-linux-musl` with the workspace's own toolchain.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The Rust target every guest binary is built for.
pub const GUEST_TARGET: &str = "x86_64-unknown-linux-musl";

/// Why the static build did not produce binaries.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The toolchain has no standard library for [`GUEST_TARGET`].
    #[error("{0}")]
    TargetMissing(String),
    /// `cargo build` ran and failed.
    #[error("cargo build --target {GUEST_TARGET} failed ({status}):\n{log}")]
    Cargo { status: String, log: String },
    /// A process could not be run.
    #[error("cannot run {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
}

/// Paths of the built static binaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticBinaries {
    /// `spindle-qemu-init`, the guest's `/init`.
    pub init: PathBuf,
    /// `b10x-spindle`.
    pub spindle: PathBuf,
    /// `spindle-qemu-probe`, a network and resource probe for guests.
    pub probe: PathBuf,
}

impl StaticBinaries {
    /// Build the guest binaries of the workspace at `workspace` into `target_dir`.
    ///
    /// Runs the `cargo` that runs this process (`$CARGO`) when there is one, with `--locked`, so
    /// the guest is built from the same lockfile and toolchain as the host.
    pub fn build(workspace: &Path, target_dir: &Path) -> Result<Self, BuildError> {
        ensure_target(workspace)?;
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
        let output = Command::new(&cargo)
            .current_dir(workspace)
            .args(["build", "--locked", "--target", GUEST_TARGET])
            .arg("--target-dir")
            .arg(target_dir)
            .args(["-p", "spindle-cli", "--bin", "b10x-spindle"])
            .args(["-p", "spindle-qemu", "--bin", "spindle-qemu-init"])
            .args(["--bin", "spindle-qemu-probe"])
            .env("CARGO_PROFILE_DEV_STRIP", "symbols")
            .output()
            .map_err(|source| BuildError::Spawn {
                program: cargo.to_string_lossy().into_owned(),
                source,
            })?;
        if !output.status.success() {
            return Err(BuildError::Cargo {
                status: output.status.to_string(),
                log: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        let dir = target_dir.join(GUEST_TARGET).join("debug");
        Ok(Self {
            init: dir.join("spindle-qemu-init"),
            spindle: dir.join("b10x-spindle"),
            probe: dir.join("spindle-qemu-probe"),
        })
    }
}

fn ensure_target(workspace: &Path) -> Result<(), BuildError> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let output = Command::new(&rustc)
        .current_dir(workspace)
        .args(["--print", "sysroot"])
        .output()
        .map_err(|source| BuildError::Spawn {
            program: rustc.to_string_lossy().into_owned(),
            source,
        })?;
    let sysroot = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let library = sysroot.join("lib/rustlib").join(GUEST_TARGET).join("lib");
    if library.is_dir() {
        Ok(())
    } else {
        Err(BuildError::TargetMissing(format!(
            "the Rust target {GUEST_TARGET} is not installed for the toolchain at {} \
             (rustup target add {GUEST_TARGET})",
            sysroot.display()
        )))
    }
}
