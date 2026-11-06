//! Launch, collect and serve: the control process of one deployment.

use std::collections::BTreeMap;
use std::io::BufReader;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use ess_compiler::EssIr;
use spindle_capability::{Capability, Environment};
use spindle_target::{
    Artifact, EnvironmentAdmission, LaunchPlan, Launcher, Peer, Placement, ProcessLauncher,
    Running, Target, TargetKind, TargetState, place,
};
use spindle_telemetry::{Frame, FrameReader};

use crate::answer::{self, DeployedInstance, Deployment, InstanceState};
use crate::history::{Clock, SystemClock, WINDOW};
use crate::hub::Hub;
use crate::placement::{InstanceEntry, PlacementDocument};
use crate::server::{Served, Server, ViewPaths};

/// The setting the control process gives an instance of a component reached over the network:
/// the address its ingress listens on (`SPINDLE_SETTING_INGRESS` for a process).
pub const INGRESS_SETTING: &str = "ingress";

/// The setting beside [`INGRESS_SETTING`]: the token every request to that ingress carries, one per
/// deployment (`SPINDLE_SETTING_INGRESS_TOKEN` for a process). The ingress also refuses a `Host`
/// other than `127.0.0.1:<port>` or `localhost:<port>`, as the control process does.
pub const INGRESS_TOKEN_SETTING: &str = "ingress_token";

/// How often the supervisor looks for frames, exits and the stop flag.
const POLL: Duration = Duration::from_millis(10);

/// What a component's instances run, by component.
pub type Artifacts = Box<dyn Fn(&str) -> Artifact + Send>;

/// How the control process runs.
pub struct Options {
    /// The port to serve on, on `127.0.0.1`; `0` lets the system choose.
    pub port: u16,
    /// Where consoles and, when its path is short enough, the telemetry socket go.
    pub work: PathBuf,
    /// How long samples are kept.
    pub window: Duration,
    /// The clock samples are stamped with.
    pub clock: Arc<dyn Clock>,
}

impl Options {
    /// Port `0`, the last [`WINDOW`] of samples on the system clock, and `work` as the work
    /// directory.
    pub fn new(work: &Path) -> Self {
        Self {
            port: 0,
            work: work.to_path_buf(),
            window: WINDOW,
            clock: Arc::new(SystemClock),
        }
    }

    /// The same options serving on `port`.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
}

/// Why the control process did not start.
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    /// The placement names a component the specification does not declare.
    #[error(
        "instance {instance} runs component {component}, which the specification does not declare"
    )]
    UnknownComponent {
        /// The instance.
        instance: String,
        /// The component it names.
        component: String,
    },
    /// The placement sets the setting the control process gives itself.
    #[error("instance {instance} sets {setting:?}, which the control process sets")]
    ReservedSetting {
        /// The instance.
        instance: String,
        /// The setting.
        setting: &'static str,
    },
    /// The operating system refused.
    #[error("{what}: {source}")]
    Io {
        /// What was being done.
        what: String,
        /// The underlying error.
        source: std::io::Error,
    },
    /// A launcher could not be set up.
    #[error("{0}")]
    Launcher(String),
}

fn io(what: impl Into<String>) -> impl FnOnce(std::io::Error) -> ControlError {
    let what = what.into();
    move |source| ControlError::Io { what, source }
}

/// An instance the control process did not run, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotRun {
    /// The instance.
    pub instance: String,
    /// Why, in words.
    pub why: String,
}

/// One running control process: its instances supervised on a thread of its own, the API served
/// on another. Dropping it stops every instance and the server.
pub struct Control {
    server: Option<Server>,
    token: String,
    ingress_token: String,
    ingress: BTreeMap<String, SocketAddr>,
    not_run: Vec<NotRun>,
    stop: Arc<AtomicBool>,
    supervisor: Option<JoinHandle<()>>,
    collector: Option<JoinHandle<()>>,
    socket: PathBuf,
    _socket_dir: Option<tempfile::TempDir>,
}

impl std::fmt::Debug for Control {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Control")
            .field("address", &self.local_addr())
            .field("socket", &self.socket)
            .finish_non_exhaustive()
    }
}

