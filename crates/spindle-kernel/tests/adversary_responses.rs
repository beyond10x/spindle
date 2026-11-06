//! Adversary cases for `story:kernel-command-responses`: response field types beyond the
//! fixture's two `String`s, each answered by the kernel and checked by ESS's own response check
//! for the same command and outcome.

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::{CommandRef, OutcomeRef};
use ess_conformance::direct_response::Observation;
use ess_primitives::node::Node;
use serde_json::{Map, Value};
use spindle_kernel::{MemoryStore, NoExternals, execute};

const DOMAIN: &str = r"domain: notes.desk
summary: A desk that issues a slip answering one value of each type the runtime mints.
types:
  - name: notes.desk.Code
    kind: newtype
    of: String
    prefix: cd_
  - name: notes.desk.Secret
    kind: newtype
    of: String
    prefix: sk_
commands:
  - name: notes.desk.IssueSlip
    input:
      - name: desk
        type: String
    response:
      - name: count
        type: Integer
      - name: flag
        type: Boolean
      - name: blob
        type: Bytes
      - name: at
        type: Timestamp
      - name: tags
        type: List<String>
      - name: note
        type: Optional<String>
      - name: code
        type: notes.desk.Code
      - name: secret
        type: notes.desk.Secret
    outcomes:
      - name: issued
        returns: true
        one_time_response: [secret]
        summary: The slip is issued; its secret is disclosed in this answer only.
";

const SYSTEM: &str = r"format: ess/34
system: notes
version: v1
summary: A desk that issues slips.
domains:
  - notes.desk
components:
  - component: desk
    summary: Issues slips for callers outside the process.
    owns:
      domains:
        - notes.desk
    accepts:
      commands:
        - notes.desk.IssueSlip
    reached_by: network
";

const ISSUE: &str = "notes.desk.IssueSlip";

fn ir(dir: &Path) -> spindle_spec::EssIr {
    std::fs::create_dir_all(dir.join("domains")).expect("the domains directory");
    std::fs::write(dir.join("system.yaml"), SYSTEM).expect("the system is written");
    std::fs::write(dir.join("domains/desk.yaml"), DOMAIN).expect("the domain is written");
    spindle_spec::load(dir)
        .expect("the desk fixture compiles")
        .ir
}

#[test]
fn every_minted_response_type_passes_esss_response_check() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let ir = ir(dir.path());
    let mut input = Map::new();
    input.insert("desk".to_owned(), Value::String("front".to_owned()));
    let issued = execute(
        &ir,
        &mut MemoryStore::default(),
        &mut NoExternals,
        ISSUE,
        &input,
    )
    .expect("issuing runs");
    assert_eq!(issued.outcome, "issued");
    let response = issued
        .response
        .clone()
        .unwrap_or_else(|| panic!("a returning outcome answers a response: {issued:?}"));
    let command = ir
        .commands()
        .values()
        .find(|command| command.name.to_string() == ISSUE)
        .expect("the fixture declares IssueSlip");
    let selected = OutcomeRef::new(
        CommandRef::new(command.name.clone()),
        command.outcomes[0].name.clone(),
    );
    let check = Observation::of(&ir, command, Some(selected), BTreeMap::new())
        .expect("ESS derives the response check from the command");
    let answered: BTreeMap<String, Node> = response
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                serde_json::from_value(value.clone()).expect("a JSON value is a node"),
            )
        })
        .collect();
    check.compare(Some(&answered)).unwrap_or_else(|refused| {
        panic!("ESS's response check refuses the kernel's response {response:?}: {refused}")
    });
}
