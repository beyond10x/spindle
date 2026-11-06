//! Adversary cases for the QEMU microVM launcher.
//!
//! The cases that boot a guest skip with their reason where `/dev/kvm`, QEMU, the host kernel or
//! the musl target is missing, as `microvm.rs` does. The others run anywhere: they use the
//! host's `true` as a stand-in QEMU that ignores its arguments and exits 0.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use spindle_qemu::{
    Artifact, BuildError, Exit, GUEST_TARGET, GuestImage, HostKernel, LaunchError, LaunchPlan,
    Network, QemuLauncher, Resources, StaticBinaries, reported_exit,
};

const BOOT: Duration = Duration::from_secs(90);
const ORPHAN_CHILD: &str = "SPINDLE_QEMU_ADVERSARY_ORPHAN_DIR";

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
    spindle_qemu::kvm_available()?;
    let qemu = spindle_qemu::find_qemu().ok_or("qemu-system-x86_64 is not on PATH")?;
    let kernel = HostKernel::discover().map_err(|missing| missing.to_string())?;
    let work = work_dir();
    std::fs::create_dir_all(&work).map_err(|error| format!("{}: {error}", work.display()))?;
    let binaries = match StaticBinaries::build(&workspace(), &work.join("musl-target")) {
        Ok(binaries) => binaries,
        Err(BuildError::TargetMissing(reason)) => return Err(reason),
        Err(error) => panic!("the static guest build failed: {error}"),
    };
    Ok(Host {
        qemu,
        kernel,
        binaries,
        work,
    })
}

fn host() -> Option<&'static Host> {
    static HOST: OnceLock<Result<Host, String>> = OnceLock::new();
    match HOST.get_or_init(prepare) {
        Ok(host) => Some(host),
        Err(reason) => {
            eprintln!("skipped: {reason}");
            None
        }
    }
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

fn free_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .and_then(|socket| socket.local_addr())
        .map(|address| address.port())
        .expect("a free loopback port")
}

fn group() -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::new(239, 255, 77, 1), free_port())
}

fn network() -> Network {
    Network::multicast(group(), 24)
}

fn guest_address(last: u8, port: u16) -> SocketAddr {
    SocketAddr::from(([10, 77, 0, last], port))
}

fn small() -> Resources {
    Resources {
        vcpus: 1,
        memory_mib: 256,
        disk_mib: 0,
    }
}

fn plan(id: &str, artifact: Artifact, listen: SocketAddr) -> LaunchPlan {
    LaunchPlan {
        instance_id: id.to_owned(),
        component: "rally-table".to_owned(),
        settings: Vec::new(),
        artifact,
        request: small(),
        listen,
        peers: Vec::new(),
    }
}

fn dummy_artifact(dir: &Path) -> Artifact {
    let initramfs = dir.join("dummy.cpio");
    std::fs::write(&initramfs, b"").expect("a dummy initramfs");
    Artifact::Guest {
        kernel: PathBuf::from("vmlinuz"),
        initramfs,
        cmdline: "-- /bin/true".to_owned(),
    }
}

fn probe_image(host: &Host, dir: &Path) -> PathBuf {
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

fn console(path: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(path).unwrap_or_default()).into_owned()
}

fn process_runs(pid: u32, marker: &str) -> bool {
    let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    let state = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    let zombie = state
        .rsplit_once(')')
        .is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'));
    !zombie && String::from_utf8_lossy(&cmdline).contains(marker)
}

#[test]
#[ignore = "starts the launcher's user and network namespace: needs unprivileged user namespaces (task check:vm)"]
fn waiting_with_duration_max_waits_until_the_instance_ends() {
    let Some(fake_qemu) = find_on_path("true") else {
        eprintln!("skipped: no `true` on PATH");
        return;
    };
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut launcher =
        QemuLauncher::new(scratch.path().join("instances"), network()).with_qemu(fake_qemu);
    let mut vm = launcher
        .launch(&plan(
            "forever-wait",
            dummy_artifact(scratch.path()),
            guest_address(2, 7000),
        ))
        .expect("the stand-in QEMU starts");
    let waited = vm.wait(Duration::MAX).expect("waiting works");
    assert_eq!(
        waited,
        Some(Exit {
            code: None,
            signal: None
        })
    );
}

#[test]
fn a_report_the_guest_command_printed_is_not_taken_for_the_init_report() {
    let console = "\
[    0.912345] Run /init as init process
spindle-init: eth0 10.77.0.2/24
rally 0.1.0 compiled from 4 file(s)
  summary: spindle-init: exit code=0
[    1.402311] Kernel panic - not syncing: Out of memory and no killable processes...
[    1.402500] Rebooting in 1 seconds..
";
    assert_eq!(
        reported_exit(console),
        None,
        "the guest never reported an exit; the only report-shaped text is the command's output"
    );
}