/// A random token: 128 bits from the system's random source, as hex.
fn token() -> Result<String, ControlError> {
    use std::io::Read as _;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut bytes))
        .map_err(io("reading /dev/urandom"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// A loopback address no one listens on now.
fn free_address() -> Result<SocketAddr, ControlError> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map_err(io("finding a free loopback port"))
}

/// Where each view of `component` is served by its ingress: a `GET` route with no path variable.
fn view_paths(ir: &EssIr, component: &str) -> BTreeMap<String, String> {
    let Ok(surface) = spindle_ingress::Surface::new(ir, &[component.to_owned()], None) else {
        return BTreeMap::new();
    };
    surface
        .routes()
        .iter()
        .filter(|route| {
            route.operation.kind() == "view"
                && route.method.as_str().eq_ignore_ascii_case("get")
                && route.variables().is_empty()
                && route.unserved().is_none()
        })
        .map(|route| (route.operation.name().to_owned(), route.path.clone()))
        .collect()
}

/// Whether instances of `component` are reached over the network.
fn reached_over_the_network(ir: &EssIr, component: &str) -> bool {
    spindle_capability::derive(ir, Some(component))
        .is_some_and(|requirements| requirements.contains_key(&Capability::new("ingress.network")))
}

/// The plan of one instance, everything a launcher needs but the artifact and the placed request.
struct Planned {
    entry: InstanceEntry,
    target: Target,
    admission: EnvironmentAdmission,
    listen: SocketAddr,
    ingress: Option<SocketAddr>,
}

impl Control {
    /// Places every instance of `placement` on its target, launches each one placed through its
    /// target's launcher, and serves the observe API and the UI on `127.0.0.1`.
    ///
    /// Each component is admitted against `environment`; a target provides it when its
    /// `environment` is the document's `name`. An instance that placement refuses, or whose
    /// launcher refuses or fails, is not run: it is listed by [`Control::not_run`] and its state
    /// is `Refused` or `Exited`. This build launches on `Process` targets; an instance on another
    /// kind is not run.
    pub fn start(
        ir: EssIr,
        environment: Environment,
        placement: PlacementDocument,
        artifacts: Artifacts,
        options: Options,
    ) -> Result<Self, ControlError> {
        let ingress_token = token()?;
        let token = token()?;
        let (system, components, bindings) = answer::system(&ir);
        let mut catalogues = BTreeMap::new();
        for component in &components {
            if let Some(catalogue) = spindle_telemetry::catalogue(&ir, &component.name) {
                catalogues.insert(
                    component.name.clone(),
                    serde_json::to_string(&catalogue).unwrap_or_default(),
                );
            }
        }

        let mut planned = Vec::new();
        let mut instances = Vec::new();
        let mut views: ViewPaths = BTreeMap::new();
        for entry in &placement.instances {
            if !components.iter().any(|known| known.name == entry.component) {
                return Err(ControlError::UnknownComponent {
                    instance: entry.id.clone(),
                    component: entry.component.clone(),
                });
            }
            for setting in [INGRESS_SETTING, INGRESS_TOKEN_SETTING] {
                if entry.settings.contains_key(setting) {
                    return Err(ControlError::ReservedSetting {
                        instance: entry.id.clone(),
                        setting,
                    });
                }
            }
            let declared = placement
                .target(&entry.target)
                .expect("the placement document checks every instance's target");
            let target = Target {
                id: declared.id.clone(),
                kind: declared.kind,
                environment: declared.environment.clone(),
                ceiling: declared.ceiling,
                state: TargetState::Available,
            };
            let requirements =
                spindle_capability::derive(&ir, Some(&entry.component)).unwrap_or_default();
            let ingress = if reached_over_the_network(&ir, &entry.component) {
                views
                    .entry(entry.component.clone())
                    .or_insert_with(|| view_paths(&ir, &entry.component));
                Some(free_address()?)
            } else {
                None
            };
            instances.push(DeployedInstance {
                instance_id: entry.id.clone(),
                component: entry.component.clone(),
                target_kind: target.kind,
                target: target.id.clone(),
                state: InstanceState::Planned,
                requested: entry.request,
                started_at: None,
            });
            planned.push(Planned {
                entry: entry.clone(),
                admission: EnvironmentAdmission::decide(&requirements, &environment),
                target,
                listen: free_address()?,
                ingress,
            });
        }
        let ingress = planned
            .iter()
            .filter_map(|plan| Some((plan.entry.id.clone(), plan.ingress?)))
            .collect::<BTreeMap<_, _>>();

        let hub = Arc::new(Hub::new(
            Deployment {
                system,
                components,
                bindings,
                instances,
            },
            catalogues,
            options.window,
            Arc::clone(&options.clock),
        ));

        std::fs::create_dir_all(&options.work)
            .map_err(io(format!("creating {}", options.work.display())))?;
        let (socket, socket_dir) = socket_path(&options.work)?;
        let listener = UnixListener::bind(&socket)
            .map_err(io(format!("listening on {}", socket.display())))?;
        listener
            .set_nonblocking(true)
            .map_err(io("setting the telemetry socket non-blocking"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let (frames, received) = mpsc::channel();
        let collector = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("spindle-control-collect".to_owned())
                .spawn(move || collect(&listener, &frames, &stop))
                .map_err(io("starting the collector"))?
        };

        let (bound, address) = Server::bind(options.port)
            .map_err(io(format!("binding 127.0.0.1:{}", options.port)))?;
        let server = Server::start(
            bound,
            Served {
                hub: Arc::clone(&hub),
                token: token.clone(),
                ingress_token: ingress_token.clone(),
                port: address.port(),
                ingress: ingress.clone(),
                views,
            },
        )
        .map_err(io("starting the server"))?;

        let consoles = options.work.join("consoles");
        let (launched, ready) = mpsc::channel();
        let supervisor = {
            let stop = Arc::clone(&stop);
            let hub = Arc::clone(&hub);
            let socket = socket.clone();
            let clock = Arc::clone(&options.clock);
            let ingress_token = ingress_token.clone();
            std::thread::Builder::new()
                .name("spindle-control-supervise".to_owned())
                .spawn(move || {
                    let mut supervised = Supervisor {
                        hub,
                        clock,
                        ingress_token,
                        running: Vec::new(),
                    };
                    let not_run = supervised.launch(&planned, &artifacts, &consoles, &socket);
                    let _ = launched.send(not_run);
                    supervised.supervise(&received, &stop);
                })
                .map_err(io("starting the supervisor"))?
        };
        let not_run = ready.recv().unwrap_or_default();

        Ok(Self {
            server: Some(server),
            token,
            ingress_token,
            ingress,
            not_run,
            stop,
            supervisor: Some(supervisor),
            collector: Some(collector),
            socket,
            _socket_dir: socket_dir,
        })
    }

    /// Where the API and the UI are served: `127.0.0.1:<port>`.
    pub fn local_addr(&self) -> SocketAddr {
        self.server
            .as_ref()
            .map(Server::local_addr)
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)))
    }

    /// The token every request carries.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The token every request to an instance's ingress carries ([`INGRESS_TOKEN_SETTING`]).
    pub fn ingress_token(&self) -> &str {
        &self.ingress_token
    }

    /// The UI's address with the token: `http://127.0.0.1:<port>/?t=<token>`.
    pub fn url(&self) -> String {
        format!("http://{}/?t={}", self.local_addr(), self.token)
    }

    /// Where the ingress of `instance` listens, for an instance of a component reached over the
    /// network.
    pub fn ingress(&self, instance: &str) -> Option<SocketAddr> {
        self.ingress.get(instance).copied()
    }

    /// Every instance that was not run, and why.
    pub fn not_run(&self) -> &[NotRun] {
        &self.not_run
    }

    /// Stops every instance, then the server.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(supervisor) = self.supervisor.take() {
            let _ = supervisor.join();
        }
        if let Some(collector) = self.collector.take() {
            let _ = collector.join();
        }
        self.server.take();
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The name of the socket file, and of the directory made for it where `work` is too deep.
const SOCKET: &str = "telemetry.sock";
const SOCKET_DIR_PREFIX: &str = "spindle-control-";
/// The length of the directory name `tempfile` makes: the prefix and six random characters.
const SOCKET_DIR_NAME: usize = SOCKET_DIR_PREFIX.len() + 6;

