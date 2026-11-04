use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ess_compiler::ir::{ResolvedBody, ResolvedTypeRef};
use spindle_capability::{Admission, Capability, Environment, derive};
use spindle_target::{
    Artifact, EnvironmentAdmission, Exit, LaunchError, LaunchPlan, Launcher, Peer, Placement,
    ProcessLauncher, Refusal, ResourceKind, Resources, Running, StartError, Target, TargetKind,
    TargetState, place, start,
};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn environment(name: &str) -> Environment {
    let path = repository().join(format!("examples/environments/{name}.toml"));
    let text = std::fs::read_to_string(&path).expect("the environment document reads");
    Environment::parse(&text).expect("the environment parses")
}

fn admission(specification: &str, component: &str, environment_name: &str) -> EnvironmentAdmission {
    let loaded = spindle_spec::load(&repository().join(specification)).expect("compiles");
    let requirements = derive(&loaded.ir, Some(component)).expect("the component exists");
    EnvironmentAdmission::decide(&requirements, &environment(environment_name))
}

fn target(kind: TargetKind, environment: &str, ceiling: Resources) -> Target {
    Target {
        id: "target-1".to_owned(),
        kind,
        environment: environment.to_owned(),
        ceiling,
        state: TargetState::Available,
    }
}

fn address(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn plan(instance_id: &str, settings: &[(&str, &str)], request: Resources) -> LaunchPlan {
    LaunchPlan {
        instance_id: instance_id.to_owned(),
        component: "rally-table".to_owned(),
        settings: settings
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        artifact: Artifact::HostBinary {
            path: PathBuf::from(env!("CARGO_BIN_EXE_spindle-target-fixture")),
            args: vec!["--table".to_owned(), "one".to_owned()],
        },
        request,
        listen: address(7100),
        peers: vec![
            Peer {
                component: "rally-player-west".to_owned(),
                address: address(7101),
            },
            Peer {
                component: "rally-player-east".to_owned(),
                address: address(7102),
            },
        ],
    }
}

#[derive(Default)]
struct Recording {
    launched: Vec<Resources>,
}

impl Launcher for Recording {
    fn kind(&self) -> TargetKind {
        TargetKind::QemuMicrovm
    }

    fn enforces(&self) -> BTreeSet<ResourceKind> {
        ResourceKind::ALL.into_iter().collect()
    }

    fn launch(&mut self, plan: &LaunchPlan) -> Result<Box<dyn Running>, LaunchError> {
        self.launched.push(plan.request);
        Err(LaunchError::InvalidPlan(
            "the recording launcher starts nothing".to_owned(),
        ))
    }
}

fn console_text(running: &dyn Running, until: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = std::fs::read_to_string(running.console()).unwrap_or_default();
        if text.contains(until) || Instant::now() > deadline {
            return text;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_request_above_the_ceiling_is_refused_before_launch() {
    let admitted = admission("examples/rally/ess", "rally-table", "microvm");
    assert!(admitted.admission.is_admitted(), "{admitted:?}");
    let microvm = target(
        TargetKind::QemuMicrovm,
        "microvm",
        Resources {
            vcpus: 1,
            memory_mib: 128,
            disk_mib: 64,
        },
    );
    let request = Resources {
        vcpus: 2,
        memory_mib: 512,
        disk_mib: 64,
    };

    let expected = vec![
        Refusal::AboveCeiling {
            resource: ResourceKind::Vcpus,
            requested: 2,
            ceiling: 1,
        },
        Refusal::AboveCeiling {
            resource: ResourceKind::Memory,
            requested: 512,
            ceiling: 128,
        },
    ];
    assert_eq!(
        place(&microvm, &request, &admitted),
        Placement::Refused {
            reasons: expected.clone()
        }
    );

    let mut launcher = Recording::default();
    match start(
        &mut launcher,
        &microvm,
        &admitted,
        &plan("table-1", &[], request),
    ) {
        Err(StartError::Refused(reasons)) => assert_eq!(reasons, expected),
        Err(other) => panic!("expected a refusal, got {other}"),
        Ok(_) => panic!("expected a refusal, got a running instance"),
    }
    assert!(
        launcher.launched.is_empty(),
        "a refused instance is never launched"
    );

    let within = Resources {
        vcpus: 1,
        memory_mib: 128,
        disk_mib: 64,
    };
    assert_eq!(
        place(&microvm, &within, &admitted),
        Placement::Placed { request: within }
    );
    assert!(
        start(
            &mut launcher,
            &microvm,
            &admitted,
            &plan("table-1", &[], within)
        )
        .is_err()
    );
    assert_eq!(
        launcher.launched,
        vec![within],
        "a placed instance reaches the launcher"
    );
}

#[test]
fn an_instance_whose_environment_refuses_its_component_is_refused() {
    let refused = admission("ess", "spindle-control", "microvm");
    assert_eq!(refused.environment, "microvm");
    let Admission::Refused { missing } = &refused.admission else {
        panic!("microvm should refuse spindle-control: {refused:?}");
    };
    assert_eq!(
        missing,
        &vec![
            Capability::new("external.spindle.deployment.DecideAdmission.refused"),
            Capability::new("external.spindle.placement.PlaceInstance.refused"),
        ]
    );

    let mut microvm = target(TargetKind::QemuMicrovm, "microvm", Resources::default());
    assert_eq!(
        place(&microvm, &Resources::default(), &refused),
        Placement::Refused {
            reasons: vec![Refusal::NotAdmitted {
                missing: missing.clone()
            }]
        }
    );

    microvm.state = TargetState::Retired;
    let over = Resources {
        vcpus: 1,
        ..Resources::default()
    };
    let Placement::Refused { reasons } = place(&microvm, &over, &refused) else {
        panic!("a retired target places nothing");
    };
    assert_eq!(
        reasons,
        vec![
            Refusal::TargetRetired,
            Refusal::NotAdmitted {
                missing: missing.clone()
            },
            Refusal::AboveCeiling {
                resource: ResourceKind::Vcpus,
                requested: 1,
                ceiling: 0,
            },
        ],
        "every reason is named, not only the first"
    );
    assert_eq!(
        reasons[1].to_string(),
        "the target's environment does not provide \
         external.spindle.deployment.DecideAdmission.refused, \
         external.spindle.placement.PlaceInstance.refused"
    );

    let mut launcher = Recording::default();
    let result = start(
        &mut launcher,
        &target(TargetKind::QemuMicrovm, "microvm", Resources::default()),
        &refused,
        &plan("control-1", &[], Resources::default()),
    );
    assert!(matches!(result, Err(StartError::Refused(_))));
    assert!(launcher.launched.is_empty());
}

#[test]
fn an_admission_decided_for_another_environment_is_refused() {
    let laptop = admission("examples/rally/ess", "rally-table", "laptop");
    assert!(laptop.admission.is_admitted(), "{laptop:?}");
    let microvm = target(TargetKind::QemuMicrovm, "microvm", Resources::default());

    let mismatch = Refusal::EnvironmentMismatch {
        target: "microvm".to_owned(),
        admitted: "laptop".to_owned(),
    };
    assert_eq!(
        place(&microvm, &Resources::default(), &laptop),
        Placement::Refused {
            reasons: vec![mismatch.clone()]
        }
    );
    assert_eq!(
        mismatch.to_string(),
        "the admission was decided for environment laptop, and the target provides microvm"
    );

    let refused_elsewhere = admission("ess", "spindle-control", "microvm");
    let laptop_target = target(TargetKind::QemuMicrovm, "laptop", Resources::default());
    assert_eq!(
        place(&laptop_target, &Resources::default(), &refused_elsewhere),
        Placement::Refused {
            reasons: vec![Refusal::EnvironmentMismatch {
                target: "laptop".to_owned(),
                admitted: "microvm".to_owned(),
            }]
        },
        "another environment's missing capabilities say nothing about this one"
    );

    let mut launcher = Recording::default();
    assert!(matches!(
        start(
            &mut launcher,
            &microvm,
            &laptop,
            &plan("table-1", &[], Resources::default())
        ),
        Err(StartError::Refused(reasons)) if reasons == vec![mismatch.clone()]
    ));
    assert!(launcher.launched.is_empty());
}

#[test]
fn a_zero_request_is_held_to_the_target_ceiling() {
    let admitted = admission("examples/rally/ess", "rally-table", "microvm");
    let microvm = target(
        TargetKind::QemuMicrovm,
        "microvm",
        Resources {
            vcpus: 2,
            memory_mib: 256,
            disk_mib: 0,
        },
    );
    let request = Resources {
        vcpus: 0,
        memory_mib: 128,
        disk_mib: 0,
    };
    let resolved = Resources {
        vcpus: 2,
        memory_mib: 128,
        disk_mib: 0,
    };
    assert_eq!(
        place(&microvm, &request, &admitted),
        Placement::Placed { request: resolved },
        "an unasked resource takes the ceiling; a zero ceiling stays zero"
    );

    let mut launcher = Recording::default();
    assert!(
        start(
            &mut launcher,
            &microvm,
            &admitted,
            &plan("table-1", &[], request)
        )
        .is_err()
    );
    assert_eq!(
        launcher.launched,
        vec![resolved],
        "the launcher receives the resolved request, so it enforces the ceiling"
    );

    let zero = target(TargetKind::QemuMicrovm, "microvm", Resources::default());
    assert_eq!(
        place(&zero, &Resources::default(), &admitted),
        Placement::Placed {
            request: Resources::default()
        }
    );
}

#[test]
fn the_process_launcher_starts_and_stops_an_instance_and_reports_its_exit() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path()).with_grace(Duration::from_secs(5));
    assert_eq!(launcher.kind(), TargetKind::Process);

    let host = target(TargetKind::Process, "laptop", Resources::default());
    let admitted = admission("examples/rally/ess", "rally-table", "laptop");
    let mut serving = start(
        &mut launcher,
        &host,
        &admitted,
        &plan(
            "table-1",
            &[("match-id", "m1"), ("points", "5")],
            Resources::default(),
        ),
    )
    .expect("the instance starts");
    assert_eq!(serving.instance_id(), "table-1");
    assert_eq!(serving.console(), consoles.path().join("table-1.log"));

    let text = console_text(serving.as_ref(), "ready");
    for line in [
        "arg --table",
        "arg one",
        "env SPINDLE_COMPONENT=rally-table",
        "env SPINDLE_INSTANCE_ID=table-1",
        "env SPINDLE_LISTEN=127.0.0.1:7100",
        "env SPINDLE_PEER_RALLY_PLAYER_EAST=127.0.0.1:7102",
        "env SPINDLE_PEER_RALLY_PLAYER_WEST=127.0.0.1:7101",
        "env SPINDLE_SETTING_MATCH_ID=m1",
        "env SPINDLE_SETTING_POINTS=5",
        "ready",
    ] {
        assert!(
            text.lines().any(|candidate| candidate == line),
            "{line:?} missing from the console:\n{text}"
        );
    }

    assert_eq!(
        serving.wait(Duration::from_millis(100)).expect("waits"),
        None,
        "the instance runs until it is stopped"
    );
    let stopped = serving.stop().expect("stops");
    assert_eq!(
        stopped,
        Exit {
            code: None,
            signal: Some(15)
        }
    );
    assert_eq!(serving.stop().expect("stops again"), stopped);

    let mut exiting = start(
        &mut launcher,
        &host,
        &admitted,
        &plan(
            "table-2",
            &[("mode", "exit"), ("exit_code", "3")],
            Resources::default(),
        ),
    )
    .expect("the instance starts");
    let exit = exiting
        .wait(Duration::from_secs(10))
        .expect("waits")
        .expect("the instance exits on its own");
    assert_eq!(
        exit,
        Exit {
            code: Some(3),
            signal: None
        }
    );
    assert_eq!(exiting.stop().expect("stop after exit"), exit);
}

#[test]
fn a_launcher_refuses_a_resource_it_cannot_enforce() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path());
    assert!(launcher.enforces().is_empty());

    for (request, kind) in [
        (
            Resources {
                vcpus: 1,
                ..Resources::default()
            },
            ResourceKind::Vcpus,
        ),
        (
            Resources {
                memory_mib: 256,
                disk_mib: 64,
                ..Resources::default()
            },
            ResourceKind::Memory,
        ),
        (
            Resources {
                disk_mib: 64,
                ..Resources::default()
            },
            ResourceKind::Disk,
        ),
    ] {
        match launcher.launch(&plan("table-1", &[], request)) {
            Err(LaunchError::Unenforced(refused)) => assert_eq!(refused, kind),
            Err(other) => panic!("expected Unenforced({kind}), got {other}"),
            Ok(_) => panic!("expected Unenforced({kind}), got a running instance"),
        }
    }
    assert!(
        !consoles.path().join("table-1.log").exists(),
        "nothing started, so no console was written"
    );

    let admitted = admission("examples/rally/ess", "rally-table", "laptop");
    let memory_only = target(
        TargetKind::Process,
        "laptop",
        Resources {
            memory_mib: 1024,
            ..Resources::default()
        },
    );
    let request = Resources {
        memory_mib: 256,
        ..Resources::default()
    };
    assert_eq!(
        place(&memory_only, &request, &admitted),
        Placement::Placed { request }
    );
    assert!(matches!(
        start(
            &mut launcher,
            &memory_only,
            &admitted,
            &plan("table-1", &[], request)
        ),
        Err(StartError::Launch(LaunchError::Unenforced(
            ResourceKind::Memory
        )))
    ));

    let capped = target(
        TargetKind::Process,
        "laptop",
        Resources {
            vcpus: 4,
            ..Resources::default()
        },
    );
    assert!(
        matches!(
            start(
                &mut launcher,
                &capped,
                &admitted,
                &plan("table-1", &[], Resources::default())
            ),
            Err(StartError::Launch(LaunchError::Unenforced(
                ResourceKind::Vcpus
            )))
        ),
        "a ceiling resolves an unasked resource, and the process launcher cannot hold it"
    );

    assert!(matches!(
        launcher.launch(&LaunchPlan {
            artifact: Artifact::Guest {
                kernel: PathBuf::from("vmlinux"),
                initramfs: PathBuf::from("initramfs.cpio"),
                cmdline: String::new(),
            },
            ..plan("table-1", &[], Resources::default())
        }),
        Err(LaunchError::UnsupportedArtifact {
            launcher: TargetKind::Process,
            ..
        })
    ));

    assert!(matches!(
        start(
            &mut Recording::default(),
            &memory_only,
            &admitted,
            &plan("table-1", &[], Resources::default())
        ),
        Err(StartError::WrongLauncher {
            target: TargetKind::Process,
            launcher: TargetKind::QemuMicrovm
        })
    ));
}