#[test]
fn the_init_report_that_ends_a_line_wins_over_report_text_before_it() {
    let console = "\
spindle-init: eth0 10.77.0.2/24
progress: spindle-init: exit code=0spindle-init: exit code=3
[    1.500000] reboot: Power down
";
    assert_eq!(
        reported_exit(console),
        Some(Exit {
            code: Some(3),
            signal: None
        }),
        "init's own report is the last one on the console"
    );
}

#[test]
fn a_network_prefix_longer_than_32_is_refused() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let launcher = QemuLauncher::new(
        scratch.path().join("instances"),
        Network::multicast(group(), 33),
    );
    let argv = launcher.command(&plan(
        "prefix",
        dummy_artifact(scratch.path()),
        guest_address(2, 7000),
    ));
    assert!(
        matches!(argv, Err(LaunchError::InvalidPlan(_))),
        "a /33 network was accepted: {argv:?}"
    );
}

#[test]
fn the_subnet_broadcast_address_is_not_a_guest_address() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let launcher = QemuLauncher::new(scratch.path().join("instances"), network());
    let argv = launcher.command(&plan(
        "broadcast",
        dummy_artifact(scratch.path()),
        guest_address(255, 7000),
    ));
    assert!(
        matches!(argv, Err(LaunchError::InvalidPlan(_))),
        "10.77.0.255 on a /24 was accepted as a guest address: {argv:?}"
    );
}

