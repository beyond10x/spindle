//! What every QEMU of one launcher shares: the thread that spawns them, the user and network
//! namespace they run in, and the ids of the instances that still run.
//!
//! An unprivileged user maps itself to root in a new user namespace (`unshare --user
//! --map-root-user --net`), which owns a new network namespace with nothing but a loopback. QEMU
//! is entered into both with `nsenter`, so its `-netdev socket` multicast group lives on that
//! loopback: no socket of another process on the host can join it.
//!
//! Every child is spawned from one thread with `PR_SET_PDEATHSIG(SIGKILL)`. The thread lives as
//! long as any launcher clone or instance does, and dies with the process, so a QEMU never
//! outlives the process that launched it.

use std::collections::BTreeMap;
use std::io::{self, Read};
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use spindle_target::LaunchError;

const READY_WITHIN: Duration = Duration::from_secs(10);

/// What spawning answers off Linux: user and network namespaces, KVM and the parent-death signal
/// are Linux's.
#[cfg(not(target_os = "linux"))]
pub const LINUX_ONLY: &str = "the QEMU microVM launcher runs on Linux only";

pub(crate) fn failure(what: impl Into<String>, detail: impl Into<String>) -> LaunchError {
    LaunchError::Io {
        what: what.into(),
        source: io::Error::other(detail.into()),
    }
}

pub(crate) fn io_error(what: impl Into<String>) -> impl FnOnce(io::Error) -> LaunchError {
    let what = what.into();
    move |source| LaunchError::Io { what, source }
}

type Request = (Command, mpsc::Sender<io::Result<Child>>);

/// The thread every child of a launcher is spawned from.
#[derive(Debug)]
struct Spawner {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    requests: Mutex<mpsc::Sender<Request>>,
}

impl Spawner {
    fn start() -> io::Result<Self> {
        let (requests, incoming) = mpsc::channel::<Request>();
        std::thread::Builder::new()
            .name("spindle-qemu-spawner".to_owned())
            .spawn(move || {
                for (mut command, reply) in incoming {
                    let _ = reply.send(command.spawn());
                }
            })?;
        Ok(Self {
            requests: Mutex::new(requests),
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn spawn(&self, _command: Command) -> io::Result<Child> {
        Err(io::Error::new(io::ErrorKind::Unsupported, LINUX_ONLY))
    }

    #[cfg(target_os = "linux")]
    fn spawn(&self, mut command: Command) -> io::Result<Child> {
        let launcher = std::process::id();
        // SAFETY: the closure runs in the forked child before exec and only makes the
        // async-signal-safe calls prctl(2) and getppid(2).
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::getppid() as u32 != launcher {
                    return Err(io::Error::other("the launching process is gone"));
                }
                Ok(())
            });
        }
        let (reply, answer) = mpsc::channel();
        self.requests
            .lock()
            .map_err(|_| io::Error::other("the spawner lock is poisoned"))?
            .send((command, reply))
            .map_err(|_| io::Error::other("the spawner thread is gone"))?;
        answer
            .recv()
            .map_err(|_| io::Error::other("the spawner thread is gone"))?
    }
}

/// The holder of the user and network namespace: `unshare … cat`, which lives until its stdin
/// closes or it is killed.
#[derive(Debug)]
struct Namespace {
    holder: Child,
    _stdin: Option<ChildStdin>,
}

impl Drop for Namespace {
    fn drop(&mut self) {
        let _ = self.holder.kill();
        let _ = self.holder.wait();
    }
}

/// State shared by every clone of one launcher and every instance it started.
#[derive(Debug)]
pub(crate) struct Shared {
    spawner: Mutex<Option<Spawner>>,
    namespace: Mutex<Option<Namespace>>,
    live: Mutex<BTreeMap<String, u32>>,
}

impl Shared {
    pub(crate) fn new() -> Self {
        Self {
            spawner: Mutex::new(None),
            namespace: Mutex::new(None),
            live: Mutex::new(BTreeMap::new()),
        }
    }

