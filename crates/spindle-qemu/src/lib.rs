//! The `QemuMicrovm` target: spindle instances as QEMU microVMs.
//!
//! A guest is the host's own kernel image, the virtio modules it needs, and an initramfs whose
//! `/init` is `spindle-qemu-init`: it mounts the pseudo file systems, loads the modules, gives
//! `eth0` the instance's address from the kernel command line, runs the guest command after `--`
//! with the plan's [`guest_variables`] and each line of its output prefixed by [`GUEST_OUTPUT`],
//! reports the command's exit on the console and powers off. `b10x-spindle play` started so plays
//! the plan's component, as it does under the process launcher.
//!
//! [`QemuLauncher`] implements [`spindle_target::Launcher`]. It starts one
//! `qemu-system-x86_64 -M microvm` per instance with KVM, no root, the requested vCPUs and memory
//! and the console in a file. Every QEMU of one launcher runs in one user and network namespace
//! the launcher owns, and they join one `-netdev socket` multicast network on that namespace's
//! loopback, so no other process on the host can reach the VM network. With
//! [`QemuLauncher::with_telemetry`] each guest also gets a `vhost-vsock-device` and reports
//! `spindle-telemetry/1` frames to a vsock port of the host. Disk is not enforced. A
//! zero vCPU or memory request is refused: placement resolves it to the target's ceiling, and a
//! QEMU target must have one.

mod cpio;
mod guest;
mod kernel;
mod namespace;
mod static_build;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

pub use cpio::Initramfs;
pub use guest::{GuestError, GuestImage, LAUNCH_PLAN, MODULE_LIST};
pub use kernel::{GUEST_MODULES, GuestModule, HostKernel, KernelMissing, kernel_release_of};
pub use spindle_target::{
    Artifact, Exit, LaunchError, LaunchPlan, Launcher, Peer, ResourceKind, ResourceSample,
    Resources, Running, TargetKind,
};
pub use static_build::{BuildError, GUEST_TARGET, StaticBinaries};

use namespace::{Shared, failure, io_error};
use spindle_target::{
    ENV_COMPONENT, ENV_INSTANCE_ID, ENV_LISTEN, ENV_PEER_PREFIX, ENV_SETTING_PREFIX, env_name,
};

/// The console text the guest init reports the command's exit with: `code=<n>` or `signal=<n>`
/// follows it.
pub const EXIT_REPORT: &str = "spindle-init: exit ";
/// The prefix the guest init puts before every line the guest command writes.
pub const GUEST_OUTPUT: &str = "guest| ";

/// The VM network every instance of one launcher joins: a QEMU `-netdev socket` multicast group
/// on the loopback of the launcher's own network namespace, and an IPv4 prefix the guest
/// addresses share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    group: SocketAddrV4,
    prefix_len: u8,
}

impl Network {
    /// A network on the multicast `group` (an address in 224.0.0.0/4 and a port) whose guests
    /// share a `/prefix_len` IPv4 subnet. A prefix over 32 is refused when an instance is planned.
    pub fn multicast(group: SocketAddrV4, prefix_len: u8) -> Self {
        Self { group, prefix_len }
    }

    /// The multicast group and port.
    pub fn group(&self) -> SocketAddrV4 {
        self.group
    }

    /// The prefix length of the guest subnet.
    pub fn prefix_len(&self) -> u8 {
        self.prefix_len
    }

    /// The `-netdev` value for one instance.
    pub fn netdev(&self) -> String {
        format!(
            "socket,id=net0,mcast={},localaddr={}",
            self.group,
            Ipv4Addr::LOCALHOST
        )
    }
}

/// Starts instances as QEMU microVMs. Clones share the namespace, the spawning thread and the
/// registry of live instance ids.
#[derive(Debug, Clone)]
pub struct QemuLauncher {
    qemu: PathBuf,
    work_dir: PathBuf,
    network: Network,
    shared: Arc<Shared>,
    telemetry: Option<u32>,
}

