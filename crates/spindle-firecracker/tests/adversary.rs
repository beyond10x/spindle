//! Adversary cases for the Firecracker launcher.
//!
//! The cases without a VM run everywhere: they start the launcher's namespace with a stand-in
//! binary (`true`) and show which plans are not refused before anything starts. The cases marked
//! `#[ignore]` boot real microVMs and fail, rather than skip, when the host lacks what they need;
//! run them with `--ignored` and `SPINDLE_FIRECRACKER`, `SPINDLE_FIRECRACKER_KERNEL` set.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use spindle_firecracker::{
    Artifact, Firecracker, FirecrackerVm, Initramfs, LaunchError, LaunchPlan, Peer, Resources,
};

const BOOT_TIMEOUT: Duration = Duration::from_secs(90);
const NEEDS_A_VM: &str = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, \
                          SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn stand_in_binary() -> PathBuf {
    ["/usr/bin/true", "/bin/true"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .expect("a `true` binary stands in for firecracker")
}

fn plan(kernel: &Path, initramfs: &Path, id: &str, listen: &str, init: &str) -> LaunchPlan {
    LaunchPlan {
        instance_id: id.to_string(),
        component: id.to_string(),
        settings: vec![("tick".to_string(), "50ms".to_string())],
        artifact: Artifact::Guest {
            kernel: kernel.to_path_buf(),
            initramfs: initramfs.to_path_buf(),
            cmdline: format!("-- {init}"),
        },
        request: Resources {
            vcpus: 1,
            memory_mib: 256,
            disk_mib: 0,
        },
        listen: listen.parse().expect("a socket address"),
        peers: Vec::new(),
    }
}

struct Dummy {
    directory: tempfile::TempDir,
    kernel: PathBuf,
    initramfs: PathBuf,
}

fn dummy(initramfs_bytes: u64) -> Dummy {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let kernel = directory.path().join("vmlinux");
    std::fs::write(&kernel, b"not a kernel").expect("a kernel file");
    let initramfs = directory.path().join("initramfs.cpio");
    std::fs::File::create(&initramfs)
        .and_then(|file| file.set_len(initramfs_bytes))
        .expect("an initramfs file");
    Dummy {
        directory,
        kernel,
        initramfs,
    }
}

fn assert_refused_up_front(dummy: &Dummy, plan: &LaunchPlan, why: &str) {
    let state = dummy.directory.path().join("state");
    let mut launcher = Firecracker::new(stand_in_binary(), state.clone());
    match launcher.launch(plan) {
        Err(LaunchError::InvalidPlan(_)) => {}
        Err(other) => panic!(
            "{why}: expected InvalidPlan before anything starts, but the launch went on and \
             failed with: {other}"
        ),
        Ok(_) => panic!("{why}: expected InvalidPlan, got a running instance"),
    }
    assert!(
        !state.exists(),
        "{why}: nothing is prepared before the refusal"
    );
}

#[test]
fn a_kernel_command_line_over_2048_bytes_is_refused_before_anything_starts() {
    let dummy = dummy(4096);
    let mut plan = plan(
        &dummy.kernel,
        &dummy.initramfs,
        "long",
        "172.16.0.2:7000",
        "exchange",
    );
    plan.settings = vec![("blob".to_string(), "a".repeat(3000))];
    assert_refused_up_front(&dummy, &plan, "a 3000-byte setting value");
}

#[test]
fn a_value_with_a_byte_the_kernel_splits_on_is_refused_before_anything_starts() {
    let dummy = dummy(4096);
    let mut plan = plan(
        &dummy.kernel,
        &dummy.initramfs,
        "split",
        "172.16.0.2:7000",
        "exchange",
    );
    plan.settings = vec![(
        "greeting".to_string(),
        "voil\u{e0}SPINDLE_SETTING_INJECTED=yes".to_string(),
    )];
    assert_refused_up_front(
        &dummy,
        &plan,
        "a value with U+00E0, whose UTF-8 ends in 0xA0, a kernel command-line space",
    );
}

#[test]
fn an_initramfs_larger_than_the_requested_memory_is_refused_before_anything_starts() {
    let dummy = dummy(64 * 1024 * 1024);
    let mut plan = plan(
        &dummy.kernel,
        &dummy.initramfs,
        "small",
        "172.16.0.2:7000",
        "exchange",
    );
    plan.request.memory_mib = 32;
    assert_refused_up_front(&dummy, &plan, "a 64 MiB initramfs in a 32 MiB guest");
}

#[test]
fn a_launch_that_failed_can_be_retried_with_the_same_instance_id() {
    let dummy = dummy(4096);
    let plan = plan(
        &dummy.kernel,
        &dummy.initramfs,
        "retry",
        "172.16.0.2:7000",
        "exchange",
    );
    let mut launcher = Firecracker::new(stand_in_binary(), dummy.directory.path().join("state"));
    let first = launcher.launch(&plan);
    assert!(
        matches!(first, Err(LaunchError::Vmm { .. })),
        "the stand-in exits before configuring: {:?}",
        first.err()
    );
    match launcher.launch(&plan) {
        Err(LaunchError::InvalidPlan(message)) if message.contains("already launched") => {
            panic!("a launch that started nothing blocks its retry: {message}")
        }
        _ => {}
    }
}

#[test]
fn guest_init_reports_a_status_for_the_largest_exchange_timeout() {
    let output = Command::new(env!("CARGO_BIN_EXE_spindle-guest-init"))
        .args(["exchange", "--timeout", &u64::MAX.to_string()])
        .env("SPINDLE_COMPONENT", "left")
        .env("SPINDLE_LISTEN", "127.0.0.1:0")
        .env("SPINDLE_PEER_RIGHT", "127.0.0.1:9")
        .output()
        .expect("guest init runs on the host");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("spindle-init: exit status "),
        "init prints its status line, as PID 1 must before it reboots; status {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
}

struct Host {
    firecracker: PathBuf,
    kernel: PathBuf,
    init: PathBuf,
    spindle: PathBuf,
}

fn host() -> Host {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .unwrap_or_else(|error| panic!("{NEEDS_A_VM}: /dev/kvm: {error}"));
    let firecracker = Firecracker::locate().unwrap_or_else(|error| panic!("{NEEDS_A_VM}: {error}"));
    let kernel = std::env::var_os("SPINDLE_FIRECRACKER_KERNEL")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .unwrap_or_else(|| panic!("{NEEDS_A_VM}: SPINDLE_FIRECRACKER_KERNEL"));
    let bin = std::env::var_os("SPINDLE_GUEST_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target/x86_64-unknown-linux-musl/release"));
    let init = bin.join("spindle-guest-init");
    let spindle = bin.join("b10x-spindle");
    for path in [&init, &spindle] {
        assert!(
            path.is_file(),
            "{NEEDS_A_VM}: {} is missing",
            path.display()
        );
    }
    Host {
        firecracker,
        kernel,
        init,
        spindle,
    }
}

