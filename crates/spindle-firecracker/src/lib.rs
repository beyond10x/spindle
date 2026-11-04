//! Launch Spindle guests as Firecracker microVMs, without root.
//!
//! A [`Firecracker`] launcher implements [`spindle_target::Launcher`] with the Firecracker release
//! binary. It runs Firecracker inside a user and network namespace it owns, where each instance
//! gets a tap on one bridge (`172.16.0.1/24`), so instances of one launcher reach each other over
//! TCP while the host network is untouched. vCPUs and memory are set through `PUT
//! /machine-config` on the API socket; disk is not enforced.
//!
//! The guest is addressed from the kernel command line (`ip=`), and the plan reaches its `/init`
//! as the environment variables the process launcher sets ([`spindle_target::ENV_INSTANCE_ID`],
//! [`spindle_target::ENV_COMPONENT`], [`spindle_target::ENV_LISTEN`], one
//! `SPINDLE_PEER_<COMPONENT>` per peer component and one `SPINDLE_SETTING_<NAME>` per setting).
//! The artifact's `cmdline` is appended after them, so `-- <arguments>` passes arguments to
//! `/init`.
//!
//! An instance's [`Exit`] is its guest program's: `/init` prints `spindle-init: exit status <n>`
//! on the serial console before it reboots, and the launcher reads the status from the console
//! (not from a separate channel). A guest that ends without that line ended without reporting:
//! `Exit { code: None, signal: None }`.

mod api;
mod initramfs;
mod namespace;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::net::{Ipv4Addr, SocketAddr};
use std::os::fd::AsRawFd;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use initramfs::Initramfs;
use namespace::Namespace;
pub use spindle_target::{
    Artifact, Exit, LaunchPlan, Launcher, Peer, ResourceKind, ResourceSample, Resources, Running,
    TargetKind,
};

/// The variable naming the Firecracker binary; `firecracker` on `PATH` otherwise.
pub const FIRECRACKER_VARIABLE: &str = "SPINDLE_FIRECRACKER";

/// The bridge's address in the launcher's namespace, the guests' gateway.
pub const GATEWAY: Ipv4Addr = Ipv4Addr::new(172, 16, 0, 1);

/// The prefix length of the guests' network.
pub const PREFIX: u8 = 24;

/// What the console's last `/init` line starts with, followed by the guest program's status.
pub const GUEST_STATUS: &str = "spindle-init: exit status ";

/// What Firecracker 1.17 inserts into the kernel command line of a guest with one network
/// device, measured on the guest console (`Command line: … SPINDLE_SETTING_TICK=50ms pci=off
/// virtio_mmio.device=4K@0xc0001000:5 -- …`).
pub const FIRECRACKER_INSERTS: &str = " pci=off virtio_mmio.device=4K@0xc0001000:5";

/// The longest kernel command line this launcher passes: the x86 kernel's 2048 bytes, less its
/// terminating NUL and what Firecracker inserts.
pub const MAX_COMMAND_LINE: usize = 2048 - 1 - FIRECRACKER_INSERTS.len();

/// The memory a guest needs besides its initramfs: kernel, page tables and the guest program.
pub const BASE_MEMORY_MIB: u64 = 64;

/// How many times its own size an initramfs needs in guest memory: the archive, its unpacked
/// copy, and room for the program to run.
pub const INITRAMFS_FACTOR: u64 = 3;

const SOCKET_WITHIN: Duration = Duration::from_secs(10);
const MAX_GUEST_VARIABLES: usize = 28;
const MIB: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("the firecracker launcher cannot enforce a {0} request")]
    Unenforced(ResourceKind),
    #[error("a Firecracker launcher cannot run a {0} artifact")]
    UnsupportedArtifact(&'static str),
    #[error("invalid launch plan: {0}")]
    InvalidPlan(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("network namespace: {0}")]
    Network(String),
    #[error("{request} was answered {status}: {body}")]
    Api {
        request: String,
        status: u16,
        body: String,
    },
    /// Firecracker itself failed: it exited before it was configured, or with a status other
    /// than the 0 a guest reboot gives.
    #[error("firecracker {status}; console:\n{console}")]
    Vmm { status: String, console: String },
}

