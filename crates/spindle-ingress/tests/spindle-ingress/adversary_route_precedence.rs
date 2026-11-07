//! Adversary cases for `Surface::resolve` choosing among routes that match one request path
//! (`story:route-literal-segment-precedence`), held to the rule ESS 0.90.0 defines for its own
//! served applications (`docs/design/http-presentation-binding.md`, "Path templates", in ESS at
//! the pinned tag): a method's routes are tried in `ess_gen::http::dispatch_order`, a request
//! segment is compared with a literal after decoding percent-encoded unreserved characters
//! (RFC 3986 §6.2.2.2, so `%7Earchived` reaches a `~archived` literal), a `405` names each
//! method once in `Allow`, and two templates equal but for variable names are one route
//! (`ESS-HTTP-010`).
//!
//! The specifications are written into a temporary directory, and each binding is compiled by
//! ESS against them, so every surface here is one ESS admits.

use std::path::Path;

use ess_compiler::EssIr;
use spindle_ingress::{Resolved, Surface, SurfaceError};

const SYSTEM: &str = "format: ess/34
system: catalogue
version: v1
summary: Things asked after by id, and the collection's modules asked after by name.
domains:
  - catalogue.things
components:
  - component: things
    summary: Answers what is asked of a thing and of the modules over the network.
    owns:
      domains:
        - catalogue.things
    accepts:
      commands:
        - catalogue.things.AskThing
        - catalogue.things.AskModules
        - catalogue.things.AddModule
    publishes:
      events:
        - catalogue.things.ThingAsked
        - catalogue.things.ModulesAsked
        - catalogue.things.ModuleAdded
    reached_by: network
";

const THINGS: &str = "domain: catalogue.things
summary: Asks after a thing by id and after the modules, each answered by its own outcome.
types:
  - name: catalogue.things.ThingId
    kind: newtype
    of: String
commands:
  - name: catalogue.things.AskThing
    input:
      - name: id
        type: catalogue.things.ThingId
    outcomes:
      - name: thing-asked
        emits: [catalogue.things.ThingAsked]
        payload:
          catalogue.things.ThingAsked: {id: input.id}
        summary: The thing is asked after by its id.
  - name: catalogue.things.AskModules
    input: []
    outcomes:
      - name: modules-asked
        emits: [catalogue.things.ModulesAsked]
        payload:
          catalogue.things.ModulesAsked: {}
        summary: The modules are asked after.
  - name: catalogue.things.AddModule
    input: []
    outcomes:
      - name: module-added
        emits: [catalogue.things.ModuleAdded]
        payload:
          catalogue.things.ModuleAdded: {}
        summary: A module is added.
events:
  - name: catalogue.things.ThingAsked
    fields:
      - name: id
        type: catalogue.things.ThingId
  - name: catalogue.things.ModulesAsked
    fields: []
  - name: catalogue.things.ModuleAdded
    fields: []
";

/// Two components, each owning one domain with one command taking a key.
const PAIR: &str = "format: ess/34
system: catalogue
version: v1
summary: Things and parts, each asked after by its key, by two components.
domains:
  - catalogue.things
  - catalogue.parts
components:
  - component: things
    summary: Answers what is asked of a thing over the network.
    owns:
      domains:
        - catalogue.things
    accepts:
      commands:
        - catalogue.things.AskThing
    publishes:
      events:
        - catalogue.things.ThingAsked
    reached_by: network
  - component: parts
    summary: Answers what is asked of a part over the network.
    owns:
      domains:
        - catalogue.parts
    accepts:
      commands:
        - catalogue.parts.AskPart
    publishes:
      events:
        - catalogue.parts.PartAsked
    reached_by: network
";

const PAIR_THINGS: &str = "domain: catalogue.things
summary: Asks after a thing by id.
types:
  - name: catalogue.things.ThingId
    kind: newtype
    of: String
commands:
  - name: catalogue.things.AskThing
    input:
      - name: id
        type: catalogue.things.ThingId
    outcomes:
      - name: thing-asked
        emits: [catalogue.things.ThingAsked]
        payload:
          catalogue.things.ThingAsked: {id: input.id}
        summary: The thing is asked after by its id.
events:
  - name: catalogue.things.ThingAsked
    fields:
      - name: id
        type: catalogue.things.ThingId
";

const PAIR_PARTS: &str = "domain: catalogue.parts
summary: Asks after a part by key.
types:
  - name: catalogue.parts.PartKey
    kind: newtype
    of: String
commands:
  - name: catalogue.parts.AskPart
    input:
      - name: key
        type: catalogue.parts.PartKey
    outcomes:
      - name: part-asked
        emits: [catalogue.parts.PartAsked]
        payload:
          catalogue.parts.PartAsked: {key: input.key}
        summary: The part is asked after by its key.
events:
  - name: catalogue.parts.PartAsked
    fields:
      - name: key
        type: catalogue.parts.PartKey
";

/// The specification of `files` (`(relative path, text)`), compiled.
fn ir(files: &[(&str, &str)]) -> EssIr {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a temporary directory");
    for (name, text) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(&path, text).expect("the file is written");
    }
    load(dir.path())
}

fn load(root: &Path) -> EssIr {
    spindle_spec::load(root)
        .unwrap_or_else(|error| panic!("the specification compiles: {error:?}"))
        .ir
}