fn guest_root(host: &Host, directory: &Path) -> PathBuf {
    let mut image = Initramfs::spindle_guest(&host.init, &host.spindle).expect("the guest root");
    image
        .copy_tree(&root().join("examples/rally/ess"), "spec")
        .expect("the rally specification is copied");
    let path = directory.join("guest.cpio");
    let mut file = std::fs::File::create(&path).expect("the initramfs file");
    image.write_to(&mut file).expect("the initramfs is written");
    path
}

fn evidence(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter(|line| {
            [
                "Unknown kernel command line",
                "Initramfs unpacking",
                "spindle-init:",
                "panicked",
                "Kernel panic",
            ]
            .iter()
            .any(|needle| line.contains(needle))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn finish(vm: &mut FirecrackerVm) -> String {
    let exit = vm.wait(BOOT_TIMEOUT).expect("waiting succeeds");
    if exit.is_none() {
        let _ = vm.stop();
    }
    format!(
        "exit {exit:?}, guest_status {:?}; console tail:\n{}",
        vm.guest_status(),
        evidence(vm.console())
    )
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_guest_whose_program_fails_is_not_reported_as_exit_0() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path());
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"));
    let plan = plan(
        &host.kernel,
        &initramfs,
        "failing",
        "172.16.0.2:7000",
        "run no-such-subcommand",
    );
    let mut vm = launcher.launch(&plan).expect("the instance launches");
    let exit = vm
        .wait(BOOT_TIMEOUT)
        .expect("waiting succeeds")
        .unwrap_or_else(|| {
            let _ = vm.stop();
            panic!("the guest did not end within {BOOT_TIMEOUT:?}")
        });
    let status = vm.guest_status();
    assert_ne!(status, Some(0), "the guest program failed");
    assert_ne!(
        exit.code,
        Some(0),
        "the Exit a Launcher reports (and ReportExit records as the instance's code) says the \
         instance succeeded although its program exited {status:?}"
    );
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_guest_too_small_for_its_initramfs_is_refused_at_launch() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path());
    let size_mib = std::fs::metadata(&initramfs).expect("the initramfs").len() / (1024 * 1024);
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"));
    let mut plan = plan(
        &host.kernel,
        &initramfs,
        "tiny",
        "172.16.0.2:7000",
        "run check /spec",
    );
    plan.request.memory_mib = 64;
    match launcher.launch(&plan) {
        Err(LaunchError::InvalidPlan(_)) => {}
        Err(other) => panic!("expected InvalidPlan, got {other}"),
        Ok(mut vm) => panic!(
            "a 64 MiB guest with a {size_mib} MiB initramfs was launched; {}",
            finish(&mut vm)
        ),
    }
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn two_instances_of_one_launcher_cannot_share_an_address() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path());
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"));
    let first = plan(
        &host.kernel,
        &initramfs,
        "first",
        "172.16.0.2:7000",
        "run check /spec",
    );
    let second = plan(
        &host.kernel,
        &initramfs,
        "second",
        "172.16.0.2:7000",
        "run check /spec",
    );
    let mut vm = launcher
        .launch(&first)
        .expect("the first instance launches");
    let refused = launcher.launch(&second);
    let _ = vm.stop();
    match refused {
        Err(LaunchError::InvalidPlan(_)) => {}
        Err(other) => panic!("expected InvalidPlan, got {other}"),
        Ok(mut other) => {
            let _ = other.stop();
            panic!(
                "a second instance with the first one's address (and so its MAC) was put on \
                 the same bridge"
            )
        }
    }
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn plans_firecracker_rejects_are_refused_as_invalid_plans() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let initramfs = guest_root(&host, directory.path());
    let mut launcher = Firecracker::new(host.firecracker.clone(), directory.path().join("state"));
    let mut long = plan(
        &host.kernel,
        &initramfs,
        "long",
        "172.16.0.2:7000",
        "run check /spec",
    );
    long.settings = vec![("blob".to_string(), "a".repeat(3000))];
    let mut split = plan(
        &host.kernel,
        &initramfs,
        "split",
        "172.16.0.3:7000",
        "run check /spec",
    );
    split.settings = vec![(
        "greeting".to_string(),
        "voil\u{e0}SPINDLE_SETTING_INJECTED=yes".to_string(),
    )];
    let mut outcomes = Vec::new();
    for plan in [long, split] {
        let outcome = match launcher.launch(&plan) {
            Err(LaunchError::InvalidPlan(_)) => continue,
            Err(other) => format!("failed late: {other}"),
            Ok(mut vm) => format!("launched: {}", finish(&mut vm)),
        };
        outcomes.push(format!("{}: {outcome}", plan.instance_id));
    }
    assert!(outcomes.is_empty(), "{}", outcomes.join("\n\n"));
}

