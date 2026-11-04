//! The `Launcher` port and the plan it receives.

use std::collections::BTreeSet;
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::ResourceSample;

/// The kind of a target: `spindle.placement.TargetKind`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum TargetKind {
    /// A child process on the host.
    Process,
    /// A QEMU `microvm` machine.
    QemuMicrovm,
    /// A Firecracker microVM.
    Firecracker,
    /// A container.
    Docker,
    /// A set of containers started together.
    Compose,
    /// A cluster.
    Kubernetes,
    /// A browser tab.
    Browser,
}

impl TargetKind {
    /// Every kind, in the order the specification declares them.
    pub const ALL: [Self; 7] = [
        Self::Process,
        Self::QemuMicrovm,
        Self::Firecracker,
        Self::Docker,
        Self::Compose,
        Self::Kubernetes,
        Self::Browser,
    ];

    /// The variant's name in the specification.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Process => "Process",
            Self::QemuMicrovm => "QemuMicrovm",
            Self::Firecracker => "Firecracker",
            Self::Docker => "Docker",
            Self::Compose => "Compose",
            Self::Kubernetes => "Kubernetes",
            Self::Browser => "Browser",
        }
    }
}

impl fmt::Display for TargetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One kind of resource an instance can ask for and a target can cap.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// Virtual CPUs.
    Vcpus,
    /// Memory, in MiB.
    Memory,
    /// Disk, in MiB.
    Disk,
}

impl ResourceKind {
    /// Every kind, in field order of [`Resources`].
    pub const ALL: [Self; 3] = [Self::Vcpus, Self::Memory, Self::Disk];

    /// The field of `spindle.placement.Resources` that carries it.
    pub fn field(self) -> &'static str {
        match self {
            Self::Vcpus => "vcpus",
            Self::Memory => "memory_mib",
            Self::Disk => "disk_mib",
        }
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.field())
    }
}

/// An amount of each resource: `spindle.placement.Resources`.
///
/// As a request, zero means no limit was asked for that resource. As a ceiling, it is the most one
/// instance may take, so a zero ceiling admits only a zero request.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Resources {
    /// Virtual CPUs.
    pub vcpus: u32,
    /// Memory, in MiB.
    pub memory_mib: u64,
    /// Disk, in MiB.
    pub disk_mib: u64,
}

impl Resources {
    /// The amount of `kind`.
    pub fn get(&self, kind: ResourceKind) -> u64 {
        match kind {
            ResourceKind::Vcpus => u64::from(self.vcpus),
            ResourceKind::Memory => self.memory_mib,
            ResourceKind::Disk => self.disk_mib,
        }
    }

    /// The kinds with a non-zero amount, in [`ResourceKind::ALL`] order.
    pub fn asked(&self) -> impl Iterator<Item = ResourceKind> + '_ {
        ResourceKind::ALL
            .into_iter()
            .filter(|kind| self.get(*kind) != 0)
    }
}

/// What a launcher runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Artifact {
    /// A binary on the host, run with these arguments.
    HostBinary {
        /// The executable.
        path: PathBuf,
        /// Its arguments, after the program name.
        args: Vec<String>,
    },
    /// A guest booted from a kernel and an initramfs.
    Guest {
        /// The kernel image.
        kernel: PathBuf,
        /// The initramfs holding the instance's binary.
        initramfs: PathBuf,
        /// The kernel command line.
        cmdline: String,
    },
}

impl Artifact {
    /// The variant's name, for messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::HostBinary { .. } => "host binary",
            Self::Guest { .. } => "guest",
        }
    }
}

/// Another instance this one's channels connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// Its component.
    pub component: String,
    /// Where it listens.
    pub address: SocketAddr,
}

/// Everything a launcher needs to start one placed instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    /// `spindle.placement.InstanceId`.
    pub instance_id: String,
    /// The component the instance runs.
    pub component: String,
    /// Its settings, in order: `spindle.placement.Setting`.
    pub settings: Vec<(String, String)>,
    /// What to run.
    pub artifact: Artifact,
    /// The resources it asked for.
    pub request: Resources,
    /// Where it listens.
    pub listen: SocketAddr,
    /// Where its peers listen.
    pub peers: Vec<Peer>,
}

/// How an instance ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Exit {
    /// The exit code, when it exited.
    pub code: Option<i32>,
    /// The signal that ended it, when one did.
    pub signal: Option<i32>,
}

/// Why a launcher did not start, wait for or stop an instance.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    /// The plan asks for a resource this launcher cannot hold the instance to.
    #[error("the launcher cannot enforce a {0} request")]
    Unenforced(ResourceKind),
    /// The plan's artifact is not one this launcher runs.
    #[error("a {launcher} launcher cannot run a {artifact} artifact")]
    UnsupportedArtifact {
        /// The launcher's kind.
        launcher: TargetKind,
        /// The artifact's kind.
        artifact: &'static str,
    },
    /// The plan cannot be carried out as written.
    #[error("invalid launch plan: {0}")]
    InvalidPlan(String),
    /// The operating system refused.
    #[error("{what}: {source}")]
    Io {
        /// What was being done.
        what: String,
        /// The underlying error.
        source: std::io::Error,
    },
}

/// A kind of target's way of starting instances.
pub trait Launcher {
    /// The target kind it implements.
    fn kind(&self) -> TargetKind;
    /// The resources it holds an instance to.
    fn enforces(&self) -> BTreeSet<ResourceKind>;
    /// Starts the instance `plan` describes.
    ///
    /// Refuses with [`LaunchError::Unenforced`] before anything starts when the plan asks for a
    /// resource not in [`Self::enforces`].
    fn launch(&mut self, plan: &LaunchPlan) -> Result<Box<dyn Running>, LaunchError>;
}

/// One started instance.
pub trait Running {
    /// The instance's id.
    fn instance_id(&self) -> &str;
    /// The file its output goes to.
    fn console(&self) -> &Path;
    /// Waits up to `timeout` for the instance to end; `None` when it is still running.
    fn wait(&mut self, timeout: Duration) -> Result<Option<Exit>, LaunchError>;
    /// Stops the instance and reports how it ended. An instance that already ended reports that.
    fn stop(&mut self) -> Result<Exit, LaunchError>;

    /// What the instance costs the host now, against what it asked for; `None` when the
    /// launcher does not sample.
    fn resources(&self) -> Option<ResourceSample> {
        None
    }
}

/// The check every launcher makes first: the first resource `request` asks for that `enforces`
/// does not hold.
pub fn refuse_unenforced(
    enforces: &BTreeSet<ResourceKind>,
    request: &Resources,
) -> Result<(), LaunchError> {
    match request.asked().find(|kind| !enforces.contains(kind)) {
        Some(kind) => Err(LaunchError::Unenforced(kind)),
        None => Ok(()),
    }
}
