//! Acceptance tests of the Firecracker launcher.
//!
//! The VM tests are `#[ignore]`d: they need `/dev/kvm`, the Firecracker release binary
//! (`SPINDLE_FIRECRACKER` or `PATH`), an uncompressed guest kernel (`SPINDLE_FIRECRACKER_KERNEL`)
//! and the static guest binaries (`cargo build --release --config profile.release.strip=true
//! --target x86_64-unknown-linux-musl -p spindle-cli -p spindle-firecracker`, or
//! `SPINDLE_GUEST_BIN_DIR`; debug builds do not fit a 256 MiB guest). Run with `--ignored`, each
//! fails naming what is missing.

use std::io::BufReader;
use std::net::SocketAddr;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use spindle_capability::{Environment, derive};
use spindle_firecracker::{
    Artifact, FIRECRACKER_INSERTS, Firecracker, FirecrackerVm, Initramfs, LaunchError, LaunchPlan,
    MAX_COMMAND_LINE, Peer, ResourceKind, Resources, TELEMETRY_INSERTS, TargetKind,
    kernel_command_line, telemetry_command_line,
};
use spindle_target::{EnvironmentAdmission, Target, TargetState};
use spindle_telemetry::FrameReader;

const BOOT_TIMEOUT: Duration = Duration::from_secs(90);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Host {
    firecracker: PathBuf,
    kernel: PathBuf,
    init: PathBuf,
    spindle: PathBuf,
}

/// Every reason the VM tests cannot run here, or what they run with.
fn requirements(
    kvm: &Path,
    firecracker: Result<PathBuf, LaunchError>,
    kernel: Option<PathBuf>,
    bin: &Path,
) -> Result<Host, String> {
    let mut missing = Vec::new();
    if let Err(error) = std::fs::OpenOptions::new().read(true).write(true).open(kvm) {
        missing.push(format!("{} cannot be opened: {error}", kvm.display()));
    }
    let firecracker = firecracker
        .map_err(|error| missing.push(error.to_string()))
        .ok();
    let kernel = match kernel {
        None => {
            missing.push(
                "SPINDLE_FIRECRACKER_KERNEL names no guest kernel (an uncompressed vmlinux)"
                    .to_string(),
            );
            None
        }
        Some(kernel) if !kernel.is_file() => {
            missing.push(format!(
                "SPINDLE_FIRECRACKER_KERNEL={} is not a file",
                kernel.display()
            ));
            None
        }
        Some(kernel) => Some(kernel),
    };
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

fn host() -> Host {
    let bin = std::env::var_os("SPINDLE_GUEST_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target/x86_64-unknown-linux-musl/release"));
    requirements(
        Path::new("/dev/kvm"),
        Firecracker::locate(),
        std::env::var_os("SPINDLE_FIRECRACKER_KERNEL").map(PathBuf::from),
        &bin,
    )
    .unwrap_or_else(|reason| panic!("this VM test cannot run here: {reason}"))
}

fn guest_root(host: &Host, directory: &Path, name: &str) -> PathBuf {
    let mut image = Initramfs::spindle_guest(&host.init, &host.spindle).expect("the guest root");
    image
        .copy_tree(&root().join("examples/rally/ess"), "spec")
        .expect("the rally specification is copied");
    let path = directory.join(format!("{name}.cpio"));
    let mut file = std::fs::File::create(&path).expect("the initramfs file");
    image.write_to(&mut file).expect("the initramfs is written");
    path
}

fn plan(
    host: &Host,
    initramfs: &Path,
    id: &str,
    listen: &str,
    peers: &[(&str, &str)],
    init: &str,
) -> LaunchPlan {
    LaunchPlan {
        instance_id: id.to_string(),
        component: id.to_string(),
        settings: vec![("tick".to_string(), "50ms".to_string())],
        artifact: Artifact::Guest {
            kernel: host.kernel.clone(),
            initramfs: initramfs.to_path_buf(),
            cmdline: format!("-- {init}"),
        },
        request: Resources {
            vcpus: 2,
            memory_mib: 256,
            disk_mib: 0,
        },
        listen: listen.parse().expect("a socket address"),
        peers: peers
            .iter()
            .map(|(component, address)| Peer {
                component: component.to_string(),
                address: address.parse::<SocketAddr>().expect("a socket address"),
            })
            .collect(),
    }
}

