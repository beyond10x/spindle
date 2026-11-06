//! The `process` launcher: an instance is a child process on the host.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rustix::io::Errno;
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};

use crate::{
    Artifact, Exit, LaunchError, LaunchPlan, Launcher, ResourceKind, ResourceSample, Resources,
    Running, TargetKind, process_group_usage, process_usage, refuse_unenforced,
};

/// The instance's id.
pub const ENV_INSTANCE_ID: &str = "SPINDLE_INSTANCE_ID";
/// The component it runs.
pub const ENV_COMPONENT: &str = "SPINDLE_COMPONENT";
/// The address it listens on.
pub const ENV_LISTEN: &str = "SPINDLE_LISTEN";
/// Prefix of one variable per setting, named by [`env_name`] of the setting's name.
pub const ENV_SETTING_PREFIX: &str = "SPINDLE_SETTING_";
/// Prefix of one variable per peer component, named by [`env_name`] of the component. Its value
/// is the address of every peer of that component, comma-separated, in plan order.
pub const ENV_PEER_PREFIX: &str = "SPINDLE_PEER_";

/// The longest a dropped instance is given between `SIGTERM` and `SIGKILL`, so that dropping one
/// never blocks for the launcher's whole grace period.
const DROP_GRACE: Duration = Duration::from_secs(1);

const POLL: Duration = Duration::from_millis(10);

/// `name` as the suffix of an environment variable: ASCII letters and digits upper-cased, every
/// other character `_`. `rally-table` becomes `RALLY_TABLE`.
pub fn env_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Launches each instance as a child process of the caller, in a process group of its own.
///
/// It enforces no resource, so it refuses every plan that asks for one. Standard output and
/// standard error go to `<console_dir>/<instance_id>.log`; a launch that starts nothing leaves the
/// previous console as it was. Stopping sends `SIGTERM` to the instance's process group, then
/// `SIGKILL` to the group once the instance has exited or the grace period is over, so nothing
/// the instance started outlives it. On Linux the instance is also sent `SIGTERM` by the system
/// when the thread that launched it ends (`PR_SET_PDEATHSIG`), so an instance never outlives its
/// launcher's process, even one killed without a chance to stop it; one whose launcher is already
/// gone when it starts is not started. Other systems have no parent-death signal: there an
/// instance whose launcher is killed without stopping it keeps running.
#[derive(Debug, Clone)]
pub struct ProcessLauncher {
    console_dir: PathBuf,
    grace: Duration,
    telemetry: Option<spindle_telemetry::Endpoint>,
}

impl ProcessLauncher {
    /// A launcher writing consoles under `console_dir`, with a five-second grace period.
    pub fn new(console_dir: impl Into<PathBuf>) -> Self {
        Self {
            console_dir: console_dir.into(),
            grace: Duration::from_secs(5),
            telemetry: None,
        }
    }

    /// The same launcher with another grace period between `SIGTERM` and `SIGKILL`.
    /// [`Duration::MAX`] waits for as long as the instance takes.
    pub fn with_grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    /// The same launcher giving every instance the Unix socket `socket` as its telemetry endpoint
    /// (`SPINDLE_TELEMETRY=unix:<socket>`). The caller listens on it; an instance identifies
    /// itself in every frame.
    ///
    /// Refused with [`LaunchError::InvalidPlan`] when the path is not UTF-8 or longer than
    /// [`spindle_telemetry::MAX_UNIX_PATH`] bytes: the instance could never connect to it.
    pub fn with_telemetry(mut self, socket: impl Into<PathBuf>) -> Result<Self, LaunchError> {
        let endpoint = spindle_telemetry::Endpoint::unix(socket)
            .map_err(|error| LaunchError::InvalidPlan(format!("telemetry endpoint: {error}")))?;
        self.telemetry = Some(endpoint);
        Ok(self)
    }
}

impl Launcher for ProcessLauncher {
    fn kind(&self) -> TargetKind {
        TargetKind::Process
    }

    fn enforces(&self) -> BTreeSet<ResourceKind> {
        BTreeSet::new()
    }

