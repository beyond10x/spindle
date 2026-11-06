//! The rally example on three microVMs: the placement document `examples/rally/placement.toml`,
//! the QEMU guest that plays one component, and a match played on any launcher.
//!
//! Shared by `crates/spindle-qemu/tests/spindle-qemu/rally.rs` and
//! `crates/spindle-firecracker/tests/spindle-firecracker/rally.rs`.
//! Every instance a match starts is dropped when the match is read or a check fails, and both
//! launchers kill a VM that still runs when its handle is dropped.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_qemu::{BuildError, GuestImage, HostKernel, Network, QemuLauncher, StaticBinaries};
use spindle_target::{Artifact, LaunchPlan, Launcher, Peer, Resources, Running};

/// How long the table's VM may take to boot, link and play the match to its end.
pub const MATCH_WITHIN: Duration = Duration::from_secs(180);
/// How long a player's VM may take to end once the table's has.
pub const PLAYER_ENDS_WITHIN: Duration = Duration::from_secs(60);
/// The component that opens the match and ends it.
pub const TABLE: &str = "rally-table";
/// Where every guest finds the rally specification.
pub const GUEST_SPEC: &str = "/spec";
/// Where every guest finds the environment documents of `examples/environments`.
pub const GUEST_ENVIRONMENTS: &str = "/env";

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace")
}

pub fn spec() -> PathBuf {
    root().join("examples/rally/ess")
}

pub fn environments() -> PathBuf {
    root().join("examples/environments")
}

/// One instance of the placement: a component, the address it listens on, and its request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub component: String,
    pub listen: SocketAddr,
    pub request: Resources,
}

/// The placement document: the environment every target provides and one instance per
/// component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub environment: String,
    pub instances: Vec<Placed>,
}

/// Reads `examples/rally/placement.toml`.
pub fn placement() -> Placement {
    let path = root().join("examples/rally/placement.toml");
    let text = std::fs::read_to_string(&path).expect("the placement document reads");
    let document: toml::Table = text.parse().expect("the placement document is TOML");
    let text_of = |table: &toml::Table, key: &str| {
        table
            .get(key)
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("{}: `{key}` is not text", path.display()))
            .to_owned()
    };
    let number = |table: &toml::Table, key: &str| {
        table
            .get(key)
            .and_then(toml::Value::as_integer)
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or_else(|| panic!("{}: `{key}` is not a count", path.display()))
    };
    let instances = document
        .get("instance")
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| panic!("{}: no [[instance]]", path.display()))
        .iter()
        .map(|instance| {
            let table = instance.as_table().expect("an instance is a table");
            Placed {
                component: text_of(table, "component"),
                listen: text_of(table, "listen")
                    .parse()
                    .expect("`listen` is an address"),
                request: Resources {
                    vcpus: u32::try_from(number(table, "vcpus")).expect("a vCPU count"),
                    memory_mib: number(table, "memory_mib"),
                    disk_mib: 0,
                },
            }
        })
        .collect();
    Placement {
        environment: text_of(&document, "environment"),
        instances,
    }
}

impl Placement {
    /// The `b10x-spindle` arguments every guest runs: `play` of one component, which it reads
    /// from `SPINDLE_COMPONENT`, against the placement's environment, to one point.
    pub fn play_arguments(&self) -> Vec<String> {
        [
            "play",
            GUEST_SPEC,
            "--environment",
            &format!("{GUEST_ENVIRONMENTS}/{}.toml", self.environment),
            "--target-score",
            "1",
            "--speed",
            "50",
        ]
        .map(str::to_owned)
        .to_vec()
    }

    /// One launch plan per instance, every other instance a peer, all running `artifact`.
    pub fn plans(&self, artifact: &Artifact) -> Vec<LaunchPlan> {
        self.instances
            .iter()
            .map(|placed| LaunchPlan {
                instance_id: placed.component.clone(),
                component: placed.component.clone(),
                settings: Vec::new(),
                artifact: artifact.clone(),
                request: placed.request,
                listen: placed.listen,
                peers: self
                    .instances
                    .iter()
                    .filter(|other| other.component != placed.component)
                    .map(|other| Peer {
                        component: other.component.clone(),
                        address: other.listen,
                    })
                    .collect(),
            })
            .collect()
    }
}