    fn spawn(&self, command: Command) -> io::Result<Child> {
        let mut spawner = self
            .spawner
            .lock()
            .map_err(|_| io::Error::other("the spawner lock is poisoned"))?;
        if spawner.is_none() {
            *spawner = Some(Spawner::start()?);
        }
        spawner
            .as_ref()
            .map_or_else(|| Err(io::Error::other("no spawner")), |s| s.spawn(command))
    }

    /// The holder's pid, starting the namespace (with its loopback up) on first use.
    fn holder(&self) -> Result<u32, LaunchError> {
        let mut namespace = self
            .namespace
            .lock()
            .map_err(|_| failure("the namespace", "its lock is poisoned"))?;
        if let Some(namespace) = namespace.as_ref() {
            return Ok(namespace.holder.id());
        }
        let mut command = Command::new("unshare");
        command
            .args(["--user", "--map-root-user", "--net", "--", "cat"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut holder = self
            .spawn(command)
            .map_err(io_error("starting `unshare --user --map-root-user --net`"))?;
        let stdin = holder.stdin.take();
        let started = Namespace {
            holder,
            _stdin: stdin,
        };
        let (pid, held) = ready(started)?;
        let output = self
            .entered(pid, Path::new("ip"))
            .args(["link", "set", "lo", "up"])
            .stdin(Stdio::null())
            .output()
            .map_err(io_error("starting `nsenter … ip link set lo up`"))?;
        if !output.status.success() {
            return Err(failure(
                "`ip link set lo up` in the launcher's namespace",
                format!(
                    "{}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }
        *namespace = Some(held);
        Ok(pid)
    }

    fn entered(&self, holder: u32, program: &Path) -> Command {
        let mut command = Command::new("nsenter");
        command
            .arg(format!("--target={holder}"))
            .args(["--user", "--net", "--preserve-credentials", "--"])
            .arg(program);
        command
    }

    /// Spawn `argv` inside the launcher's namespace. `nsenter` replaces itself with the
    /// program, so the child's pid is the program's.
    pub(crate) fn spawn_entered(
        &self,
        argv: &[String],
        stdout: Stdio,
        stderr: Stdio,
    ) -> Result<Child, LaunchError> {
        let holder = self.holder()?;
        let mut command = self.entered(holder, Path::new(&argv[0]));
        command
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr);
        self.spawn(command)
            .map_err(io_error(format!("starting {} through nsenter", argv[0])))
    }

    /// Lock the registry of live instances.
    pub(crate) fn live(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, u32>>, LaunchError> {
        self.live
            .lock()
            .map_err(|_| failure("the instance registry", "its lock is poisoned"))
    }

    /// Forget `instance` if it is still registered with `pid`.
    pub(crate) fn ended(&self, instance: &str, pid: u32) {
        if let Ok(mut live) = self.live.lock()
            && live.get(instance) == Some(&pid)
        {
            live.remove(instance);
        }
    }
}

fn ready(mut namespace: Namespace) -> Result<(u32, Namespace), LaunchError> {
    let pid = namespace.holder.id();
    let started = Instant::now();
    loop {
        if let Some(status) = namespace
            .holder
            .try_wait()
            .map_err(io_error("waiting for `unshare`"))?
        {
            let mut stderr = String::new();
            if let Some(mut pipe) = namespace.holder.stderr.take() {
                let _ = pipe.read_to_string(&mut stderr);
            }
            return Err(failure(
                "`unshare --user --map-root-user --net`",
                format!("exited with {status}: {}", stderr.trim()),
            ));
        }
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        if comm.trim() == "cat" {
            return Ok((pid, namespace));
        }
        if started.elapsed() > READY_WITHIN {
            return Err(failure(
                "the launcher's namespace",
                format!("not ready within {READY_WITHIN:?}"),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Whether `pid` names a process that has not ended (neither gone nor a zombie).
pub(crate) fn runs(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    stat.rsplit_once(')')
        .is_some_and(|(_, rest)| !rest.trim_start().starts_with('Z'))
}