fn declared_type<'ir>(ir: &'ir spindle_spec::EssIr, name: &str) -> &'ir ResolvedBody {
    ir.types()
        .iter()
        .find(|(candidate, _)| candidate.to_string() == name)
        .map(|(_, declared)| &declared.body)
        .unwrap_or_else(|| panic!("the specification declares {name}"))
}

#[test]
fn target_kind_resources_and_exit_mirror_the_specification() {
    let loaded = spindle_spec::load(&repository().join("ess")).expect("compiles");

    let declared = declared_type(&loaded.ir, "spindle.placement.Resources");
    let ResolvedBody::Struct { fields, .. } = declared else {
        panic!("Resources is a struct: {declared:?}");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ResourceKind::ALL
            .iter()
            .map(|kind| kind.field())
            .collect::<Vec<_>>()
    );

    let declared = declared_type(&loaded.ir, "spindle.placement.TargetKind");
    let ResolvedBody::Enum { variants } = declared else {
        panic!("TargetKind is an enum: {declared:?}");
    };
    assert_eq!(
        variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<Vec<_>>(),
        TargetKind::ALL
            .iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>()
    );

    let exited = loaded
        .ir
        .events()
        .iter()
        .find(|(name, _)| name.to_string() == "spindle.placement.InstanceExited")
        .map(|(_, event)| event)
        .expect("spindle.placement declares InstanceExited");
    let exit = serde_json::to_value(Exit {
        code: None,
        signal: None,
    })
    .expect("serialises");
    let mut expected: Vec<&str> = vec!["instance_id"];
    expected.extend(
        exit.as_object()
            .expect("an object")
            .keys()
            .map(String::as_str),
    );
    assert_eq!(
        exited
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        expected,
        "InstanceExited carries what Exit reports"
    );
    for field in exited.fields.iter().skip(1) {
        assert!(
            matches!(field.type_ref, ResolvedTypeRef::Optional { .. }),
            "{} is optional, as in Exit: {}",
            field.name,
            field.type_ref
        );
    }
}