    fn launch(&mut self, plan: &LaunchPlan) -> Result<Box<dyn Running>, LaunchError> {
        refuse_unenforced(&self.enforces(), &plan.request)?;
        let Artifact::HostBinary { path, args } = &plan.artifact else {
            return Err(LaunchError::UnsupportedArtifact {
                launcher: self.kind(),
                artifact: plan.artifact.kind(),
            });
        };
        if !is_file_name(&plan.instance_id) {
            return Err(LaunchError::InvalidPlan(format!(
                "instance id {:?} is not a plain file name of letters, digits, '.', '-' and '_'",
                plan.instance_id
            )));
        }
        let variables = launch_variables(plan, self.telemetry.as_ref())?;

        fs::create_dir_all(&self.console_dir).map_err(|source| LaunchError::Io {
            what: format!("creating {}", self.console_dir.display()),
            source,
        })?;
        let console = self.console_dir.join(format!("{}.log", plan.instance_id));
        let partial = self
            .console_dir
            .join(format!("{}.log.partial", plan.instance_id));
        let io = |source| LaunchError::Io {
            what: format!("opening {}", partial.display()),
            source,
        };
        let stdout = File::create(&partial).map_err(io)?;
        let stderr = match stdout.try_clone() {
            Ok(stderr) => stderr,
            Err(source) => {
                let _ = fs::remove_file(&partial);
                return Err(io(source));
            }
        };

        let mut command = Command::new(path);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .process_group(0);
        #[cfg(target_os = "linux")]
        {
            let parent = rustix::process::getpid();
            // SAFETY: the closure runs in the child between fork and exec and calls only `prctl`
            // and `getppid`, both async-signal-safe system calls; it allocates nothing.
            unsafe {
                command.pre_exec(move || {
                    rustix::process::set_parent_process_death_signal(Some(Signal::TERM))?;
                    if rustix::process::getppid() != Some(parent) {
                        return Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
                    }
                    Ok(())
                });
            }
        }
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("SPINDLE_") {
                command.env_remove(name);
            }
        }
        command.envs(variables);
        let started = Instant::now();
        let child = match command.spawn() {
            Ok(child) => child,
            Err(source) => {
                let _ = fs::remove_file(&partial);
                return Err(LaunchError::Io {
                    what: format!("starting {}", path.display()),
                    source,
                });
            }
        };
        let pid = Pid::from_child(&child);
        let mut instance = ProcessInstance {
            instance_id: plan.instance_id.clone(),
            console,
            child,
            pid,
            exit: None,
            reaped: false,
            grace: self.grace,
            started,
            request: plan.request,
        };
        if let Err(source) = fs::rename(&partial, &instance.console) {
            let _ = instance.terminate(Duration::ZERO);
            let _ = fs::remove_file(&partial);
            return Err(LaunchError::Io {
                what: format!("moving the console to {}", instance.console.display()),
                source,
            });
        }
        Ok(Box::new(instance))
    }
}