struct Checked<'a> {
    kernel: &'a Path,
    initramfs: &'a Path,
    cmdline: &'a str,
    address: Ipv4Addr,
}

impl QemuLauncher {
    /// A launcher keeping each instance's files under `work_dir/<instance_id>` and joining every
    /// instance to `network`. Runs `qemu-system-x86_64` from `PATH`.
    pub fn new(work_dir: PathBuf, network: Network) -> Self {
        Self {
            qemu: PathBuf::from("qemu-system-x86_64"),
            work_dir,
            network,
            shared: Arc::new(Shared::new()),
            telemetry: None,
        }
    }

    /// Run this QEMU binary instead.
    pub fn with_qemu(mut self, qemu: PathBuf) -> Self {
        self.qemu = qemu;
        self
    }

    /// The same launcher giving every guest a `vhost-vsock-device` with a context id of its own,
    /// and `vsock:2:<port>` as its telemetry endpoint: the guest init passes it to the guest
    /// command as `SPINDLE_TELEMETRY`. The caller listens on vsock port `port` of the host.
    pub fn with_telemetry(mut self, port: u32) -> Self {
        self.telemetry = Some(port);
        self
    }

    /// The telemetry port guests report to, if any.
    pub fn telemetry(&self) -> Option<u32> {
        self.telemetry
    }

    /// The network instances join.
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// The resources this launcher enforces: vCPUs and memory.
    pub fn enforces(&self) -> BTreeSet<ResourceKind> {
        BTreeSet::from([ResourceKind::Vcpus, ResourceKind::Memory])
    }

