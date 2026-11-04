//! Acceptance cases for the QEMU microVM launcher.
//!
//! The cases marked `#[ignore]` boot guests. They need `/dev/kvm`, `qemu-system-x86_64`, the host
//! kernel image with its virtio modules, `unshare`, `nsenter` and `ip`, and the
//! `x86_64-unknown-linux-musl` Rust target; run them with `--ignored`, and they fail, naming what
//! is missing, on a host without one. `SPINDLE_QEMU_WORK_DIR` moves the guest build products
//! (static binaries, initramfs images, consoles) out of the tree's `target/`.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use spindle_capability::{Environment, derive};
use spindle_qemu::{
    Artifact, BuildError, Exit, GUEST_OUTPUT, GuestImage, HostKernel, LaunchError, LaunchPlan,
    Launcher, Network, Peer, QemuLauncher, ResourceKind, Resources, StaticBinaries, TargetKind,
};
use spindle_target::{EnvironmentAdmission, StartError, Target, TargetState};

const BOOT: Duration = Duration::from_secs(90);
const NEEDS: &str = "boots QEMU microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and \
                     its virtio modules, unshare, nsenter, ip and the x86_64-unknown-linux-musl target";

struct Host {
    qemu: PathBuf,
    kernel: HostKernel,
    binaries: StaticBinaries,
    work: PathBuf,
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn work_dir() -> PathBuf {
    std::env::var_os("SPINDLE_QEMU_WORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_TARGET_TMPDIR")).join("spindle-qemu"))
}

fn prepare() -> Result<Host, String> {
    let (qemu, kernel) = spindle_qemu::vm_prerequisites()?;
    let work = work_dir();
    std::fs::create_dir_all(&work).map_err(|error| format!("{}: {error}", work.display()))?;
    let binaries = match StaticBinaries::build(&workspace(), &work.join("musl-target")) {
        Ok(binaries) => binaries,
        Err(BuildError::TargetMissing(reason)) => return Err(reason),
        Err(error) => return Err(format!("the static guest build failed: {error}")),
    };
    Ok(Host {
        qemu,
        kernel,
        binaries,
        work,
    })
}

/// The host, or a panic naming what it lacks: these cases run only when asked for.
fn host() -> &'static Host {
    static HOST: OnceLock<Result<Host, String>> = OnceLock::new();
    match HOST.get_or_init(prepare) {
        Ok(host) => host,
        Err(reason) => panic!("this host cannot boot a QEMU microVM: {reason}"),
    }
}

fn free_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .and_then(|socket| socket.local_addr())
        .map(|address| address.port())
        .expect("a free loopback port")
}

fn network() -> Network {
    Network::multicast(
        SocketAddrV4::new(Ipv4Addr::new(239, 255, 77, 1), free_port()),
        24,
    )
}

fn guest_address(last: u8, port: u16) -> SocketAddr {
    SocketAddr::from(([10, 77, 0, last], port))
}

fn plan(id: &str, artifact: Artifact, request: Resources, listen: SocketAddr) -> LaunchPlan {
    LaunchPlan {
        instance_id: id.to_owned(),
        component: "rally-table".to_owned(),
        settings: vec![("tick_ms".to_owned(), "100".to_owned())],
        artifact,
        request,
        listen,
        peers: Vec::new(),
    }
}

fn small() -> Resources {
    Resources {
        vcpus: 1,
        memory_mib: 256,
        disk_mib: 0,
    }
}

fn paper_artifact() -> Artifact {
    Artifact::Guest {
        kernel: PathBuf::from("vmlinuz"),
        initramfs: PathBuf::from("initramfs.cpio"),
        cmdline: "-- /bin/spindle-qemu-probe facts".to_owned(),
    }
}

fn image_with_probe(host: &Host, dir: &Path) -> PathBuf {
    let mut image = GuestImage::new(&host.binaries.init, &host.kernel).expect("a guest image");
    image
        .binary("/bin/spindle-qemu-probe", &host.binaries.probe)
        .expect("the probe binary");
    let path = dir.join("probe.cpio");
    image.write(&path).expect("the initramfs is written");
    path
}

fn guest(host: &Host, initramfs: PathBuf, command: &str) -> Artifact {
    Artifact::Guest {
        kernel: host.kernel.image.clone(),
        initramfs,
        cmdline: format!("-- {command}"),
    }
}