/// The telemetry socket: in `work` when its path fits `sun_path`; otherwise in a new directory,
/// kept as long as the control process runs, under the first of `$XDG_RUNTIME_DIR`,
/// `/run/user/<uid>` and the temporary directory that exists and is short enough.
fn socket_path(work: &Path) -> Result<(PathBuf, Option<tempfile::TempDir>), ControlError> {
    let fits = |path: &Path| path.as_os_str().len() <= spindle_telemetry::MAX_UNIX_PATH;
    let inside = work.join(SOCKET);
    if fits(&inside) {
        return Ok((inside, None));
    }
    let mut bases = Vec::new();
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()) {
        bases.push(PathBuf::from(runtime));
    }
    if let Ok(own) = std::fs::metadata("/proc/self") {
        use std::os::unix::fs::MetadataExt as _;
        bases.push(PathBuf::from(format!("/run/user/{}", own.uid())));
    }
    bases.push(std::env::temp_dir());
    let room = SOCKET_DIR_NAME + 1 + SOCKET.len();
    let base = bases
        .iter()
        .find(|base| {
            base.is_dir() && base.as_os_str().len() + 1 + room <= spindle_telemetry::MAX_UNIX_PATH
        })
        .ok_or_else(|| {
            ControlError::Launcher(format!(
                "no directory for the telemetry socket: {} and each of {} leave a path longer \
                 than {} bytes",
                work.display(),
                bases
                    .iter()
                    .map(|base| base.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                spindle_telemetry::MAX_UNIX_PATH
            ))
        })?;
    let dir = tempfile::Builder::new()
        .prefix(SOCKET_DIR_PREFIX)
        .tempdir_in(base)
        .map_err(io(format!(
            "creating a directory for the telemetry socket in {}",
            base.display()
        )))?;
    Ok((dir.path().join(SOCKET), Some(dir)))
}

