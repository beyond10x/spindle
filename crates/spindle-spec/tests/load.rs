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
        vec!["components.yaml", "domains/deployment.yaml", "system.yaml"]
    );
    assert!(
        loaded
            .ir
            .commands()
            .keys()
            .any(|name| name.to_string() == "spindle.deployment.DecideAdmission")
    );
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
fn an_empty_directory_is_refused_by_name() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let error = spindle_spec::load(directory.path()).expect_err("nothing to load");
    assert!(matches!(error, spindle_spec::LoadError::Empty(_)));
}