fn launcher(host: &Host, dir: &Path) -> QemuLauncher {
    QemuLauncher::new(dir.join("instances"), network()).with_qemu(host.qemu.clone())
}

fn console(path: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(path).unwrap_or_default()).into_owned()
}

/// The lines the guest command wrote, without the init's prefix.
fn guest_lines(console: &str) -> Vec<String> {
    console
        .lines()
        .filter_map(|line| line.trim_end_matches('\r').strip_prefix(GUEST_OUTPUT))
        .map(str::to_owned)
        .collect()
}

fn finished(vm: &mut spindle_qemu::QemuInstance) -> Exit {
    match vm.wait(BOOT) {
        Ok(Some(exit)) => exit,
        Ok(None) => {
            let text = console(vm.console());
            let _ = vm.stop();
            panic!("the guest did not finish within {BOOT:?}; console:\n{text}");
        }
        Err(error) => panic!(
            "waiting failed: {error}; console:\n{}",
            console(vm.console())
        ),
    }
}

const OK: Exit = Exit {
    code: Some(0),
    signal: None,
};

#[test]
#[ignore = "boots QEMU microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules, unshare, nsenter, ip and the x86_64-unknown-linux-musl target"]
fn a_qemu_microvm_instance_boots_and_runs_b10x_spindle() {
    let host = host();
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let rally = workspace().join("examples/rally/ess");
    let loaded = spindle_spec::load(&rally).expect("the rally specification loads on the host");

    let mut image = GuestImage::new(&host.binaries.init, &host.kernel).expect("a guest image");
    image
        .binary("/bin/b10x-spindle", &host.binaries.spindle)
        .expect("the b10x-spindle binary");
    image.tree("/spec", &rally).expect("the specification");
    let initramfs = dir.path().join("spindle.cpio");
    image.write(&initramfs).expect("the initramfs is written");

    let mut launcher = launcher(host, dir.path());
    let artifact = guest(host, initramfs, "/bin/b10x-spindle check /spec");
    let mut vm = launcher
        .launch(&plan("table-1", artifact, small(), guest_address(2, 7000)))
        .expect("the microVM starts");
    let exit = finished(&mut vm);
    let text = console(vm.console());

    assert_eq!(exit, OK, "console:\n{text}");
    let expected = format!(
        "{} {} compiled from {} file(s)",
        loaded.ir.system(),
        loaded.ir.version(),
        loaded.files.len()
    );
    assert!(
        guest_lines(&text)
            .iter()
            .any(|line| line.starts_with(&expected)),
        "the console lacks {expected:?}:\n{text}"
    );
}

#[test]
#[ignore = "boots QEMU microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules, unshare, nsenter, ip and the x86_64-unknown-linux-musl target"]
fn a_failing_guest_command_reports_its_exit_code() {
    let host = host();
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let initramfs = image_with_probe(host, dir.path());
    let mut launcher = launcher(host, dir.path());
    let artifact = guest(host, initramfs, "/bin/spindle-qemu-probe exit 7");
    let mut vm = launcher
        .launch(&plan("exit-7", artifact, small(), guest_address(2, 7000)))
        .expect("the microVM starts");
    let exit = finished(&mut vm);
    assert_eq!(
        exit,
        Exit {
            code: Some(7),
            signal: None
        },
        "console:\n{}",
        console(vm.console())
    );
}

#[test]
#[ignore = "boots QEMU microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules, unshare, nsenter, ip and the x86_64-unknown-linux-musl target"]
fn two_qemu_instances_reach_each_other_over_tcp() {
    let host = host();
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let initramfs = image_with_probe(host, dir.path());
    let mut launcher = launcher(host, dir.path());

    let server = guest_address(2, 7000);
    let client = guest_address(3, 7000);
    let mut serve = plan(
        "west",
        guest(
            host,
            initramfs.clone(),
            &format!("/bin/spindle-qemu-probe serve {server}"),
        ),
        small(),
        server,
    );
    serve.peers.push(Peer {
        component: "east".to_owned(),
        address: client,
    });
    let mut call = plan(
        "east",
        guest(
            host,
            initramfs,
            &format!("/bin/spindle-qemu-probe call {server} ping-from-east"),
        ),
        small(),
        client,
    );
    call.peers.push(Peer {
        component: "west".to_owned(),
        address: server,
    });

    let mut west = launcher.launch(&serve).expect("the serving microVM starts");
    let mut east = launcher.launch(&call).expect("the calling microVM starts");
    let east_exit = finished(&mut east);
    let west_exit = finished(&mut west);
    let east_console = console(east.console());
    let west_console = console(west.console());

    assert_eq!(
        east_exit, OK,
        "east:\n{east_console}\nwest:\n{west_console}"
    );
    assert_eq!(west_exit, OK, "west:\n{west_console}");
    assert!(
        guest_lines(&east_console).contains(&"reply: pong ping-from-east".to_owned()),
        "east never got the reply:\n{east_console}"
    );
    assert!(
        guest_lines(&west_console)
            .iter()
            .any(|line| line.starts_with("served 10.77.0.3:")),
        "west never served east:\n{west_console}"
    );
}

