//! A placement entry naming a component the specification does not declare is refused before
//! anything is launched (`story:unknown-component-refused-at-deploy`).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use spindle_control::{Control, ControlError, Options, PlacementDocument};
use spindle_target::Artifact;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn spec() -> PathBuf {
    root().join("tests/fixtures/notes")
}

fn environment() -> PathBuf {
    root().join("tests/fixtures/loopback.toml")
}

/// A `pad` instance, then an instance of `shelf_component`, on one process target.
fn placement(shelf_component: &str) -> String {
    format!(
        r#"format = "spindle-placement/1"

[[targets]]
id = "host"
kind = "Process"
environment = "loopback"

[[instances]]
id = "pad-0"
component = "pad"
target = "host"
settings = {{ period_ms = "200", seed_note = "n-1" }}

[[instances]]
id = "shelf-0"
component = "{shelf_component}"
target = "host"
settings = {{ period_ms = "200" }}
"#
    )
}

/// Starts the control process on `placement`, counting every artifact asked for: one per instance
/// launched.
fn start(
    placement: &str,
    work: &Path,
    launched: &Arc<AtomicUsize>,
) -> Result<Control, ControlError> {
    let loaded = spindle_spec::load(&spec()).expect("the fixture compiles");
    let text = std::fs::read_to_string(environment()).expect("the environment is read");
    let environment_document =
        spindle_capability::Environment::parse(&text).expect("the environment parses");
    let placement = PlacementDocument::parse(placement).expect("the placement parses");
    let fixture = PathBuf::from(env!("CARGO_BIN_EXE_spindle-control-fixture"));
    let args = vec![
        spec().display().to_string(),
        environment().display().to_string(),
    ];
    let counted = Arc::clone(launched);
    Control::start(
        loaded.ir,
        environment_document,
        placement,
        Box::new(move |_component: &str| {
            counted.fetch_add(1, Ordering::SeqCst);
            Artifact::HostBinary {
                path: fixture.clone(),
                args: args.clone(),
            }
        }),
        Options::new(work),
    )
}

#[test]
fn a_placement_naming_an_undeclared_component_is_refused_before_anything_is_launched() {
    let work = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a work directory");
    let launched = Arc::new(AtomicUsize::new(0));

    let refused = start(&placement("shelff"), work.path(), &launched)
        .expect_err("a placement naming an undeclared component is refused");

    match &refused {
        ControlError::UnknownComponent {
            instance,
            component,
        } => {
            assert_eq!(instance, "shelf-0");
            assert_eq!(component, "shelff");
        }
        other => panic!("refused for another reason: {other}"),
    }
    let message = refused.to_string();
    assert!(
        message.contains("shelf-0") && message.contains("shelff"),
        "the refusal names the entry and the component: {message}"
    );
    assert_eq!(
        launched.load(Ordering::SeqCst),
        0,
        "no instance is launched, not even the declared one before it"
    );
    assert!(
        !work.path().join("consoles").exists(),
        "no instance is registered with a console"
    );
}

#[test]
fn the_same_placement_with_every_component_declared_deploys() {
    let work = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a work directory");
    let launched = Arc::new(AtomicUsize::new(0));

    let control =
        start(&placement("shelf"), work.path(), &launched).expect("the control process starts");

    assert_eq!(launched.load(Ordering::SeqCst), 2, "both instances launch");
    assert!(control.not_run().is_empty(), "{:?}", control.not_run());
    control.stop();
}