    fn check<'a>(&self, plan: &'a LaunchPlan) -> Result<Checked<'a>, LaunchError> {
        spindle_target::refuse_unenforced(&self.enforces(), &plan.request)?;
        let Artifact::Guest {
            kernel,
            initramfs,
            cmdline,
        } = &plan.artifact
        else {
            return Err(LaunchError::UnsupportedArtifact {
                launcher: TargetKind::QemuMicrovm,
                artifact: plan.artifact.kind(),
            });
        };
        let invalid = |reason: String| Err(LaunchError::InvalidPlan(reason));
        if plan.request.vcpus == 0 || plan.request.memory_mib == 0 {
            return invalid(format!(
                "a QEMU microVM needs a vCPU and a memory amount; the request is {} vCPU(s) and \
                 {} MiB (placement resolves a zero request to the target's ceiling)",
                plan.request.vcpus, plan.request.memory_mib
            ));
        }
        let id = &plan.instance_id;
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return invalid(format!(
                "instance id {id:?} is not letters, digits, '-' and '_'"
            ));
        }
        let prefix = self.network.prefix_len;
        if prefix > 32 {
            return invalid(format!("network prefix /{prefix} is longer than 32"));
        }
        let IpAddr::V4(address) = plan.listen.ip() else {
            return invalid(format!("listen address {} is not IPv4", plan.listen));
        };
        if address.is_unspecified()
            || address.is_loopback()
            || address.is_multicast()
            || address.is_broadcast()
        {
            return invalid(format!(
                "listen address {} is not a guest unicast address",
                plan.listen
            ));
        }
        if prefix <= 30 {
            let host_bits = u32::MAX >> prefix;
            let host = u32::from(address) & host_bits;
            if host == 0 || host == host_bits {
                return invalid(format!(
                    "listen address {address} is the network or broadcast address of its /{prefix}"
                ));
            }
        }
        if self.work_dir.to_string_lossy().contains(',') {
            return invalid(format!(
                "work directory {} contains ',', which QEMU options cannot hold",
                self.work_dir.display()
            ));
        }
        Ok(Checked {
            kernel,
            initramfs,
            cmdline,
            address,
        })
    }

    fn instance_dir(&self, plan: &LaunchPlan) -> PathBuf {
        self.work_dir.join(&plan.instance_id)
    }

    /// The QEMU argv (program first) [`launch`](Self::launch) runs for `plan`, inside the
    /// launcher's namespace.
    ///
    /// With telemetry, each call takes a new guest context id for the vsock device ([`next_cid`]),
    /// and is refused once the process has taken them all.
    pub fn command(&self, plan: &LaunchPlan) -> Result<Vec<String>, LaunchError> {
        self.check(plan)?;
        let cid = if self.telemetry.is_some() {
            next_cid()?
        } else {
            0
        };
        self.argv(plan, cid)
    }

    fn argv(&self, plan: &LaunchPlan, cid: u32) -> Result<Vec<String>, LaunchError> {
        let checked = self.check(plan)?;
        let dir = self.instance_dir(plan);
        let octets = checked.address.octets();
        let mac = format!(
            "52:54:00:{:02x}:{:02x}:{:02x}",
            octets[1], octets[2], octets[3]
        );
        let append = format!(
            "console=ttyS0 reboot=t panic=-1 spindle.addr={}/{} {}",
            checked.address, self.network.prefix_len, checked.cmdline
        );
        let mut argv = vec![
            self.qemu.to_string_lossy().into_owned(),
            "-M".to_owned(),
            "microvm".to_owned(),
            "-enable-kvm".to_owned(),
            "-cpu".to_owned(),
            "host".to_owned(),
            "-smp".to_owned(),
            plan.request.vcpus.to_string(),
            "-m".to_owned(),
            plan.request.memory_mib.to_string(),
            "-nodefaults".to_owned(),
            "-no-user-config".to_owned(),
            "-nographic".to_owned(),
            "-no-reboot".to_owned(),
            "-serial".to_owned(),
            format!("file:{}", dir.join("console.log").display()),
            "-netdev".to_owned(),
            self.network.netdev(),
            "-device".to_owned(),
            format!("virtio-net-device,netdev=net0,mac={mac}"),
            "-kernel".to_owned(),
            checked.kernel.to_string_lossy().into_owned(),
            "-initrd".to_owned(),
            dir.join("initramfs.cpio").to_string_lossy().into_owned(),
            "-append".to_owned(),
            append,
        ];
        if self.telemetry.is_some() {
            argv.push("-device".to_owned());
            argv.push(format!("vhost-vsock-device,guest-cid={cid}"));
        }
        Ok(argv)
    }

    /// Start the instance: refuse what is not enforced or not valid, refuse an id that still
    /// runs, write its initramfs (the artifact's, followed by an archive holding
    /// [`LAUNCH_PLAN`]), and start QEMU in the launcher's namespace.
    pub fn launch(&mut self, plan: &LaunchPlan) -> Result<QemuInstance, LaunchError> {
        let argv = self.command(plan)?;
        let checked = self.check(plan)?;
        let document = launch_document(plan, self.telemetry)?;
        let mut live = self.shared.live()?;
        if let Some(pid) = live.get(&plan.instance_id)
            && namespace::runs(*pid)
        {
            return Err(LaunchError::InvalidPlan(format!(
                "instance {} is still running (QEMU pid {pid})",
                plan.instance_id
            )));
        }

        let dir = self.instance_dir(plan);
        fs::create_dir_all(&dir).map_err(io_error(format!("creating {}", dir.display())))?;
        let mut bytes = fs::read(checked.initramfs)
            .map_err(io_error(format!("reading {}", checked.initramfs.display())))?;
        bytes.resize(bytes.len().next_multiple_of(4), 0);
        let mut launch = Initramfs::new();
        launch.file(LAUNCH_PLAN, 0o644, document.into_bytes());
        let combined = dir.join("initramfs.cpio");
        launch
            .write_to(&mut bytes)
            .map_err(io_error("writing the launch archive"))?;
        fs::write(&combined, bytes).map_err(io_error(format!("writing {}", combined.display())))?;

        let console = dir.join("console.log");
        if console.exists() {
            fs::remove_file(&console)
                .map_err(io_error(format!("removing {}", console.display())))?;
        }
        let log = dir.join("qemu.log");
        let output =
            fs::File::create(&log).map_err(io_error(format!("creating {}", log.display())))?;
        let errors = output
            .try_clone()
            .map_err(io_error(format!("opening {}", log.display())))?;
        let started = Instant::now();
        let child = self
            .shared
            .spawn_entered(&argv, output.into(), errors.into())?;
        live.insert(plan.instance_id.clone(), child.id());
        drop(live);
        Ok(QemuInstance {
            instance_id: plan.instance_id.clone(),
            console,
            log,
            argv,
            child,
            exit: None,
            shared: Arc::clone(&self.shared),
            started,
            request: plan.request,
        })
    }
}

