//! The integration tests of `spindle-embed`, one module per subject, linked into one test binary
//! (`story:one-test-binary-per-crate`): a host opened in-process, as every wasm host opens one,
//! answered one request at a time.

mod clock;