fn console(path: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(path).unwrap_or_default()).into_owned()
}

/// The guest's exit, after it ends within [`BOOT_TIMEOUT`], with its console.
fn ended(vm: &mut FirecrackerVm) -> (spindle_firecracker::Exit, String) {
    let exit = vm.wait(BOOT_TIMEOUT);
    let text = console(vm.console());
    match exit {
        Ok(Some(exit)) => (exit, text),
        Ok(None) => {
            let _ = vm.stop();
            panic!(
                "{} did not end within {BOOT_TIMEOUT:?}; console:\n{text}",
                vm.instance_id()
            )
        }
        Err(error) => panic!("{} failed: {error}", vm.instance_id()),
    }
}

fn memory_mib(text: &str, cpus: u32) -> u64 {
    let marker = format!("spindle-init: cpus={cpus} memory_mib=");
    text.lines()
        .find_map(|line| line.split(marker.as_str()).nth(1))
        .and_then(|rest| rest.trim().parse::<u64>().ok())
        .unwrap_or_else(|| panic!("the guest reports {cpus} vCPUs and its memory:\n{text}"))
}

fn rally_check_line() -> String {
    let loaded = spindle_spec::load(&root().join("examples/rally/ess")).expect("rally loads");
    format!(
        "{} {} compiled from {} file(s)",
        loaded.ir.system(),
        loaded.ir.version(),
        loaded.files.len()
    )
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_firecracker_instance_boots_and_runs_b10x_spindle() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path(), "boot");
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"));
    assert_eq!(launcher.kind(), TargetKind::Firecracker);
    assert_eq!(
        launcher.enforces().into_iter().collect::<Vec<_>>(),
        vec![ResourceKind::Vcpus, ResourceKind::Memory]
    );

    let plan = plan(
        &host,
        &initramfs,
        "table",
        "172.16.0.2:7000",
        &[],
        "run check /spec",
    );
    let mut vm = launcher.launch(&plan).expect("the instance launches");
    assert_eq!(vm.instance_id(), "table");
    let (exit, text) = ended(&mut vm);
    assert_eq!(
        exit,
        spindle_firecracker::Exit {
            code: Some(0),
            signal: None
        },
        "console:\n{text}"
    );
    let expected = rally_check_line();
    assert!(
        text.contains(&expected),
        "the console shows `{expected}`:\n{text}"
    );
    let memory = memory_mib(&text, 2);
    assert!(
        (200..=256).contains(&memory),
        "the guest has about the 256 MiB requested, not {memory}:\n{text}"
    );
    assert!(
        text.contains("spindle-init: SPINDLE_SETTING_TICK=50ms"),
        "the settings reach the guest:\n{text}"
    );
    assert!(
        !directory.path().join("state/table/api.sock").exists(),
        "the API socket is gone once the VM ended"
    );
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn two_firecracker_instances_reach_each_other_over_tcp() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path(), "pair");
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"));

    let left = plan(
        &host,
        &initramfs,
        "left",
        "172.16.0.2:7000",
        &[("right", "172.16.0.3:7000")],
        "exchange",
    );
    let right = plan(
        &host,
        &initramfs,
        "right",
        "172.16.0.3:7000",
        &[("left", "172.16.0.2:7000")],
        "exchange",
    );
    let mut vms = [
        launcher.launch(&left).expect("left launches"),
        launcher.launch(&right).expect("right launches"),
    ];
    let mut consoles = Vec::new();
    for vm in &mut vms {
        let (exit, text) = ended(vm);
        assert_eq!(exit.code, Some(0), "{exit:?}; console:\n{text}");
        consoles.push(text);
    }
    assert!(
        consoles[0]
            .contains("spindle-init: right at 172.16.0.3:7000 answered: right got hello from left"),
        "left reached right over TCP:\n{}",
        consoles[0]
    );
    assert!(
        consoles[1]
            .contains("spindle-init: left at 172.16.0.2:7000 answered: left got hello from right"),
        "right reached left over TCP:\n{}",
        consoles[1]
    );
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_rally_component_placed_on_a_firecracker_target_runs_within_its_ceiling() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path(), "placed");
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"));
    let loaded = spindle_spec::load(&root().join("examples/rally/ess")).expect("rally loads");
    let requirements = derive(&loaded.ir, Some("rally-table")).expect("the component exists");
    let text = std::fs::read_to_string(root().join("examples/environments/microvm.toml"))
        .expect("the environment reads");
    let environment = Environment::parse(&text).expect("the environment parses");
    let admission = EnvironmentAdmission::decide(&requirements, &environment);
    let target = Target {
        id: "firecracker-1".to_string(),
        kind: TargetKind::Firecracker,
        environment: environment.name.clone(),
        ceiling: Resources {
            vcpus: 2,
            memory_mib: 192,
            disk_mib: 0,
        },
        state: TargetState::Available,
    };
    let mut plan = plan(
        &host,
        &initramfs,
        "rally-table",
        "172.16.0.2:7000",
        &[],
        "run check /spec",
    );
    plan.request = Resources::default();

    let mut running = spindle_target::start(&mut launcher, &target, &admission, &plan)
        .expect("the placed instance starts");
    assert_eq!(running.instance_id(), "rally-table");
    let exit = running
        .wait(BOOT_TIMEOUT)
        .expect("waiting succeeds")
        .unwrap_or_else(|| {
            let _ = running.stop();
            panic!("the guest did not end within {BOOT_TIMEOUT:?}")
        });
    let text = console(running.console());
    assert_eq!(exit.code, Some(0), "{exit:?}; console:\n{text}");
    assert!(text.contains(&rally_check_line()), "console:\n{text}");
    let memory = memory_mib(&text, 2);
    assert!(
        (150..=192).contains(&memory),
        "the request left at zero runs with the ceiling's 192 MiB, not {memory}:\n{text}"
    );
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_command_line_at_the_limit_reaches_init_whole() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path(), "long");
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"));
    let mut plan = plan(
        &host,
        &initramfs,
        "long",
        "172.16.0.2:7000",
        &[],
        "run check /spec",
    );
    plan.settings = vec![("pad".to_string(), "p".to_string())];
    let short = kernel_command_line(&plan).expect("valid").len();
    let pad = "p".repeat(1 + MAX_COMMAND_LINE - short);
    plan.settings = vec![("pad".to_string(), pad.clone())];
    assert_eq!(
        kernel_command_line(&plan).expect("at the limit").len(),
        MAX_COMMAND_LINE
    );
    let mut vm = launcher
        .launch(&plan)
        .expect("a command line at the limit launches");
    let (exit, text) = ended(&mut vm);
    assert_eq!(exit.code, Some(0), "{exit:?}; console:\n{text}");
    let received = text
        .lines()
        .find_map(|line| {
            line.trim_end()
                .strip_prefix("spindle-init: SPINDLE_SETTING_PAD=")
        })
        .unwrap_or_else(|| panic!("init reports the padding variable:\n{text}"));
    assert_eq!(
        received.len(),
        pad.len(),
        "the last variable reaches init whole"
    );
    assert!(text.contains(&rally_check_line()), "console:\n{text}");
}

