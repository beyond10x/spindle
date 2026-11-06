//! Off Linux the Firecracker launcher and the guest init refuse by name
//! (`story:macos-host-builds`).

use std::io::ErrorKind;
use std::net::SocketAddr;
use std::process::Command;

use spindle_firecracker::{Artifact, Firecracker, LINUX_ONLY, LaunchError, LaunchPlan, Resources};

#[test]
fn a_launch_is_refused_as_unsupported_at_its_first_spawn() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let binary = dir.path().join("firecracker");
    let kernel = dir.path().join("vmlinux");
    let initramfs = dir.path().join("guest.cpio");
    for file in [&binary, &kernel, &initramfs] {
        std::fs::write(file, b"").expect("a stand-in file");
    }
    let plan = LaunchPlan {
        instance_id: "off-linux-0".to_owned(),
        component: "rally-table".to_owned(),
        settings: Vec::new(),
        artifact: Artifact::Guest {
            kernel,
            initramfs,
            cmdline: "-- run".to_owned(),
        },
        request: Resources {
            vcpus: 1,
            memory_mib: 256,
            disk_mib: 0,
        },
        listen: SocketAddr::from(([172, 16, 0, 2], 7000)),
        peers: Vec::new(),
    };
    let mut launcher = Firecracker::new(binary, dir.path().join("state"));
    match launcher.launch(&plan) {
        Err(LaunchError::Io { source, .. }) => {
            assert_eq!(source.kind(), ErrorKind::Unsupported, "{source}");
            assert_eq!(source.to_string(), LINUX_ONLY);
        }
        Err(other) => panic!("refused, but not as unsupported: {other}"),
        Ok(_) => panic!("a Firecracker microVM was launched off Linux"),
    }
}

#[test]
fn the_guest_init_exits_with_status_2() {
    let output = Command::new(env!("CARGO_BIN_EXE_spindle-guest-init"))
        .output()
        .expect("the guest init runs");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Linux guest only"),
        "{output:?}"
    );
}
