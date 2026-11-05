//! The rally example on three Firecracker microVMs, and the same placement on QEMU and on
//! Firecracker (design § 12, M3).
//!
//! `#[ignore]`d: run by `task check:vm`. Each case fails naming what is missing, as the cases of
//! `tests/firecracker.rs` do: `/dev/kvm`, the Firecracker binary (`SPINDLE_FIRECRACKER` or
//! `PATH`), an uncompressed guest kernel (`SPINDLE_FIRECRACKER_KERNEL`), and for the QEMU half
//! `qemu-system-x86_64`, the host kernel with its virtio modules and the musl target. The static
//! guest binaries are built by the case itself (`cargo build --release --target
//! x86_64-unknown-linux-musl`, stripped, into this workspace's target directory), so they are
//! always those of the tree under test.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde_json::json;
use spindle_firecracker::{Artifact, Firecracker, Initramfs, Launcher, TargetKind};

#[path = "../../spindle-qemu/tests/support/rally.rs"]
mod rally;

const GUEST_TARGET: &str = "x86_64-unknown-linux-musl";

struct Host {
    firecracker: PathBuf,
    kernel: PathBuf,
    init: PathBuf,
    spindle: PathBuf,
}

/// The Firecracker host, or a panic naming everything this host lacks to boot the rally on
/// Firecracker.
fn firecracker() -> Host {
    let mut missing = Vec::new();
    if let Err(error) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
    {
        missing.push(format!("/dev/kvm cannot be opened: {error}"));
    }
    let firecracker = Firecracker::locate()
        .map_err(|error| missing.push(error.to_string()))
        .ok();
    let kernel = std::env::var_os("SPINDLE_FIRECRACKER_KERNEL").map(PathBuf::from);
    match &kernel {
        None => missing.push(
            "SPINDLE_FIRECRACKER_KERNEL names no guest kernel (an uncompressed vmlinux)".to_owned(),
        ),
        Some(kernel) if !kernel.is_file() => missing.push(format!(
            "SPINDLE_FIRECRACKER_KERNEL={} is not a file",
            kernel.display()
        )),
        Some(_) => {}
    }
    let (Some(firecracker), Some(kernel), true) = (firecracker, kernel, missing.is_empty()) else {
        panic!(
            "this host cannot boot the rally on Firecracker: {}",
            missing.join("; ")
        );
    };
    let (init, spindle) = guest_binaries().clone();
    Host {
        firecracker,
        kernel,
        init,
        spindle,
    }
}

/// `spindle-guest-init` and `b10x-spindle`, built once per test process for [`GUEST_TARGET`] in
/// release, stripped, by the `cargo` that runs the tests (`$CARGO`, `--locked`). A failed build,
/// or a binary missing after it, fails the case.
fn guest_binaries() -> &'static (PathBuf, PathBuf) {
    static BUILT: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
    BUILT.get_or_init(|| {
        let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .parent()
            .expect("the target directory")
            .to_path_buf();
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
        let output = Command::new(&cargo)
            .current_dir(rally::root())
            .args(["build", "--locked", "--release", "--target", GUEST_TARGET])
            .args(["--config", "profile.release.strip=true"])
            .arg("--target-dir")
            .arg(&target_dir)
            .args(["-p", "spindle-cli", "--bin", "b10x-spindle"])
            .args(["-p", "spindle-firecracker", "--bin", "spindle-guest-init"])
            .output()
            .unwrap_or_else(|error| panic!("cannot run {}: {error}", cargo.to_string_lossy()));
        assert!(
            output.status.success(),
            "the static guest build failed ({}):\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let bin = target_dir.join(GUEST_TARGET).join("release");
        let init = bin.join("spindle-guest-init");
        let spindle = bin.join("b10x-spindle");
        for path in [&init, &spindle] {
            assert!(
                path.is_file(),
                "{} is missing after the static guest build",
                path.display()
            );
        }
        (init, spindle)
    })
}