impl LaunchError {
    fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

impl From<LaunchError> for spindle_target::LaunchError {
    fn from(error: LaunchError) -> Self {
        match error {
            LaunchError::Unenforced(kind) => Self::Unenforced(kind),
            LaunchError::UnsupportedArtifact(artifact) => Self::UnsupportedArtifact {
                launcher: TargetKind::Firecracker,
                artifact,
            },
            LaunchError::InvalidPlan(message) => Self::InvalidPlan(message),
            LaunchError::Io { context, source } => Self::Io {
                what: context,
                source,
            },
            LaunchError::NotFound(message) => Self::Io {
                what: message,
                source: std::io::ErrorKind::NotFound.into(),
            },
            other @ (LaunchError::Network(_)
            | LaunchError::Api { .. }
            | LaunchError::Vmm { .. }) => Self::Io {
                what: "firecracker".to_string(),
                source: std::io::Error::other(other.to_string()),
            },
        }
    }
}

/// The guest addresses of one launcher's live instances.
type Addresses = Arc<Mutex<BTreeSet<Ipv4Addr>>>;

/// Launches guests with one Firecracker binary; its instances share one namespace and bridge.
#[derive(Debug)]
pub struct Firecracker {
    binary: PathBuf,
    state: PathBuf,
    namespace: Option<Arc<Namespace>>,
    taps: u32,
    live: Addresses,
}

impl Firecracker {
    /// The binary named by `SPINDLE_FIRECRACKER`, or `firecracker` on `PATH`.
    pub fn locate() -> Result<PathBuf, LaunchError> {
        if let Some(named) = std::env::var_os(FIRECRACKER_VARIABLE) {
            let path = PathBuf::from(named);
            return if path.is_file() {
                Ok(path)
            } else {
                Err(LaunchError::NotFound(format!(
                    "{FIRECRACKER_VARIABLE}={} is not a file",
                    path.display()
                )))
            };
        }
        std::env::var_os("PATH")
            .iter()
            .flat_map(std::env::split_paths)
            .map(|directory| directory.join("firecracker"))
            .find(|path| path.is_file())
            .ok_or_else(|| {
                LaunchError::NotFound(format!(
                    "no firecracker binary: set {FIRECRACKER_VARIABLE} or put `firecracker` on PATH"
                ))
            })
    }

    /// A launcher for `binary` keeping each instance's socket, console and log under
    /// `state/<instance id>/`. Nothing starts until the first launch.
    pub fn new(binary: PathBuf, state: PathBuf) -> Self {
        Self {
            binary,
            state,
            namespace: None,
            taps: 0,
            live: Addresses::default(),
        }
    }

    pub fn kind(&self) -> TargetKind {
        TargetKind::Firecracker
    }

    pub fn enforces(&self) -> BTreeSet<ResourceKind> {
        BTreeSet::from([ResourceKind::Vcpus, ResourceKind::Memory])
    }