/// Accepts instances' telemetry connections until `stop`, reading each on a thread of its own.
fn collect(
    listener: &UnixListener,
    frames: &mpsc::Sender<(Option<u32>, Frame)>,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let frames = frames.clone();
                let _ = std::thread::Builder::new()
                    .name("spindle-control-frames".to_owned())
                    .spawn(move || read(stream, &frames));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL);
            }
            Err(_) => std::thread::sleep(POLL),
        }
    }
}

/// Reads frames from one connection until it closes or sends something that is not a frame, each
/// with the process id of the connection's peer.
///
/// The connection is bound to the instance id of its first frame: a later frame naming another id
/// is dropped. Which connection may speak for an instance is the supervisor's check
/// ([`Supervisor::frame`]): the peer must be in the instance's process group.
fn read(stream: UnixStream, frames: &mpsc::Sender<(Option<u32>, Frame)>) {
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let peer = peer_pid(&stream);
    let mut reader = FrameReader::new(BufReader::new(stream));
    let mut bound: Option<String> = None;
    while let Ok(Some(frame)) = reader.next_frame() {
        let id = bound.get_or_insert_with(|| frame.instance_id.clone());
        if frame.instance_id == *id && frames.send((peer, frame)).is_err() {
            return;
        }
    }
}

/// The process id of the peer of a Unix stream: `SO_PEERCRED` on Linux, `LOCAL_PEERPID` on
/// Apple systems; `None` where neither exists, so no frame counts for an instance in a process
/// group of its own.
#[cfg(target_os = "linux")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    rustix::net::sockopt::socket_peercred(stream)
        .ok()
        .map(|credentials| credentials.pid.as_raw_nonzero().get().unsigned_abs())
}

#[cfg(target_vendor = "apple")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd as _;
    let mut pid: libc::pid_t = 0;
    let mut length = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: `pid` and `length` are valid for writing for the duration of the call.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&raw mut pid).cast(),
            &raw mut length,
        )
    };
    (result == 0 && pid > 0).then_some(pid.unsigned_abs())
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
fn peer_pid(_stream: &UnixStream) -> Option<u32> {
    None
}

/// The thread that owns the launchers and the running instances.
struct Supervisor {
    hub: Arc<Hub>,
    clock: Arc<dyn Clock>,
    ingress_token: String,
    running: Vec<(String, Box<dyn Running>)>,
}

impl Supervisor {
    fn now(&self) -> String {
        spindle_telemetry::timestamp(self.clock.now())
    }