fn console(instance: &dyn Running) -> String {
    String::from_utf8_lossy(&std::fs::read(instance.console()).unwrap_or_default()).into_owned()
}

/// Every JSON object `b10x-spindle` printed on a console, whatever the guest put before it.
pub fn lines(console: &str) -> Vec<Value> {
    console
        .lines()
        .filter_map(|line| {
            let start = line.find('{')?;
            serde_json::from_str(line[start..].trim_end()).ok()
        })
        .collect()
}

fn ran_delivered(console: &str, command: &str) -> bool {
    lines(console).iter().any(|line| {
        line.get("ran") == Some(&json!(command)) && line.get("cause") == Some(&json!("delivered"))
    })
}

/// What a match on three VMs showed: the table's last `MatchState` view, every console, and the
/// line each guest's `b10x-spindle` printed once listening (`component`, `listening`, `peers`),
/// by instance id.
#[derive(Debug)]
pub struct Played {
    pub view: Map<String, Value>,
    pub consoles: Vec<(String, String)>,
    pub links: BTreeMap<String, Value>,
}

impl Played {
    /// The winner the table's last view names.
    pub fn winner(&self) -> &str {
        self.view
            .get("winner")
            .and_then(Value::as_str)
            .filter(|side| !side.is_empty())
            .unwrap_or_else(|| panic!("the last view has no winner: {:?}", self.view))
    }
}