impl Host {
    /// A guest holding `b10x-spindle`, the rally specification and the environment documents,
    /// written under `dir`, whose `/init` runs `b10x-spindle` with `arguments`.
    fn artifact(&self, dir: &Path, arguments: &[String]) -> Artifact {
        let mut image = Initramfs::spindle_guest(&self.init, &self.spindle).expect("a guest root");
        image
            .copy_tree(&rally::spec(), rally::GUEST_SPEC)
            .expect("the specification");
        image
            .copy_tree(&rally::environments(), rally::GUEST_ENVIRONMENTS)
            .expect("the environment documents");
        let initramfs = dir.join("rally-firecracker.cpio");
        let mut file = std::fs::File::create(&initramfs).expect("the initramfs file");
        image.write_to(&mut file).expect("the initramfs is written");
        Artifact::Guest {
            kernel: self.kernel.clone(),
            initramfs,
            cmdline: format!("-- run {}", arguments.join(" ")),
        }
    }

    fn launcher(&self, dir: &Path) -> Firecracker {
        Firecracker::new(self.firecracker.clone(), dir.join("state"))
    }
}

#[test]
#[ignore = "boots three Firecracker microVMs: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl target"]
fn a_match_between_three_firecracker_microvms_ends_with_a_winner() {
    let host = firecracker();
    let placement = rally::placement();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a work directory");
    let artifact = host.artifact(dir.path(), &placement.play_arguments());
    let mut launcher = host.launcher(dir.path());

    let played = rally::play(&mut launcher, &placement.plans(&artifact));

    assert!(
        ["West", "East"].contains(&played.winner()),
        "{:?}",
        played.view
    );
}

#[test]
#[ignore = "boots three QEMU and three Firecracker microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules, the musl target, SPINDLE_FIRECRACKER and SPINDLE_FIRECRACKER_KERNEL"]
fn the_same_rally_placement_runs_on_qemu_and_on_firecracker() {
    let host = firecracker();
    let qemu = rally::qemu();
    let placement = rally::placement();
    let arguments = placement.play_arguments();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a work directory");
    let qemu_artifact = qemu.artifact(dir.path(), &arguments);
    let firecracker_artifact = host.artifact(dir.path(), &arguments);
    assert_ne!(
        qemu_artifact, firecracker_artifact,
        "each target kind boots its own guest"
    );

    let mut qemu_launcher = qemu.launcher(&dir.path().join("qemu"));
    let mut firecracker_launcher = host.launcher(&dir.path().join("firecracker"));
    let mut links = Vec::new();
    for (kind, launcher, artifact) in [
        (
            TargetKind::QemuMicrovm,
            &mut qemu_launcher as &mut dyn Launcher,
            &qemu_artifact,
        ),
        (
            TargetKind::Firecracker,
            &mut firecracker_launcher as &mut dyn Launcher,
            &firecracker_artifact,
        ),
    ] {
        assert_eq!(launcher.kind(), kind);
        let played = rally::play(launcher, &placement.plans(artifact));
        assert!(
            ["West", "East"].contains(&played.winner()),
            "{kind:?}: {:?}",
            played.view
        );
        for placed in &placement.instances {
            let link = played.links.get(&placed.component).unwrap_or_else(|| {
                panic!(
                    "{kind:?}: no instance {} ran: {:?}",
                    placed.component, played.links
                )
            });
            assert_eq!(
                (&link["component"], &link["listening"]),
                (&json!(placed.component), &json!(placed.listen.to_string())),
                "{kind:?}: the guest of {} plays it at the document's address",
                placed.component
            );
        }
        links.push((kind, played.links));
    }
    assert_eq!(
        links[0].1, links[1].1,
        "the guests on {:?} and on {:?} played the same components at the same addresses with \
         the same peers",
        links[0].0, links[1].0
    );
}