impl Launcher for QemuLauncher {
    fn kind(&self) -> TargetKind {
        TargetKind::QemuMicrovm
    }

    fn enforces(&self) -> BTreeSet<ResourceKind> {
        QemuLauncher::enforces(self)
    }

    fn launch(&mut self, plan: &LaunchPlan) -> Result<Box<dyn Running>, LaunchError> {
        Ok(Box::new(QemuLauncher::launch(self, plan)?))
    }
}

/// The most guest context ids one process gives out.
pub const MAX_GUESTS_PER_PROCESS: u32 = 1024;

static GUESTS: AtomicU32 = AtomicU32::new(0);

/// The next guest context id of this process: `3 + ((pid - 1) << 10 | n)` for its `n`th guest,
/// `n` below [`MAX_GUESTS_PER_PROCESS`]. A context id is unique on the host, and process ids
/// lie in `1..=2^22`, so two processes never share one, and every launcher of one process
/// draws from the same count. The result stays below `u32::MAX`, which means any context id.
pub fn next_cid() -> Result<u32, LaunchError> {
    let n = GUESTS.fetch_add(1, Ordering::Relaxed);
    if n >= MAX_GUESTS_PER_PROCESS {
        return Err(LaunchError::InvalidPlan(format!(
            "this process has given out all {MAX_GUESTS_PER_PROCESS} of its vsock guest context ids"
        )));
    }
    let pid = std::process::id().wrapping_sub(1) & 0x3f_ffff;
    Ok(3 + ((pid << 10) | n))
}

/// The variables the guest init gives the guest command, as the process launcher sets them for
/// a child: [`ENV_INSTANCE_ID`], [`ENV_COMPONENT`], [`ENV_LISTEN`], the telemetry endpoint when
/// there is one, one `SPINDLE_SETTING_<NAME>` per setting and one `SPINDLE_PEER_<COMPONENT>` per
/// peer component, its addresses comma-separated in plan order. A plan whose names collide is
/// refused.
pub fn guest_variables(
    plan: &LaunchPlan,
    telemetry: Option<u32>,
) -> Result<BTreeMap<String, String>, LaunchError> {
    let mut variables = BTreeMap::new();
    let mut set = |name: String, value: String, from: &str| {
        if variables.insert(name.clone(), value).is_some() {
            return Err(LaunchError::InvalidPlan(format!(
                "{from} maps to {name}, which is already set"
            )));
        }
        Ok(())
    };
    set(
        ENV_INSTANCE_ID.to_owned(),
        plan.instance_id.clone(),
        "the instance id",
    )?;
    set(
        ENV_COMPONENT.to_owned(),
        plan.component.clone(),
        "the component",
    )?;
    set(
        ENV_LISTEN.to_owned(),
        plan.listen.to_string(),
        "the listen address",
    )?;
    if let Some(port) = telemetry {
        set(
            spindle_telemetry::ENV_TELEMETRY.to_owned(),
            spindle_telemetry::Endpoint::host_vsock(port).to_string(),
            "the telemetry endpoint",
        )?;
    }
    for (name, value) in &plan.settings {
        if name.is_empty() {
            return Err(LaunchError::InvalidPlan("a setting has no name".to_owned()));
        }
        set(
            format!("{ENV_SETTING_PREFIX}{}", env_name(name)),
            value.clone(),
            &format!("setting {name:?}"),
        )?;
    }
    let mut peers: BTreeMap<String, (&str, Vec<String>)> = BTreeMap::new();
    for peer in &plan.peers {
        if peer.component.is_empty() {
            return Err(LaunchError::InvalidPlan(
                "a peer has no component".to_owned(),
            ));
        }
        let name = format!("{ENV_PEER_PREFIX}{}", env_name(&peer.component));
        let (component, addresses) = peers
            .entry(name.clone())
            .or_insert_with(|| (peer.component.as_str(), Vec::new()));
        if *component != peer.component {
            return Err(LaunchError::InvalidPlan(format!(
                "peer components {component:?} and {:?} both map to {name}",
                peer.component
            )));
        }
        addresses.push(peer.address.to_string());
    }
    for (name, (component, addresses)) in peers {
        set(
            name,
            addresses.join(","),
            &format!("peer component {component:?}"),
        )?;
    }
    Ok(variables)
}