fn all(consoles: &[(String, String)]) -> String {
    consoles
        .iter()
        .map(|(id, text)| format!("--- {id}\n{text}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Launches every plan on `launcher`, waits for the table to end the match and for each player
/// to end after it, and checks that each VM exited 0 and that commands crossed between VMs:
/// the table ran a delivered `MovePaddle`, each player a delivered `ObserveBall`.
pub fn play(launcher: &mut dyn Launcher, plans: &[LaunchPlan]) -> Played {
    let mut instances: Vec<Box<dyn Running>> = Vec::new();
    for plan in plans {
        match launcher.launch(plan) {
            Ok(instance) => instances.push(instance),
            Err(error) => panic!("{} does not start: {error}", plan.instance_id),
        }
    }
    let snapshot = |instances: &[Box<dyn Running>]| {
        instances
            .iter()
            .map(|instance| {
                (
                    instance.instance_id().to_owned(),
                    console(instance.as_ref()),
                )
            })
            .collect::<Vec<_>>()
    };
    let table = instances
        .iter()
        .position(|instance| instance.instance_id() == TABLE)
        .expect("the placement places the table");
    let exit = instances[table]
        .wait(MATCH_WITHIN)
        .expect("the table's VM is observable");
    let Some(exit) = exit else {
        panic!(
            "the table did not end the match within {MATCH_WITHIN:?}:\n{}",
            all(&snapshot(&instances))
        );
    };
    assert_eq!(
        exit.code,
        Some(0),
        "the table's VM: {exit:?}\n{}",
        all(&snapshot(&instances))
    );
    for index in (0..instances.len()).filter(|index| *index != table) {
        let ended = instances[index]
            .wait(PLAYER_ENDS_WITHIN)
            .expect("the player's VM is observable");
        assert_eq!(
            ended.and_then(|exit| exit.code),
            Some(0),
            "{} ends once the table is gone ({ended:?}):\n{}",
            instances[index].instance_id(),
            all(&snapshot(&instances))
        );
    }
    let consoles = snapshot(&instances);
    drop(instances);

    let table_console = &consoles[table].1;
    let view = lines(table_console)
        .into_iter()
        .filter_map(|line| line.get("view").and_then(Value::as_object).cloned())
        .next_back()
        .unwrap_or_else(|| panic!("the table printed no MatchState view:\n{}", all(&consoles)));
    assert_eq!(view.get("state"), Some(&json!("Won")), "{view:?}");
    assert!(
        ran_delivered(table_console, "rally.table.MovePaddle"),
        "a player's request crossed the VM network to the table:\n{}",
        all(&consoles)
    );
    for (id, text) in consoles.iter().filter(|(id, _)| id != TABLE) {
        let side = if id.contains("west") { "west" } else { "east" };
        assert!(
            ran_delivered(text, &format!("rally.{side}.ObserveBall")),
            "the table's ball crossed the VM network to {id}:\n{}",
            all(&consoles)
        );
    }
    let links = consoles
        .iter()
        .map(|(id, text)| {
            let link = lines(text)
                .into_iter()
                .find(|line| line.get("listening").is_some())
                .unwrap_or_else(|| panic!("{id} never printed its listening line:\n{text}"));
            (id.clone(), link)
        })
        .collect();
    eprintln!(
        "{:?}: {} instances exited 0; links: {}; the table's last view: {}",
        launcher.kind(),
        consoles.len(),
        json!(links),
        Value::Object(view.clone())
    );
    Played {
        view,
        consoles,
        links,
    }
}

/// What a host needs to boot the rally on QEMU microVMs: QEMU, the host kernel and the static
/// guest binaries, built once per test process.
pub struct Qemu {
    qemu: PathBuf,
    kernel: HostKernel,
    binaries: StaticBinaries,
}

/// The QEMU host, or a panic naming what this host lacks to boot a QEMU microVM (`/dev/kvm`,
/// `qemu-system-x86_64`, the host kernel and its modules, the musl target) or why the static
/// guest build failed, as `tests/spindle-qemu/microvm.rs` does: these cases run only when asked for.
pub fn qemu() -> Qemu {
    let (qemu, kernel) = spindle_qemu::vm_prerequisites()
        .unwrap_or_else(|reason| panic!("this host cannot boot a QEMU microVM: {reason}"));
    let work = std::env::var_os("SPINDLE_QEMU_WORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_TARGET_TMPDIR")).join("spindle-qemu"));
    std::fs::create_dir_all(&work).unwrap_or_else(|error| panic!("{}: {error}", work.display()));
    match StaticBinaries::build(&root(), &work.join("musl-target")) {
        Ok(binaries) => Qemu {
            qemu,
            kernel,
            binaries,
        },
        Err(BuildError::TargetMissing(reason)) => {
            panic!("this host cannot boot a QEMU microVM: {reason}")
        }
        Err(error) => panic!("the static guest build failed: {error}"),
    }
}

impl Qemu {
    /// A guest holding `b10x-spindle`, the rally specification and the environment documents,
    /// written under `dir`, that runs `b10x-spindle` with `arguments`.
    pub fn artifact(&self, dir: &Path, arguments: &[String]) -> Artifact {
        let mut image = GuestImage::new(&self.binaries.init, &self.kernel).expect("a guest image");
        image
            .binary("/bin/b10x-spindle", &self.binaries.spindle)
            .expect("the b10x-spindle binary");
        image.tree(GUEST_SPEC, &spec()).expect("the specification");
        image
            .tree(GUEST_ENVIRONMENTS, &environments())
            .expect("the environment documents");
        let initramfs = dir.join("rally-qemu.cpio");
        image.write(&initramfs).expect("the initramfs is written");
        Artifact::Guest {
            kernel: self.kernel.image.clone(),
            initramfs,
            cmdline: format!("-- /bin/b10x-spindle {}", arguments.join(" ")),
        }
    }

    /// A launcher keeping its instances under `dir`, on a network of its own whose guests
    /// share the placement's `/24`.
    pub fn launcher(&self, dir: &Path) -> QemuLauncher {
        let port = UdpSocket::bind("127.0.0.1:0")
            .and_then(|socket| socket.local_addr())
            .map(|address| address.port())
            .expect("a free loopback port");
        let network =
            Network::multicast(SocketAddrV4::new(Ipv4Addr::new(239, 255, 77, 2), port), 24);
        QemuLauncher::new(dir.join("instances"), network).with_qemu(self.qemu.clone())
    }
}