#[test]
fn a_resource_firecracker_does_not_enforce_is_refused_before_launch() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut launcher = Firecracker::new(
        directory.path().join("no-such-firecracker"),
        directory.path().join("state"),
    );
    let plan = LaunchPlan {
        instance_id: "disk".to_string(),
        component: "disk".to_string(),
        settings: Vec::new(),
        artifact: Artifact::Guest {
            kernel: directory.path().join("vmlinux"),
            initramfs: directory.path().join("initramfs.cpio"),
            cmdline: String::new(),
        },
        request: Resources {
            vcpus: 1,
            memory_mib: 128,
            disk_mib: 64,
        },
        listen: "172.16.0.2:7000".parse().expect("a socket address"),
        peers: Vec::new(),
    };
    match launcher.launch(&plan) {
        Err(LaunchError::Unenforced(ResourceKind::Disk)) => {}
        Err(other) => panic!("expected Unenforced(Disk), got {other}"),
        Ok(_) => panic!("expected Unenforced(Disk), got a running instance"),
    }
    assert!(
        !directory.path().join("state").exists(),
        "nothing was prepared before the refusal"
    );
}

#[test]
fn a_vm_test_that_cannot_run_names_everything_it_misses() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let reason = match requirements(
        &directory.path().join("kvm"),
        Err(LaunchError::NotFound(
            "no firecracker binary: set SPINDLE_FIRECRACKER or put `firecracker` on PATH"
                .to_string(),
        )),
        None,
        &directory.path().join("bin"),
    ) {
        Ok(_) => panic!("nothing is there, so the VM tests cannot run"),
        Err(reason) => reason,
    };
    for needed in [
        "kvm cannot be opened",
        "set SPINDLE_FIRECRACKER or put `firecracker` on PATH",
        "SPINDLE_FIRECRACKER_KERNEL names no guest kernel",
        "spindle-guest-init is missing",
        "b10x-spindle is missing",
        "x86_64-unknown-linux-musl",
    ] {
        assert!(reason.contains(needed), "`{needed}` in: {reason}");
    }
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_firecracker_instance_reports_its_share_of_the_requested_quota() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path(), "sampled");
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"));
    let plan = plan(
        &host,
        &initramfs,
        "sampled",
        "172.16.0.2:7000",
        &[("nobody", "172.16.0.9:7000")],
        "exchange --timeout 60",
    );
    let mut vm = launcher.launch(&plan).expect("the instance launches");
    assert_eq!(
        vm.wait(Duration::from_secs(2)).expect("waits"),
        None,
        "console:\n{}",
        console(vm.console())
    );

    let sample = vm.resources().expect("a running microVM is sampled");
    let cpu_share = sample.cpu_seconds / (sample.wall_seconds * 2.0);
    let memory_share = sample.memory_bytes as f64 / (256.0 * 1024.0 * 1024.0);
    assert!(
        sample.cpu_seconds > 0.0,
        "Firecracker has used CPU: {sample:?}"
    );
    assert!(
        sample.memory_bytes >= 8 << 20,
        "Firecracker holds memory: {sample:?}"
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

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_firecracker_instance_reports_over_vsock() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path(), "reporting");
    let port = 7341;
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"))
        .with_telemetry(port);
    let plan = plan(
        &host,
        &initramfs,
        "reporting",
        "172.16.0.2:7000",
        &[],
        "telemetry --frames 3",
    );
    let mut vm = launcher.launch(&plan).expect("the instance launches");
    let socket = vm
        .telemetry_socket()
        .expect("a launcher with telemetry gives the VM a vsock device");
    assert_eq!(
        socket,
        directory
            .path()
            .join(format!("state/reporting/vsock.sock_{port}"))
    );
    // A socket path must fit the 108 bytes of `sun_path`: bind through a descriptor of its
    // directory, as the launcher reaches the API socket.
    let parent = std::fs::File::open(socket.parent().expect("a directory"))
        .expect("the instance directory opens");
    let short = PathBuf::from(format!(
        "/proc/self/fd/{}/{}",
        std::os::fd::AsRawFd::as_raw_fd(&parent),
        socket.file_name().expect("a name").to_string_lossy()
    ));
    let listener = UnixListener::bind(&short).expect("the collector listens on the vsock UDS");
    listener
        .set_nonblocking(true)
        .expect("a non-blocking listener");
    let started = std::time::Instant::now();
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if started.elapsed() > BOOT_TIMEOUT {
                    let text = console(vm.console());
                    let _ = vm.stop();
                    panic!("the guest never connected; console:\n{text}");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => panic!("accept: {error}"),
        }
    };
    stream.set_nonblocking(false).expect("a blocking stream");
    stream
        .set_read_timeout(Some(BOOT_TIMEOUT))
        .expect("a read timeout");
    let mut collector = FrameReader::new(BufReader::new(stream));
    let mut sequences = Vec::new();
    for _ in 0..3 {
        let frame = match collector.next_frame() {
            Ok(Some(frame)) => frame,
            other => {
                let text = console(vm.console());
                let _ = vm.stop();
                panic!("no frame from the guest ({other:?}); console:\n{text}");
            }
        };
        assert_eq!(frame.instance_id, "reporting");
        assert!(
            frame.resources.cpu_seconds > 0.0 && frame.resources.memory_bytes > 0,
            "the guest's own readings arrive: {:?}",
            frame.resources
        );
        sequences.push(frame.sequence);
    }
    let (exit, text) = ended(&mut vm);
    assert_eq!(
        exit,
        spindle_firecracker::Exit {
            code: Some(0),
            signal: None
        },
        "console:\n{text}"
    );
    assert!(
        sequences.windows(2).all(|pair| pair[0] < pair[1]),
        "sequence numbers increase: {sequences:?}"
    );
    assert!(
        text.contains(&format!("spindle-init: SPINDLE_TELEMETRY=vsock:2:{port}")),
        "the endpoint reaches the guest:\n{text}"
    );
    let inserted = format!("{FIRECRACKER_INSERTS}{TELEMETRY_INSERTS} ");
    assert!(
        text.lines()
            .any(|line| line.contains("Command line:") && line.contains(&inserted)),
        "Firecracker inserts `{inserted}` with a vsock device:\n{text}"
    );
}