    /// Places and launches every planned instance, in order; returns those not run.
    fn launch(
        &mut self,
        planned: &[Planned],
        artifacts: &Artifacts,
        consoles: &Path,
        socket: &Path,
    ) -> Vec<NotRun> {
        let mut not_run = Vec::new();
        let mut process = match ProcessLauncher::new(consoles).with_telemetry(socket) {
            Ok(launcher) => Ok(launcher),
            Err(error) => Err(error.to_string()),
        };
        for plan in planned {
            let id = plan.entry.id.as_str();
            let request = match place(&plan.target, &plan.entry.request, &plan.admission) {
                Placement::Placed { request } => {
                    self.hub.state(id, InstanceState::Placed, None);
                    request
                }
                Placement::Refused { reasons } => {
                    self.hub.state(id, InstanceState::Refused, None);
                    not_run.push(NotRun {
                        instance: id.to_owned(),
                        why: reasons
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("; "),
                    });
                    continue;
                }
            };
            let launcher: Result<&mut dyn Launcher, String> = match plan.target.kind {
                TargetKind::Process => process
                    .as_mut()
                    .map(|launcher| launcher as &mut dyn Launcher)
                    .map_err(|why| why.clone()),
                other => Err(format!("this build launches no {other} target")),
            };
            let mut settings = plan
                .entry
                .settings
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>();
            if let Some(ingress) = plan.ingress {
                settings.push((INGRESS_SETTING.to_owned(), ingress.to_string()));
                settings.push((INGRESS_TOKEN_SETTING.to_owned(), self.ingress_token.clone()));
            }
            let launch_plan = LaunchPlan {
                instance_id: id.to_owned(),
                component: plan.entry.component.clone(),
                settings,
                artifact: artifacts(&plan.entry.component),
                request,
                listen: plan.listen,
                peers: planned
                    .iter()
                    .filter(|other| other.entry.component != plan.entry.component)
                    .map(|other| Peer {
                        component: other.entry.component.clone(),
                        address: other.listen,
                    })
                    .collect(),
            };
            let started = self.now();
            match launcher.and_then(|launcher| {
                launcher
                    .launch(&launch_plan)
                    .map_err(|error| error.to_string())
            }) {
                Ok(running) => {
                    self.hub.state(id, InstanceState::Running, Some(started));
                    self.running.push((id.to_owned(), running));
                }
                Err(why) => {
                    self.hub.state(id, InstanceState::Exited, None);
                    not_run.push(NotRun {
                        instance: id.to_owned(),
                        why,
                    });
                }
            }
        }
        not_run
    }

    /// Takes frames and exits until `stop`, then stops every instance still running.
    fn supervise(&mut self, frames: &mpsc::Receiver<(Option<u32>, Frame)>, stop: &AtomicBool) {
        while !stop.load(Ordering::SeqCst) {
            match frames.recv_timeout(POLL) {
                Ok((peer, frame)) => self.frame(peer, frame),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(POLL),
            }
            self.reap();
        }
        for (id, mut running) in self.running.drain(..) {
            let _ = running.stop();
            self.hub.state(&id, InstanceState::Exited, None);
        }
    }

    /// Publishes `frame` as a sample of its instance, with what the instance costs the host where
    /// its launcher samples it. A frame for an instance that is not running is dropped, and so is
    /// one for an instance in a process group of its own sent over a connection whose peer,
    /// `peer`, is not in that group.
    fn frame(&self, peer: Option<u32>, frame: Frame) {
        let Some((_, running)) = self.running.iter().find(|(id, _)| *id == frame.instance_id)
        else {
            return;
        };
        if let Some(group) = running.process_group() {
            let from = peer
                .and_then(|pid| rustix::process::Pid::from_raw(i32::try_from(pid).ok()?))
                .and_then(|pid| rustix::process::getpgid(Some(pid)).ok())
                .map(|from| from.as_raw_nonzero().get().unsigned_abs());
            if from != Some(group) {
                return;
            }
        }
        let instance = frame.instance_id.clone();
        let mut sample = frame;
        if let Some(host) = running.resources() {
            sample.resources.cpu_seconds = host.cpu_seconds;
            sample.resources.memory_bytes = host.memory_bytes;
            sample.resources.cpu_share = host.cpu_share;
            sample.resources.memory_share = host.memory_share;
        }
        if let Ok(value) = serde_json::to_value(&sample) {
            self.hub.sample(&instance, value);
        }
    }

    /// Moves every instance that has ended to `Exited`.
    fn reap(&mut self) {
        let mut index = 0;
        while index < self.running.len() {
            let ended = matches!(self.running[index].1.wait(Duration::ZERO), Ok(Some(_)));
            if ended {
                let (id, mut running) = self.running.remove(index);
                let _ = running.stop();
                self.hub.state(&id, InstanceState::Exited, None);
            } else {
                index += 1;
            }
        }
    }
}