fn facts(text: &str) -> (Option<u64>, Option<u64>, Option<String>) {
    let lines = guest_lines(text);
    let value = |key: &str| {
        lines
            .iter()
            .find_map(|line| line.strip_prefix(key).map(str::to_owned))
    };
    (
        value("cpus=").and_then(|cpus| cpus.parse().ok()),
        value("mem_total_kib=").and_then(|kib| kib.parse().ok()),
        value("instance="),
    )
}

fn assert_guest_resources(text: &str, vcpus: u64, memory_mib: u64, instance: &str) {
    let (cpus, total_kib, id) = facts(text);
    assert_eq!(cpus, Some(vcpus), "console:\n{text}");
    let total_kib = total_kib.unwrap_or_else(|| panic!("no mem_total_kib line:\n{text}"));
    assert!(
        total_kib > (memory_mib - 64) * 1024 && total_kib <= memory_mib * 1024,
        "guest memory {total_kib} KiB is not about {memory_mib} MiB:\n{text}"
    );
    assert_eq!(
        id.as_deref(),
        Some(instance),
        "the launch plan is not in the guest:\n{text}"
    );
}

fn pair(argv: &[String], flag: &str) -> Option<String> {
    argv.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

#[test]
fn vcpus_and_memory_are_in_the_qemu_argv() {
    let request = Resources {
        vcpus: 2,
        memory_mib: 320,
        disk_mib: 0,
    };
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let argv = QemuLauncher::new(scratch.path().to_path_buf(), network())
        .command(&plan(
            "facts",
            paper_artifact(),
            request,
            guest_address(4, 7000),
        ))
        .expect("an argv");
    assert_eq!(pair(&argv, "-smp").as_deref(), Some("2"), "argv: {argv:?}");
    assert_eq!(pair(&argv, "-m").as_deref(), Some("320"), "argv: {argv:?}");
    assert_eq!(
        pair(&argv, "-M").as_deref(),
        Some("microvm"),
        "argv: {argv:?}"
    );
    for flag in [
        "-enable-kvm",
        "-nodefaults",
        "-no-user-config",
        "-nographic",
    ] {
        assert!(
            argv.iter().any(|arg| arg == flag),
            "argv lacks {flag}: {argv:?}"
        );
    }
    assert_eq!(
        pair(&argv, "-cpu").as_deref(),
        Some("host"),
        "argv: {argv:?}"
    );
}

#[test]
#[ignore = "boots QEMU microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules, unshare, nsenter, ip and the x86_64-unknown-linux-musl target"]
fn vcpus_and_memory_are_passed_to_the_vm() {
    let host = host();
    let request = Resources {
        vcpus: 2,
        memory_mib: 320,
        disk_mib: 0,
    };
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let initramfs = image_with_probe(host, dir.path());
    let mut launcher = launcher(host, dir.path());
    let mut vm = launcher
        .launch(&plan(
            "facts",
            guest(host, initramfs, "/bin/spindle-qemu-probe facts"),
            request,
            guest_address(4, 7000),
        ))
        .expect("the microVM starts");
    assert_eq!(pair(vm.argv(), "-smp").as_deref(), Some("2"));
    assert_eq!(pair(vm.argv(), "-m").as_deref(), Some("320"));
    let exit = finished(&mut vm);
    let text = console(vm.console());
    assert_eq!(exit, OK, "console:\n{text}");
    assert_guest_resources(&text, 2, 320, "facts");
}

fn microvm_admission() -> EnvironmentAdmission {
    let loaded = spindle_spec::load(&workspace().join("examples/rally/ess")).expect("compiles");
    let requirements = derive(&loaded.ir, Some("rally-table")).expect("the component exists");
    let path = workspace().join("examples/environments/microvm.toml");
    let text = std::fs::read_to_string(&path).expect("the environment document reads");
    let environment = Environment::parse(&text).expect("the environment parses");
    EnvironmentAdmission::decide(&requirements, &environment)
}

fn qemu_target(ceiling: Resources) -> Target {
    Target {
        id: "qemu-1".to_owned(),
        kind: TargetKind::QemuMicrovm,
        environment: "microvm".to_owned(),
        ceiling,
        state: TargetState::Available,
    }
}

#[test]
#[ignore = "boots QEMU microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules, unshare, nsenter, ip and the x86_64-unknown-linux-musl target"]
fn an_instance_started_through_the_target_contract_gets_the_ceiling() {
    let host = host();
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let initramfs = image_with_probe(host, dir.path());
    let mut launcher = launcher(host, dir.path());
    assert_eq!(Launcher::kind(&launcher), TargetKind::QemuMicrovm);
    let target = qemu_target(Resources {
        vcpus: 2,
        memory_mib: 320,
        disk_mib: 0,
    });
    let unasked = plan(
        "placed",
        guest(host, initramfs, "/bin/spindle-qemu-probe facts"),
        Resources::default(),
        guest_address(8, 7000),
    );
    let mut running = spindle_target::start(&mut launcher, &target, &microvm_admission(), &unasked)
        .expect("placed and launched");
    let exit = match running.wait(BOOT).expect("waits") {
        Some(exit) => exit,
        None => {
            let _ = running.stop();
            panic!(
                "the guest did not finish; console:\n{}",
                console(running.console())
            );
        }
    };
    let text = console(running.console());
    assert_eq!(running.instance_id(), "placed");
    assert_eq!(exit, OK, "console:\n{text}");
    assert_guest_resources(&text, 2, 320, "placed");
}

#[test]
fn a_qemu_target_without_a_ceiling_is_refused_by_the_launcher() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let instances = scratch.path().join("instances");
    let mut launcher = QemuLauncher::new(instances.clone(), network());
    let unasked = plan(
        "unbounded",
        paper_artifact(),
        Resources::default(),
        guest_address(8, 7000),
    );
    let started = spindle_target::start(
        &mut launcher,
        &qemu_target(Resources::default()),
        &microvm_admission(),
        &unasked,
    );
    assert!(
        matches!(
            started,
            Err(StartError::Launch(LaunchError::InvalidPlan(_)))
        ),
        "a zero request on a zero ceiling reached QEMU: {:?}",
        started.map(|running| running.instance_id().to_owned())
    );
    assert!(
        !instances.exists(),
        "something was prepared for a refused plan"
    );
}

