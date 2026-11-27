use std::fs;
use std::path::Path;

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<name> sits two levels below the repository root")
}

#[test]
fn spindles_own_specification_compiles() {
    let loaded = spindle_spec::load(&repository_root().join("ess")).expect("ess/ compiles");
    assert_eq!(loaded.ir.system().to_string(), "spindle");
    assert_eq!(
        loaded.files,
        vec![
            "system.yaml",
            "components.yaml",
            "domains/deployment.yaml",
            "domains/placement.yaml",
            "domains/observe.yaml",
            "domains/environment.yaml",
            "domains/console.yaml"
        ]
    );
    assert!(
        loaded
            .ir
            .commands()
            .keys()
            .any(|name| name.to_string() == "spindle.deployment.DecideAdmission")
    );
    let scenarios = loaded
        .scenarios
        .iter()
        .map(|scenario| scenario.origin.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        scenarios,
        vec![
            "scenarios/launch-not-admitted.yaml",
            "scenarios/admission-missing-provided.yaml",
            "scenarios/admission-refused.yaml",
            "scenarios/admission-admitted.yaml",
        ]
    );
    assert_eq!(
        loaded.scenarios[0].text,
        fs::read_to_string(repository_root().join("ess/scenarios/launch-not-admitted.yaml"))
            .expect("the authored scenario reads")
    );
}

#[test]
fn the_scenarios_an_inputs_manifest_lists_are_read() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path();
    fs::write(
        root.join("system.yaml"),
        "format: ess/34\nsystem: listed\nversion: v1\ndomains: []\n",
    )
    .expect("write");
    fs::create_dir(root.join("scenarios")).expect("the scenarios directory");
    fs::write(root.join("scenarios/one.yaml"), "type: ess-scenario/1\n").expect("write");
    fs::write(
        root.join("scenarios/unlisted.yaml"),
        "type: ess-scenario/1\n",
    )
    .expect("write");
    fs::write(
        root.join("ess-inputs.yaml"),
        "format: ess-inputs/2\nspecification:\n  - system.yaml\nscenarios:\n  - scenarios/one.yaml\n",
    )
    .expect("write");
    let loaded = spindle_spec::load(root).expect("the listed scenario is read");
    assert_eq!(loaded.files, vec!["system.yaml"]);
    let scenarios = loaded
        .scenarios
        .iter()
        .map(|scenario| (scenario.origin.as_str(), scenario.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        scenarios,
        vec![("scenarios/one.yaml", "type: ess-scenario/1\n")]
    );

    fs::write(
        root.join("ess-inputs.yaml"),
        "format: ess-inputs/2\nspecification:\n  - system.yaml\nscenarios:\n  - scenarios/missing.yaml\n",
    )
    .expect("write");
    let error = spindle_spec::load(root).expect_err("a listed scenario that is missing is refused");
    assert!(
        error.to_string().contains("scenarios/missing.yaml"),
        "{error}"
    );

    fs::remove_file(root.join("ess-inputs.yaml")).expect("remove the manifest");
    let loaded = spindle_spec::load(&root.join("system.yaml")).expect("one file loads");
    assert!(
        loaded.scenarios.is_empty(),
        "without a manifest no scenario is implied: {:?}",
        loaded.scenarios
    );
}

/// `load_model` checks the manifest and every file it lists as `load` does, and reads no scenario
/// text: a listed scenario that does not read as text loads the model, which `load` refuses; one
/// that is missing is refused by both.
#[test]
fn a_model_load_reads_no_scenario() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path();
    fs::write(
        root.join("system.yaml"),
        "format: ess/34\nsystem: listed\nversion: v1\ndomains: []\n",
    )
    .expect("write");
    fs::create_dir(root.join("scenarios")).expect("the scenarios directory");
    fs::write(root.join("scenarios/one.yaml"), [0xff, 0xfe, 0x00, 0x80]).expect("write");
    fs::write(
        root.join("ess-inputs.yaml"),
        "format: ess-inputs/2\nspecification:\n  - system.yaml\nscenarios:\n  - scenarios/one.yaml\n",
    )
    .expect("write");
    let model = spindle_spec::load_model(root).expect("the model loads without the scenario");
    assert_eq!(model.files, vec!["system.yaml"]);
    assert_eq!(model.ir.system().to_string(), "listed");
    let error = spindle_spec::load(root).expect_err("load reads the scenario");
    assert!(error.to_string().contains("scenarios/one.yaml"), "{error}");

    fs::write(
        root.join("ess-inputs.yaml"),
        "format: ess-inputs/2\nspecification:\n  - system.yaml\nscenarios:\n  - scenarios/missing.yaml\n",
    )
    .expect("write");
    for error in [
        spindle_spec::load_model(root).expect_err("a listed scenario that is missing is refused"),
        spindle_spec::load(root).expect_err("a listed scenario that is missing is refused"),
    ] {
        assert!(
            error.to_string().contains("scenarios/missing.yaml"),
            "{error}"
        );
    }
}

