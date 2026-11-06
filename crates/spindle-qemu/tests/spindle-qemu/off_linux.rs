//! Off Linux the QEMU launcher and the guest init refuse by name (`story:macos-host-builds`).

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::process::Command;

use spindle_qemu::{
    Artifact, LINUX_ONLY, LaunchError, LaunchPlan, Network, QemuLauncher, Resources,
};

#[test]
fn a_launch_is_refused_as_unsupported_at_its_first_spawn() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = dir.path().join("guest.cpio");
    std::fs::write(&initramfs, b"").expect("an initramfs");
    let plan = LaunchPlan {
        instance_id: "off-linux-0".to_owned(),
        component: "rally-table".to_owned(),
        settings: Vec::new(),
        artifact: Artifact::Guest {
            kernel: dir.path().join("vmlinuz"),
            initramfs,
            cmdline: "-- /bin/true".to_owned(),
        },
        request: Resources {
            vcpus: 1,
            memory_mib: 256,
            disk_mib: 0,
        },
        listen: SocketAddr::from(([10, 77, 0, 2], 7000)),
        peers: Vec::new(),
    };
    let network = Network::multicast(SocketAddrV4::new(Ipv4Addr::new(239, 255, 77, 1), 7001), 24);
    let mut launcher = QemuLauncher::new(dir.path().join("work"), network);
    let launched = launcher.launch(&plan);
    match launched {
        Err(LaunchError::Io { source, .. }) => {
            assert_eq!(source.kind(), ErrorKind::Unsupported, "{source}");
            assert_eq!(source.to_string(), LINUX_ONLY);
        }
        Err(other) => panic!("refused, but not as unsupported: {other}"),
        Ok(_) => panic!("a QEMU microVM was launched off Linux"),
    }
}

#[test]
fn the_guest_init_exits_with_status_2() {
    let output = Command::new(env!("CARGO_BIN_EXE_spindle-qemu-init"))
        .output()
        .expect("the guest init runs");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Linux guest only"),
        "{output:?}"
    );
}