/// `routes`, the `routes:` list of an `ess-http/1` binding, compiled by ESS against `ir`.
fn binding(ir: &EssIr, routes: &str) -> ess_gen::http::Binding {
    let text = format!(
        "type: ess-http/1\nspecification:\n  system: catalogue\n  version: v1\n  source_digest: sha256:{}\nroutes:\n{routes}",
        ir.source_digest()
    );
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

fn things() -> EssIr {
    ir(&[("system.yaml", SYSTEM), ("domains/things.yaml", THINGS)])
}

/// The path of the route `method` and `path` resolve to, or what else they resolve to.
fn resolved(surface: &Surface, method: &str, path: &str) -> String {
    match surface.resolve(method, path) {
        Resolved::Found(route, _) => route.path.clone(),
        Resolved::Malformed(route, why) => format!("malformed {}: {why}", route.path),
        Resolved::WrongMethod(methods) => format!("405 {methods:?}"),
        Resolved::NotFound => "404".to_owned(),
        other => format!("{other:?}"),
    }
}

const TILDE: &str = "  - command: catalogue.things.AskThing
    method: get
    path: /things/{id}
    parameters: {id: input.id}
    outcomes:
      thing-asked: {status: 200}
  - command: catalogue.things.AskModules
    method: get
    path: /things/~modules
    outcomes:
      modules-asked: {status: 200}
";

/// ESS 0.90.0's served applications compare a request segment with a literal after decoding
/// percent-encoded unreserved characters: `/things/%7Emodules` is the path `/things/~modules`
/// (RFC 3986 §6.2.2.2, RFC 9110 §4.2.3), so the literal route answers it, not the by-id route
/// with the id `~modules`. A client that escapes `~` (Java's `URLEncoder`, Python's `quote`
/// before 3.7) sends exactly this.
#[test]
fn adversary_a_percent_encoded_tilde_reaches_the_tilde_literal() {
    let ir = things();
    let binding = binding(&ir, TILDE);
    let surface = Surface::new(&ir, &["things".to_owned()], Some(&binding)).expect("a surface");
    assert_eq!(
        resolved(&surface, "GET", "/things/~modules"),
        "/things/~modules"
    );
    assert_eq!(
        resolved(&surface, "GET", "/things/%7Emodules"),
        "/things/~modules",
        "`%7E` is `~`: the literal route answers, as ESS's served application answers"
    );
    assert_eq!(
        resolved(&surface, "GET", "/things/%7emodules"),
        "/things/~modules",
        "a lower-case escape is the same character"
    );
}

/// The same rule for a letter: `%6D` is `m`, an unreserved character, so `/things/%6Dodules`
/// reaches the literal `/things/modules`.
#[test]
fn adversary_a_percent_encoded_letter_reaches_the_literal() {
    let ir = things();
    let binding = binding(
        &ir,
        "  - command: catalogue.things.AskThing
    method: get
    path: /things/{id}
    parameters: {id: input.id}
    outcomes:
      thing-asked: {status: 200}
  - command: catalogue.things.AskModules
    method: get
    path: /things/modules
    outcomes:
      modules-asked: {status: 200}
",
    );
    let surface = Surface::new(&ir, &["things".to_owned()], Some(&binding)).expect("a surface");
    assert_eq!(
        resolved(&surface, "GET", "/things/%6Dodules"),
        "/things/modules",
        "`%6D` is `m`: the literal route answers, as ESS's served application answers"
    );
}

/// A nested pair on one method (`POST /things/{id}` beside `POST /things/modules`, admitted from
/// ESS 0.89.0) both match `/things/modules`. Another method there is a `405` whose `Allow` names
/// `POST` once: ESS's served application sorts and deduplicates the methods it names, and the
/// story keeps the 405 "naming the methods of those routes".
#[test]
fn adversary_a_405_names_a_method_two_matching_routes_share_once() {
    let ir = things();
    let binding = binding(
        &ir,
        "  - command: catalogue.things.AskThing
    method: post
    path: /things/{id}
    parameters: {id: input.id}
    outcomes:
      thing-asked: {status: 200}
  - command: catalogue.things.AddModule
    method: post
    path: /things/modules
    outcomes:
      module-added: {status: 201}
",
    );
    let surface = Surface::new(&ir, &["things".to_owned()], Some(&binding)).expect("a surface");
    assert_eq!(
        resolved(&surface, "POST", "/things/modules"),
        "/things/modules"
    );
    let Resolved::WrongMethod(methods) = surface.resolve("DELETE", "/things/modules") else {
        panic!("DELETE on a served path is a 405");
    };
    let named: Vec<String> = methods
        .iter()
        .map(|method| method.as_str().to_uppercase())
        .collect();
    assert_eq!(named, ["POST"], "each method once, as `Allow` names it");
}

/// Two components on one listener with templates equal but for variable names,
/// `GET /things/{id}` and `GET /things/{key}`: ESS calls them one route (`ESS-HTTP-010`), and
/// `Surface` claims to refuse two components claiming one route ([`SurfaceError::Collision`]).
/// Accepting them makes every `GET /things/<x>` answered by whichever component comes first, and
/// the other route unreachable.
#[test]
fn adversary_two_components_claiming_one_template_collide() {
    let ir = ir(&[
        ("system.yaml", PAIR),
        ("domains/things.yaml", PAIR_THINGS),
        ("domains/parts.yaml", PAIR_PARTS),
    ]);
    let binding = binding(
        &ir,
        "  - command: catalogue.things.AskThing
    method: get
    path: /things/{id}
    parameters: {id: input.id}
    outcomes:
      thing-asked: {status: 200}
  - command: catalogue.parts.AskPart
    method: get
    path: /things/{key}
    parameters: {key: input.key}
    outcomes:
      part-asked: {status: 200}
",
    );
    let components = ["things".to_owned(), "parts".to_owned()];
    match Surface::new(&ir, &components, Some(&binding)) {
        Err(SurfaceError::Collision { .. }) => {}
        Err(other) => panic!("refused, but not as a collision: {other:?}"),
        Ok(surface) => panic!(
            "two components claim GET /things/<x>; every request goes to {}",
            resolved(&surface, "GET", "/things/x")
        ),
    }
}