    /// Boot `plan` as a microVM.
    ///
    /// Every refusal comes before anything is created or started: an unenforced resource, a
    /// zero vCPU or memory request (placement resolves zero to the target's ceiling first), an
    /// address outside the guest network or already used by a live instance, a value a kernel
    /// command line cannot carry, a command line over [`MAX_COMMAND_LINE`], a missing file, and
    /// memory below [`required_memory_mib`] for the initramfs. A launch that fails after that
    /// removes what it created, so the same instance id can be launched again.
    pub fn launch(&mut self, plan: &LaunchPlan) -> Result<FirecrackerVm, LaunchError> {
        let (kernel, initramfs) = match &plan.artifact {
            Artifact::Guest {
                kernel, initramfs, ..
            } => (kernel, initramfs),
            other @ Artifact::HostBinary { .. } => {
                return Err(LaunchError::UnsupportedArtifact(other.kind()));
            }
        };
        if let Err(spindle_target::LaunchError::Unenforced(kind)) =
            spindle_target::refuse_unenforced(&self.enforces(), &plan.request)
        {
            return Err(LaunchError::Unenforced(kind));
        }
        if plan.request.vcpus == 0 || plan.request.memory_mib == 0 {
            return Err(LaunchError::InvalidPlan(format!(
                "a Firecracker guest needs a vCPU and a memory size; the plan asks for {} vCPUs \
                 and {} MiB (placement resolves zero to the target's ceiling)",
                plan.request.vcpus, plan.request.memory_mib
            )));
        }
        validate_instance_id(&plan.instance_id)?;
        let address = guest_address(plan.listen)?;
        let boot_args = kernel_command_line(plan)?;
        for (what, path) in [
            ("firecracker binary", &self.binary),
            ("guest kernel", kernel),
            ("initramfs", initramfs),
        ] {
            if !path.is_file() {
                return Err(LaunchError::NotFound(format!(
                    "the {what} {} is not a file",
                    path.display()
                )));
            }
        }
        let initramfs_bytes = std::fs::metadata(initramfs)
            .map_err(|source| LaunchError::io(format!("reading {}", initramfs.display()), source))?
            .len();
        let required = required_memory_mib(initramfs_bytes);
        if plan.request.memory_mib < required {
            return Err(LaunchError::InvalidPlan(format!(
                "{} MiB of guest memory is too little for a {:.1} MiB initramfs: it needs at \
                 least {required} MiB ({INITRAMFS_FACTOR} × the initramfs + {BASE_MEMORY_MIB} MiB)",
                plan.request.memory_mib,
                initramfs_bytes as f64 / MIB as f64
            )));
        }
        let kernel = std::path::absolute(kernel)
            .map_err(|source| LaunchError::io("resolving the kernel path", source))?;
        let initramfs = std::path::absolute(initramfs)
            .map_err(|source| LaunchError::io("resolving the initramfs path", source))?;
        let directory = self.state.join(&plan.instance_id);
        if directory.exists() {
            return Err(LaunchError::InvalidPlan(format!(
                "instance `{}` was already launched here ({} exists)",
                plan.instance_id,
                directory.display()
            )));
        }
        {
            let mut live = self
                .live
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if !live.insert(address) {
                return Err(LaunchError::InvalidPlan(format!(
                    "{address} is the address of a live instance of this launcher"
                )));
            }
        }

        let started = self.start(plan, &directory, address, &kernel, &initramfs, boot_args);
        if started.is_err() {
            self.live
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .remove(&address);
            let _ = std::fs::remove_dir_all(&directory);
        }
        started
    }

