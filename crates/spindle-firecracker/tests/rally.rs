//! The rally example on three Firecracker microVMs, and the same placement on QEMU and on
//! Firecracker (design § 12, M3).
//!
//! `#[ignore]`d: run by `task check:vm`. Each case prints why and returns where a prerequisite is
//! absent: `/dev/kvm`, the Firecracker binary (`SPINDLE_FIRECRACKER` or `PATH`), an uncompressed
//! guest kernel (`SPINDLE_FIRECRACKER_KERNEL`), the static guest binaries (`cargo build --release
//! --config profile.release.strip=true --target x86_64-unknown-linux-musl -p spindle-cli -p
//! spindle-firecracker`, or `SPINDLE_GUEST_BIN_DIR`), and for the QEMU half `qemu-system-x86_64`,
//! the host kernel with its virtio modules and the musl target.

use std::path::{Path, PathBuf};

use spindle_firecracker::{Artifact, Firecracker, Initramfs};

#[path = "../../spindle-qemu/tests/support/rally.rs"]
mod rally;

struct Host {
    firecracker: PathBuf,
    kernel: PathBuf,
    init: PathBuf,
    spindle: PathBuf,
}

/// The Firecracker host, or every reason this host cannot boot the rally on Firecracker.
fn firecracker() -> Result<Host, String> {
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
    let bin = std::env::var_os("SPINDLE_GUEST_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| rally::root().join("target/x86_64-unknown-linux-musl/release"));
    let init = bin.join("spindle-guest-init");
    let spindle = bin.join("b10x-spindle");
    for path in [&init, &spindle] {
        if !path.is_file() {
            missing.push(format!(
                "{} is missing: build the guest binaries with `cargo build --release --config \
                 profile.release.strip=true --target x86_64-unknown-linux-musl -p spindle-cli -p \
                 spindle-firecracker` or set SPINDLE_GUEST_BIN_DIR",
                path.display()
            ));
        }
    }
    match (firecracker, kernel) {
        (Some(firecracker), Some(kernel)) if missing.is_empty() => Ok(Host {
            firecracker,
            kernel,
            init,
            spindle,
        }),
        _ => Err(missing.join("; ")),
    }
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
#[ignore = "boots three Firecracker microVMs: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_match_between_three_firecracker_microvms_ends_with_a_winner() {
    let host = match firecracker() {
        Ok(host) => host,
        Err(reason) => {
            return rally::skip(
                "a_match_between_three_firecracker_microvms_ends_with_a_winner",
                &reason,
            );
        }
    };
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
#[ignore = "boots three QEMU and three Firecracker microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules, the musl target, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn the_same_rally_placement_runs_on_qemu_and_on_firecracker() {
    const TEST: &str = "the_same_rally_placement_runs_on_qemu_and_on_firecracker";
    let host = match firecracker() {
        Ok(host) => host,
        Err(reason) => return rally::skip(TEST, &reason),
    };
    let qemu = match rally::qemu() {
        Ok(qemu) => qemu,
        Err(reason) => return rally::skip(TEST, &reason),
    };
    let placement = rally::placement();
    let arguments = placement.play_arguments();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a work directory");

    let on_qemu = placement.plans(&qemu.artifact(dir.path(), &arguments));
    let on_firecracker = placement.plans(&host.artifact(dir.path(), &arguments));
    let without_artifact = |plans: &[spindle_firecracker::LaunchPlan]| {
        plans
            .iter()
            .map(|plan| {
                (
                    plan.instance_id.clone(),
                    plan.component.clone(),
                    plan.listen,
                    plan.request,
                    plan.peers.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        without_artifact(&on_qemu),
        without_artifact(&on_firecracker),
        "only the artifact differs between the two targets"
    );

    let mut qemu_launcher = qemu.launcher(&dir.path().join("qemu"));
    let mut firecracker_launcher = host.launcher(&dir.path().join("firecracker"));
    for (kind, launcher, plans) in [
        (
            spindle_firecracker::TargetKind::QemuMicrovm,
            &mut qemu_launcher as &mut dyn spindle_firecracker::Launcher,
            &on_qemu,
        ),
        (
            spindle_firecracker::TargetKind::Firecracker,
            &mut firecracker_launcher as &mut dyn spindle_firecracker::Launcher,
            &on_firecracker,
        ),
    ] {
        assert_eq!(launcher.kind(), kind);
        let played = rally::play(launcher, plans);
        assert!(
            ["West", "East"].contains(&played.winner()),
            "{kind:?}: {:?}",
            played.view
        );
    }
}
