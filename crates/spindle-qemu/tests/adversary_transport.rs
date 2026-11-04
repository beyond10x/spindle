//! Adversary cases for the QEMU launcher's telemetry: guest context ids.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::path::PathBuf;

use spindle_qemu::{Artifact, LaunchPlan, Network, QemuLauncher, Resources};

fn network() -> Network {
    let port = UdpSocket::bind("127.0.0.1:0")
        .and_then(|socket| socket.local_addr())
        .map(|address| address.port())
        .expect("a free loopback port");
    Network::multicast(SocketAddrV4::new(Ipv4Addr::new(239, 255, 77, 1), port), 24)
}

fn plan(id: &str) -> LaunchPlan {
    LaunchPlan {
        instance_id: id.to_owned(),
        component: "rally-table".to_owned(),
        settings: Vec::new(),
        artifact: Artifact::Guest {
            kernel: PathBuf::from("vmlinuz"),
            initramfs: PathBuf::from("initramfs.cpio"),
            cmdline: "-- /bin/spindle-qemu-probe facts".to_owned(),
        },
        request: Resources {
            vcpus: 1,
            memory_mib: 256,
            disk_mib: 0,
        },
        listen: SocketAddr::from(([10, 77, 0, 4], 7000)),
        peers: Vec::new(),
    }
}

fn cid(argv: &[String]) -> String {
    argv.iter()
        .find_map(|arg| arg.strip_prefix("vhost-vsock-device,guest-cid="))
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("no vsock device in {argv:?}"))
}

/// A context id is unique on the host. Two launchers of one process (two deployments, or two
/// test threads) must not give their first guests the same one: QEMU then fails to start the
/// second with `vhost-vsock: unable to set guest cid: Address already in use`.
#[test]
fn two_launchers_of_one_process_give_their_guests_different_context_ids() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let first = QemuLauncher::new(scratch.path().join("a"), network()).with_telemetry(7341);
    let second = QemuLauncher::new(scratch.path().join("b"), network()).with_telemetry(7341);
    let a = cid(&first.command(&plan("left")).expect("an argv"));
    let b = cid(&second.command(&plan("right")).expect("an argv"));
    assert_ne!(a, b, "both launchers' next guest takes context id {a}");
}
