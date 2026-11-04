//! A stand-in instance for the process launcher's tests.
//!
//! It prints its arguments and every `SPINDLE_*` variable it was given, one per line, then writes
//! `ready` to standard error. With the setting `mode` = `exit` it exits with the code in the
//! setting `exit_code`; otherwise it runs until it is signalled.

use std::io::Write;
use std::time::Duration;

fn main() {
    let mut out = std::io::stdout().lock();
    for argument in std::env::args().skip(1) {
        writeln!(out, "arg {argument}").expect("stdout");
    }
    let mut variables: Vec<(String, String)> = std::env::vars()
        .filter(|(name, _)| name.starts_with("SPINDLE_"))
        .collect();
    variables.sort();
    for (name, value) in &variables {
        writeln!(out, "env {name}={value}").expect("stdout");
    }
    out.flush().expect("stdout");
    drop(out);
    eprintln!("ready");

    let setting = |name: &str| std::env::var(format!("SPINDLE_SETTING_{name}")).ok();
    if setting("MODE").as_deref() == Some("exit") {
        let code = setting("EXIT_CODE")
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        std::process::exit(code);
    }
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}