    fn start(
        &mut self,
        plan: &LaunchPlan,
        directory: &Path,
        address: Ipv4Addr,
        kernel: &Path,
        initramfs: &Path,
        boot_args: String,
    ) -> Result<FirecrackerVm, LaunchError> {
        std::fs::create_dir_all(directory).map_err(|source| {
            LaunchError::io(format!("creating {}", directory.display()), source)
        })?;
        let namespace = match &self.namespace {
            Some(namespace) => Arc::clone(namespace),
            None => {
                let namespace = Arc::new(Namespace::start(GATEWAY, PREFIX)?);
                self.namespace = Some(Arc::clone(&namespace));
                namespace
            }
        };
        let tap = format!("fc{}", self.taps);
        self.taps += 1;
        namespace.add_tap(&tap)?;

        let console = directory.join("console.log");
        let log = directory.join("firecracker.log");
        let opened = File::create(&console)
            .and_then(|file| Ok((file.try_clone()?, file)))
            .and_then(|files| File::create(&log).map(|_| files));
        let (console_out, console_err) = match opened {
            Ok(files) => files,
            Err(source) => {
                let _ = namespace.remove_tap(&tap);
                return Err(LaunchError::io(
                    format!("creating the console and log in {}", directory.display()),
                    source,
                ));
            }
        };
        let mut command = namespace.command(&self.binary);
        command
            .current_dir(directory)
            .args(["--api-sock", "api.sock", "--log-path", "firecracker.log"])
            .stdin(Stdio::null())
            .stdout(console_out)
            .stderr(console_err);
        let started = Instant::now();
        let child = match namespace.spawn(command) {
            Ok(child) => child,
            Err(source) => {
                let _ = namespace.remove_tap(&tap);
                return Err(LaunchError::io("starting firecracker", source));
            }
        };
        let mut vm = FirecrackerVm {
            instance_id: plan.instance_id.clone(),
            console,
            socket: directory.join("api.sock"),
            child,
            outcome: None,
            tap: Some(tap.clone()),
            address,
            live: Arc::clone(&self.live),
            namespace,
            started,
            request: plan.request,
        };

        let handle = File::open(directory).map_err(|source| {
            LaunchError::io(format!("opening {}", directory.display()), source)
        })?;
        let socket = PathBuf::from(format!("/proc/self/fd/{}/api.sock", handle.as_raw_fd()));
        vm.wait_for_socket(&socket)?;
        api::put(
            &socket,
            "/machine-config",
            &serde_json::json!({
                "vcpu_count": plan.request.vcpus,
                "mem_size_mib": plan.request.memory_mib,
            }),
        )?;
        api::put(
            &socket,
            "/boot-source",
            &serde_json::json!({
                "kernel_image_path": kernel,
                "initrd_path": initramfs,
                "boot_args": boot_args,
            }),
        )?;
        let [a, b, c, d] = address.octets();
        api::put(
            &socket,
            "/network-interfaces/eth0",
            &serde_json::json!({
                "iface_id": "eth0",
                "host_dev_name": tap,
                "guest_mac": format!("06:00:{a:02X}:{b:02X}:{c:02X}:{d:02X}"),
            }),
        )?;
        api::put(
            &socket,
            "/actions",
            &serde_json::json!({"action_type": "InstanceStart"}),
        )?;
        Ok(vm)
    }
}

impl Launcher for Firecracker {
    fn kind(&self) -> TargetKind {
        Firecracker::kind(self)
    }

    fn enforces(&self) -> BTreeSet<ResourceKind> {
        Firecracker::enforces(self)
    }

    fn launch(
        &mut self,
        plan: &LaunchPlan,
    ) -> Result<Box<dyn Running>, spindle_target::LaunchError> {
        Ok(Box::new(Firecracker::launch(self, plan)?))
    }
}

/// The guest memory, in MiB, an initramfs of `bytes` needs: [`INITRAMFS_FACTOR`] times its size
/// plus [`BASE_MEMORY_MIB`].
pub fn required_memory_mib(bytes: u64) -> u64 {
    (bytes.saturating_mul(INITRAMFS_FACTOR)).div_ceil(MIB) + BASE_MEMORY_MIB
}

#[derive(Debug, Clone)]
enum Outcome {
    Ended(Exit),
    Failed { status: String },
}

/// One running (or ended) microVM. Dropping it kills a VM that is still running.
#[derive(Debug)]
pub struct FirecrackerVm {
    instance_id: String,
    console: PathBuf,
    socket: PathBuf,
    child: Child,
    outcome: Option<Outcome>,
    tap: Option<String>,
    address: Ipv4Addr,
    live: Addresses,
    namespace: Arc<Namespace>,
    started: Instant,
    request: Resources,
}

impl FirecrackerVm {
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// The guest's serial console and Firecracker's own messages.
    pub fn console(&self) -> &Path {
        &self.console
    }

