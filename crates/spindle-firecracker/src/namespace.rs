//! A user and network namespace the launcher owns, with one bridge its instances' taps join.
//!
//! An unprivileged user maps itself to root in a new user namespace (`unshare --user
//! --map-root-user --net`); that root holds `CAP_NET_ADMIN` over the new network namespace, so it
//! may create the bridge and the taps there, and Firecracker, entered into the same namespaces
//! with `nsenter`, may open them. Nothing on the host network changes.
//!
//! The namespace's holder and every Firecracker are started by one spawner thread that lives as
//! long as the namespace, each with `PR_SET_PDEATHSIG(SIGKILL)`: the kernel sends that signal when
//! the thread that forked a process ends, so when the launcher's process dies, however it dies,
//! its VMs and its namespace die with it.

use std::io::Read;
use std::net::Ipv4Addr;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::LaunchError;

/// The bridge every tap of one namespace joins.
pub(crate) const BRIDGE: &str = "spindle0";

const READY_WITHIN: Duration = Duration::from_secs(10);

type Request = (Command, mpsc::Sender<std::io::Result<Child>>);

/// A thread that forks every long-lived child, so that their parent-death signal fires only when
/// it ends.
#[derive(Debug)]
struct Spawner {
    requests: Option<mpsc::Sender<Request>>,
    thread: Option<JoinHandle<()>>,
}

impl Spawner {
    fn start() -> std::io::Result<Self> {
        let (requests, received) = mpsc::channel::<Request>();
        let thread = std::thread::Builder::new()
            .name("spindle-firecracker-spawner".to_string())
            .spawn(move || {
                for (mut command, reply) in received {
                    let _ = reply.send(spawn_with_parent_death_signal(&mut command));
                }
            })?;
        Ok(Self {
            requests: Some(requests),
            thread: Some(thread),
        })
    }

    fn spawn(&self, command: Command) -> std::io::Result<Child> {
        let (reply, answer) = mpsc::channel();
        let gone = || std::io::Error::other("the spawner thread has ended");
        self.requests
            .as_ref()
            .ok_or_else(gone)?
            .send((command, reply))
            .map_err(|_| gone())?;
        answer.recv().map_err(|_| gone())?
    }
}

impl Drop for Spawner {
    fn drop(&mut self) {
        self.requests = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What spawning answers off Linux: user and network namespaces, taps, KVM and the parent-death
/// signal are Linux's.
#[cfg(not(target_os = "linux"))]
pub const LINUX_ONLY: &str = "the Firecracker launcher runs on Linux only";

#[cfg(not(target_os = "linux"))]
fn spawn_with_parent_death_signal(_command: &mut Command) -> std::io::Result<Child> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        LINUX_ONLY,
    ))
}

#[cfg(target_os = "linux")]
fn spawn_with_parent_death_signal(command: &mut Command) -> std::io::Result<Child> {
    let parent = std::process::id();
    // SAFETY: the closure runs between fork and exec and calls only the async-signal-safe
    // `prctl`, `getppid` and `__errno_location` (through `last_os_error`).
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if u32::try_from(libc::getppid()).ok() != Some(parent) {
                return Err(std::io::Error::other("the launcher exited during the fork"));
            }
            Ok(())
        });
    }
    command.spawn()
}

#[derive(Debug)]
pub(crate) struct Namespace {
    holder: Child,
    spawner: Spawner,
}

impl Namespace {
    /// Start the namespace and bring up the bridge at `gateway/prefix`.
    pub(crate) fn start(gateway: Ipv4Addr, prefix: u8) -> Result<Self, LaunchError> {
        let spawner = Spawner::start()
            .map_err(|source| LaunchError::io("starting the spawner thread", source))?;
        let mut command = Command::new("unshare");
        command
            .args(["--user", "--map-root-user", "--net", "--", "cat"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let holder = spawner
            .spawn(command)
            .map_err(|source| LaunchError::io("starting `unshare`", source))?;
        let mut namespace = Self { holder, spawner };
        namespace.ready()?;
        namespace.ip(&["link", "set", "lo", "up"])?;
        namespace.ip(&["link", "add", BRIDGE, "type", "bridge"])?;
        namespace.ip(&["addr", "add", &format!("{gateway}/{prefix}"), "dev", BRIDGE])?;
        namespace.ip(&["link", "set", BRIDGE, "up"])?;
        Ok(namespace)
    }

    /// `program` run inside the namespace; `nsenter` replaces itself with it, so the child's PID
    /// is the program's.
    pub(crate) fn command(&self, program: &Path) -> Command {
        let mut command = Command::new("nsenter");
        command
            .arg(format!("--target={}", self.holder.id()))
            .args(["--user", "--net", "--preserve-credentials", "--"])
            .arg(program);
        command
    }

    /// Start a long-lived child from the spawner thread, so it dies with the launcher's process.
    pub(crate) fn spawn(&self, command: Command) -> std::io::Result<Child> {
        self.spawner.spawn(command)
    }

    /// A persistent tap on the bridge, up.
    pub(crate) fn add_tap(&self, name: &str) -> Result<(), LaunchError> {
        self.ip(&["tuntap", "add", name, "mode", "tap"])?;
        self.ip(&["link", "set", name, "master", BRIDGE])?;
        self.ip(&["link", "set", name, "up"])
    }

    pub(crate) fn remove_tap(&self, name: &str) -> Result<(), LaunchError> {
        self.ip(&["link", "del", name])
    }

    fn ready(&mut self) -> Result<(), LaunchError> {
        let pid = self.holder.id();
        let started = Instant::now();
        loop {
            if let Some(status) = self
                .holder
                .try_wait()
                .map_err(|source| LaunchError::io("waiting for `unshare`", source))?
            {
                let mut stderr = String::new();
                if let Some(mut pipe) = self.holder.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Err(LaunchError::Network(format!(
                    "`unshare --user --map-root-user --net` exited with {status}: {}",
                    stderr.trim()
                )));
            }
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            if comm.trim() == "cat" {
                return Ok(());
            }
            if started.elapsed() > READY_WITHIN {
                return Err(LaunchError::Network(format!(
                    "the namespace was not ready within {READY_WITHIN:?}"
                )));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn ip(&self, args: &[&str]) -> Result<(), LaunchError> {
        let output = self
            .command(Path::new("ip"))
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| LaunchError::io("starting `nsenter … ip`", source))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(LaunchError::Network(format!(
                "`ip {}` in the namespace exited with {}: {}",
                args.join(" "),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}

impl Drop for Namespace {
    fn drop(&mut self) {
        let _ = self.holder.kill();
        let _ = self.holder.wait();
    }
}