#[test]
fn a_zero_vcpu_or_memory_request_is_refused() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let launcher = QemuLauncher::new(scratch.path().join("instances"), network());
    for request in [
        Resources {
            vcpus: 0,
            memory_mib: 256,
            disk_mib: 0,
        },
        Resources {
            vcpus: 1,
            memory_mib: 0,
            disk_mib: 0,
        },
    ] {
        let argv = launcher.command(&plan(
            "zero",
            paper_artifact(),
            request,
            guest_address(2, 7000),
        ));
        assert!(
            matches!(argv, Err(LaunchError::InvalidPlan(_))),
            "{request:?} was accepted: {argv:?}"
        );
    }
}

#[test]
fn a_missing_prerequisite_is_named_in_the_reason() {
    let empty = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let missing = HostKernel::discover_in(empty.path(), empty.path(), "6.0.0-test")
        .expect_err("an empty /boot holds no kernel");
    let reason = missing.to_string();
    assert!(
        reason.contains("no readable kernel image for release 6.0.0-test"),
        "{reason}"
    );
    assert!(NEEDS.contains("/dev/kvm") && NEEDS.contains("musl"));
    if let Err(reason) = spindle_qemu::vm_prerequisites() {
        assert!(
            ["/dev/kvm", "qemu-system-x86_64", "kernel", "module"]
                .iter()
                .any(|needed| reason.contains(needed)),
            "the reason names nothing: {reason}"
        );
    }
}