fn pids_with_cwd(directory: &Path) -> Vec<i32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| {
            std::fs::read_link(format!("/proc/{pid}/cwd")).is_ok_and(|cwd| cwd == directory)
        })
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|stat| stat.rsplit(") ").next().map(|rest| !rest.starts_with('Z')))
                .unwrap_or(false)
        })
        .collect()
}

const HELPER_STATE: &str = "SPINDLE_ADVERSARY_HELPER_STATE";

struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_killed_launcher_leaves_no_firecracker_running() {
    let host = host();
    if let Some(state) = std::env::var_os(HELPER_STATE) {
        let state = PathBuf::from(state);
        let initramfs = guest_root(&host, state.parent().expect("a parent"));
        let mut launcher = Firecracker::new(host.firecracker.clone(), state);
        let mut plan = plan(
            &host.kernel,
            &initramfs,
            "orphan",
            "172.16.0.2:7000",
            "exchange --timeout 600",
        );
        plan.peers = vec![Peer {
            component: "nobody".to_string(),
            address: "172.16.0.9:7000".parse::<SocketAddr>().expect("an address"),
        }];
        let _vm = launcher.launch(&plan).expect("the instance launches");
        std::thread::sleep(Duration::from_secs(600));
        return;
    }
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let directory_path = directory.path().canonicalize().expect("a real path");
    let state = directory_path.join("state");
    let child = Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--exact",
            "a_killed_launcher_leaves_no_firecracker_running",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HELPER_STATE, &state)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the helper launcher starts");
    let mut child = KillOnDrop(child);
    let instance = state.join("orphan");
    let started = Instant::now();
    let pids = loop {
        let pids = pids_with_cwd(&instance);
        if !pids.is_empty() && instance.join("api.sock").exists() {
            break pids;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the helper launched no firecracker"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    std::thread::sleep(Duration::from_secs(2));
    child.0.kill().expect("the helper is killed");
    child.0.wait().expect("the helper is reaped");
    std::thread::sleep(Duration::from_secs(3));
    let survivors: Vec<_> = pids
        .iter()
        .copied()
        .filter(|pid| pids_with_cwd(&instance).contains(pid))
        .collect();
    for pid in &survivors {
        // SAFETY: `kill` takes no pointers; the PID is a firecracker this test started.
        unsafe {
            libc::kill(*pid, libc::SIGKILL);
        }
    }
    assert!(
        survivors.is_empty(),
        "firecracker {survivors:?} outlived the launcher process that started it (killed now)"
    );
}

#[test]
#[ignore = "boots Firecracker: needs /dev/kvm, SPINDLE_FIRECRACKER, SPINDLE_FIRECRACKER_KERNEL and the musl guest binaries"]
fn a_dropped_vm_in_a_deep_state_path_leaves_no_process_and_no_api_socket() {
    let host = host();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let deep = directory
        .path()
        .canonicalize()
        .expect("a real path")
        .join("d".repeat(120))
        .join("state");
    let initramfs = guest_root(&host, directory.path());
    let mut launcher = Firecracker::new(host.firecracker.clone(), deep.clone());
    let mut plan = plan(
        &host.kernel,
        &initramfs,
        "dropped",
        "172.16.0.2:7000",
        "exchange --timeout 600",
    );
    plan.peers = vec![Peer {
        component: "nobody".to_string(),
        address: "172.16.0.9:7000".parse::<SocketAddr>().expect("an address"),
    }];
    let instance = deep.join("dropped");
    assert!(
        instance.join("api.sock").as_os_str().len() > 108,
        "the socket path is longer than sun_path"
    );
    let vm = launcher.launch(&plan).expect("a deep state path launches");
    let pids = pids_with_cwd(&instance);
    assert!(
        !pids.is_empty(),
        "firecracker runs in its instance directory"
    );
    let net = std::fs::read_link(format!("/proc/{}/ns/net", pids[0])).expect("its namespace");
    drop(vm);
    drop(launcher);
    std::thread::sleep(Duration::from_millis(500));
    let in_namespace: Vec<_> = std::fs::read_dir("/proc")
        .expect("/proc")
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| std::fs::read_link(format!("/proc/{pid}/ns/net")).is_ok_and(|n| n == net))
        .collect();
    assert!(
        in_namespace.is_empty(),
        "processes {in_namespace:?} are still in the launcher's namespace"
    );
    assert!(
        !instance.join("api.sock").exists(),
        "the API socket of a stopped VM is left behind at {}",
        instance.join("api.sock").display()
    );
}
