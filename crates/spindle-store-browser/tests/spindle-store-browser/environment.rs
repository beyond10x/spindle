//! The browser environment document binds `store.durable` to this store.

use std::path::Path;

use spindle_capability::{Admission, Environment, admit, derive};

#[test]
fn the_browser_environment_binds_store_durable_to_indexeddb() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("../../examples/environments/browser.toml"))
        .expect("examples/environments/browser.toml reads");
    let environment = Environment::parse(&text).expect("the browser environment parses");
    let ir = spindle_spec::load(&root.join("tests/fixtures/library"))
        .expect("the library fixture compiles")
        .ir;
    let requirements = derive(&ir, None).expect("the system's requirements derive");
    let Admission::Admitted { bindings } = admit(&requirements, &environment) else {
        panic!("the browser environment admits the library fixture")
    };
    let durable = bindings
        .iter()
        .find(|binding| binding.capability.as_str() == "store.durable")
        .expect("the library fixture needs store.durable");
    assert_eq!(
        durable.implementation,
        spindle_store_browser::IMPLEMENTATION
    );
    assert_eq!(spindle_store_browser::IMPLEMENTATION, "indexeddb");
}