    /// The exit, or `None` when the VM still runs after `timeout`.
    ///
    /// The exit's code is the guest program's status read from the console; `None` when the
    /// guest ended without reporting one. Firecracker failing is an error, not an exit.
    pub fn wait(&mut self, timeout: Duration) -> Result<Option<Exit>, LaunchError> {
        let started = Instant::now();
        loop {
            if let Some(outcome) = self.poll()? {
                return self.report(outcome).map(Some);
            }
            if started.elapsed() >= timeout {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// What Firecracker costs the host now (its `/proc/<pid>`, the VMM included) against the
    /// vCPUs and memory the plan asked for; `None` once the VM has ended.
    pub fn resources(&self) -> Option<ResourceSample> {
        if self.outcome.is_some() {
            return None;
        }
        let usage = spindle_target::process_usage(self.child.id()).ok()?;
        Some(ResourceSample::of(
            usage,
            self.started.elapsed(),
            &self.request,
        ))
    }

    /// Kill the VM if it still runs, and report how it ended: a VM stopped here reports
    /// `SIGKILL` and the status its guest printed, if any.
    pub fn stop(&mut self) -> Result<Exit, LaunchError> {
        if let Some(outcome) = self.poll()? {
            return self.report(outcome);
        }
        self.child
            .kill()
            .map_err(|source| LaunchError::io("killing firecracker", source))?;
        self.child
            .wait()
            .map_err(|source| LaunchError::io("waiting for firecracker", source))?;
        let exit = Exit {
            code: self.guest_status(),
            signal: Some(libc::SIGKILL),
        };
        self.finished(Outcome::Ended(exit));
        Ok(exit)
    }

    /// The raw console reading: the status in the last `spindle-init: exit status <n>` line.
    pub fn guest_status(&self) -> Option<i32> {
        let bytes = std::fs::read(&self.console).ok()?;
        String::from_utf8_lossy(&bytes)
            .lines()
            .rev()
            .find_map(|line| line.trim_end().strip_prefix(GUEST_STATUS)?.parse().ok())
    }

    fn report(&self, outcome: Outcome) -> Result<Exit, LaunchError> {
        match outcome {
            Outcome::Ended(exit) => Ok(exit),
            Outcome::Failed { status } => Err(LaunchError::Vmm {
                status,
                console: self.console_tail(),
            }),
        }
    }

    fn poll(&mut self) -> Result<Option<Outcome>, LaunchError> {
        if let Some(outcome) = &self.outcome {
            return Ok(Some(outcome.clone()));
        }
        let Some(status) = self
            .child
            .try_wait()
            .map_err(|source| LaunchError::io("waiting for firecracker", source))?
        else {
            return Ok(None);
        };
        let outcome = if status.success() {
            Outcome::Ended(Exit {
                code: self.guest_status(),
                signal: None,
            })
        } else {
            Outcome::Failed {
                status: match (status.code(), status.signal()) {
                    (Some(code), _) => format!("exited with status {code}"),
                    (None, Some(signal)) => format!("was ended by signal {signal}"),
                    (None, None) => format!("ended: {status}"),
                },
            }
        };
        self.finished(outcome.clone());
        Ok(Some(outcome))
    }

    /// Record the outcome once and release what the VM held: its tap, its API socket and its
    /// address.
    fn finished(&mut self, outcome: Outcome) {
        self.outcome = Some(outcome);
        if let Some(tap) = self.tap.take() {
            let _ = self.namespace.remove_tap(&tap);
            let _ = std::fs::remove_file(&self.socket);
            self.live
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .remove(&self.address);
        }
    }

    fn console_tail(&self) -> String {
        let bytes = std::fs::read(&self.console).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<_> = text.lines().collect();
        lines[lines.len().saturating_sub(40)..].join("\n")
    }

    fn wait_for_socket(&mut self, socket: &Path) -> Result<(), LaunchError> {
        let started = Instant::now();
        loop {
            if std::os::unix::net::UnixStream::connect(socket).is_ok() {
                return Ok(());
            }
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|source| LaunchError::io("waiting for firecracker", source))?
            {
                let outcome = Outcome::Failed {
                    status: format!("exited before it was configured ({status})"),
                };
                self.finished(outcome.clone());
                return self.report(outcome).map(|_| ());
            }
            if started.elapsed() > SOCKET_WITHIN {
                return Err(LaunchError::Network(format!(
                    "firecracker opened no API socket within {SOCKET_WITHIN:?}"
                )));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Running for FirecrackerVm {
    fn instance_id(&self) -> &str {
        FirecrackerVm::instance_id(self)
    }

    fn console(&self) -> &Path {
        FirecrackerVm::console(self)
    }

    fn wait(&mut self, timeout: Duration) -> Result<Option<Exit>, spindle_target::LaunchError> {
        Ok(FirecrackerVm::wait(self, timeout)?)
    }

    fn stop(&mut self) -> Result<Exit, spindle_target::LaunchError> {
        Ok(FirecrackerVm::stop(self)?)
    }

    fn resources(&self) -> Option<ResourceSample> {
        FirecrackerVm::resources(self)
    }
}

impl Drop for FirecrackerVm {
    fn drop(&mut self) {
        if self.outcome.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.finished(Outcome::Ended(Exit {
                code: None,
                signal: Some(libc::SIGKILL),
            }));
        }
    }
}

fn validate_instance_id(id: &str) -> Result<(), LaunchError> {
    let valid = !id.is_empty()
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(LaunchError::InvalidPlan(format!(
            "instance id `{id}` must be letters, digits, `-`, `_` and `.`, not starting with `.`"
        )))
    }
}

fn guest_address(listen: SocketAddr) -> Result<Ipv4Addr, LaunchError> {
    let SocketAddr::V4(listen) = listen else {
        return Err(LaunchError::InvalidPlan(format!(
            "listen address {listen} is not IPv4"
        )));
    };
    let address = *listen.ip();
    let mask = u32::MAX << (32 - PREFIX);
    let host = u32::from(address) & !mask;
    if u32::from(address) & mask != u32::from(GATEWAY) & mask
        || address == GATEWAY
        || host == 0
        || host == !mask
    {
        return Err(LaunchError::InvalidPlan(format!(
            "listen address {address} is not a guest address in {}/{PREFIX} other than the \
             gateway {GATEWAY}",
            Ipv4Addr::from(u32::from(GATEWAY) & mask)
        )));
    }
    Ok(address)
}

/// Whether the kernel keeps `c` inside one argument: printable ASCII other than `"`. Any other
/// byte may split it; the kernel treats every byte its `isspace` accepts as a separator, and that
/// includes 0xA0, the last byte of `à` in UTF-8.
fn carries(c: char) -> bool {
    c.is_ascii_graphic() && c != '"'
}

/// The kernel command line `plan` boots with, before Firecracker inserts
/// [`FIRECRACKER_INSERTS`]; refused when it cannot carry the plan.
pub fn kernel_command_line(plan: &LaunchPlan) -> Result<String, LaunchError> {
    let address = guest_address(plan.listen)?;
    let cmdline = match &plan.artifact {
        Artifact::Guest { cmdline, .. } => cmdline.trim(),
        other @ Artifact::HostBinary { .. } => {
            return Err(LaunchError::UnsupportedArtifact(other.kind()));
        }
    };
    let mask = Ipv4Addr::from(u32::MAX << (32 - PREFIX));
    let mut args = vec![
        "console=ttyS0".to_string(),
        "reboot=k".to_string(),
        "panic=-1".to_string(),
        "pci=off".to_string(),
        format!("ip={address}::{GATEWAY}:{mask}::eth0:off"),
    ];
    let mut variables: Vec<(String, String)> = vec![
        (
            spindle_target::ENV_INSTANCE_ID.to_string(),
            plan.instance_id.clone(),
        ),
        (
            spindle_target::ENV_COMPONENT.to_string(),
            plan.component.clone(),
        ),
        (
            spindle_target::ENV_LISTEN.to_string(),
            plan.listen.to_string(),
        ),
    ];
    let mut peers: BTreeMap<String, (&str, Vec<String>)> = BTreeMap::new();
    for peer in &plan.peers {
        let name = format!(
            "{}{}",
            spindle_target::ENV_PEER_PREFIX,
            spindle_target::env_name(&peer.component)
        );
        let entry = peers
            .entry(name.clone())
            .or_insert_with(|| (peer.component.as_str(), Vec::new()));
        if entry.0 != peer.component {
            return Err(LaunchError::InvalidPlan(format!(
                "peer components `{}` and `{}` both become {name}",
                entry.0, peer.component
            )));
        }
        entry.1.push(peer.address.to_string());
    }
    variables.extend(
        peers
            .into_iter()
            .map(|(name, (_, addresses))| (name, addresses.join(","))),
    );
    for (setting, value) in &plan.settings {
        variables.push((
            format!(
                "{}{}",
                spindle_target::ENV_SETTING_PREFIX,
                spindle_target::env_name(setting)
            ),
            value.clone(),
        ));
    }
    let mut names = BTreeSet::new();
    for (name, value) in &variables {
        if value.is_empty() || !value.chars().all(carries) {
            return Err(LaunchError::InvalidPlan(format!(
                "{name} = `{}` cannot be passed on a kernel command line: it must be non-empty \
                 printable ASCII without spaces or `\"`",
                value.escape_debug()
            )));
        }
        if !names.insert(name.as_str()) {
            return Err(LaunchError::InvalidPlan(format!(
                "two entries of the plan both become {name}"
            )));
        }
        args.push(format!("{name}={value}"));
    }
    if variables.len() > MAX_GUEST_VARIABLES {
        return Err(LaunchError::InvalidPlan(format!(
            "{} variables exceed the {MAX_GUEST_VARIABLES} a kernel passes to init",
            variables.len()
        )));
    }
    if !cmdline.chars().all(|c| c == ' ' || carries(c)) {
        return Err(LaunchError::InvalidPlan(format!(
            "the artifact's command line `{}` may hold only printable ASCII, spaces and no `\"`",
            cmdline.escape_debug()
        )));
    }
    if !cmdline.is_empty() {
        args.push(cmdline.to_string());
    }
    let line = args.join(" ");
    if line.len() > MAX_COMMAND_LINE {
        return Err(LaunchError::InvalidPlan(format!(
            "the kernel command line is {} bytes; the most is {MAX_COMMAND_LINE} (2048 less the \
             NUL and the {} bytes Firecracker inserts)",
            line.len(),
            FIRECRACKER_INSERTS.len()
        )));
    }
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> LaunchPlan {
        LaunchPlan {
            instance_id: "left".to_string(),
            component: "rally-table".to_string(),
            settings: vec![("serve.side".to_string(), "left".to_string())],
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
            peers: vec![
                Peer {
                    component: "rally-player".to_string(),
                    address: "172.16.0.3:7000".parse().expect("an address"),
                },
                Peer {
                    component: "rally-player".to_string(),
                    address: "172.16.0.4:7000".parse().expect("an address"),
                },
            ],
        }
    }

    #[test]
    fn the_plan_reaches_init_on_the_kernel_command_line() {
        assert_eq!(
            kernel_command_line(&plan()).expect("valid"),
            "console=ttyS0 reboot=k panic=-1 pci=off \
             ip=172.16.0.2::172.16.0.1:255.255.255.0::eth0:off SPINDLE_INSTANCE_ID=left \
             SPINDLE_COMPONENT=rally-table SPINDLE_LISTEN=172.16.0.2:7000 \
             SPINDLE_PEER_RALLY_PLAYER=172.16.0.3:7000,172.16.0.4:7000 \
             SPINDLE_SETTING_SERVE_SIDE=left -- exchange"
        );
    }

    #[test]
    fn addresses_outside_the_guest_network_are_refused() {
        for listen in [
            "172.16.0.1:7000",
            "172.16.0.0:7000",
            "172.16.0.255:7000",
            "10.0.0.2:7000",
            "[::1]:7000",
        ] {
            let error = guest_address(listen.parse().expect("an address"));
            assert!(
                matches!(error, Err(LaunchError::InvalidPlan(_))),
                "{listen}: {error:?}"
            );
        }
        assert_eq!(
            guest_address("172.16.0.254:1".parse().expect("an address")).expect("valid"),
            Ipv4Addr::new(172, 16, 0, 254)
        );
    }

    #[test]
    fn values_a_kernel_command_line_cannot_carry_are_refused() {
        let with_setting = |value: &str| {
            let mut plan = plan();
            plan.settings = vec![("name".to_string(), value.to_string())];
            plan
        };
        let mut clashing = plan();
        clashing.settings = vec![
            ("a.b".to_string(), "1".to_string()),
            ("a-b".to_string(), "2".to_string()),
        ];
        let mut peers_clash = plan();
        peers_clash.peers[1].component = "rally.player".to_string();
        let mut cmdline = plan();
        cmdline.artifact = Artifact::Guest {
            kernel: PathBuf::from("vmlinux"),
            initramfs: PathBuf::from("initramfs.cpio"),
            cmdline: "-- run check /sp\u{e0}c".to_string(),
        };
        for (why, plan) in [
            ("a space", with_setting("two words")),
            ("a quote", with_setting("say\"hi")),
            ("U+00E0", with_setting("voil\u{e0}")),
            ("U+0420", with_setting("\u{420}")),
            ("U+2020", with_setting("\u{2020}")),
            ("a tab", with_setting("a\tb")),
            ("an empty value", with_setting("")),
            ("two settings with one name", clashing),
            ("two peer components with one name", peers_clash),
            ("a non-ASCII init argument", cmdline),
        ] {
            let error = kernel_command_line(&plan);
            assert!(
                matches!(error, Err(LaunchError::InvalidPlan(_))),
                "{why}: {error:?}"
            );
        }
    }

    #[test]
    fn a_command_line_is_refused_one_byte_past_the_limit() {
        let mut plan = plan();
        plan.settings = Vec::new();
        let base = kernel_command_line(&plan).expect("valid").len();
        let overhead = " SPINDLE_SETTING_PAD=".len();
        let mut at = |length: usize| {
            plan.settings = vec![("pad".to_string(), "a".repeat(length))];
            kernel_command_line(&plan)
        };
        let fits = MAX_COMMAND_LINE - base - overhead;
        assert_eq!(at(fits).expect("at the limit").len(), MAX_COMMAND_LINE);
        assert!(matches!(at(fits + 1), Err(LaunchError::InvalidPlan(_))));
    }

    #[test]
    fn the_memory_rule_is_three_times_the_initramfs_and_64_mib() {
        assert_eq!(required_memory_mib(0), 64);
        assert_eq!(required_memory_mib(12_294_656), 100);
        assert_eq!(required_memory_mib(64 * MIB), 256);
    }

    #[test]
    fn a_host_binary_is_not_a_guest() {
        let mut plan = plan();
        plan.artifact = Artifact::HostBinary {
            path: PathBuf::from("b10x-spindle"),
            args: Vec::new(),
        };
        let mut launcher = Firecracker::new(PathBuf::from("firecracker"), PathBuf::from("state"));
        assert!(matches!(
            launcher.launch(&plan),
            Err(LaunchError::UnsupportedArtifact("host binary"))
        ));
    }

    #[test]
    fn a_zero_vcpu_or_memory_request_is_an_invalid_plan() {
        for request in [
            Resources {
                vcpus: 0,
                memory_mib: 128,
                disk_mib: 0,
            },
            Resources {
                vcpus: 1,
                memory_mib: 0,
                disk_mib: 0,
            },
        ] {
            let mut plan = plan();
            plan.request = request;
            let mut launcher =
                Firecracker::new(PathBuf::from("firecracker"), PathBuf::from("state"));
            assert!(
                matches!(launcher.launch(&plan), Err(LaunchError::InvalidPlan(_))),
                "{request:?}"
            );
        }
    }

    #[test]
    fn errors_reach_the_port_with_their_kind() {
        assert!(matches!(
            spindle_target::LaunchError::from(LaunchError::Unenforced(ResourceKind::Disk)),
            spindle_target::LaunchError::Unenforced(ResourceKind::Disk)
        ));
        assert!(matches!(
            spindle_target::LaunchError::from(LaunchError::InvalidPlan("x".to_string())),
            spindle_target::LaunchError::InvalidPlan(_)
        ));
        assert!(matches!(
            spindle_target::LaunchError::from(LaunchError::UnsupportedArtifact("host binary")),
            spindle_target::LaunchError::UnsupportedArtifact {
                launcher: TargetKind::Firecracker,
                artifact: "host binary"
            }
        ));
    }
}