#[test]
fn a_refused_document_reports_what_ess_said() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    fs::write(
        directory.path().join("system.yaml"),
        "format: ess/34\nsystem: broken\nversion: \"0.1\"\ndomains: []\n",
    )
    .expect("write");
    let error = spindle_spec::load(directory.path()).expect_err("an invalid version is refused");
    assert!(
        error.to_string().contains("versions are written"),
        "{error}"
    );
}

#[test]
fn an_inputs_manifest_decides_which_files_are_read() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path();
    fs::write(
        root.join("system.yaml"),
        "format: ess/34\nsystem: listed\nversion: v1\ndomains: []\n",
    )
    .expect("write");
    fs::write(root.join("stray.yaml"), "not: an ess document\n").expect("write");
    fs::write(
        root.join("ess-inputs.yaml"),
        "format: ess-inputs/2\nrequires: ess 0.87\nspecification:\n  - system.yaml\nscenarios: []\n",
    )
    .expect("write");
    let loaded = spindle_spec::load(root).expect("only the listed file is read");
    assert_eq!(loaded.files, vec!["system.yaml"]);

    fs::write(
        root.join("ess-inputs.yaml"),
        "format: ess-inputs/2\nspecification:\n  - system.yaml\n  - missing.yaml\nscenarios: []\n",
    )
    .expect("write");
    let error = spindle_spec::load(root).expect_err("a listed file that is missing is refused");
    assert!(error.to_string().contains("missing.yaml"), "{error}");
}

#[test]
fn an_empty_directory_is_refused_by_name() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let error = spindle_spec::load(directory.path()).expect_err("nothing to load");
    assert!(matches!(error, spindle_spec::LoadError::Empty(_)));
}

/// A domain whose command `demo.shelf.PlaceBook` takes a shelf's identity twice, at the top of its
/// input (`shelf`) and as the member of a struct input (`wrap.shelf`), and guards its first branch
/// with `when_related: {via: <via>, exists: false}`.
fn shelf_specification(root: &Path, via: &str) {
    fs::write(
        root.join("system.yaml"),
        "format: ess/34\nsystem: demo\nversion: v1\ndomains:\n  - demo.shelf\n",
    )
    .expect("write");
    fs::create_dir_all(root.join("domains")).expect("the domains directory");
    fs::write(
        root.join("domains/shelf.yaml"),
        format!(
            "domain: demo.shelf
types:
  - {{name: demo.shelf.ShelfId, kind: newtype, of: String}}
  - {{name: demo.shelf.BookId, kind: newtype, of: String}}
  - name: demo.shelf.Wrap
    kind: struct
    fields:
      - {{name: shelf, type: demo.shelf.ShelfId}}
entities:
  - name: demo.shelf.Shelf
    identity: {{name: shelf_id, type: demo.shelf.ShelfId}}
    lifecycle: {{initial: Open, states: [Open], terminal: [Open]}}
  - name: demo.shelf.Book
    identity: {{name: book_id, type: demo.shelf.BookId}}
    lifecycle: {{initial: Placed, states: [Placed], terminal: [Placed]}}
errors:
  - name: demo.shelf.NoShelf
    summary: No shelf carries the id.
    fields: []
commands:
  - name: demo.shelf.PlaceBook
    input:
      - {{name: book_id, type: demo.shelf.BookId}}
      - {{name: shelf, type: demo.shelf.ShelfId}}
      - {{name: wrap, type: demo.shelf.Wrap}}
    outcomes:
      - name: no-shelf
        when_related: {{via: {via}, exists: false}}
        error: demo.shelf.NoShelf
        summary: The named shelf does not exist.
      - name: placed
        creates: demo.shelf.Book
        instance: book_id
        emits: [demo.shelf.BookPlaced]
        payload:
          demo.shelf.BookPlaced: {{book_id: input.book_id}}
        summary: The book is placed.
events:
  - name: demo.shelf.BookPlaced
    fields:
      - {{name: book_id, type: demo.shelf.BookId}}
"
        ),
    )
    .expect("write");
}

/// `story:related-via-nested-input`: ESS refuses a `when_related:` guard whose `via:` names a
/// member of a struct input when it checks the specification (`undeclared_reference`), so no IR
/// with such a guard reaches the kernel, whose read of `ResolvedRelatedVia::Input` is flat. The
/// day ESS accepts the guard this test fails, and the kernel's read follows the path then.
#[test]
fn nested_related_via_is_rejected_at_load() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path();

    shelf_specification(root, "input.shelf");
    spindle_spec::load(root).expect("the same guard through a top-level input loads");

    shelf_specification(root, "input.wrap.shelf");
    let error = spindle_spec::load(root).expect_err("a guard through a nested member is refused");
    let text = error.to_string();
    assert!(
        matches!(error, spindle_spec::LoadError::Refused(_)),
        "{text}"
    );
    assert!(text.contains("[undeclared_reference]"), "{text}");
    assert!(
        text.lines()
            .any(|line| line.contains("[undeclared_reference]")
                && line.contains("demo.shelf.PlaceBook")
                && line.contains("outcomes.no-shelf.when_related")
                && line.contains("`wrap.shelf`")),
        "{text}"
    );
}
