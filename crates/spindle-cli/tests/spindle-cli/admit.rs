use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .output()
        .expect("b10x-spindle runs")
}

#[test]
fn admitted_exits_zero_and_prints_the_bindings() {
    let output = spindle(&[
        "admit",
        "ess",
        "--environment",
        "examples/environments/laptop.toml",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let admission: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(admission["outcome"], "admitted");
    assert_eq!(admission["bindings"].as_array().map(Vec::len), Some(7));
}

#[test]
fn refused_exits_three_and_names_the_missing_capability() {
    let output = spindle(&[
        "admit",
        "ess",
        "--environment",
        "examples/environments/microvm.toml",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let admission: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(admission["outcome"], "refused");
    assert_eq!(
        admission["missing"],
        serde_json::json!([
            "external.spindle.deployment.DecideAdmission.refused",
            "external.spindle.placement.PlaceInstance.above-ceiling",
            "external.spindle.placement.PlaceInstance.environment-mismatch",
            "external.spindle.placement.PlaceInstance.not-admitted",
            "external.spindle.placement.PlaceInstance.target-retired",
            "external.spindle.placement.PlanInstance.not-admitted"
        ])
    );
}

#[test]
fn an_unknown_component_is_an_error() {
    let output = spindle(&["capabilities", "ess", "--component", "nowhere"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("no component"));
}

/// `tests/fixtures/cross-component-reads` of `spindle-host`: `catalogue` owns resources, and
/// `grants` issues grants reading them (`IssueGrant`, `IssueNamedGrant`) and labels them
/// (`LabelGrantedResource`, an `affects:` entry over the catalogue's entity).
const CROSS: &str = "crates/spindle-host/tests/fixtures/cross-component-reads";
/// `tests/fixtures/metering-split` of `spindle-kernel`: `meter` owns accounts and charges and
/// guards its charges by the holds `holds` owns (`ChargeHeld`, `ChargeDirect`, `ChargeUsage`).
const METERING: &str = "crates/spindle-kernel/tests/fixtures/metering-split";
const LAPTOP: &str = "examples/environments/laptop.toml";

/// The commands that read an entity another component of their fixture owns.
const CROSS_READS: [&str; 5] = [
    "access.grants.IssueGrant",
    "access.grants.IssueNamedGrant",
    "metering.usage.ChargeHeld",
    "metering.usage.ChargeDirect",
    "metering.usage.ChargeUsage",
];

/// The lines `admit` prints on stderr naming a command or an outcome it does not serve.
fn refusals(output: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.contains("not served: ") || line.contains("unsupported outcome "))
        .map(str::to_owned)
        .collect()
}

/// `admit <spec> --environment laptop`, with `--component <component>` where one is given.
fn admit(spec: &str, component: Option<&str>) -> std::process::Output {
    let mut arguments = vec!["admit", spec, "--environment", LAPTOP];
    if let Some(component) = component {
        arguments.extend(["--component", component]);
    }
    let output = spindle(&arguments);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    output
}

/// `admit` without `--component` runs every component in one host, so a read of another
/// component's entity is read from its owner's store and served; an `affects:` entry over one
/// stays refused by name.
#[test]
fn a_guard_over_another_components_row_is_read_from_its_owner() {
    for spec in [CROSS, METERING] {
        let refused = refusals(&admit(spec, None));
        for command in CROSS_READS {
            assert!(
                refused.iter().all(|line| !line.contains(command)),
                "{spec}: {command} is served: {refused:#?}"
            );
        }
        if spec == CROSS {
            assert_eq!(
                refused,
                [
                    "b10x-spindle: not served: access.grants.LabelGrantedResource, refused for \
                     every request: unsupported: an `affects:` entry over \
                     `access.resources.Resource`, owned by component `catalogue`: the kernel \
                     writes only the store of component `grants`, which runs the command, in \
                     `access.grants.LabelGrantedResource/labelled`"
                ],
            );
        }
    }
}

/// `admit --component <runner>` runs that component alone, as `play --component` does: a read of
/// another component's entity is refused by name, naming the entity, its owner and the running
/// component.
#[test]
fn a_split_run_refuses_a_read_of_another_components_entity_by_name() {
    let meter = refusals(&admit(METERING, Some("meter")));
    for (command, guarded) in [
        ("metering.usage.ChargeHeld", "no-hold"),
        ("metering.usage.ChargeDirect", "on-hold"),
        ("metering.usage.ChargeUsage", "on-hold"),
    ] {
        let named = format!(
            "b10x-spindle: not served: {command}, refused for every request: unsupported: the \
             `when_related:` guard of `{command}/{guarded}`, which reads the rows of \
             `metering.holds.Hold`, owned by component `holds`: the kernel reads only the store \
             of component `meter`, which runs the command, in `{command}/"
        );
        assert!(
            meter.iter().any(|line| line.starts_with(&named)),
            "{named} in {meter:#?}"
        );
    }
    assert_eq!(
        meter.len(),
        3,
        "ChargeOnce reads only the meter's rows: {meter:#?}"
    );

    let grants = refusals(&admit(CROSS, Some("grants")));
    for command in ["access.grants.IssueGrant", "access.grants.IssueNamedGrant"] {
        let line = grants
            .iter()
            .find(|line| line.contains(&format!("not served: {command},")))
            .unwrap_or_else(|| panic!("{command} is not served: {grants:#?}"));
        for named in [
            "`access.resources.Resource`",
            "owned by component `catalogue`",
            "component `grants`, which runs the command",
        ] {
            assert!(line.contains(named), "{named} in {line}");
        }
    }
    assert!(
        refusals(&admit(METERING, Some("holds"))).is_empty(),
        "the holds component reads only its own rows"
    );
}

/// `run` reading stdin keeps every component's rows in one store: a command reading another
/// component's entity answers what ESS's interpreter answers on the same steps
/// (`spindle-host`'s `cross_component_reads` compares them), and is not refused at start.
#[test]
fn a_stdin_run_reads_another_components_entity() {
    let cases: [(&str, &str, &[&str]); 2] = [
        (
            CROSS,
            concat!(
                r#"{"command": "access.resources.AddResource", "input": {"resource_id": "r1", "name": "printer"}}"#,
                "\n",
                r#"{"command": "access.resources.AddResource", "input": {"resource_id": "r2", "name": "scanner"}}"#,
                "\n",
                r#"{"command": "access.grants.IssueGrant", "input": {"grant_id": "g1", "holder": "ada", "name": "scanner"}}"#,
                "\n",
                r#"{"command": "access.grants.IssueNamedGrant", "input": {"grant_id": "g2", "holder": "bo", "resource_id": "r1"}}"#,
                "\n",
            ),
            &["added", "added", "issued", "issued"],
        ),
        (
            METERING,
            concat!(
                r#"{"command": "metering.usage.OpenAccount", "input": {"account_id": "a0a0a0a0-0000-4000-8000-000000000001", "org": "north"}}"#,
                "\n",
                r#"{"command": "metering.usage.ChargeHeld", "input": {"charge_id": "c0c0c0c0-0000-4000-8000-000000000001", "account_id": "a0a0a0a0-0000-4000-8000-000000000001", "hold_id": "b0b0b0b0-0000-4000-8000-000000000001"}}"#,
                "\n",
                r#"{"command": "metering.holds.PlaceHold", "input": {"hold_id": "b0b0b0b0-0000-4000-8000-000000000001", "org": "north"}}"#,
                "\n",
                r#"{"command": "metering.usage.ChargeHeld", "input": {"charge_id": "c0c0c0c0-0000-4000-8000-000000000002", "account_id": "a0a0a0a0-0000-4000-8000-000000000001", "hold_id": "b0b0b0b0-0000-4000-8000-000000000001"}}"#,
                "\n",
                r#"{"command": "metering.usage.ChargeDirect", "input": {"charge_id": "c0c0c0c0-0000-4000-8000-000000000003", "account_id": "a0a0a0a0-0000-4000-8000-000000000001", "org": "north"}}"#,
                "\n",
                r#"{"command": "metering.usage.ChargeUsage", "input": {"charge_id": "c0c0c0c0-0000-4000-8000-000000000004", "account_id": "a0a0a0a0-0000-4000-8000-000000000001"}}"#,
                "\n",
            ),
            &[
                "opened", "no-hold", "placed", "charged", "on-hold", "on-hold",
            ],
        ),
    ];
    for (spec, lines, expected) in cases {
        let output = stdin_run(spec, lines);
        assert_eq!(output.status.code(), Some(0), "{spec}: {output:?}");
        let outcomes: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| {
                let result: serde_json::Value =
                    serde_json::from_str(line).expect("each result is one JSON line");
                result["outcome"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{spec}: an outcome in {result}"))
                    .to_owned()
            })
            .collect();
        assert_eq!(outcomes, expected, "{spec}");
        let refused = refusals(&output);
        for command in CROSS_READS {
            assert!(
                refused.iter().all(|line| !line.contains(command)),
                "{spec}: {command} is not refused at start: {refused:#?}"
            );
        }
    }
}

/// `run <spec> --environment laptop`, reading `lines` on stdin.
fn stdin_run(spec: &str, lines: &str) -> std::process::Output {
    use std::io::Write as _;
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state.sqlite"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes())
        .expect("the lines are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

/// `metering-split` with `meter` reached over the network, served by `run` on loopback in one host
/// beside `holds`: a request for a command whose guard reads a hold answers the outcome ESS's
/// interpreter selects with its status, not `501`, and `GET /api/served` lists its route served.
#[test]
fn a_served_run_answers_a_cross_component_guard() {
    use std::io::{BufRead as _, BufReader};
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let spec = directory.path().join("metering");
    std::fs::create_dir_all(spec.join("domains")).expect("the fixture's directory");
    for domain in ["holds.yaml", "usage.yaml"] {
        std::fs::copy(
            root().join(METERING).join("domains").join(domain),
            spec.join("domains").join(domain),
        )
        .expect("the domain is copied");
    }
    let system = std::fs::read_to_string(root().join(METERING).join("system.yaml"))
        .expect("the system is read");
    std::fs::write(
        spec.join("system.yaml"),
        system.replacen("reached_by: in_process", "reached_by: network", 1),
    )
    .expect("the system is written");
    let seed = directory.path().join("seed.json");
    std::fs::write(
        &seed,
        serde_json::json!([
            {"command": "metering.usage.OpenAccount", "input": {"account_id": "a0a0a0a0-0000-4000-8000-000000000001", "org": "north"}, "request_id": "seed-account"},
            {"command": "metering.holds.PlaceHold", "input": {"hold_id": "b0b0b0b0-0000-4000-8000-000000000001", "org": "north"}, "request_id": "seed-hold"},
        ])
        .to_string(),
    )
    .expect("the seed is written");

    let mut served = Served(
        Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
            .current_dir(root())
            .env("TMPDIR", directory.path())
            .arg("run")
            .arg(&spec)
            .args(["--environment", LAPTOP, "--seed"])
            .arg(&seed)
            .arg("--state")
            .arg(directory.path().join("state"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("b10x-spindle starts"),
    );
    let child = &mut served.0;
    let stderr = child.stderr.take().expect("stderr is piped");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut first = String::new();
    BufReader::new(child.stdout.take().expect("stdout is piped"))
        .read_line(&mut first)
        .expect("run prints where it listens");
    let ready: serde_json::Value =
        serde_json::from_str(&first).unwrap_or_else(|error| panic!("{first:?}: {error}"));
    let address: std::net::SocketAddr = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("an address in {ready}"));
    let mut printed = Vec::new();
    loop {
        let line = receiver
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap_or_else(|_| panic!("the seed ends: {printed:#?}"));
        let ended = line.contains(" ended: ");
        printed.push(line);
        if ended {
            break;
        }
    }
    assert!(
        printed.iter().all(|line| !line.contains("not served: ")),
        "every route is served: {printed:#?}"
    );

    let (status, listed) = exchange(address, "GET", "/api/served", None);
    assert_eq!(status, 200, "{listed}");
    let row = listed["operations"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["name"] == "metering.usage.ChargeHeld")
        })
        .unwrap_or_else(|| panic!("ChargeHeld is listed: {listed}"))
        .clone();
    assert_eq!(row["served"], true, "{row}");
    let method = row["method"].as_str().expect("a method").to_owned();
    let path = row["path"].as_str().expect("a path").to_owned();
    let mut answered = Vec::new();
    for (number, hold) in [
        (1, "b0b0b0b0-0000-4000-8000-000000000001"),
        (2, "b0b0b0b0-0000-4000-8000-0000000000ff"),
    ] {
        let (status, body) = exchange(
            address,
            &method,
            &path,
            Some(&serde_json::json!({
                "charge_id": format!("c0c0c0c0-0000-4000-8000-00000000000{number}"),
                "account_id": "a0a0a0a0-0000-4000-8000-000000000001",
                "hold_id": hold,
            })),
        );
        assert_ne!(status, 501, "{body}");
        answered.push((
            body["outcome"].as_str().unwrap_or_default().to_owned(),
            status,
        ));
    }
    assert_eq!(
        answered
            .iter()
            .map(|(outcome, _)| outcome.as_str())
            .collect::<Vec<_>>(),
        ["charged", "no-hold"],
        "the hold the holds component keeps is read: {answered:?}"
    );
    assert!(answered[0].1 < 300 && answered[1].1 >= 400, "{answered:?}");
    drop(served);
}

/// A running `b10x-spindle run`, stopped when dropped.
struct Served(std::process::Child);

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// One request on its own connection: the status and the JSON body.
fn exchange(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
) -> (u16, serde_json::Value) {
    use std::io::{Read as _, Write as _};
    let timeout = std::time::Duration::from_secs(10);
    let mut stream =
        std::net::TcpStream::connect_timeout(&address, timeout).expect("the listener accepts");
    stream
        .set_read_timeout(Some(timeout))
        .expect("a read timeout");
    let body = body.map(ToString::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    let answer = String::from_utf8(answer).expect("the answer is UTF-8");
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}