fn launch_document(plan: &LaunchPlan, telemetry: Option<u32>) -> Result<String, LaunchError> {
    let environment: serde_json::Map<String, serde_json::Value> = guest_variables(plan, telemetry)?
        .into_iter()
        .map(|(name, value)| (name, serde_json::Value::String(value)))
        .collect();
    let settings: serde_json::Map<String, serde_json::Value> = plan
        .settings
        .iter()
        .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
        .collect();
    let peers: Vec<serde_json::Value> = plan
        .peers
        .iter()
        .map(|peer| {
            serde_json::json!({
                "component": peer.component,
                "address": peer.address.to_string(),
            })
        })
        .collect();
    let mut document = serde_json::json!({
        "instance_id": plan.instance_id,
        "component": plan.component,
        "settings": settings,
        "listen": plan.listen.to_string(),
        "peers": peers,
        "environment": environment,
    });
    if let Some(port) = telemetry {
        document["telemetry"] =
            serde_json::Value::String(spindle_telemetry::Endpoint::host_vsock(port).to_string());
    }
    let mut text = document.to_string();
    text.push('\n');
    Ok(text)
}

/// A started microVM. Dropping it kills QEMU if it still runs.
#[derive(Debug)]
pub struct QemuInstance {
    instance_id: String,
    console: PathBuf,
    log: PathBuf,
    argv: Vec<String>,
    child: Child,
    exit: Option<Exit>,
    shared: Arc<Shared>,
    started: Instant,
    request: Resources,
}

impl QemuInstance {
    /// The plan's instance id.
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// The file the guest's serial console is written to.
    pub fn console(&self) -> &Path {
        &self.console
    }

    /// The file QEMU's own output is written to.
    pub fn log(&self) -> &Path {
        &self.log
    }