#[test]
fn a_console_byte_that_is_not_utf8_does_not_lose_the_exit_report() {
    let Some(host) = host() else { return };
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let source = dir.path().join("rawbyte.rs");
    std::fs::write(
        &source,
        "use std::io::Write;\n\
         fn main() {\n\
         \x20   let mut out = std::io::stdout().lock();\n\
         \x20   out.write_all(b\"guest log: caf\\xe9\\n\").unwrap();\n\
         \x20   out.flush().unwrap();\n\
         }\n",
    )
    .expect("the helper source");
    let helper = dir.path().join("rawbyte");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let built = Command::new(&rustc)
        .current_dir(workspace())
        .args([
            "--edition",
            "2021",
            "--target",
            GUEST_TARGET,
            "-C",
            "strip=symbols",
            "-o",
        ])
        .arg(&helper)
        .arg(&source)
        .output()
        .expect("rustc runs");
    assert!(
        built.status.success(),
        "the helper did not build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let mut image = GuestImage::new(&host.binaries.init, &host.kernel).expect("a guest image");
    image.binary("/bin/rawbyte", &helper).expect("the helper");
    let initramfs = dir.path().join("rawbyte.cpio");
    image.write(&initramfs).expect("the initramfs is written");

    let mut launcher =
        QemuLauncher::new(dir.path().join("instances"), network()).with_qemu(host.qemu.clone());
    let mut vm = launcher
        .launch(&plan(
            "rawbyte",
            guest(host, initramfs, "/bin/rawbyte"),
            guest_address(2, 7000),
        ))
        .expect("the microVM starts");
    let exit = vm.wait(BOOT).expect("waits");
    let text = console(vm.console());
    if exit.is_none() {
        let _ = vm.stop();
    }
    assert!(
        text.contains("spindle-init: exit code=0"),
        "the guest did not report:\n{text}"
    );
    assert_eq!(
        exit,
        Some(Exit {
            code: Some(0),
            signal: None
        }),
        "the console holds `spindle-init: exit code=0`:\n{text}"
    );
}

#[test]
fn a_second_launch_of_a_running_instance_id_is_refused() {
    let Some(host) = host() else { return };
    let Some(fake_qemu) = find_on_path("true") else {
        eprintln!("skipped: no `true` on PATH");
        return;
    };
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let initramfs = probe_image(host, dir.path());
    let mut launcher =
        QemuLauncher::new(dir.path().join("instances"), network()).with_qemu(host.qemu.clone());
    let listen = guest_address(2, 7000);
    let mut first = launcher
        .launch(&plan(
            "twice",
            guest(
                host,
                initramfs.clone(),
                &format!("/bin/spindle-qemu-probe serve {listen}"),
            ),
            listen,
        ))
        .expect("the first microVM starts");
    let started = Instant::now();
    while !first.console().exists() && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let first_console_existed = first.console().exists();

    let mut again = launcher.clone().with_qemu(fake_qemu);
    let second = again.launch(&plan("twice", guest(host, initramfs, "/bin/true"), listen));
    let first_console_survives = first.console().exists();
    let still_running = first.wait(Duration::ZERO).expect("waits").is_none();
    let _ = first.stop();
    if let Ok(mut second) = second {
        let _ = second.stop();
        panic!(
            "a second launch of the running instance `twice` was accepted \
             (first still running: {still_running}; its console existed: {first_console_existed}, \
             still exists after the second launch: {first_console_survives})"
        );
    }
}

#[test]
fn qemu_does_not_outlive_the_process_that_launched_it() {
    let Some(host) = host() else { return };

    if let Some(dir) = std::env::var_os(ORPHAN_CHILD) {
        let dir = PathBuf::from(dir);
        let initramfs = probe_image(host, &dir);
        let mut launcher =
            QemuLauncher::new(dir.join("instances"), network()).with_qemu(host.qemu.clone());
        let listen = guest_address(2, 7000);
        let vm = launcher
            .launch(&plan(
                "orphan",
                guest(
                    host,
                    initramfs,
                    &format!("/bin/spindle-qemu-probe serve {listen}"),
                ),
                listen,
            ))
            .expect("the microVM starts");
        let report = format!("{}\n", vm.pid());
        std::fs::write(dir.join("pid.tmp"), report).expect("the pid file");
        std::fs::rename(dir.join("pid.tmp"), dir.join("pid")).expect("the pid file");
        std::thread::sleep(Duration::from_secs(300));
        drop(vm);
        return;
    }

    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let mut child = Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "--exact",
            "adversary::qemu_does_not_outlive_the_process_that_launched_it",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ORPHAN_CHILD, dir.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the launching process starts");
    let pid_file = dir.path().join("pid");
    let started = Instant::now();
    while !pid_file.exists() && started.elapsed() < Duration::from_secs(180) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let qemu_pid: Option<u32> = std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|text| text.trim().parse().ok());

    let _ = child.kill();
    let _ = child.wait();
    let Some(qemu_pid) = qemu_pid else {
        panic!("the launching process never reported a QEMU pid");
    };
    let marker = dir.path().join("instances").join("orphan");
    let marker = marker.to_string_lossy().into_owned();
    std::thread::sleep(Duration::from_secs(2));
    let orphaned = process_runs(qemu_pid, &marker);
    if orphaned {
        // SAFETY: kill(2) on the QEMU this test started, identified by pid and its argv.
        unsafe { libc::kill(qemu_pid as libc::pid_t, libc::SIGKILL) };
        let gone = Instant::now();
        while process_runs(qemu_pid, &marker) && gone.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    assert!(
        !orphaned,
        "QEMU pid {qemu_pid} kept running after the process that launched it was killed"
    );
}

#[test]
fn no_process_outside_the_launcher_receives_the_vm_network() {
    let Some(host) = host() else { return };
    let dir = tempfile::tempdir_in(&host.work).expect("a work directory");
    let initramfs = probe_image(host, dir.path());
    let network = network();
    let group = network.group();

    // SAFETY: plain socket(2), setsockopt(2) and bind(2) calls on a descriptor this test owns.
    let sniffer = unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
        assert!(fd >= 0, "socket: {}", std::io::Error::last_os_error());
        let one: libc::c_int = 1;
        let set = libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            std::ptr::addr_of!(one).cast(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );
        assert_eq!(set, 0, "SO_REUSEADDR: {}", std::io::Error::last_os_error());
        let address = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: group.port().to_be(),
            sin_addr: libc::in_addr {
                s_addr: u32::from_ne_bytes(group.ip().octets()),
            },
            sin_zero: [0; 8],
        };
        let bound = libc::bind(
            fd,
            std::ptr::addr_of!(address).cast(),
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        );
        assert_eq!(bound, 0, "bind: {}", std::io::Error::last_os_error());
        UdpSocket::from_raw_fd(fd)
    };
    sniffer
        .join_multicast_v4(group.ip(), &Ipv4Addr::LOCALHOST)
        .expect("joins the group on loopback");
    sniffer
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("a read timeout");

    let mut launcher =
        QemuLauncher::new(dir.path().join("instances"), network).with_qemu(host.qemu.clone());
    let mut vm = launcher
        .launch(&plan(
            "sniffed",
            guest(
                host,
                initramfs,
                "/bin/spindle-qemu-probe call 10.77.0.9:7000 secret --within 30",
            ),
            guest_address(2, 7000),
        ))
        .expect("the microVM starts");

    let mut frame = [0u8; 2048];
    let mut received = None;
    let started = Instant::now();
    while received.is_none() && started.elapsed() < Duration::from_secs(40) {
        match sniffer.recv_from(&mut frame) {
            Ok((length, from)) => received = Some((length, from)),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if vm.wait(Duration::ZERO).expect("waits").is_some() {
                    break;
                }
            }
            Err(error) => panic!("recv: {error}"),
        }
    }
    let _ = vm.stop();
    if let Some((length, from)) = received {
        panic!(
            "a socket opened by another process received a {length}-byte frame of the VM network \
             from {from} (ethertype {:02x}{:02x})",
            frame[12], frame[13]
        );
    }
}