#[test]
fn a_disk_request_is_refused_before_anything_starts() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let instances = scratch.path().join("instances");
    let mut launcher = QemuLauncher::new(instances.clone(), network());
    assert_eq!(
        QemuLauncher::enforces(&launcher)
            .into_iter()
            .collect::<Vec<_>>(),
        vec![ResourceKind::Vcpus, ResourceKind::Memory]
    );
    let request = Resources {
        vcpus: 1,
        memory_mib: 256,
        disk_mib: 64,
    };
    let refused = launcher.launch(&plan(
        "disk",
        paper_artifact(),
        request,
        guest_address(5, 7000),
    ));
    assert!(
        matches!(refused, Err(LaunchError::Unenforced(ResourceKind::Disk))),
        "expected Unenforced(Disk), got {:?}",
        refused.map(|vm| vm.instance_id().to_owned())
    );
    assert!(
        !instances.exists(),
        "something was prepared for a refused plan"
    );
}

#[test]
fn a_host_binary_artifact_is_refused() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut launcher = QemuLauncher::new(scratch.path().join("instances"), network());
    let artifact = Artifact::HostBinary {
        path: PathBuf::from("b10x-spindle"),
        args: Vec::new(),
    };
    let refused = launcher.launch(&plan("host", artifact, small(), guest_address(6, 7000)));
    assert!(matches!(
        refused,
        Err(LaunchError::UnsupportedArtifact {
            launcher: TargetKind::QemuMicrovm,
            artifact: "host binary"
        })
    ));
}

#[test]
#[ignore = "boots QEMU microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules, unshare, nsenter, ip and the x86_64-unknown-linux-musl target"]
fn stop_kills_a_running_instance() {
    let host = host();
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let initramfs = image_with_probe(host, dir.path());
    let mut launcher = launcher(host, dir.path());
    let listen = guest_address(7, 7000);
    let mut vm = launcher
        .launch(&plan(
            "forever",
            guest(
                host,
                initramfs,
                &format!("/bin/spindle-qemu-probe serve {listen}"),
            ),
            small(),
            listen,
        ))
        .expect("the microVM starts");
    assert_eq!(vm.wait(Duration::from_secs(2)).expect("waits"), None);
    let exit = vm.stop().expect("stops");
    assert_eq!(
        exit,
        Exit {
            code: None,
            signal: Some(libc::SIGKILL)
        }
    );
}

#[test]
#[ignore = "boots QEMU microVMs: needs /dev/kvm, qemu-system-x86_64, the host kernel and its virtio modules, unshare, nsenter, ip and the x86_64-unknown-linux-musl target"]
fn a_qemu_instance_reports_its_share_of_the_requested_quota() {
    let host = host();
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let initramfs = image_with_probe(host, dir.path());
    let mut launcher = launcher(host, dir.path());
    let listen = guest_address(8, 7000);
    let request = Resources {
        vcpus: 2,
        memory_mib: 256,
        disk_mib: 0,
    };
    let mut vm = launcher
        .launch(&plan(
            "sampled",
            guest(
                host,
                initramfs,
                &format!("/bin/spindle-qemu-probe serve {listen}"),
            ),
            request,
            listen,
        ))
        .expect("the microVM starts");
    assert_eq!(vm.wait(Duration::from_secs(2)).expect("waits"), None);

    let sample = vm.resources().expect("a running microVM is sampled");
    let cpu_share = sample.cpu_seconds / (sample.wall_seconds * 2.0);
    let memory_share = sample.memory_bytes as f64 / (256.0 * 1024.0 * 1024.0);
    assert!(sample.cpu_seconds > 0.0, "QEMU has used CPU: {sample:?}");
    assert!(
        sample.memory_bytes >= 8 << 20,
        "QEMU holds memory: {sample:?}"
    );
    assert!(sample.wall_seconds >= 2.0, "{sample:?}");
    let close = |share: Option<f64>, expected: f64| {
        share.is_some_and(|share| (share - expected).abs() <= expected.abs() * 1e-12)
    };
    assert!(
        close(sample.cpu_share, cpu_share),
        "cpu_share is cpu_seconds / (wall_seconds * 2 vCPUs) = {cpu_share}: {sample:?}"
    );
    assert!(
        close(sample.memory_share, memory_share),
        "memory_share is memory_bytes / 256 MiB = {memory_share}: {sample:?}"
    );

    vm.stop().expect("stops");
    assert_eq!(vm.resources(), None, "a stopped microVM is not sampled");
}