fn is_file_name(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('.')
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

/// The variables an instance of `plan` runs with: [`ENV_INSTANCE_ID`], [`ENV_COMPONENT`],
/// [`ENV_LISTEN`], `SPINDLE_TELEMETRY` when there is an endpoint, one
/// `SPINDLE_SETTING_<NAME>` per setting and one `SPINDLE_PEER_<COMPONENT>` per peer component,
/// its addresses comma-separated in plan order. The process launcher sets them for its child; a
/// microVM launcher hands them to the guest's init.
///
/// Refused as [`LaunchError::InvalidPlan`]: a setting without a name, a peer without a
/// component, two entries that map to one variable, and a value holding a NUL byte, which no
/// process environment can carry.
pub fn launch_variables(
    plan: &LaunchPlan,
    telemetry: Option<&spindle_telemetry::Endpoint>,
) -> Result<BTreeMap<String, String>, LaunchError> {
    let mut variables = BTreeMap::new();
    let mut set = |name: String, value: String, from: &str| {
        if value.contains('\0') {
            return Err(LaunchError::InvalidPlan(format!(
                "{from} gives {name} a NUL byte, which no process environment can carry"
            )));
        }
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
    if let Some(endpoint) = telemetry {
        set(
            spindle_telemetry::ENV_TELEMETRY.to_owned(),
            endpoint.to_string(),
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

/// A child process started by [`ProcessLauncher`], leading its own process group.
///
/// The child is observed without being reaped until it is stopped or dropped, so its process id,
/// which is also the group's id, cannot be reused while the group is still signalled by it.
struct ProcessInstance {
    instance_id: String,
    console: PathBuf,
    child: Child,
    pid: Pid,
    exit: Option<Exit>,
    reaped: bool,
    grace: Duration,
    started: Instant,
    request: Resources,
}

impl ProcessInstance {
    fn io(&self, what: &str, errno: Errno) -> LaunchError {
        LaunchError::Io {
            what: format!("{what} {}", self.instance_id),
            source: errno.into(),
        }
    }

    /// Records the exit if the child has ended, leaving it waitable.
    fn observe(&mut self) -> Result<Option<Exit>, LaunchError> {
        if self.exit.is_none() {
            let options = WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG;
            let status = waitid(WaitId::Pid(self.pid), options)
                .map_err(|errno| self.io("waiting for", errno))?;
            self.exit = status.map(|status| Exit {
                code: status.exit_status(),
                signal: status.terminating_signal(),
            });
        }
        Ok(self.exit)
    }

    fn signal_group(&self, signal: Signal) -> Result<(), LaunchError> {
        match kill_process_group(self.pid, signal) {
            Ok(()) | Err(Errno::SRCH) => Ok(()),
            Err(errno) => Err(self.io("signalling the process group of", errno)),
        }
    }

    /// `SIGTERM` to the group, up to `grace` for the child to exit, `SIGKILL` to the group, then
    /// the child is reaped.
    fn terminate(&mut self, grace: Duration) -> Result<Exit, LaunchError> {
        if let (true, Some(exit)) = (self.reaped, self.exit) {
            return Ok(exit);
        }
        self.signal_group(Signal::TERM)?;
        self.wait(grace)?;
        self.signal_group(Signal::KILL)?;
        let status = self.child.wait().map_err(|source| LaunchError::Io {
            what: format!("reaping {}", self.instance_id),
            source,
        })?;
        self.reaped = true;
        Ok(*self.exit.get_or_insert(Exit {
            code: status.code(),
            signal: status.signal(),
        }))
    }
}

impl Running for ProcessInstance {
    fn process_group(&self) -> Option<u32> {
        Some(self.pid.as_raw_nonzero().get().unsigned_abs())
    }

    fn instance_id(&self) -> &str {
        &self.instance_id
    }

    fn console(&self) -> &Path {
        &self.console
    }

    fn wait(&mut self, timeout: Duration) -> Result<Option<Exit>, LaunchError> {
        let deadline = Instant::now().checked_add(timeout);
        loop {
            if let Some(exit) = self.observe()? {
                return Ok(Some(exit));
            }
            let pause = match deadline {
                None => POLL,
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Ok(None);
                    }
                    POLL.min(deadline - now)
                }
            };
            std::thread::sleep(pause);
        }
    }

    fn stop(&mut self) -> Result<Exit, LaunchError> {
        self.terminate(self.grace)
    }

    /// The sum over the instance's process group, so a child it started counts; `None` once
    /// the instance has ended. Shares are `None` too: this launcher holds it to no limit.
    ///
    /// The instance has ended once its leader has, whether or not [`wait`](Running::wait) has
    /// reaped it yet: a leader that exited and is not yet reaped (a zombie) is still a member of
    /// its group, and is not sampled as running.
    fn resources(&self) -> Option<ResourceSample> {
        if self.exit.is_some() || self.reaped {
            return None;
        }
        let leader = self.pid.as_raw_nonzero().get().unsigned_abs();
        process_usage(leader).ok()?;
        let usage = process_group_usage(leader).ok()?;
        Some(ResourceSample::of(
            usage,
            self.started.elapsed(),
            &self.request,
        ))
    }
}

impl Drop for ProcessInstance {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.terminate(self.grace.min(DROP_GRACE));
        }
    }
}