    /// The argv QEMU was started with.
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    /// The QEMU process id.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Wait up to `timeout` for the instance to end; `None` when it still runs. A timeout too
    /// large for the clock waits without a deadline.
    pub fn wait(&mut self, timeout: Duration) -> Result<Option<Exit>, LaunchError> {
        if let Some(exit) = self.exit {
            return Ok(Some(exit));
        }
        let deadline = Instant::now().checked_add(timeout);
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(io_error("waiting for QEMU"))?
            {
                return self.finish(status).map(Some);
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// What QEMU costs the host now (its `/proc/<pid>`, the VMM included) against the vCPUs and
    /// memory the plan asked for; `None` once the instance has ended.
    pub fn resources(&self) -> Option<ResourceSample> {
        if self.exit.is_some() || !namespace::runs(self.child.id()) {
            return None;
        }
        let usage = spindle_target::process_usage(self.child.id()).ok()?;
        Some(ResourceSample::of(
            usage,
            self.started.elapsed(),
            &self.request,
        ))
    }

    /// End the instance: kill QEMU if it still runs. An instance that already ended reports how.
    pub fn stop(&mut self) -> Result<Exit, LaunchError> {
        if let Some(exit) = self.wait(Duration::ZERO)? {
            return Ok(exit);
        }
        match self.child.kill() {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
            Err(error) => return Err(io_error("killing QEMU")(error)),
        }
        let status = self.child.wait().map_err(io_error("waiting for QEMU"))?;
        self.finish(status)
    }

    fn finish(&mut self, status: ExitStatus) -> Result<Exit, LaunchError> {
        self.shared.ended(&self.instance_id, self.child.id());
        let exit = if let Some(signal) = status.signal() {
            Exit {
                code: None,
                signal: Some(signal),
            }
        } else if status.success() {
            let console =
                String::from_utf8_lossy(&fs::read(&self.console).unwrap_or_default()).into_owned();
            reported_exit(&console).unwrap_or(Exit {
                code: None,
                signal: None,
            })
        } else {
            let log =
                String::from_utf8_lossy(&fs::read(&self.log).unwrap_or_default()).into_owned();
            return Err(failure(
                format!("QEMU for instance {}", self.instance_id),
                format!("exited with {status}: {}", log.trim()),
            ));
        };
        self.exit = Some(exit);
        Ok(exit)
    }
}

impl Running for QemuInstance {
    fn instance_id(&self) -> &str {
        QemuInstance::instance_id(self)
    }

    fn console(&self) -> &Path {
        QemuInstance::console(self)
    }

    fn wait(&mut self, timeout: Duration) -> Result<Option<Exit>, LaunchError> {
        QemuInstance::wait(self, timeout)
    }

    fn stop(&mut self) -> Result<Exit, LaunchError> {
        QemuInstance::stop(self)
    }

    fn resources(&self) -> Option<ResourceSample> {
        QemuInstance::resources(self)
    }
}

impl Drop for QemuInstance {
    fn drop(&mut self) {
        if self.exit.is_none() && matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.shared.ended(&self.instance_id, self.child.id());
    }
}

/// The exit the guest init reported on `console`: the last report on it, unless the kernel
/// panicked after it, in which case init did not end the guest and the report is not init's.
///
/// The init prefixes every line of the guest command's output with [`GUEST_OUTPUT`] and writes
/// its own report last, so text the command printed is never the last report of a guest that
/// init ended.
pub fn reported_exit(console: &str) -> Option<Exit> {
    let start = console.rfind(EXIT_REPORT)?;
    let rest = &console[start + EXIT_REPORT.len()..];
    let (report, after) = rest.split_once('\n').unwrap_or((rest, ""));
    if after.contains("Kernel panic") {
        return None;
    }
    let (kind, value) = report.trim().split_once('=')?;
    let value: i32 = value.trim().parse().ok()?;
    match kind {
        "code" => Some(Exit {
            code: Some(value),
            signal: None,
        }),
        "signal" => Some(Exit {
            code: None,
            signal: Some(value),
        }),
        _ => None,
    }
}

/// `Ok` when `/dev/kvm` opens for reading and writing; otherwise why not.
pub fn kvm_available() -> Result<(), String> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .map(drop)
        .map_err(|error| format!("/dev/kvm is not usable: {error}"))
}

/// `qemu-system-x86_64` on `PATH`, if there is one.
pub fn find_qemu() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("qemu-system-x86_64"))
        .find(|candidate| candidate.is_file())
}

/// What a host lacks to boot a QEMU microVM guest, checked in order: `/dev/kvm`,
/// `qemu-system-x86_64`, the host kernel image and its guest modules. `Ok` holds the QEMU binary
/// and the kernel.
pub fn vm_prerequisites() -> Result<(PathBuf, HostKernel), String> {
    kvm_available()?;
    let qemu = find_qemu().ok_or("qemu-system-x86_64 is not on PATH")?;
    let kernel = HostKernel::discover().map_err(|missing| missing.to_string())?;
    Ok((qemu, kernel))
}