#[test]
fn a_telemetry_endpoint_fits_within_the_command_line_limit() {
    let mut host_plan = LaunchPlan {
        instance_id: "left".to_string(),
        component: "rally-table".to_string(),
        settings: Vec::new(),
        artifact: Artifact::Guest {
            kernel: PathBuf::from("vmlinux"),
            initramfs: PathBuf::from("initramfs.cpio"),
            cmdline: "-- exchange".to_string(),
        },
        request: Resources {
            vcpus: 1,
            memory_mib: 128,
            disk_mib: 0,
        },
        listen: "172.16.0.2:7000".parse().expect("an address"),
        peers: Vec::new(),
    };
    let plain = kernel_command_line(&host_plan).expect("valid");
    assert_eq!(
        telemetry_command_line(&host_plan, None).expect("valid"),
        plain
    );
    let reporting = telemetry_command_line(&host_plan, Some(7341)).expect("valid with an endpoint");
    assert!(
        reporting.contains(" SPINDLE_TELEMETRY=vsock:2:7341"),
        "{reporting}"
    );

    let limit = MAX_COMMAND_LINE - TELEMETRY_INSERTS.len();
    let overhead = " SPINDLE_SETTING_PAD=".len();
    let fits = limit - reporting.len() - overhead;
    let mut padded = |length: usize| {
        host_plan.settings = vec![("pad".to_string(), "a".repeat(length))];
        telemetry_command_line(&host_plan, Some(7341))
    };
    assert_eq!(padded(fits).expect("at the limit").len(), limit);
    assert!(
        matches!(padded(fits + 1), Err(LaunchError::InvalidPlan(_))),
        "one byte more than Firecracker leaves with a vsock device is refused"
    );
}
